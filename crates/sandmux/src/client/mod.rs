//! Lado cliente do daemon: conexão, subida do daemon e modo anexado.

pub mod prefix;
pub mod systemd;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use nix::sys::signal::{SigSet, Signal};
use nix::sys::termios::{self, SetArg, Termios};
use sandmux_proto::msg::{DaemonInfo, check_welcome};
use sandmux_proto::term::ModeTracker;
use sandmux_proto::{Frame, Msg, PROTOCOL, Size, read_frame, write_control, write_data};

use crate::{launch, paths};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Diretório de dados do cliente, o mesmo cálculo do `Host`.
pub fn data_dir() -> PathBuf {
    match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(p) if p.is_absolute() => p.join("sandmux"),
        _ => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".local/share/sandmux"),
    }
}

/// Uma conexão aceita pelo daemon.
pub struct Conn {
    pub stream: UnixStream,
    pub info: DaemonInfo,
}

impl Conn {
    /// Conecta ao daemon que já está rodando. `None` quando não há daemon.
    pub fn connect() -> Result<Option<Conn>> {
        let mut stream = match UnixStream::connect(paths::socket_path()) {
            Ok(s) => s,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e).context("conectar ao daemon"),
        };
        write_control(
            &mut stream,
            &Msg::Hello {
                protocol: PROTOCOL,
                version: VERSION.into(),
                data_dir: data_dir(),
            },
        )?;
        let reply = match read_frame(&mut stream)? {
            Some(Frame::Control(m)) => m,
            _ => bail!("o daemon encerrou a conexão na saudação"),
        };
        let info = check_welcome(&reply, PROTOCOL, VERSION).map_err(|e| anyhow!(e))?;
        Ok(Some(Conn { stream, info }))
    }

    pub fn send(&mut self, msg: &Msg) -> Result<()> {
        write_control(&mut self.stream, msg)?;
        Ok(())
    }

    /// A próxima mensagem de controle, mostrando os avisos no caminho.
    pub fn reply(&mut self) -> Result<Msg> {
        loop {
            match read_frame(&mut self.stream)? {
                Some(Frame::Control(Msg::Warning { message })) => {
                    eprintln!("sandmux: aviso: {message}");
                }
                Some(Frame::Control(m)) => return Ok(m),
                Some(Frame::Data(_)) => {}
                None => bail!("o daemon encerrou a conexão"),
            }
        }
    }

    /// Envia e espera a resposta; `Error` vira erro.
    pub fn call(&mut self, msg: &Msg) -> Result<Msg> {
        self.send(msg)?;
        match self.reply()? {
            Msg::Error { message } => Err(anyhow!(message)),
            m => Ok(m),
        }
    }
}

pub fn wait_for_daemon(timeout: Duration) -> Result<Conn> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(c) = Conn::connect()? {
            return Ok(c);
        }
        if Instant::now() >= deadline {
            bail!(
                "o daemon não respondeu em {}; veja o log em {}",
                paths::socket_path().display(),
                log_path().display()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn log_path() -> PathBuf {
    paths::state_dir().join("sandmuxd.log")
}

/// Sobe o daemon como processo próprio, desligado do terminal.
fn start_direct() -> Result<()> {
    let dir = paths::state_dir();
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .with_context(|| format!("abrir {}", log_path().display()))?;
    let mut cmd = Command::new(launch::sibling("sandmuxd"));
    cmd.env("SANDMUX_START_MODE", "direct")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    // SAFETY: setsid e close_range são seguros depois de fork.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // O daemon não herda descritores soltos do cliente (o terminal
            // dele, por exemplo).
            libc::syscall(
                libc::SYS_close_range,
                3u32,
                u32::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            );
            Ok(())
        });
    }
    let mut child = cmd.spawn().context("iniciar o sandmuxd")?;
    // A thread que colhe o daemon nasce com todos os sinais bloqueados, para
    // não receber os sinais que o modo anexado trata.
    let old = SigSet::all().thread_swap_mask(nix::sys::signal::SigmaskHow::SIG_BLOCK)?;
    std::thread::spawn(move || child.wait());
    old.thread_set_mask()?;
    Ok(())
}

