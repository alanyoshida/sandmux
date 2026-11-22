//! O registro do sandbox no auxiliar root (`sandmux-helper`), nos kernels
//! com o Landlock abaixo do ABI 6: o pedido `scope` com o user namespace do
//! filho e a lista de dispositivos do `/dev` do sandbox. O auxiliar nega por
//! BPF LSM o que o Landlock não tem (sockets abstratos de fora, sinais para
//! fora e, no ABI 4, `ioctl` em dispositivo fora da lista) enquanto a
//! conexão devolvida estiver aberta.

use std::io::IoSlice;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub use sandmux_net::procinfo::TabScope;

use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};
use nix::unistd::Pid;
use sandmux_spec::{LaunchSpec, Mount, tr};

/// Socket do serviço do sistema.
const SOCKET: &str = "/run/sandmux-helper.sock";
/// Variável que troca o socket (testes).
const SOCKET_ENV: &str = "SANDMUX_HELPER_SOCKET";
/// Dispositivos na lista, no máximo (o mesmo teto do auxiliar).
const MAX_DEVS: usize = 16;

fn socket_path() -> PathBuf {
    std::env::var_os(SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SOCKET))
}

/// `MKDEV` do kernel: 12 bits de número principal e 20 de secundário.
fn kdev(major: u32, minor: u32) -> u32 {
    (major << 20) | (minor & 0xf_ffff)
}

/// Um `st_rdev` (`makedev` da glibc) no formato do kernel.
fn from_rdev(rdev: u64) -> (u32, u32) {
    let major = (((rdev >> 32) & 0xffff_f000) | ((rdev >> 8) & 0xfff)) as u32;
    let minor = (((rdev >> 12) & 0xffff_ff00) | (rdev & 0xff)) as u32;
    (major, minor)
}

/// Os dispositivos do `/dev` do sandbox, como pares `(número, máscara)` no
/// formato do kernel: os nós montados pela especificação, o `tty` (5:0), o
/// `ptmx` (5:2) e os pseudoterminais (números principais 136 a 143). Quando
/// passam do teto, os nós montados entram pelo número principal.
pub fn devices(mounts: &[Mount]) -> Vec<(u32, u32)> {
    let nodes: Vec<(u32, u32)> = mounts
        .iter()
        .filter_map(|m| match m {
            Mount::Device { source, .. } => {
                let md = std::fs::metadata(source).ok()?;
                let ft = md.file_type();
                (ft.is_char_device() || ft.is_block_device())
                    .then(|| from_rdev(std::os::unix::fs::MetadataExt::rdev(&md)))
            }
            _ => None,
        })
        .collect();
    // O BPF vale desde o registro, e o Landlock só nos arquivos abertos
    // depois das regras: entram também o TUN (10:200), que o filho configura
    // antes de o comando rodar e que o sandbox não alcança (não está no `/dev`
    // dele), e os consoles de texto (número principal 4), que chegam como a
    // entrada herdada de um `sandmux run` num console.
    let mut out = vec![
        (kdev(5, 0), u32::MAX),
        (kdev(5, 2), u32::MAX),
        (kdev(136, 0), 0xff80_0000),
        (kdev(10, 200), u32::MAX),
        (kdev(4, 0), 0xfff0_0000),
    ];
    let mut push = |v: (u32, u32)| {
        if !out.contains(&v) {
            out.push(v);
        }
    };
    let exact: Vec<(u32, u32)> = nodes
        .iter()
        .map(|&(ma, mi)| (kdev(ma, mi), u32::MAX))
        .collect();
    let mut uniq = exact.clone();
    uniq.dedup();
    if 5 + uniq.len() <= MAX_DEVS {
        exact.into_iter().for_each(&mut push);
    } else {
        for &(ma, _) in &nodes {
            push((kdev(ma, 0), 0xfff0_0000));
        }
    }
    out.truncate(MAX_DEVS);
    out
}

