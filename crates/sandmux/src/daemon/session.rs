//! Sessões: lançamento num PTY, leitura da saída, reenvio ao anexar e
//! tamanho da tela.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender, TrySendError};

use anyhow::{Context, Result, anyhow};
use sandmux_proto::term::{ModeTracker, OscFilter};
use sandmux_proto::{LaunchOptions, Msg, SessionInfo, SessionState, Size};

use super::Shared;
use crate::launch;

/// O que vai para a fila de um cliente anexado.
#[derive(Debug)]
pub enum Out {
    Data(Vec<u8>),
    Msg(Msg),
}

/// Fila de cada cliente; cheia, o cliente é desanexado.
pub const CLIENT_QUEUE: usize = 4096;

pub struct Client {
    pub id: u64,
    pub size: Size,
    pub tx: SyncSender<Out>,
}

pub struct Live {
    pub pid: i32,
    pub master: Arc<File>,
}

pub struct Session {
    pub id: u32,
    pub name: String,
    pub opts: LaunchOptions,
    pub state: SessionState,
    pub parser: vt100::Parser,
    pub tracker: ModeTracker,
    pub filter: OscFilter,
    pub clients: Vec<Client>,
    pub size: Size,
    pub live: Option<Live>,
    pub killing: bool,
    /// Escrita no clipboard liberada no último lançamento.
    pub clipboard: bool,
    /// Nomes de `pass-env` do último lançamento.
    pub pass_env: Vec<String>,
    history: usize,
}

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v.max(1)).unwrap_or(NonZeroU16::MIN)
}

impl Session {
    pub fn new(
        id: u32,
        name: String,
        opts: LaunchOptions,
        state: SessionState,
        history: usize,
        size: Option<Size>,
    ) -> Self {
        let size = size.unwrap_or(DEFAULT_SIZE);
        let clipboard = opts.allow_clipboard == Some(true);
        let filter = OscFilter::new(clipboard);
        let pass_env = opts.pass_env.clone();
        Session {
            id,
            name,
            opts,
            state,
            parser: vt100::Parser::new(nz(size.rows), nz(size.cols), history),
            tracker: ModeTracker::new(),
            filter,
            clients: Vec::new(),
            size,
            live: None,
            killing: false,
            clipboard,
            pass_env,
            history,
        }
    }

    pub fn pid(&self) -> Option<i32> {
        self.live.as_ref().map(|l| l.pid)
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id,
            name: self.name.clone(),
            harness: self.opts.harness.clone(),
            workdir: self.opts.workdir.clone(),
            state: self.state.clone(),
            clients: self.clients.len(),
            pass_env: self.pass_env.clone(),
            opts: self.opts.clone(),
        }
    }

    /// Começa um processo novo: terminal limpo, histórico novo.
    fn reset_terminal(&mut self, size: Size) {
        self.size = size;
        self.parser = vt100::Parser::new(nz(size.rows), nz(size.cols), self.history);
        self.tracker = ModeTracker::new();
        self.filter = OscFilter::new(self.clipboard);
    }

    /// Envia a todos os clientes; quem tem a fila cheia é desanexado.
    pub fn broadcast(&mut self, out: impl Fn() -> Out) {
        self.clients
            .retain(|c| !matches!(c.tx.try_send(out()), Err(TrySendError::Full(_))));
    }

    /// Desanexa todos os clientes com uma mensagem final.
    pub fn drop_clients(&mut self, last: Msg) {
        for c in self.clients.drain(..) {
            let _ = c.tx.try_send(Out::Msg(last.clone()));
        }
    }

    /// Aplica o menor tamanho entre os clientes anexados.
    pub fn recompute_size(&mut self) {
        let Some(size) = Size::min_of(self.clients.iter().map(|c| c.size)) else {
            return;
        };
        if size == self.size {
            return;
        }
        self.size = size;
        self.parser
            .screen_mut()
            .set_size(nz(size.rows), nz(size.cols));
        if let Some(l) = &self.live {
            set_winsize(l.master.as_raw_fd(), size);
        }
    }

    /// Bytes que reproduzem, num terminal recém-anexado, o histórico, a tela
    /// e os modos da sessão.
    pub fn replay(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let screen = self.parser.screen_mut();
        let (rows, cols) = screen.size();
        let alt = screen.alternate_screen();
        if !alt {
            screen.set_scrollback(usize::MAX);
            let total = screen.scrollback();
            let mut done = 0;
            while done < total {
                screen.set_scrollback(total - done);
                let take = (usize::from(rows.get())).min(total - done);
                for line in screen.rows(0, cols.get()).take(take) {
                    out.extend_from_slice(line.trim_end().as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
                done += take;
            }
            screen.set_scrollback(0);
            if total > 0 {
                // Empurra o histórico para a rolagem antes de redesenhar.
                for _ in 0..rows.get() {
                    out.extend_from_slice(b"\r\n");
                }
            }
        } else {
            out.extend_from_slice(b"\x1b[?1049h");
        }
        out.extend_from_slice(screen.state_formatted().as_bytes());
        out.extend(self.tracker.replay());
        out
    }
}

pub fn set_winsize(fd: RawFd, size: Size) {
    let ws = libc::winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ioctl com uma estrutura winsize válida.
    unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) };
}