/// Como o daemon deve subir: `direct` ou `systemd`.
fn start_mode() -> &'static str {
    if std::env::var("SANDMUX_DAEMON_START").as_deref() == Ok("direct") || !systemd::available() {
        "direct"
    } else {
        "systemd"
    }
}

/// Conecta ao daemon, subindo-o se preciso.
pub fn connect_or_start() -> Result<Conn> {
    if let Some(c) = Conn::connect()? {
        return Ok(c);
    }
    start_daemon()?;
    wait_for_daemon(Duration::from_secs(5))
}

pub fn start_daemon() -> Result<()> {
    if start_mode() == "systemd" {
        let exec = launch::sibling("sandmuxd");
        let exec = exec.canonicalize().unwrap_or(exec);
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        match systemd::install_and_start(&exec, data_home.as_deref()) {
            Ok(()) => return Ok(()),
            Err(e) => eprintln!("sandmux: aviso: {e:#}; subindo o daemon sem o systemd"),
        }
    }
    start_direct()
}

/// Variáveis enviadas ao daemon: a lista segura, o `SHELL` e as de
/// `--pass-env`. Uma variável pedida que não existe é erro.
pub fn client_vars(pass_env: &[String]) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    for k in sandmux_policy::lists::SAFE_ENV.iter().chain(&["SHELL"]) {
        if let Ok(v) = std::env::var(k) {
            vars.insert(k.to_string(), v);
        }
    }
    for k in pass_env {
        let v = std::env::var(k)
            .map_err(|_| anyhow!(sandmux_policy::PolicyError::MissingEnv(k.clone()).to_string()))?;
        vars.insert(k.clone(), v);
    }
    Ok(vars)
}

/// Tamanho do terminal da saída padrão.
pub fn terminal_size() -> Size {
    let mut ws = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ioctl com uma estrutura winsize válida.
    let ok = unsafe { libc::ioctl(std::io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut ws) } == 0;
    if ok && ws.ws_col > 0 && ws.ws_row > 0 {
        Size {
            cols: ws.ws_col,
            rows: ws.ws_row,
        }
    } else {
        Size { cols: 80, rows: 24 }
    }
}

/// Terminal em modo cru, restaurado ao sair.
struct RawMode {
    orig: Termios,
}

impl RawMode {
    fn enter() -> Result<RawMode> {
        let stdin = std::io::stdin();
        let orig = termios::tcgetattr(stdin.as_fd())?;
        let mut raw = orig.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(stdin.as_fd(), SetArg::TCSANOW, &raw)?;
        Ok(RawMode { orig })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(std::io::stdin().as_fd(), SetArg::TCSANOW, &self.orig);
    }
}

/// Como terminou o modo anexado.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// Ctrl+b d, ou sinal de término no cliente.
    Detached,
    /// Terminal fechado (SIGHUP ou fim da entrada).
    Hangup,
    /// Desanexado pelo daemon, com o motivo.
    Remote(String),
    Exited(i32),
    Error(String),
}

fn set_outcome(o: &Mutex<Option<Outcome>>, v: Outcome) {
    let mut g = o.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(v);
    }
}

/// Segue anexado depois de pedir `Create`, `Restart` ou `Attach` com
/// anexo. Devolve o código de saída do comando do cliente.
pub fn attached(conn: Conn) -> Result<i32> {
    attached_inner(conn, None)
}

/// Como [`attached`], quando o evento `Attached` já foi lido.
pub fn attached_after(conn: Conn, id: u32, name: String) -> Result<i32> {
    attached_inner(conn, Some((id, name)))
}

