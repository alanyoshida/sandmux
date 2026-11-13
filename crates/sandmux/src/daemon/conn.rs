//! Uma conexão de cliente: saudação, pedidos e o modo anexado.

use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use sandmux_proto::msg::answer_hello;
use sandmux_proto::{Frame, Msg, SessionState, Size, read_frame, write_control, write_data};

use super::session::{self, CLIENT_QUEUE, Client, Out};
use super::{Shared, State};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn handle(shared: Arc<Shared>, stream: UnixStream) {
    let _ = serve(&shared, stream);
}

fn list_text(st: &State) -> String {
    if st.sessions.is_empty() {
        return " Não há sessões.".into();
    }
    let mut s = String::from(" Sessões:");
    for x in st.sessions.values() {
        s.push_str(&format!("\n  {}  {}  {}", x.id, x.name, x.state));
    }
    s
}

fn resolve(st: &State, id: Option<u32>) -> Result<u32, String> {
    match id {
        Some(i) if st.sessions.contains_key(&i) => Ok(i),
        Some(i) => Err(format!("não existe a sessão {i}.{}", list_text(st))),
        None => {
            let live: Vec<u32> = st
                .sessions
                .values()
                .filter(|s| s.state == SessionState::Viva)
                .map(|s| s.id)
                .collect();
            match live.as_slice() {
                [only] => Ok(*only),
                [] => Err(format!("não há sessão viva.{}", list_text(st))),
                many => Err(format!(
                    "há {} sessões vivas; escolha uma com `sandmux attach <id>`.{}",
                    many.len(),
                    list_text(st)
                )),
            }
        }
    }
}

fn error(stream: &mut UnixStream, message: impl Into<String>) -> Result<()> {
    write_control(
        stream,
        &Msg::Error {
            message: message.into(),
        },
    )?;
    Ok(())
}

fn serve(shared: &Arc<Shared>, mut stream: UnixStream) -> Result<()> {
    let Some(Frame::Control(hello)) = read_frame(&mut stream)? else {
        return Ok(());
    };
    let (reply, ok) = answer_hello(
        &hello,
        shared.protocol,
        VERSION,
        shared.pid,
        &shared.start_mode,
        &shared.base.data_dir,
    );
    write_control(&mut stream, &reply)?;
    if !ok {
        return Ok(());
    }
    loop {
        let Some(frame) = read_frame(&mut stream)? else {
            return Ok(());
        };
        let Frame::Control(msg) = frame else { continue };
        match msg {
            Msg::List => {
                let sessions = shared.list(&shared.lock());
                write_control(&mut stream, &Msg::Sessions { sessions })?;
            }
            Msg::Status => {
                let sessions = shared.lock().sessions.len();
                write_control(
                    &mut stream,
                    &Msg::StatusInfo {
                        pid: shared.pid,
                        protocol: shared.protocol,
                        version: VERSION.into(),
                        start_mode: shared.start_mode.clone(),
                        sessions,
                        socket: shared.socket.clone(),
                    },
                )?;
            }
            Msg::Stop => {
                write_control(&mut stream, &Msg::Ok)?;
                let _ = stream.flush();
                shared.stop();
            }
            Msg::Kill { id } => match kill(shared, id) {
                Ok(()) => write_control(&mut stream, &Msg::Ok)?,
                Err(e) => error(&mut stream, e)?,
            },
            Msg::Detach { id } => {
                let mut st = shared.lock();
                match st.sessions.get_mut(&id) {
                    Some(s) => {
                        s.drop_clients(Msg::Detached {
                            reason: format!(
                                "desanexado por `sandmux detach {id}`; a sessão continua viva"
                            ),
                        });
                        drop(st);
                        write_control(&mut stream, &Msg::Ok)?;
                    }
                    None => {
                        let text = list_text(&st);
                        drop(st);
                        error(&mut stream, format!("não existe a sessão {id}.{text}"))?;
                    }
                }
            }
            Msg::Create {
                opts,
                vars,
                size,
                attach,
            } => match session::spawn(shared, &opts, &vars, size) {
                Err(e) => error(&mut stream, format!("{e:#}"))?,
                Ok(spawned) => {
                    for w in &spawned.warnings {
                        write_control(&mut stream, &Msg::Warning { message: w.clone() })?;
                    }
                    let (id, name) = session::register(shared, opts, spawned, size);
                    write_control(&mut stream, &Msg::Created { id, name })?;
                    if attach {
                        return attached(shared, stream, id, size);
                    }
                }
            },
            Msg::Restart {
                id,
                vars,
                size,
                attach,
            } => match session::relaunch(shared, id, &vars, Some(size)) {
                Err(e) => error(&mut stream, format!("{e:#}"))?,
                Ok(warnings) => {
                    for w in warnings {
                        write_control(&mut stream, &Msg::Warning { message: w })?;
                    }
                    let name = shared
                        .lock()
                        .sessions
                        .get(&id)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    write_control(&mut stream, &Msg::Created { id, name })?;
                    if attach {
                        return attached(shared, stream, id, size);
                    }
                }
            },
            Msg::Attach { id, size } => {
                let resolved = resolve(&shared.lock(), id);
                match resolved {
                    Ok(id) => return attached(shared, stream, id, size),
                    Err(e) => error(&mut stream, e)?,
                }
            }
            other => error(&mut stream, format!("pedido inesperado: {other:?}"))?,
        }
    }
}