fn seqpacket() -> std::io::Result<OwnedFd> {
    // SAFETY: socket devolve um descritor novo ou -1.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: descritor recém-criado.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Conecta ao auxiliar root.
fn connect() -> Result<OwnedFd, String> {
    connect_fail().map_err(Fail::message)
}

/// Como [`connect`], separando o socket que não existe (o auxiliar não está
/// instalado) do auxiliar que não atende (a troca de auxiliar).
fn connect_fail() -> Result<OwnedFd, Fail> {
    let path = socket_path();
    let sock = seqpacket().map_err(|e| Fail::NoAnswer(e.to_string()))?;
    let addr = nix::sys::socket::UnixAddr::new(&path).map_err(|e| Fail::NoAnswer(e.to_string()))?;
    nix::sys::socket::connect(sock.as_raw_fd(), &addr).map_err(|e| {
        let m = sandmux_spec::trf!(
            "o auxiliar root não responde em {} ({e})",
            "the root helper does not answer at {} ({e})",
            path.display()
        );
        if e == nix::errno::Errno::ENOENT {
            Fail::Missing(m)
        } else {
            Fail::NoAnswer(m)
        }
    })?;
    Ok(sock)
}

/// Por que um pedido ao auxiliar não deu certo.
#[derive(Debug, PartialEq)]
enum Fail {
    /// Sem resposta: o auxiliar não está lá, ou caiu no meio (a troca de
    /// auxiliar). Vale tentar de novo.
    NoAnswer(String),
    /// O socket do auxiliar não existe. Numa troca, ele some só entre o
    /// auxiliar velho sair e o novo criar o socket; por mais que isso, o
    /// auxiliar não está instalado.
    Missing(String),
    /// O auxiliar recusou, com o motivo.
    Refused(String),
}

impl Fail {
    fn message(self) -> String {
        match self {
            Fail::NoAnswer(m) | Fail::Missing(m) | Fail::Refused(m) => m,
        }
    }
}

/// Lê a resposta do auxiliar, com um prazo de 10 s.
fn reply(sock: &OwnedFd) -> Result<serde_json::Value, String> {
    answer(sock).map_err(Fail::message)
}

fn answer(sock: &OwnedFd) -> Result<serde_json::Value, Fail> {
    let tv = libc::timeval {
        tv_sec: 10,
        tv_usec: 0,
    };
    // SAFETY: setsockopt lê a struct do tamanho dado.
    unsafe {
        libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&tv as *const libc::timeval).cast(),
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }
    let mut buf = vec![0u8; 4097];
    // SAFETY: recv escreve no máximo buf.len() bytes.
    let n = unsafe { libc::recv(sock.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
    if n <= 0 {
        return Err(Fail::NoAnswer(
            tr!(
                "o auxiliar root não respondeu ao pedido",
                "the root helper did not answer the request"
            )
            .into(),
        ));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&buf[..n as usize]).map_err(|e| Fail::Refused(e.to_string()))?;
    if v["ok"] != true {
        return Err(Fail::Refused(
            v["error"].as_str().unwrap_or("?").to_string(),
        ));
    }
    Ok(v)
}

/// O auxiliar pode completar o confinamento (o pedido `scope`)? `Err` com
/// o motivo.
pub fn status() -> Result<(), String> {
    let sock = connect()?;
    let req = serde_json::json!({"op": "status"}).to_string();
    sendmsg::<()>(
        sock.as_raw_fd(),
        &[IoSlice::new(req.as_bytes())],
        &[],
        MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .map_err(|e| e.to_string())?;
    let v = reply(&sock)?;
    if v["scope"] == true {
        return Ok(());
    }
    Err(v["scope_reason"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            tr!(
                "o auxiliar root é antigo e não tem o pedido scope; rode `sandmux setup`",
                "the root helper is old and lacks the scope request; run `sandmux setup`"
            )
            .into()
        }))
}

/// O escopo registrado de um sandbox: a conexão que o mantém, o user
/// namespace (aberto antes da liberação, quando o processo 1 ainda deixa) e
/// os parâmetros, para registrar de novo.
pub struct Registered {
    pub conn: OwnedFd,
    pub ns: Arc<OwnedFd>,
    pub params: TabScope,
}

/// Quanto o registro espera o auxiliar (a troca de auxiliar).
const RETRY_FOR: Duration = Duration::from_secs(30);
const RETRY_EVERY: Duration = Duration::from_millis(300);
/// No lançamento, quanto o registro espera um socket que não existe: o
/// bastante para a troca recriar o socket, sem fazer quem não tem o
/// auxiliar esperar os 30 s.
const MISSING_FOR: Duration = Duration::from_secs(1);

/// Registra o sandbox do filho no auxiliar. Precisa ser chamado antes da
/// liberação do filho.
pub fn register(child: Pid, abi: i32, spec: &LaunchSpec) -> Result<Registered, String> {
    let ns = std::fs::File::open(format!("/proc/{child}/ns/user")).map_err(|e| e.to_string())?;
    let ns = Arc::new(OwnedFd::from(ns));
    let params = TabScope {
        init_pid: child.as_raw() as u32,
        ioctl: abi < 5,
        devs: devices(&spec.mounts),
    };
    let conn = retry(RETRY_FOR, MISSING_FOR, RETRY_EVERY, || {
        try_register(&ns, &params)
    })?;
    Ok(Registered { conn, ns, params })
}

/// O pedido `scope` de novo, depois de uma queda do auxiliar, enquanto ele
/// não responde, por até [`RETRY_FOR`], também sem o socket (o socket do
/// systemd pode ser recriado). Uma recusa com motivo não se repete.
fn register_ns(ns: &OwnedFd, params: &TabScope) -> Result<OwnedFd, String> {
    retry(RETRY_FOR, RETRY_FOR, RETRY_EVERY, || {
        try_register(ns, params)
    })
}

/// Tenta `f` enquanto ele não tem resposta, até `total`; sem o socket do
/// auxiliar, só até `missing`.
fn retry<T>(
    total: Duration,
    missing: Duration,
    every: Duration,
    mut f: impl FnMut() -> Result<T, Fail>,
) -> Result<T, String> {
    let start = Instant::now();
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(Fail::Refused(m)) => return Err(m),
            Err(Fail::Missing(m)) if start.elapsed() >= missing.min(total) => return Err(m),
            Err(Fail::NoAnswer(m)) if start.elapsed() >= total => return Err(m),
            Err(Fail::Missing(_) | Fail::NoAnswer(_)) => {
                std::thread::sleep(every.min(missing.min(total)))
            }
        }
    }
}