pub fn signal(pid: i32, sig: i32) {
    // SAFETY: kill num PID de um filho nosso.
    unsafe { libc::kill(pid, sig) };
}

/// Pede ao harness que termine: SIGTERM e SIGHUP, como um terminal que
/// fecha (shells interativos ignoram SIGTERM). O lançador repassa os dois.
pub fn terminate(pid: i32) {
    signal(pid, libc::SIGTERM);
    signal(pid, libc::SIGHUP);
}

// ---------------------------------------------------------------- lançamento

pub struct LaunchJob {
    cmd: Command,
    inherit: Vec<RawFd>,
    reply: mpsc::Sender<std::io::Result<Child>>,
}

/// Thread de vida longa que cria os processos dos sandboxes.
pub fn start_launcher() -> mpsc::Sender<LaunchJob> {
    let (tx, rx) = mpsc::channel::<LaunchJob>();
    std::thread::Builder::new()
        .name("lancador".into())
        .spawn(move || {
            // SAFETY: getpid não falha.
            let me = unsafe { libc::getpid() };
            for mut job in rx {
                let inherit = job.inherit.clone();
                // SAFETY: só chamadas de sistema seguras depois de fork.
                unsafe {
                    job.cmd.pre_exec(move || child_setup(me, &inherit));
                }
                let _ = job.reply.send(job.cmd.spawn());
            }
        })
        .expect("thread de lançamento");
    tx
}

/// Preparo do filho, entre o fork e o exec do `sandmux-run`.
fn child_setup(parent: libc::pid_t, inherit: &[RawFd]) -> std::io::Result<()> {
    let check = |r: libc::c_int| {
        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    // SAFETY: chamadas de sistema sobre o próprio processo e seus descritores.
    unsafe {
        check(libc::setsid())?;
        check(libc::ioctl(0, libc::TIOCSCTTY, 0))?;
        check(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0))?;
        if libc::getppid() != parent {
            libc::_exit(1);
        }
        // Máscara e disposições de sinal limpas: o daemon bloqueia os de
        // término e ignora SIGPIPE.
        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        check(libc::sigprocmask(
            libc::SIG_SETMASK,
            &empty,
            std::ptr::null_mut(),
        ))?;
        for sig in [libc::SIGPIPE, libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(sig, libc::SIG_DFL);
        }
        // Nenhum descritor do daemon (PTYs de outras sessões, sockets) passa
        // para o sandbox: só a especificação e a trava.
        check(libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        ) as libc::c_int)?;
        for &fd in inherit {
            check(libc::fcntl(fd, libc::F_SETFD, 0))?;
        }
    }
    Ok(())
}

fn set_cloexec(fd: RawFd) {
    // SAFETY: fcntl sobre um descritor válido.
    unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
}

/// Host do lançamento: o do daemon, com a pasta, as variáveis e o shell do
/// pedido.
fn host_for(
    shared: &Shared,
    opts: &LaunchOptions,
    vars: &BTreeMap<String, String>,
) -> sandmux_policy::Host {
    let mut host = shared.base.clone();
    host.cwd = opts.workdir.clone();
    host.env = vars.clone();
    if let Some(sh) = vars
        .get("SHELL")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        host.user_shell = sh;
    }
    host
}

/// Processo lançado, ainda sem sessão registrada.
pub struct Spawned {
    pub child: Child,
    pub master: Arc<File>,
    pub warnings: Vec<String>,
    /// Política efetiva do lançamento que importa ao daemon.
    pub clipboard: bool,
    pub pass_env: Vec<String>,
}

