//! `sandmuxd`: o daemon de sessões de um usuário.
//!
//! Threads: a principal aceita conexões; uma thread por conexão lê os pedidos
//! (e, anexada, uma segunda escreve para o cliente a partir de uma fila
//! limitada); uma por sessão lê o PTY e, quando ele fecha, colhe o código de
//! saída; uma thread fixa lança os sandboxes (o `PR_SET_PDEATHSIG` é ligado
//! à thread que cria o filho); uma espera os sinais de término. O estado
//! fica sob um único mutex, nunca segurado durante I/O bloqueante.

mod conn;
mod session;
pub mod store;

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use nix::sys::signal::{SigSet, Signal};
use sandmux_policy::Host;
use sandmux_proto::{PROTOCOL, SessionInfo, SessionState};

use crate::paths;
pub use session::{Out, Session};

/// Estado compartilhado do daemon.
pub struct Shared {
    pub state: Mutex<State>,
    /// Sinalizado a cada mudança de sessão (fim, remoção).
    pub changed: Condvar,
    launcher: Mutex<mpsc::Sender<session::LaunchJob>>,
    pub base: Host,
    pub socket: PathBuf,
    pub start_mode: String,
    pub protocol: u32,
    pub history: usize,
    pub pid: u32,
}

pub struct State {
    pub sessions: BTreeMap<u32, Session>,
    pub next_id: u32,
    pub next_client: u64,
    pub stopping: bool,
}

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn sessions_file(&self) -> PathBuf {
        paths::sessions_file(&self.base.data_dir)
    }

    /// Grava a lista de sessões. Erros vão para o log; a sessão segue.
    pub fn persist(&self, st: &State) {
        let saved = store::Saved {
            next_id: st.next_id,
            sessions: st
                .sessions
                .values()
                .map(|s| store::SavedSession {
                    id: s.id,
                    name: s.name.clone(),
                    opts: s.opts.clone(),
                    state: s.state.clone(),
                })
                .collect(),
        };
        if let Err(e) = store::save(&self.sessions_file(), &saved) {
            eprintln!("sandmuxd: aviso: gravar a lista de sessões: {e:#}");
        }
    }

    pub fn list(&self, st: &State) -> Vec<SessionInfo> {
        st.sessions.values().map(Session::info).collect()
    }

    /// Para o daemon: encerra os harnesses mantendo as sessões vivas
    /// marcadas como vivas, apaga o socket e termina o processo.
    pub fn stop(&self) -> ! {
        let pids: Vec<i32> = {
            let mut st = self.lock();
            st.stopping = true;
            self.persist(&st);
            st.sessions.values().filter_map(|s| s.pid()).collect()
        };
        eprintln!("sandmuxd: parando, {} sessões vivas", pids.len());
        for pid in &pids {
            session::terminate(*pid);
        }
        if !self.wait_until(Duration::from_secs(5), |st| {
            st.sessions.values().all(|s| s.pid().is_none())
        }) {
            let left: Vec<i32> = self
                .lock()
                .sessions
                .values()
                .filter_map(|s| s.pid())
                .collect();
            for pid in left {
                session::signal(pid, libc::SIGKILL);
            }
            self.wait_until(Duration::from_secs(5), |st| {
                st.sessions.values().all(|s| s.pid().is_none())
            });
        }
        self.persist(&self.lock());
        let _ = std::fs::remove_file(&self.socket);
        // Deixa as filas dos clientes escoarem o aviso de parada.
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(0);
    }

    /// Espera até `cond` valer ou o prazo acabar. Devolve se valeu.
    pub fn wait_until(&self, timeout: Duration, cond: impl Fn(&State) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        let mut st = self.lock();
        loop {
            if cond(&st) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            st = self
                .changed
                .wait_timeout(st, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Ambiente usado quando não há cliente (restauração).
    fn default_vars(&self) -> BTreeMap<String, String> {
        let mut v = BTreeMap::new();
        v.insert("TERM".into(), "xterm-256color".into());
        for k in ["LANG", "TZ", "COLORTERM"] {
            if let Some(x) = self.base.env.get(k) {
                v.insert(k.into(), x.clone());
            }
        }
        v.insert(
            "SHELL".into(),
            self.base.user_shell.to_string_lossy().into_owned(),
        );
        v
    }
}

/// Decide se aceita uma conexão a partir do UID do outro lado.
pub fn accept_peer(peer: Option<u32>, me: u32) -> bool {
    peer == Some(me)
}

fn peer_uid(s: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: buffer do tamanho certo para SO_PEERCRED.
    let r = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    (r == 0).then_some(cred.uid)
}

/// Toma a trava de instância única. `None` quando outro daemon a tem.
fn take_instance_lock(path: &std::path::Path) -> Result<Option<File>> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("abrir {}", path.display()))?;
    // SAFETY: flock sobre um descritor válido.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Ok(None);
    }
    f.set_len(0)?;
    writeln!(f, "{}", std::process::id())?;
    Ok(Some(f))
}