fn try_register(ns: &OwnedFd, params: &TabScope) -> Result<OwnedFd, Fail> {
    let sock = connect_fail()?;
    let req = serde_json::json!({
        "op": "scope",
        "ioctl": params.ioctl,
        "init_pid": params.init_pid,
        "devs": params.devs,
    })
    .to_string();
    let fds = [ns.as_raw_fd()];
    sendmsg::<()>(
        sock.as_raw_fd(),
        &[IoSlice::new(req.as_bytes())],
        &[ControlMessage::ScmRights(&fds)],
        MsgFlags::MSG_NOSIGNAL,
        None,
    )
    .map_err(|e| Fail::NoAnswer(e.to_string()))?;
    answer(&sock)?;
    Ok(sock)
}

/// Quem refaz o escopo depois de uma queda do auxiliar.
pub const KEEPER_UNKNOWN: u8 = 0;
/// O daemon (ele congela a aba e registra de novo).
pub const KEEPER_DAEMON: u8 = 1;
/// O próprio lançador.
pub const KEEPER_SELF: u8 = 2;

/// A vigia do escopo: quando a conexão cai (o auxiliar saiu), registra de
/// novo, a não ser que o daemon tenha assumido (`keeper`); sem o registro em
/// [`RETRY_FOR`], mata o sandbox.
pub fn keep(reg: Registered, child: Pid, keeper: Arc<AtomicU8>) {
    let Registered {
        mut conn,
        ns,
        params,
    } = reg;
    let _ = std::thread::Builder::new()
        .name("sandmux-escopo".into())
        .spawn(move || {
            loop {
                wait_closed(&conn);
                // Numa sessão do daemon, a resposta dele pode ainda não ter
                // chegado.
                let deadline = Instant::now() + Duration::from_secs(15);
                while keeper.load(Ordering::SeqCst) == KEEPER_UNKNOWN && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if keeper.load(Ordering::SeqCst) == KEEPER_DAEMON {
                    return;
                }
                match register_ns(&ns, &params) {
                    Ok(c) => conn = c,
                    Err(e) => {
                        eprintln!(
                            "{}",
                            sandmux_spec::trf!(
                                "sandmux: o escopo do sandbox não voltou depois da queda do auxiliar root ({e}); o sandbox para",
                                "sandmux: the sandbox scope did not come back after the root helper dropped ({e}); the sandbox stops"
                            )
                        );
                        let _ = nix::sys::signal::kill(child, nix::sys::signal::Signal::SIGKILL);
                        return;
                    }
                }
            }
        });
}