/// Compõe e lança um sandbox num PTY novo.
pub fn spawn(
    shared: &Shared,
    opts: &LaunchOptions,
    vars: &BTreeMap<String, String>,
    size: Size,
) -> Result<Spawned> {
    let host = host_for(shared, opts, vars);
    let prepared = launch::prepare(&host, opts)?;
    let ws = nix::pty::Winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = nix::pty::openpty(Some(&ws), None).context("abrir o PTY")?;
    set_cloexec(pty.master.as_raw_fd());
    set_cloexec(pty.slave.as_raw_fd());
    let slave: OwnedFd = pty.slave;
    let mut cmd = prepared.command();
    cmd.stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    let (tx, rx) = mpsc::channel();
    shared
        .launcher
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(LaunchJob {
            cmd,
            inherit: prepared.inherited_fds(),
            reply: tx,
        })
        .map_err(|_| anyhow!("a thread de lançamento parou"))?;
    let child = rx
        .recv()
        .map_err(|_| anyhow!("a thread de lançamento parou"))?
        .with_context(|| format!("executar {}", prepared.runner.display()))?;
    Ok(Spawned {
        child,
        master: Arc::new(File::from(pty.master)),
        warnings: prepared.warnings,
        clipboard: prepared.clipboard,
        pass_env: prepared.pass_env,
    })
}

/// Liga um processo lançado à sessão `id` e começa a ler a saída.
pub fn attach_process(shared: &Arc<Shared>, id: u32, spawned: Spawned, size: Size) {
    let Spawned {
        child,
        master,
        clipboard,
        pass_env,
        ..
    } = spawned;
    let pid = child.id() as i32;
    {
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.clipboard = clipboard;
            s.pass_env = pass_env;
            s.reset_terminal(size);
            s.state = SessionState::Viva;
            s.killing = false;
            s.live = Some(Live {
                pid,
                master: master.clone(),
            });
        }
        shared.persist(&st);
    }
    let shared = shared.clone();
    std::thread::Builder::new()
        .name(format!("sessao-{id}"))
        .spawn(move || reader(shared, id, pid, master, child))
        .expect("thread da sessão");
}

/// Relança uma sessão existente (reinício ou restauração).
pub fn relaunch(
    shared: &Arc<Shared>,
    id: u32,
    vars: &BTreeMap<String, String>,
    size: Option<Size>,
) -> Result<Vec<String>> {
    let (opts, cur) = {
        let st = shared.lock();
        let s = st
            .sessions
            .get(&id)
            .ok_or_else(|| anyhow!("não existe a sessão {id}"))?;
        if s.live.is_some() {
            return Err(anyhow!(
                "a sessão {id} está viva; para relançá-la, encerre-a antes com `sandmux kill {id}`"
            ));
        }
        (s.opts.clone(), s.size)
    };
    let size = size.unwrap_or(cur);
    let spawned = spawn(shared, &opts, vars, size)?;
    let warnings = spawned.warnings.clone();
    attach_process(shared, id, spawned, size);
    Ok(warnings)
}

fn reader(shared: Arc<Shared>, id: u32, pid: i32, master: Arc<File>, mut child: Child) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match (&*master).read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut st = shared.lock();
                if let Some(s) = st.sessions.get_mut(&id) {
                    let data = s.filter.filter(&buf[..n]);
                    if data.is_empty() {
                        continue;
                    }
                    s.parser.process(&data);
                    s.tracker.feed(&data);
                    s.broadcast(|| Out::Data(data.clone()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let code = match child.wait() {
        Ok(status) => status.code().unwrap_or_else(|| {
            128 + std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)
        }),
        Err(_) => 1,
    };
    let mut st = shared.lock();
    let stopping = st.stopping;
    let mut remove = false;
    if let Some(s) = st.sessions.get_mut(&id)
        && s.pid() == Some(pid)
    {
        s.live = None;
        if stopping {
            s.drop_clients(Msg::Detached {
                reason: "o daemon parou".into(),
            });
        } else if s.killing {
            s.drop_clients(Msg::Detached {
                reason: format!("a sessão {id} foi encerrada por `sandmux kill`"),
            });
            remove = true;
        } else {
            s.state = SessionState::Encerrada { codigo: code };
            s.drop_clients(Msg::Exited { id, code });
        }
    }
    if remove {
        st.sessions.remove(&id);
    }
    if !stopping {
        shared.persist(&st);
    }
    drop(st);
    shared.changed.notify_all();
}

/// Nome da sessão, `<harness>:<pasta>`. Pode repetir; o identificador não.
pub fn session_name(opts: &LaunchOptions) -> String {
    let folder = opts
        .workdir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    format!("{}:{folder}", opts.harness)
}

/// Alocação do identificador e registro da sessão nova.
pub fn register(
    shared: &Arc<Shared>,
    opts: LaunchOptions,
    spawned: Spawned,
    size: Size,
) -> (u32, String) {
    let name = session_name(&opts);
    let id = {
        let mut st = shared.lock();
        let id = st.next_id;
        st.next_id += 1;
        let s = Session::new(
            id,
            name.clone(),
            opts,
            SessionState::Viva,
            shared.history,
            Some(size),
        );
        st.sessions.insert(id, s);
        id
    };
    attach_process(shared, id, spawned, size);
    (id, name)
}