/// Encerra a sessão: SIGTERM, espera, SIGKILL. Volta quando ela saiu da lista.
fn kill(shared: &Arc<Shared>, id: u32) -> Result<(), String> {
    let pid = {
        let mut st = shared.lock();
        let Some(s) = st.sessions.get_mut(&id) else {
            return Err(format!("não existe a sessão {id}.{}", list_text(&st)));
        };
        match s.pid() {
            Some(pid) => {
                s.killing = true;
                pid
            }
            None => {
                s.drop_clients(Msg::Detached {
                    reason: format!("a sessão {id} foi encerrada por `sandmux kill`"),
                });
                st.sessions.remove(&id);
                shared.persist(&st);
                drop(st);
                shared.changed.notify_all();
                return Ok(());
            }
        }
    };
    session::terminate(pid);
    let gone = |st: &State| !st.sessions.contains_key(&id);
    if !shared.wait_until(Duration::from_secs(5), gone) {
        let same = shared.lock().sessions.get(&id).and_then(|s| s.pid()) == Some(pid);
        if same {
            session::signal(pid, libc::SIGKILL);
        }
        if !shared.wait_until(Duration::from_secs(5), gone) {
            return Err(format!("a sessão {id} não terminou"));
        }
    }
    Ok(())
}

/// Modo anexado: a saída da sessão vai para o cliente pela fila; a entrada e
/// os redimensionamentos do cliente vão para a sessão.
fn attached(shared: &Arc<Shared>, mut stream: UnixStream, id: u32, size: Size) -> Result<()> {
    let (tx, rx) = mpsc::sync_channel::<Out>(CLIENT_QUEUE);
    let mut wstream = stream.try_clone()?;
    let writer = std::thread::spawn(move || {
        for out in rx {
            let r = match out {
                Out::Data(d) => write_data(&mut wstream, &d),
                Out::Msg(m) => write_control(&mut wstream, &m),
            };
            if r.is_err() {
                break;
            }
        }
        let _ = wstream.shutdown(Shutdown::Both);
    });

    let (client_id, master) = {
        let mut st = shared.lock();
        let client_id = st.next_client;
        st.next_client += 1;
        let Some(s) = st.sessions.get_mut(&id) else {
            let _ = tx.try_send(Out::Msg(Msg::Error {
                message: format!("não existe a sessão {id}"),
            }));
            drop(st);
            drop(tx);
            let _ = writer.join();
            return Ok(());
        };
        if let SessionState::Parada { motivo } = &s.state {
            let _ = tx.try_send(Out::Msg(Msg::Error {
                message: format!(
                    "a sessão {id} está parada: {motivo}. Rode `sandmux restart {id}`"
                ),
            }));
            drop(st);
            drop(tx);
            let _ = writer.join();
            return Ok(());
        }
        let _ = tx.try_send(Out::Msg(Msg::Attached {
            id,
            name: s.name.clone(),
        }));
        let master = s.live.as_ref().map(|l| l.master.clone());
        if master.is_some() {
            s.clients.push(Client {
                id: client_id,
                size,
                tx: tx.clone(),
            });
            s.recompute_size();
        }
        let _ = tx.try_send(Out::Data(s.replay()));
        let _ = tx.try_send(Out::Msg(Msg::Replayed));
        if master.is_none()
            && let SessionState::Encerrada { codigo } = s.state
        {
            let _ = tx.try_send(Out::Msg(Msg::Exited { id, code: codigo }));
        }
        (client_id, master)
    };
    drop(tx);
    let Some(master) = master else {
        let _ = writer.join();
        return Ok(());
    };

    loop {
        match read_frame(&mut stream) {
            Ok(Some(Frame::Data(d))) => {
                let _ = (&*master).write_all(&d);
            }
            Ok(Some(Frame::Control(Msg::Resize { size }))) => {
                let mut st = shared.lock();
                if let Some(s) = st.sessions.get_mut(&id) {
                    if let Some(c) = s.clients.iter_mut().find(|c| c.id == client_id) {
                        c.size = size;
                    }
                    s.recompute_size();
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    {
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.clients.retain(|c| c.id != client_id);
            s.recompute_size();
        }
    }
    let _ = writer.join();
    Ok(())
}
