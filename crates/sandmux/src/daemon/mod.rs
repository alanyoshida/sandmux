//! `sandmuxd`: o daemon de sessões de um usuário.
//!
//! Threads: a principal aceita conexões; uma thread por conexão lê os pedidos
//! (e, anexada, uma segunda escreve para o cliente a partir de uma fila
//! limitada); uma por sessão lê o PTY e, quando ele fecha, colhe o código de
//! saída; uma thread fixa lança os sandboxes (o `PR_SET_PDEATHSIG` é ligado
//! à thread que cria o filho); uma espera os sinais de término. O estado
//! fica sob um único mutex, nunca segurado durante I/O bloqueante.

mod conn;
mod db;
mod docker;
mod dockerfiles;
mod folders;
mod history;
mod keyring;
mod net;
pub mod notify;
mod secretwatch;
mod session;
mod sshagent;
pub mod store;
pub mod vault;

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
use sandmux_proto::{LaunchOptions, PROTOCOL, SessionInfo, SessionState};
use sandmux_spec::{tr, trf};

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
    /// Docker privado de cada pasta.
    pub docker: docker::Engines,
    /// Cofre de segredos, trancado ao subir.
    pub vault: Mutex<vault::VaultState>,
    /// Vigia das chaves reais nos HOMEs sintéticos.
    pub secrets: secretwatch::Watch,
    /// Última tentativa de abrir o cofre pelo keyring.
    pub keyring_last: Mutex<Option<std::time::Instant>>,
    /// Última nota do keyring no registro.
    pub keyring_note: Mutex<Option<String>>,
    /// Notificações do desktop.
    pub notify: notify::Notifier,
    /// Histórico de tela em disco.
    pub disk_history: history::History,
}

pub struct State {
    pub sessions: BTreeMap<u32, Session>,
    /// Layouts de painéis das abas; cada um com ao menos duas sessões.
    pub layouts: Vec<sandmux_proto::pane::Pane>,
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
    pub fn persist(&self, st: &mut State) {
        prune_layouts(st);
        let saved = store::Saved {
            version: store::VERSION,
            next_id: st.next_id,
            layouts: st.layouts.clone(),
            sessions: st
                .sessions
                .values()
                .map(|s| store::SavedSession {
                    id: s.id,
                    name: s.name.clone(),
                    opts: s.opts.clone(),
                    pass_env: s.pass_env.clone(),
                    state: s.state.clone(),
                    launches: s.launches,
                    tags: s.tags.clone(),
                    vault_env: s.vault_env,
                    group: s.group.clone(),
                })
                .collect(),
        };
        if let Err(e) = store::save(&self.sessions_file(), &saved) {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: aviso: gravar a lista de sessões: {e:#}",
                    "sandmuxd: warning: saving the session list: {e:#}"
                )
            );
        }
    }

    /// Valida e grava o layout de uma aba: troca os que têm alguma folha
    /// dele.
    pub fn set_layout(&self, pane: sandmux_proto::pane::Pane) -> Result<(), String> {
        let mut st = self.lock();
        let leaves = pane.leaves();
        if leaves.len() > 1 {
            pane.check()?;
            if let Some(id) = leaves.iter().find(|i| !st.sessions.contains_key(i)) {
                return Err(trf!(
                    "não existe a sessão {id}",
                    "session {id} does not exist"
                ));
            }
        }
        st.layouts
            .retain(|l| !l.leaves().iter().any(|i| leaves.contains(i)));
        if leaves.len() > 1 {
            st.layouts.push(pane);
        }
        self.persist(&mut st);
        Ok(())
    }

    pub fn list(&self, st: &State) -> Vec<SessionInfo> {
        st.sessions
            .values()
            .map(|s| {
                let mut i = s.info();
                let w = self.secrets.waiting(&i.harness);
                i.keys.waiting_vault = w.vault;
                i.keys.no_domain = w.domain;
                i
            })
            .collect()
    }

    /// Para o daemon: encerra os harnesses mantendo as sessões vivas
    /// marcadas como vivas, apaga o socket e termina o processo.
    pub fn stop(&self) -> ! {
        let pids: Vec<i32> = {
            let mut st = self.lock();
            st.stopping = true;
            self.persist(&mut st);
            st.sessions.values().filter_map(|s| s.pid()).collect()
        };
        eprintln!(
            "{}",
            trf!(
                "sandmuxd: parando, {} sessões vivas",
                "sandmuxd: stopping, {} sessions alive",
                pids.len()
            )
        );
        for pid in &pids {
            session::terminate(*pid);
        }
        self.docker.stop_all();
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
        self.persist(&mut self.lock());
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
    pub fn default_vars(&self) -> BTreeMap<String, String> {
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

/// Tira dos layouts as sessões que não existem mais; um layout de uma folha
/// só deixa de existir.
fn prune_layouts(st: &mut State) {
    let ids: Vec<u32> = st.sessions.keys().copied().collect();
    let layouts = std::mem::take(&mut st.layouts);
    st.layouts = layouts
        .into_iter()
        .filter_map(|l| {
            let gone: Vec<u32> = l
                .leaves()
                .into_iter()
                .filter(|i| !ids.contains(i))
                .collect();
            gone.into_iter().try_fold(l, |l, id| l.remove(id))
        })
        .filter(|l| l.leaves().len() > 1)
        .collect();
}

/// PID do processo do outro lado do socket.
pub fn peer_pid(s: &UnixStream) -> Option<i32> {
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
    (r == 0 && cred.pid > 0).then_some(cred.pid)
}

/// Toma a trava de instância única. `None` quando outro daemon a tem.
fn take_instance_lock(path: &std::path::Path) -> Result<Option<File>> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| trf!("abrir {}", "open {}", path.display()))?;
    // Um daemon anterior que acabou de parar pode ainda segurar a trava por
    // um instante (as threads dele saindo): tenta por alguns segundos.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    // SAFETY: flock sobre um descritor válido.
    while unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if std::time::Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    f.set_len(0)?;
    writeln!(f, "{}", std::process::id())?;
    Ok(Some(f))
}

