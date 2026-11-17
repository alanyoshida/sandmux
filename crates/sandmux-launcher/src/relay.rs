//! Repassador do barramento do portal. O socket do daemon entra no sandbox
//! montado de fora, e o AppArmor de um programa com perfil (o `notify-send`
//! do Ubuntu) não resolve o caminho de um socket criado fora da raiz do
//! sandbox ("disconnected path") e nega a conexão. Por isso o processo 1 cria
//! o `/run/user/<uid>/bus` dentro do sandbox, e um filho dele liga cada
//! conexão ao socket do daemon, copiando os bytes nos dois sentidos.
//!
//! O repassador roda sem capabilities, com `no_new_privs` e o mesmo filtro
//! seccomp do processo 1, e não é despejável (herda o `PR_SET_DUMPABLE` do
//! processo 1). Se o agente o matar, só o portal daquela aba para.

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use sandmux_spec::{BusRelay, SeccompRules};

use crate::{seccomp, sys};

/// Cria o socket de `r.listen` e o processo que o atende.
///
/// # Safety
/// O processo precisa ter uma única thread (`fork`).
pub unsafe fn spawn(r: &BusRelay, rules: &SeccompRules) -> io::Result<()> {
    let _ = std::fs::remove_file(&r.listen);
    let listener = UnixListener::bind(&r.listen)?;
    // SAFETY: o chamador garante uma única thread.
    match unsafe { nix::unistd::fork() }.map_err(io::Error::from)? {
        nix::unistd::ForkResult::Parent { .. } => Ok(()),
        nix::unistd::ForkResult::Child => serve(listener, &r.target, rules),
    }
}

fn serve(listener: UnixListener, target: &Path, rules: &SeccompRules) -> ! {
    let _ = sys::drop_all_capabilities();
    let _ = sys::set_no_new_privs();
    let _ = seccomp::install(rules);
    // Os filhos de cada conexão se recolhem sozinhos.
    // SAFETY: troca da disposição de um sinal.
    unsafe { libc::signal(libc::SIGCHLD, libc::SIG_IGN) };
    loop {
        match listener.accept() {
            Ok((c, _)) => {
                // SAFETY: o repassador tem uma única thread.
                match unsafe { nix::unistd::fork() } {
                    Ok(nix::unistd::ForkResult::Child) => {
                        drop(listener);
                        relay(c, target);
                        // SAFETY: saída do filho da conexão.
                        unsafe { libc::_exit(0) };
                    }
                    Ok(nix::unistd::ForkResult::Parent { .. }) => drop(c),
                    Err(_) => drop(c),
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                // SAFETY: saída do repassador.
                unsafe { libc::_exit(0) };
            }
        }
    }
}

/// Copia os bytes entre o cliente e o socket do daemon até os dois lados
/// fecharem.
fn relay(mut a: UnixStream, target: &Path) {
    let Ok(mut b) = UnixStream::connect(target) else {
        return;
    };
    let mut buf = vec![0u8; 64 * 1024];
    // Lados ainda abertos para leitura.
    let (mut a_open, mut b_open) = (true, true);
    while a_open || b_open {
        let mut fds = [
            libc::pollfd {
                fd: if a_open { a.as_raw_fd() } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if b_open { b.as_raw_fd() } else { -1 },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: vetor de pollfd válido.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if n < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if a_open && fds[0].revents != 0 && !pump(&mut a, &mut b, &mut buf, &mut a_open) {
            return;
        }
        if b_open && fds[1].revents != 0 && !pump(&mut b, &mut a, &mut buf, &mut b_open) {
            return;
        }
    }
}

/// Lê de `from` e escreve em `to`. No fim do arquivo, fecha a escrita de
/// `to` e marca `from` como fechado. Devolve falso num erro.
fn pump(from: &mut UnixStream, to: &mut UnixStream, buf: &mut [u8], open: &mut bool) -> bool {
    match from.read(buf) {
        Ok(0) => {
            *open = false;
            let _ = to.shutdown(std::net::Shutdown::Write);
            true
        }
        Ok(n) => to.write_all(&buf[..n]).is_ok(),
        Err(e) if e.kind() == io::ErrorKind::Interrupted => true,
        Err(_) => false,
    }
}
