//! Auxiliar root do sandmux: prende os programas BPF LSM do confinamento ao
//! cgroup do Docker privado de uma pasta, a pedido do daemon do usuário.
//!
//! Uso:
//!
//! - pelo systemd (`sandmux-helper.socket`): recebe o socket em `LISTEN_FDS`;
//! - `sandmux-helper --socket <caminho>`: cria o socket (testes).
//!
//! Só restringe o cgroup que pertence a quem pede (`SO_PEERCRED`), e só
//! enquanto a conexão estiver aberta. Detalhes em `docs/helper.md`.

use std::os::fd::{FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::process::exit;

use sandmux::helper;

const USAGE: &str = "uso: sandmux-helper [--socket <caminho>]\n\
    Sem --socket, usa o socket do systemd (LISTEN_FDS).";

fn main() {
    let mut args = std::env::args().skip(1);
    let mut socket: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => match args.next() {
                Some(p) => socket = Some(PathBuf::from(p)),
                None => fail("falta o caminho de --socket"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("sandmux-helper {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            _ => fail(format!("argumento desconhecido: {a}\n{USAGE}")),
        }
    }
    raise_nofile();
    let listener = match socket {
        Some(p) => helper::listen(&p).unwrap_or_else(|e| fail(format!("{}: {e}", p.display()))),
        None => systemd_socket().unwrap_or_else(|| {
            fail(format!(
                "sem o socket do systemd (LISTEN_FDS) e sem --socket\n{USAGE}"
            ))
        }),
    };
    let server = helper::Server::new(Box::new(helper::Ebpf::new()));
    if let Err(e) = helper::accept_loop(server, listener) {
        fail(format!("accept: {e}"));
    }
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("sandmux-helper: {msg}");
    exit(2);
}

/// O socket passado pelo systemd (o descritor 3), se `LISTEN_PID` é este
/// processo e `LISTEN_FDS` é 1.
fn systemd_socket() -> Option<OwnedFd> {
    let pid: u32 = std::env::var("LISTEN_PID").ok()?.parse().ok()?;
    let n: u32 = std::env::var("LISTEN_FDS").ok()?.parse().ok()?;
    if pid != std::process::id() || n != 1 {
        return None;
    }
    // SAFETY: o systemd entrega o socket no descritor 3 e não o usa mais.
    let fd = unsafe { OwnedFd::from_raw_fd(3) };
    // SAFETY: liga o close-on-exec do descritor que agora é nosso.
    unsafe { libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC) };
    Some(fd)
}

/// Cada arquivo marcado fica aberto: sobe o teto de descritores ao máximo.
fn raise_nofile() {
    // SAFETY: getrlimit/setrlimit com struct válida.
    unsafe {
        let mut r: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) == 0 {
            r.rlim_cur = r.rlim_max;
            libc::setrlimit(libc::RLIMIT_NOFILE, &r);
        }
    }
}
