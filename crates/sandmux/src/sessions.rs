//! Comandos de sessão e do daemon no cliente.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sandmux::client::{self, Conn, systemd};
use sandmux::paths;
use sandmux_proto::{LaunchOptions, Msg, SessionInfo};

fn need_terminal(what: &str) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!("{what} precisa de um terminal interativo");
    }
    Ok(())
}

/// `run` com terminal: cria a sessão no daemon e anexa.
pub fn run_session(opts: LaunchOptions) -> Result<i32> {
    let vars = client::client_vars(&opts.pass_env)?;
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Create {
        opts,
        vars,
        size: client::terminal_size(),
        attach: true,
    })?;
    client::attached(conn)
}

fn list(conn: &mut Conn) -> Result<Vec<SessionInfo>> {
    match conn.call(&Msg::List)? {
        Msg::Sessions { sessions } => Ok(sessions),
        m => Err(anyhow!("resposta inesperada do daemon: {m:?}")),
    }
}

pub fn ls() -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let sessions = list(&mut conn)?;
    if sessions.is_empty() {
        println!("Não há sessões.");
        return Ok(0);
    }
    let rows: Vec<[String; 5]> = sessions
        .iter()
        .map(|s| {
            [
                s.id.to_string(),
                s.name.clone(),
                s.state.to_string(),
                s.clients.to_string(),
                s.workdir.display().to_string(),
            ]
        })
        .collect();
    let head = ["ID", "NOME", "ESTADO", "CLIENTES", "PASTA"];
    let mut w = head.map(|h| h.chars().count());
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |r: [&str; 5]| {
        let mut s = String::new();
        for (i, c) in r.iter().enumerate() {
            if i == 4 {
                s.push_str(c);
            } else {
                s.push_str(&format!("{c:<width$}  ", width = w[i]));
            }
        }
        println!("{}", s.trim_end());
    };
    line(head);
    for r in &rows {
        line([&r[0], &r[1], &r[2], &r[3], &r[4]]);
    }
    Ok(0)
}

pub fn attach(id: Option<u32>) -> Result<i32> {
    need_terminal("`sandmux attach`")?;
    let mut conn = client::connect_or_start()?;
    conn.send(&Msg::Attach {
        id,
        size: client::terminal_size(),
    })?;
    // A primeira resposta é o erro ou o começo do anexo.
    let mut peek = conn.stream.try_clone()?;
    match sandmux_proto::read_frame(&mut peek)? {
        Some(sandmux_proto::Frame::Control(Msg::Error { message })) => Err(anyhow!(message)),
        Some(sandmux_proto::Frame::Control(Msg::Attached { id, name })) => {
            // Reinsere o evento já lido para o laço do modo anexado.
            client::attached_after(conn, id, name)
        }
        other => Err(anyhow!("resposta inesperada do daemon: {other:?}")),
    }
}

pub fn detach(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Detach { id })?;
    eprintln!("sandmux: sessão {id} desanexada de todos os terminais");
    Ok(0)
}

pub fn kill(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Kill { id })?;
    eprintln!("sandmux: sessão {id} encerrada");
    Ok(0)
}

pub fn restart(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let sessions = list(&mut conn)?;
    let Some(s) = sessions.iter().find(|s| s.id == id) else {
        bail!("não existe a sessão {id}");
    };
    let vars = client::client_vars(&s.pass_env)?;
    let attach = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let reply = conn.call(&Msg::Restart {
        id,
        vars,
        size: client::terminal_size(),
        attach,
    })?;
    if attach {
        return client::attached(conn);
    }
    if let Msg::Created { id, name } = reply {
        eprintln!("sandmux: sessão {id} ({name}) relançada");
    }
    Ok(0)
}

pub fn daemon_status() -> Result<i32> {
    let Some(mut conn) = Conn::connect()? else {
        println!(
            "O daemon não está rodando (socket: {}).",
            paths::socket_path().display()
        );
        return Ok(1);
    };
    match conn.call(&Msg::Status)? {
        Msg::StatusInfo {
            pid,
            protocol,
            version,
            start_mode,
            sessions,
            socket,
        } => {
            println!("O daemon está rodando.");
            println!("  PID: {pid}");
            println!("  versão: {version} (protocolo {protocol})");
            println!("  iniciado por: {start_mode}");
            println!("  sessões: {sessions}");
            println!("  socket: {}", socket.display());
            Ok(0)
        }
        m => Err(anyhow!("resposta inesperada do daemon: {m:?}")),
    }
}

/// O processo existe e não é um zumbi?
fn process_alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => false,
        Ok(s) => !s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')),
    }
}

/// Para o daemon e espera o processo terminar. Devolve se havia daemon.
fn stop_daemon() -> Result<bool> {
    let Some(mut conn) = Conn::connect()? else {
        return Ok(false);
    };
    let pid = conn.info.pid;
    conn.call(&Msg::Stop)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while paths::socket_path().exists() || process_alive(pid) {
        if Instant::now() >= deadline {
            bail!("o daemon (PID {pid}) não parou");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(true)
}

pub fn daemon_stop() -> Result<i32> {
    if stop_daemon()? {
        eprintln!("sandmux: daemon parado; as sessões vivas voltam na próxima subida");
    } else {
        eprintln!("sandmux: o daemon não está rodando");
    }
    Ok(0)
}

pub fn daemon_restart() -> Result<i32> {
    stop_daemon()?;
    client::start_daemon()?;
    let conn = client::wait_for_daemon(Duration::from_secs(5))?;
    eprintln!(
        "sandmux: daemon reiniciado (PID {}, versão {})",
        conn.info.pid, conn.info.version
    );
    Ok(0)
}

pub fn daemon_disable() -> Result<i32> {
    stop_daemon()?;
    let call =
        std::env::var("SANDMUX_DAEMON_START").as_deref() != Ok("direct") && systemd::available();
    if systemd::disable(call)? {
        eprintln!(
            "sandmux: daemon parado e unidade {} removida",
            systemd::unit_path().display()
        );
    } else {
        eprintln!("sandmux: daemon parado; não havia unidade systemd instalada");
    }
    Ok(0)
}