fn usage() -> ! {
    eprintln!("uso: sandmuxd [--history-lines N]");
    std::process::exit(2);
}

/// Ponto de entrada do binário `sandmuxd`.
pub fn main() -> ExitCode {
    let mut history = 10_000usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--history-lines" => {
                history = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            _ => usage(),
        }
    }
    match run(history) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sandmuxd: erro: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(history: usize) -> Result<()> {
    // Os sinais de término são tratados por uma thread própria; o bloqueio é
    // herdado pelas threads criadas depois e desfeito nos filhos.
    let mut term = SigSet::empty();
    for s in [Signal::SIGTERM, Signal::SIGINT, Signal::SIGHUP] {
        term.add(s);
    }
    term.thread_block()?;
    std::env::set_current_dir("/")?;

    let base = Host::detect()?;
    let dir = paths::runtime_dir();
    paths::ensure_runtime_dir(&dir)?;
    let Some(instance) = take_instance_lock(&paths::lock_path())? else {
        let pid = std::fs::read_to_string(paths::lock_path()).unwrap_or_default();
        bail!(
            "o daemon já está rodando (PID {}); use `sandmux daemon status`",
            pid.trim()
        );
    };
    let socket = paths::socket_path();
    match std::fs::remove_file(&socket) {
        Ok(()) => eprintln!("sandmuxd: removido o socket de um daemon anterior"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context("remover o socket antigo"),
    }
    let listener =
        UnixListener::bind(&socket).with_context(|| format!("criar {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

    let protocol = std::env::var("SANDMUX_PROTOCOL_OVERRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PROTOCOL);
    let start_mode = std::env::var("SANDMUX_START_MODE").unwrap_or_else(|_| "manual".into());
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            sessions: BTreeMap::new(),
            next_id: 1,
            next_client: 1,
            stopping: false,
        }),
        changed: Condvar::new(),
        launcher: Mutex::new(session::start_launcher()),
        base,
        socket: socket.clone(),
        start_mode,
        protocol,
        history,
        pid: std::process::id(),
    });
    eprintln!(
        "sandmuxd: ouvindo em {} (PID {}, protocolo {protocol})",
        socket.display(),
        shared.pid
    );

    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            if let Ok(sig) = term.wait() {
                eprintln!("sandmuxd: recebido {sig:?}");
            }
            shared.stop();
        });
    }

    restore(&shared);

    // SAFETY: getuid não falha.
    let me = unsafe { libc::getuid() };
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !accept_peer(peer_uid(&stream), me) {
            eprintln!("sandmuxd: conexão de outro usuário recusada");
            continue;
        }
        let shared = shared.clone();
        std::thread::spawn(move || conn::handle(shared, stream));
    }
    drop(instance);
    Ok(())
}

/// Lê a lista guardada e relança as sessões que estavam vivas.
fn restore(shared: &Arc<Shared>) {
    let saved = match store::load(&shared.sessions_file()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sandmuxd: aviso: {e:#}; começando sem sessões");
            store::Saved::default()
        }
    };
    let mut relaunch = Vec::new();
    {
        let mut st = shared.lock();
        st.next_id = saved.next_id.max(1);
        for s in saved.sessions {
            let mut state = s.state.clone();
            if state == SessionState::Viva {
                if !s.opts.pass_env.is_empty() {
                    state = SessionState::Parada {
                        motivo: format!(
                            "precisa de --pass-env {}; rode `sandmux restart {}` num terminal com as variáveis",
                            s.opts.pass_env.join(", "),
                            s.id
                        ),
                    };
                } else if !s.opts.workdir.is_dir() {
                    state = SessionState::Parada {
                        motivo: format!(
                            "a pasta de trabalho {} não existe mais",
                            s.opts.workdir.display()
                        ),
                    };
                } else {
                    relaunch.push(s.id);
                }
            }
            let sess = Session::new(s.id, s.name, s.opts, state, shared.history, None);
            st.sessions.insert(s.id, sess);
        }
        shared.persist(&st);
    }
    let vars = shared.default_vars();
    let mut by_id: HashMap<u32, String> = HashMap::new();
    for id in relaunch {
        if let Err(e) = session::relaunch(shared, id, &vars, None) {
            by_id.insert(id, format!("{e:#}"));
        }
    }
    if !by_id.is_empty() {
        let mut st = shared.lock();
        for (id, err) in by_id {
            eprintln!("sandmuxd: sessão {id} não foi restaurada: {err}");
            if let Some(s) = st.sessions.get_mut(&id) {
                s.state = SessionState::Parada {
                    motivo: format!("falhou ao restaurar: {err}"),
                };
            }
        }
        shared.persist(&st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_aceita_o_mesmo_uid() {
        assert!(accept_peer(Some(1000), 1000));
        assert!(!accept_peer(Some(1001), 1000));
        assert!(!accept_peer(Some(0), 1000));
        assert!(!accept_peer(None, 1000));
    }
}