/// Espera a conexão fechar (o auxiliar não manda nada nela depois da
/// resposta).
fn wait_closed(sock: &OwnedFd) {
    // Sem o prazo do pedido: a espera é pelo fim.
    let tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    // SAFETY: setsockopt lê a struct do tamanho dado.
    unsafe {
        libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&tv as *const libc::timeval).cast(),
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );
    }
    let mut buf = [0u8; 512];
    loop {
        // SAFETY: recv escreve no máximo buf.len() bytes.
        let n = unsafe { libc::recv(sock.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n == 0
            || (n < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted)
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenta_de_novo_so_sem_resposta() {
        let ms = Duration::from_millis(1);
        let mut n = 0;
        let s5 = Duration::from_secs(5);
        let r = retry(s5, s5, ms, || {
            n += 1;
            if n < 3 {
                Err(Fail::NoAnswer("caiu".into()))
            } else {
                Ok(n)
            }
        });
        assert_eq!(r, Ok(3));
        let mut n = 0;
        let r: Result<(), String> = retry(s5, s5, ms, || {
            n += 1;
            Err(Fail::Refused("não".into()))
        });
        assert_eq!((r, n), (Err("não".into()), 1));
        let r: Result<(), String> = retry(Duration::from_millis(20), s5, ms, || {
            Err(Fail::NoAnswer("ninguém".into()))
        });
        assert_eq!(r, Err("ninguém".into()));
        // Sem o socket, desiste no prazo curto, mesmo com o longo sobrando.
        let t = Instant::now();
        let r: Result<(), String> = retry(s5, Duration::from_millis(50), ms, || {
            Err(Fail::Missing("sem socket".into()))
        });
        assert_eq!(r, Err("sem socket".into()));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        // O socket que volta dentro do prazo curto (a troca) passa.
        let mut n = 0;
        let r = retry(s5, Duration::from_millis(500), ms, || {
            n += 1;
            if n < 3 {
                Err(Fail::Missing("trocando".into()))
            } else {
                Ok(n)
            }
        });
        assert_eq!(r, Ok(3));
        // Um socket que deixa de faltar e passa a não responder segue até o
        // prazo longo.
        let t = Instant::now();
        let mut n = 0;
        let r: Result<(), String> = retry(
            Duration::from_millis(300),
            Duration::from_millis(20),
            ms,
            || {
                n += 1;
                if n < 2 {
                    Err(Fail::Missing("trocando".into()))
                } else {
                    Err(Fail::NoAnswer("subindo".into()))
                }
            },
        );
        assert_eq!(r, Err("subindo".into()));
        assert!(
            t.elapsed() >= Duration::from_millis(300),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn numeros_de_dispositivo() {
        assert_eq!(from_rdev(0x0103), (1, 3));
        assert_eq!(kdev(136, 5) & 0xff80_0000, kdev(136, 0));
        assert_eq!(kdev(143, 5) & 0xff80_0000, kdev(136, 0));
        assert_ne!(kdev(144, 5) & 0xff80_0000, kdev(136, 0));
        assert_ne!(kdev(135, 5) & 0xff80_0000, kdev(136, 0));
    }

    #[test]
    fn lista_do_dev_do_sandbox() {
        let mounts: Vec<Mount> = ["null", "zero", "tty"]
            .iter()
            .map(|d| Mount::Device {
                source: PathBuf::from("/dev").join(d),
                dest: PathBuf::from("/dev").join(d),
            })
            .collect();
        let d = devices(&mounts);
        assert!(d.contains(&(kdev(1, 3), u32::MAX)), "{d:?}");
        assert!(d.contains(&(kdev(1, 5), u32::MAX)), "{d:?}");
        // O tty não se repete.
        assert_eq!(
            d.iter().filter(|x| **x == (kdev(5, 0), u32::MAX)).count(),
            1
        );
        assert!(d.len() <= MAX_DEVS);
    }
}