fn usage() -> ! {
    eprintln!(
        "{}",
        tr!(
            "uso: sandmuxd [--history-lines N]",
            "usage: sandmuxd [--history-lines N]"
        )
    );
    std::process::exit(2);
}

/// Ponto de entrada do binário `sandmuxd`.
pub fn main() -> ExitCode {
    crate::init_language();
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
            eprintln!(
                "{}",
                trf!("sandmuxd: erro: {e:#}", "sandmuxd: error: {e:#}")
            );
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
    let base_data = base.data_dir.clone();
    // Com o cofre aberto, os segredos ficam na memória do daemon: nem o
    // `/proc/<pid>/mem` nem um core dump os entregam.
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    let dir = paths::runtime_dir();
    paths::ensure_runtime_dir(&dir)?;
    let Some(instance) = take_instance_lock(&paths::lock_path())? else {
        let pid = std::fs::read_to_string(paths::lock_path()).unwrap_or_default();
        bail!(trf!(
            "o daemon já está rodando (PID {}); use `sandmux daemon status`",
            "the daemon is already running (PID {}); use `sandmux daemon status`",
            pid.trim()
        ));
    };
    let socket = paths::socket_path();
    match std::fs::remove_file(&socket) {
        Ok(()) => eprintln!(
            "{}",
            tr!(
                "sandmuxd: removido o socket de um daemon anterior",
                "sandmuxd: removed the socket of a previous daemon"
            )
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).context(tr!("remover o socket antigo", "remove the old socket")),
    }
    let listener = UnixListener::bind(&socket)
        .with_context(|| trf!("criar {}", "create {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

    let protocol = std::env::var("SANDMUX_PROTOCOL_OVERRIDE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PROTOCOL);
    let start_mode = std::env::var("SANDMUX_START_MODE").unwrap_or_else(|_| "manual".into());
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            sessions: BTreeMap::new(),
            layouts: Vec::new(),
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
        docker: docker::Engines::default(),
        vault: Mutex::new(vault::VaultState::new(&base_data)),
        secrets: secretwatch::Watch::default(),
        keyring_last: Mutex::new(None),
        keyring_note: Mutex::new(None),
        notify: notify::Notifier::default(),
        disk_history: history::History::default(),
    });
    eprintln!(
        "{}",
        trf!(
            "sandmuxd: ouvindo em {} (PID {}, protocolo {protocol})",
            "sandmuxd: listening on {} (PID {}, protocol {protocol})",
            socket.display(),
            shared.pid
        )
    );

    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            if let Ok(sig) = term.wait() {
                eprintln!(
                    "{}",
                    trf!("sandmuxd: recebido {sig:?}", "sandmuxd: received {sig:?}")
                );
            }
            shared.stop();
        });
    }

    let logged_in = crate::login::logged_in();
    if !logged_in {
        eprintln!(
            "{}",
            tr!(
                "sandmuxd: subiu sem login (no boot); as sessões voltam no primeiro login",
                "sandmuxd: started without a login (at boot); the sessions come back at the first login"
            )
        );
    }
    history::init(&shared);
    restore(&shared, logged_in);
    history::start(shared.clone());
    net::start_retention(shared.clone());
    // Slices do Docker que ficaram de um daemon derrubado, já sem processos.
    let _ = std::thread::Builder::new()
        .name("slices-velhas".into())
        .spawn(docker::stop_empty_slices);
    net::start_policy_watch(shared.clone());
    secretwatch::start(shared.clone());
    notify::start(shared.clone());
    // Antes de atender: o primeiro cliente já encontra o cofre aberto. Com
    // o keyring desligado, não consulta nada; ligado, espera no máximo o
    // limite do keyring.
    if logged_in {
        keyring::try_unlock(&shared, true);
        // Abas que voltaram esperando o cofre: avisa se ele continua
        // trancado depois de alguns segundos.
        let shared = shared.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            shared.notify.send(notify::Event::WaitingVault);
        });
    } else {
        // Sem login, o keyring está trancado: espera o login.
        let shared = shared.clone();
        let _ = std::thread::Builder::new()
            .name("espera-login".into())
            .spawn(move || wait_login(shared));
    }

    // SAFETY: getuid não falha.
    let me = unsafe { libc::getuid() };
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !accept_peer(peer_uid(&stream), me) {
            eprintln!(
                "{}",
                tr!(
                    "sandmuxd: conexão de outro usuário recusada",
                    "sandmuxd: refused a connection from another user"
                )
            );
            continue;
        }
        let shared = shared.clone();
        std::thread::spawn(move || conn::handle(shared, stream));
    }
    drop(instance);
    Ok(())
}

