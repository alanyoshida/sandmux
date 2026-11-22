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
use sandmux_spec::{tr, trf};

use super::session::{self, CLIENT_QUEUE, Client, Out};
use super::{Shared, State};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn handle(shared: Arc<Shared>, stream: UnixStream) {
    let _ = serve(&shared, stream);
}

fn list_text(st: &State) -> String {
    if st.sessions.is_empty() {
        return tr!(" Não há sessões.", " No sessions.").into();
    }
    let mut s = String::from(tr!(" Sessões:", " Sessions:"));
    for x in st.sessions.values() {
        s.push_str(&format!("\n  {}  {}  {}", x.id, x.name, x.state));
    }
    s
}

fn resolve(st: &State, id: Option<u32>) -> Result<u32, String> {
    match id {
        Some(i) if st.sessions.contains_key(&i) => Ok(i),
        Some(i) => Err(trf!(
            "não existe a sessão {i}.{}",
            "session {i} does not exist.{}",
            list_text(st)
        )),
        None => {
            let live: Vec<u32> = st
                .sessions
                .values()
                .filter(|s| s.state == SessionState::Viva)
                .map(|s| s.id)
                .collect();
            match live.as_slice() {
                [only] => Ok(*only),
                [] => Err(trf!(
                    "não há sessão viva.{}",
                    "no session is alive.{}",
                    list_text(st)
                )),
                many => Err(trf!(
                    "há {} sessões vivas; escolha uma com `sandmux attach <id>`.{}",
                    "{} sessions are alive; pick one with `sandmux attach <id>`.{}",
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
    // O idioma pode ter mudado na configuração (página da TUI).
    crate::init_language();
    let Some(Frame::Control(hello)) = read_frame(&mut stream)? else {
        return Ok(());
    };
    // Responde no idioma do cliente, quando ele diz qual é.
    if let Msg::Hello { lang, .. } = &hello
        && let Some(l) = sandmux_spec::lang::Lang::parse(lang)
    {
        sandmux_spec::lang::set(l);
    }
    let (reply, ok) = answer_hello(
        &hello,
        shared.protocol,
        VERSION,
        shared.pid,
        &shared.start_mode,
        &shared.base.data_dir,
        &shared.base.config_dir,
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
                let (sessions, layouts) = {
                    let st = shared.lock();
                    (shared.list(&st), st.layouts.clone())
                };
                write_control(&mut stream, &Msg::Sessions { sessions, layouts })?;
            }
            Msg::SetLayout { pane } => {
                let r = shared.set_layout(pane);
                reply_unit(&mut stream, r)?;
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
            Msg::Stats { id } => {
                let st = shared.lock();
                let found = st.sessions.get(&id).map(|s| {
                    (
                        s.stats(),
                        s.opts.workdir.clone(),
                        s.live.as_ref().and(s.tags.last().cloned()),
                    )
                });
                let text = list_text(&st);
                drop(st);
                let reply = match found {
                    Some((
                        Msg::StatsOf {
                            id,
                            state,
                            stats,
                            oom_kills,
                            fork_refusals,
                            policy,
                            ..
                        },
                        w,
                        tag,
                    )) => {
                        let mut ports: Vec<sandmux_proto::msg::PortInfo> = tag
                            .and_then(|t| crate::launch::ports_file(&t).ok())
                            .map(|f| sandmux_net::ports::read_list(&f))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|p| sandmux_proto::msg::PortInfo {
                                guest: p.guest,
                                host: p.host,
                                source: "sandbox".into(),
                                addr: p.addr,
                            })
                            .collect();
                        ports.extend(shared.docker.ports(&w));
                        Msg::StatsOf {
                            id,
                            state,
                            stats,
                            oom_kills,
                            fork_refusals,
                            docker: shared.docker.stats(&w).map(Box::new),
                            policy,
                            ports,
                        }
                    }
                    Some((m, _, _)) => m,
                    None => Msg::Error {
                        message: trf!(
                            "não existe a sessão {id}.{text}",
                            "session {id} does not exist.{text}"
                        ),
                    },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::ProcessTree { id } => {
                let st = shared.lock();
                let found = st.sessions.get(&id).map(|s| {
                    if s.exec_obs.is_some() {
                        Ok(s.proc_tree.clone())
                    } else if s.live.is_none() {
                        Err(tr!("a sessão não está viva", "the session is not alive").to_string())
                    } else {
                        Err(s
                            .policy
                            .as_ref()
                            .and_then(|p| p.exec.clone())
                            .unwrap_or_else(|| {
                                tr!(
                                    "a observação pelo kernel ainda não começou",
                                    "the kernel observation has not started yet"
                                )
                                .to_string()
                            }))
                    }
                });
                let text = list_text(&st);
                drop(st);
                let reply = match found {
                    Some(Ok(tree)) => {
                        let (nodes, dead) =
                            super::proctree::render(tree, std::path::Path::new("/proc"));
                        if !dead.is_empty() {
                            let mut st = shared.lock();
                            if let Some(s) = st.sessions.get_mut(&id) {
                                for pid in dead {
                                    s.proc_tree.exit(pid);
                                }
                            }
                        }
                        Msg::ProcessTreeOf {
                            id,
                            nodes,
                            unavailable: None,
                        }
                    }
                    Some(Err(why)) => Msg::ProcessTreeOf {
                        id,
                        nodes: Vec::new(),
                        unavailable: Some(why),
                    },
                    None => Msg::Error {
                        message: trf!(
                            "não existe a sessão {id}.{text}",
                            "session {id} does not exist.{text}"
                        ),
                    },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::SetGroup { id, group } => {
                let r = sandmux_proto::msg::check_group(&group).and_then(|()| {
                    let mut st = shared.lock();
                    match st.sessions.get_mut(&id) {
                        Some(s) => {
                            s.group = group;
                            shared.persist(&mut st);
                            Ok(())
                        }
                        None => Err(trf!(
                            "não existe a sessão {id}",
                            "session {id} does not exist"
                        )),
                    }
                });
                reply_unit(&mut stream, r)?;
            }
            Msg::RenameGroup { from, to } => {
                let r = sandmux_proto::msg::check_group(&to).and_then(|()| {
                    let mut st = shared.lock();
                    let mut n = 0;
                    for s in st.sessions.values_mut().filter(|s| s.group == from) {
                        s.group = to.clone();
                        n += 1;
                    }
                    if n == 0 {
                        return Err(trf!(
                            "não existe o grupo {from:?}",
                            "group {from:?} does not exist"
                        ));
                    }
                    shared.persist(&mut st);
                    Ok(())
                });
                reply_unit(&mut stream, r)?;
            }
            Msg::Rename { id, name } => {
                let reply = match sandmux_proto::msg::check_session_name(&name) {
                    Err(e) => Msg::Error { message: e },
                    Ok(()) => {
                        let mut st = shared.lock();
                        let text = list_text(&st);
                        match st.sessions.get_mut(&id) {
                            Some(s) => {
                                s.name = name.trim().to_string();
                                shared.persist(&mut st);
                                Msg::Ok
                            }
                            None => Msg::Error {
                                message: trf!(
                                    "não existe a sessão {id}.{text}",
                                    "session {id} does not exist.{text}"
                                ),
                            },
                        }
                    }
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::VaultStatus => {
                super::keyring::try_unlock(shared, false);
                let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
                let reply = Msg::VaultInfo {
                    exists: v.exists(),
                    unlocked: v.unlocked(),
                    secrets: v.count(),
                };
                drop(v);
                write_control(&mut stream, &reply)?;
            }
            Msg::VaultInit { password } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .init(&password);
                if r.is_ok() {
                    shared.secrets.kick();
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultUnlock { password } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .unlock(&password);
                if r.is_ok() {
                    super::keyring::after_unlock(shared);
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultLock => {
                shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .lock();
                shared.cloud.clear();
                let s = shared.clone();
                std::thread::spawn(move || super::net::recompile_all(&s));
                write_control(&mut stream, &Msg::Ok)?;
            }
            Msg::VaultPasswd { old, new } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .passwd(&old, &new);
                let r = r.and_then(|()| super::keyring::after_passwd(shared, &new));
                reply_unit(&mut stream, r)?;
            }
            Msg::Focus { focused, active } => {
                if let Some(pid) = super::peer_pid(&stream) {
                    shared.notify.set_focus(pid, focused, active);
                }
                write_control(&mut stream, &Msg::Ok)?;
            }
            Msg::Cwd { id } => {
                let master = shared
                    .lock()
                    .sessions
                    .get(&id)
                    .and_then(|s| s.live.as_ref().map(|l| l.master.clone()));
                let path = master.and_then(|m| super::session::foreground_cwd(&m));
                write_control(&mut stream, &Msg::CwdOf { path })?;
            }
            Msg::KeyringStatus => {
                write_control(&mut stream, &super::keyring::status(shared))?;
            }
            Msg::KeyringOn { password } => {
                let r = super::keyring::on(shared, &password);
                reply_unit(&mut stream, r)?;
            }
            Msg::KeyringOff => {
                let r = crate::keyring::clear(&shared.base.data_dir);
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultAdd {
                name,
                value,
                domains,
                fake,
                tools,
            } => {
                let r = shared.vault.lock().unwrap_or_else(|e| e.into_inner()).add(
                    sandmux_vault::Secret {
                        name,
                        value,
                        domains,
                        fake,
                        tools,
                        kind: String::new(),
                    },
                );
                if r.is_ok() {
                    let s = shared.clone();
                    std::thread::spawn(move || super::net::recompile_all(&s));
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultRemove { name } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&name);
                if r.is_ok() {
                    shared.cloud.clear();
                    let s = shared.clone();
                    std::thread::spawn(move || super::net::recompile_all(&s));
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultSetValue { name, mut value } => {
                // Os domínios não mudam: a política de rede fica como está.
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .set_value(&name, std::mem::take(&mut value));
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultSetDomains { name, domains } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .set_domains(&name, domains);
                if r.is_ok() {
                    let s = shared.clone();
                    std::thread::spawn(move || super::net::recompile_all(&s));
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultAddCloud { cloud } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .add_cloud(cloud);
                if r.is_ok() {
                    let s = shared.clone();
                    std::thread::spawn(move || super::net::recompile_all(&s));
                }
                reply_unit(&mut stream, r.map(|_| ()))?;
            }
            Msg::VaultAddSsh { name, key, servers } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .add_ssh(name, key, servers);
                if r.is_ok() {
                    let s = shared.clone();
                    std::thread::spawn(move || super::net::recompile_all(&s));
                }
                reply_unit(&mut stream, r)?;
            }
            Msg::VaultCapture {
                tag,
                harness,
                items,
            } => {
                let _ = tag;
                let r: Result<Vec<String>, String> = {
                    let mut v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
                    items
                        .iter()
                        .map(|(role, real)| v.capture(&harness, role, real))
                        .collect()
                };
                match r {
                    Ok(fakes) => {
                        let s = shared.clone();
                        std::thread::spawn(move || super::net::recompile_all(&s));
                        write_control(&mut stream, &Msg::Captured { fakes })?;
                    }
                    Err(e) => error(&mut stream, e)?,
                }
            }
            Msg::ConfineTab {
                tag,
                mounts,
                roots,
                exclude,
                scope,
            } => {
                // O user namespace do sandbox vem logo depois do quadro.
                let scope = match scope {
                    Some(sc) => match super::recv_userns(&stream) {
                        Ok(ns) => Some((sc, ns)),
                        Err(e) => {
                            eprintln!(
                                "{}",
                                trf!(
                                    "sandmuxd: o escopo da aba {tag} não veio: {e}",
                                    "sandmuxd: the scope of tab {tag} did not arrive: {e}"
                                )
                            );
                            None
                        }
                    },
                    None => None,
                };
                let r = match super::peer_pid(&stream) {
                    Some(pid) => super::tabfiles::confine(
                        shared,
                        super::tabfiles::Request {
                            tag: tag.clone(),
                            pid: pid as u32,
                            mounts,
                            view: (roots, exclude),
                            scope,
                        },
                    ),
                    None => {
                        Err(tr!("sem as credenciais do lançador", "no launcher credentials").into())
                    }
                };
                let reply = match r {
                    Ok(c) => {
                        let (kernel, reason) = match &c.files {
                            Ok(()) => (true, None),
                            Err(e) => (false, Some(e.clone())),
                        };
                        if let Some(s) = shared.lock().sessions.get_mut(&c.id) {
                            s.tab_kernel = c.tab;
                            if let Some(p) = s.policy.as_mut() {
                                p.files = Some(match &reason {
                                    None => "kernel".into(),
                                    Some(e) => format!("supervisor: {e}"),
                                });
                            }
                        }
                        Msg::ConfineTabReply {
                            kernel,
                            reason,
                            scope: c.scope,
                        }
                    }
                    Err(e) => {
                        super::tabfiles::fallback(shared, &tag, &e);
                        Msg::ConfineTabReply {
                            kernel: false,
                            reason: Some(e),
                            scope: false,
                        }
                    }
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::ConnProcess {
                tag,
                sport,
                ip,
                port,
            } => {
                let (found, kernel) = super::execlog::conn_process(shared, &tag, sport, &ip, port);
                write_control(
                    &mut stream,
                    &Msg::ConnProcessReply {
                        pid: found.as_ref().map(|f| f.0),
                        exe: found.map(|f| f.1),
                        kernel,
                    },
                )?;
            }
            Msg::VaultInject { tag, host } => {
                let _ = tag;
                let mut secrets = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .injections();
                // As identidades de cloud do host, com o real atual (fora da
                // trava do cofre: o real pode vir da CLI do host).
                secrets.extend(super::cloud::injections(shared, &host));
                write_control(&mut stream, &Msg::Injections { secrets })?;
            }
            Msg::FolderEdit {
                id,
                action,
                value,
                rw,
            } => match super::folders::edit(shared, id, &action, &value, rw) {
                Ok(warning) => {
                    if let Some(message) = warning {
                        write_control(&mut stream, &Msg::Warning { message })?;
                    }
                    write_control(&mut stream, &Msg::Ok)?;
                }
                Err(message) => write_control(&mut stream, &Msg::Error { message })?,
            },
            Msg::DbAdd { conn } => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .add_db(conn);
                reply_unit(&mut stream, r)?;
            }
            Msg::DbList => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .db_list();
                let reply = match r {
                    Ok(list) => Msg::DbEntries { list },
                    Err(message) => Msg::Error { message },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::McpList { id } => {
                let opts = shared.lock().sessions.get(&id).map(|s| s.opts.clone());
                let reply = match opts {
                    None => Msg::Error {
                        message: trf!("não existe a sessão {id}", "session {id} does not exist"),
                    },
                    Some(o) => {
                        let mut host = shared.base.clone();
                        host.cwd = o.workdir.clone();
                        match crate::launch::mcp_entries(&host, &o) {
                            Ok(list) => Msg::Mcps { list },
                            Err(e) => Msg::Error {
                                message: format!("{e:#}"),
                            },
                        }
                    }
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::FolderList { id } => {
                let reply = match super::folders::list(shared, id) {
                    Ok(list) => Msg::Folders { list },
                    Err(message) => Msg::Error { message },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::VaultList => {
                let r = shared
                    .vault
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .list();
                let reply = match r {
                    Ok(entries) => Msg::VaultEntries { entries },
                    Err(message) => Msg::Error { message },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::NetEdit { id, action, value } => {
                let reply = match super::net::edit(shared, id, &action, &value) {
                    Ok(warning) => {
                        if let Some(message) = warning {
                            write_control(&mut stream, &Msg::Warning { message })?;
                        }
                        Msg::Ok
                    }
                    Err(message) => Msg::Error { message },
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::Net { id, since, limit } => {
                let reply = match super::net::lines(shared, id, since.as_deref(), limit) {
                    Ok(lines) => Msg::NetLog { lines },
                    Err(message) => Msg::Error { message },
                };
                write_control(&mut stream, &reply)?;
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
                            reason: trf!(
                                "desanexado por `sandmux detach {id}`; a sessão continua viva",
                                "detached by `sandmux detach {id}`; the session is still alive"
                            ),
                        });
                        drop(st);
                        write_control(&mut stream, &Msg::Ok)?;
                    }
                    None => {
                        let text = list_text(&st);
                        drop(st);
                        error(
                            &mut stream,
                            trf!(
                                "não existe a sessão {id}.{text}",
                                "session {id} does not exist.{text}"
                            ),
                        )?;
                    }
                }
            }
            Msg::ListModels => match session::list_models(shared) {
                Ok((ids, notes)) => write_control(&mut stream, &Msg::Models { ids, notes })?,
                Err(e) => error(&mut stream, format!("{e:#}"))?,
            },
            Msg::ChatCheck { host } => {
                let reply = {
                    let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
                    Msg::ChatStatus {
                        exists: v.exists(),
                        unlocked: v.unlocked(),
                        key: v.fake_for(&host).is_some(),
                    }
                };
                write_control(&mut stream, &reply)?;
            }
            Msg::Ask {
                model,
                text,
                vars,
                system,
            } => match session::ask(shared, &model, &text, system.as_deref(), &vars) {
                Ok(text) => write_control(&mut stream, &Msg::Answer { text })?,
                Err(e) => error(&mut stream, format!("{e:#}"))?,
            },
            Msg::Create {
                opts,
                vars,
                size,
                attach,
                group,
            } => {
                let group = group.unwrap_or_else(sandmux_proto::msg::main_group);
                if let Err(e) = sandmux_proto::msg::check_group(&group) {
                    error(&mut stream, e)?;
                    continue;
                }
                if opts.gateway
                    && let Some(g) = session::gateway_session(shared)
                {
                    error(
                        &mut stream,
                        trf!(
                            "já existe a sessão {g} do gateway do hermes; encerre-a com `sandmux kill {g}` antes",
                            "session {g} of the hermes gateway already exists; end it with `sandmux kill {g}` first"
                        ),
                    )?;
                    continue;
                }
                // Shell de dentro: o daemon escolhe a aba alvo e a pasta.
                let mut opts = opts;
                if opts.harness == crate::launch::ENTER {
                    match session::enter_target(shared, opts.args.first().map(String::as_str)) {
                        Ok((target, workdir)) => {
                            opts.args = vec![target.to_string()];
                            opts.workdir = workdir;
                        }
                        Err(e) => {
                            error(&mut stream, format!("{e:#}"))?;
                            continue;
                        }
                    }
                }
                let id = session::reserve(shared);
                match session::spawn(shared, id, 1, &opts, &vars, size) {
                    Err(e) => error(&mut stream, format!("{e:#}"))?,
                    Ok(spawned) => {
                        for w in &spawned.warnings {
                            write_control(&mut stream, &Msg::Warning { message: w.clone() })?;
                        }
                        let names = spawned.pass_env.clone();
                        let name = session::register(shared, id, *opts, spawned, size, group);
                        if let Some(w) = session::store_pass_env(shared, id, &names, &vars) {
                            write_control(&mut stream, &Msg::Warning { message: w })?;
                        }
                        write_control(&mut stream, &Msg::Created { id, name })?;
                        if attach {
                            return attached(shared, stream, id, size);
                        }
                    }
                }
            }
            Msg::Restart {
                id,
                vars,
                size,
                attach,
                force,
            } => match session::relaunch(shared, id, &vars, Some(size), force) {
                Err(e) => error(&mut stream, format!("{e:#}"))?,
                Ok(warnings) => {
                    for w in warnings {
                        write_control(&mut stream, &Msg::Warning { message: w })?;
                    }
                    let names = shared
                        .lock()
                        .sessions
                        .get(&id)
                        .map(|s| s.pass_env.clone())
                        .unwrap_or_default();
                    if let Some(w) = session::store_pass_env(shared, id, &names, &vars) {
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
            other => error(
                &mut stream,
                trf!(
                    "pedido inesperado: {other:?}",
                    "unexpected request: {other:?}"
                ),
            )?,
        }
    }
}

/// Encerra a sessão: SIGTERM, espera, SIGKILL. Volta quando ela saiu da lista.
fn kill(shared: &Arc<Shared>, id: u32) -> Result<(), String> {
    // Os valores de `pass-env` da sessão saem do cofre.
    shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .forget_session(id);
    let pid = {
        let mut st = shared.lock();
        let Some(s) = st.sessions.get_mut(&id) else {
            return Err(trf!(
                "não existe a sessão {id}.{}",
                "session {id} does not exist.{}",
                list_text(&st)
            ));
        };
        match s.pid() {
            Some(pid) => {
                s.killing = true;
                pid
            }
            None => {
                s.drop_clients(Msg::Detached {
                    reason: trf!(
                        "a sessão {id} foi encerrada por `sandmux kill`",
                        "session {id} was ended by `sandmux kill`"
                    ),
                });
                if let Some(s) = st.sessions.remove(&id) {
                    session::remove_chat_dir(&s.opts);
                }
                shared.persist(&mut st);
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
            return Err(trf!(
                "a sessão {id} não terminou",
                "session {id} did not end"
            ));
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
                message: trf!("não existe a sessão {id}", "session {id} does not exist"),
            }));
            drop(st);
            drop(tx);
            let _ = writer.join();
            return Ok(());
        };
        if let SessionState::Parada { motivo } = &s.state {
            let _ = tx.try_send(Out::Msg(Msg::Error {
                message: trf!(
                    "a sessão {id} está parada: {motivo}. Rode `sandmux restart {id}`",
                    "session {id} is stopped: {motivo}. Run `sandmux restart {id}`"
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

/// `Ok` ou `Error` com a mensagem.
fn reply_unit(stream: &mut UnixStream, r: Result<(), String>) -> std::io::Result<()> {
    let m = match r {
        Ok(()) => Msg::Ok,
        Err(message) => Msg::Error { message },
    };
    write_control(stream, &m).map_err(std::io::Error::other)
}