fn attached_inner(conn: Conn, known: Option<(u32, String)>) -> Result<i32> {
    let mut stream = conn.stream;
    let mut sigs = SigSet::empty();
    for s in [
        Signal::SIGWINCH,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGINT,
        Signal::SIGQUIT,
    ] {
        sigs.add(s);
    }
    sigs.thread_block()?;
    let raw = RawMode::enter()?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let closer = stream.try_clone()?;
    let outcome: Arc<Mutex<Option<Outcome>>> = Arc::new(Mutex::new(None));

    {
        let writer = writer.clone();
        let outcome = outcome.clone();
        let closer = closer.try_clone()?;
        std::thread::spawn(move || {
            while let Ok(sig) = sigs.wait() {
                match sig {
                    Signal::SIGWINCH => {
                        let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = write_control(
                            &mut *w,
                            &Msg::Resize {
                                size: terminal_size(),
                            },
                        );
                    }
                    Signal::SIGHUP => {
                        set_outcome(&outcome, Outcome::Hangup);
                        let _ = closer.shutdown(Shutdown::Both);
                        break;
                    }
                    _ => {
                        set_outcome(&outcome, Outcome::Detached);
                        let _ = closer.shutdown(Shutdown::Both);
                        break;
                    }
                }
            }
        });
    }
    {
        let writer = writer.clone();
        let outcome = outcome.clone();
        let closer = closer.try_clone()?;
        std::thread::spawn(move || {
            let mut p = prefix::Prefix::new();
            let mut buf = [0u8; 8192];
            let mut stdin = std::io::stdin().lock();
            loop {
                let n = match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        set_outcome(&outcome, Outcome::Hangup);
                        let _ = closer.shutdown(Shutdown::Both);
                        return;
                    }
                    Ok(n) => n,
                };
                for a in p.feed(&buf[..n]) {
                    match a {
                        prefix::Action::Send(b) => {
                            let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
                            if write_data(&mut *w, &b).is_err() {
                                return;
                            }
                        }
                        prefix::Action::Detach => {
                            set_outcome(&outcome, Outcome::Detached);
                            let _ = closer.shutdown(Shutdown::Both);
                            return;
                        }
                    }
                }
            }
        });
    }

    let mut tracker = ModeTracker::new();
    let mut out = std::io::stdout().lock();
    let mut session: Option<(u32, String)> = known;
    loop {
        match read_frame(&mut stream) {
            Ok(Some(Frame::Data(d))) => {
                tracker.feed(&d);
                if out.write_all(&d).and_then(|_| out.flush()).is_err() {
                    set_outcome(&outcome, Outcome::Hangup);
                    break;
                }
            }
            Ok(Some(Frame::Control(m))) => match m {
                Msg::Attached { id, name } => {
                    session = Some((id, name));
                    // O terminal pode ter mudado de tamanho desde o pedido.
                    let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = write_control(
                        &mut *w,
                        &Msg::Resize {
                            size: terminal_size(),
                        },
                    );
                }
                Msg::Exited { code, .. } => {
                    set_outcome(&outcome, Outcome::Exited(code));
                    break;
                }
                Msg::Detached { reason } => {
                    set_outcome(&outcome, Outcome::Remote(reason));
                    break;
                }
                Msg::Error { message } => {
                    set_outcome(&outcome, Outcome::Error(message));
                    break;
                }
                _ => {}
            },
            Ok(None) | Err(_) => break,
        }
    }
    let _ = out.write_all(&tracker.reset());
    let _ = out.flush();
    drop(out);
    drop(raw);
    let _ = stream.shutdown(Shutdown::Both);

    let (id, name) = session.unwrap_or((0, String::new()));
    let outcome = outcome.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match outcome {
        Some(Outcome::Exited(code)) => {
            eprintln!("sandmux: a sessão {id} ({name}) terminou com código {code}");
            Ok(code)
        }
        Some(Outcome::Detached) => {
            eprintln!(
                "sandmux: desanexado; a sessão {id} ({name}) continua viva. Para voltar: sandmux attach {id}"
            );
            Ok(0)
        }
        Some(Outcome::Hangup) => Ok(0),
        Some(Outcome::Remote(reason)) => {
            eprintln!("sandmux: {reason}");
            Ok(0)
        }
        Some(Outcome::Error(message)) => Err(anyhow!(message)),
        None => Err(anyhow!("a conexão com o daemon caiu")),
    }
}