/// Lê a lista guardada e relança as sessões que estavam vivas.
/// Por que uma sessão viva não pode voltar sozinha: pasta apagada,
/// configuração inválida, perfil que sumiu ou variáveis de `pass-env`, cujos
/// valores nunca vão para o disco.
/// Por que uma sessão viva não pode voltar sozinha.
enum Blocked {
    /// Precisa de valores de `pass-env`.
    PassEnv(String),
    Other(String),
}

fn blocked_restore(shared: &Shared, opts: &LaunchOptions, id: u32) -> Option<Blocked> {
    if !opts.workdir.is_dir() {
        return Some(Blocked::Other(trf!(
            "a pasta de trabalho {} não existe mais",
            "the work folder {} no longer exists",
            opts.workdir.display()
        )));
    }
    let mut host = shared.base.clone();
    host.cwd = opts.workdir.clone();
    match crate::launch::effective(&host, opts) {
        Err(e) => Some(Blocked::Other(trf!(
            "{e:#}; corrija e rode `sandmux restart {id}`",
            "{e:#}; fix it and run `sandmux restart {id}`"
        ))),
        Ok(eff) if !eff.pass_env.is_empty() => Some(Blocked::PassEnv(trf!(
            "precisa de --pass-env {}; rode `sandmux restart {id}` num terminal com as variáveis",
            "needs --pass-env {}; run `sandmux restart {id}` in a terminal with the variables",
            eff.pass_env_names().join(", ")
        ))),
        Ok(_) => None,
    }
}

/// Restaura a lista guardada. Sem login (`logged_in` falso), nenhuma sessão
/// é relançada: as que voltariam ficam aguardando o login.
fn restore(shared: &Arc<Shared>, logged_in: bool) {
    let saved = match store::load(&shared.sessions_file()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: aviso: {e:#}; começando sem sessões",
                    "sandmuxd: warning: {e:#}; starting with no sessions"
                )
            );
            store::Saved::default()
        }
    };
    let mut relaunch = Vec::new();
    {
        let mut st = shared.lock();
        st.next_id = saved.next_id.max(1);
        st.layouts = saved.layouts.clone();
        for s in saved.sessions {
            let mut state = s.state.clone();
            // Gravada esperando o login: vale como viva.
            if state == SessionState::AguardandoLogin {
                state = SessionState::Viva;
            }
            // Um shell de dentro não volta: o sandbox da aba alvo é outro.
            if s.opts.harness == crate::launch::ENTER && state == SessionState::Viva {
                state = SessionState::Parada {
                    motivo: tr!(
                        "o daemon reiniciou; entre de novo com Ctrl+b t na aba alvo",
                        "the daemon restarted; enter again with Ctrl+b t on the target tab"
                    )
                    .into(),
                };
            }
            // Um gateway que esperava para reiniciar volta como vivo.
            if s.opts.gateway && matches!(state, SessionState::Reiniciando { .. }) {
                state = SessionState::Viva;
            }
            // O gateway espera o cofre (os tokens dos canais estão nele).
            let vault_exists = shared
                .vault
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .exists();
            // A aba de chat também: a chave do provedor está nele.
            if state == SessionState::Viva
                && (s.opts.gateway
                    || session::is_chat(&s.opts)
                    || session::needs_vault_for_models(shared, &s.opts))
                && vault_exists
            {
                state = SessionState::AguardandoCofre;
            }
            if state == SessionState::Viva {
                match blocked_restore(shared, &s.opts, s.id) {
                    // Os valores estão no cofre: volta no desbloqueio.
                    Some(Blocked::PassEnv(_)) if s.vault_env => {
                        state = SessionState::AguardandoCofre;
                    }
                    Some(Blocked::PassEnv(motivo) | Blocked::Other(motivo)) => {
                        state = SessionState::Parada { motivo };
                    }
                    None => relaunch.push(s.id),
                }
            }
            // Sem login, nada volta: nem as que esperariam o cofre.
            if !logged_in && matches!(state, SessionState::Viva | SessionState::AguardandoCofre) {
                state = SessionState::AguardandoLogin;
                relaunch.retain(|i| *i != s.id);
            }
            let mut sess = Session::new(s.id, s.name, s.opts, state, shared.history, None);
            sess.group = s.group;
            sess.pass_env = s.pass_env;
            sess.launches = s.launches;
            sess.tags = s.tags;
            sess.vault_env = s.vault_env;
            st.sessions.insert(s.id, sess);
        }
        shared.persist(&mut st);
    }
    relaunch_all(shared, relaunch);
}

/// Relança as sessões; as que falham ficam paradas com o motivo.
fn relaunch_all(shared: &Arc<Shared>, relaunch: Vec<u32>) {
    let vars = shared.default_vars();
    let mut by_id: HashMap<u32, String> = HashMap::new();
    for id in relaunch {
        if let Err(e) = session::relaunch(shared, id, &vars, None, false) {
            by_id.insert(id, format!("{e:#}"));
        }
    }
    if !by_id.is_empty() {
        let mut st = shared.lock();
        for (id, err) in by_id {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: sessão {id} não foi restaurada: {err}",
                    "sandmuxd: session {id} was not restored: {err}"
                )
            );
            if let Some(s) = st.sessions.get_mut(&id) {
                s.state = SessionState::Parada {
                    motivo: trf!("falhou ao restaurar: {err}", "restore failed: {err}"),
                };
            }
        }
        shared.persist(&mut st);
    }
}

/// No primeiro login: as sessões que esperavam o login voltam como na
/// restauração (vivas, aguardando o cofre ou paradas com o motivo).
fn resume_after_login(shared: &Arc<Shared>) {
    let waiting: Vec<(u32, sandmux_proto::LaunchOptions, bool)> = shared
        .lock()
        .sessions
        .values()
        .filter(|s| s.state == SessionState::AguardandoLogin)
        .map(|s| (s.id, s.opts.clone(), s.vault_env))
        .collect();
    if waiting.is_empty() {
        return;
    }
    let vault_exists = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .exists();
    let mut decided = Vec::new();
    for (id, opts, vault_env) in waiting {
        let state = if (opts.gateway
            || session::is_chat(&opts)
            || session::needs_vault_for_models(shared, &opts))
            && vault_exists
        {
            SessionState::AguardandoCofre
        } else {
            match blocked_restore(shared, &opts, id) {
                Some(Blocked::PassEnv(_)) if vault_env => SessionState::AguardandoCofre,
                Some(Blocked::PassEnv(motivo) | Blocked::Other(motivo)) => {
                    SessionState::Parada { motivo }
                }
                None => SessionState::Viva,
            }
        };
        decided.push((id, state));
    }
    let mut relaunch = Vec::new();
    {
        let mut st = shared.lock();
        for (id, state) in decided {
            if let Some(s) = st.sessions.get_mut(&id) {
                if state == SessionState::Viva {
                    relaunch.push(id);
                } else {
                    s.state = state;
                }
            }
        }
        shared.persist(&mut st);
    }
    eprintln!(
        "{}",
        trf!(
            "sandmuxd: login do usuário; relançando {} sessão(ões)",
            "sandmuxd: user login; relaunching {} session(s)",
            relaunch.len()
        )
    );
    relaunch_all(shared, relaunch);
}

/// Espera o primeiro login (o daemon subiu no boot) e então relança as
/// sessões, tenta o keyring e avisa o cofre trancado.
fn wait_login(shared: Arc<Shared>) {
    while !crate::login::logged_in() {
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    resume_after_login(&shared);
    keyring::try_unlock(&shared, true);
    std::thread::sleep(std::time::Duration::from_secs(5));
    shared.notify.send(notify::Event::WaitingVault);
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
