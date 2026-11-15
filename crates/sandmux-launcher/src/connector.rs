//! Auxiliar de conexão: um processo que entra nos namespaces de usuário e de
//! rede do sandbox e abre, a pedido do supervisor, conexões TCP (`C`) e
//! ouvintes (`L`, as portas do host) no `127.0.0.1` de lá, devolvendo o
//! descritor por SCM_RIGHTS. Cada pedido tem 3 bytes: a operação e a porta.
//!
//! O supervisor não entra direto no namespace de rede do sandbox: o `setns`
//! pede `CAP_SYS_ADMIN` também no namespace de usuário de quem chama. Entrando
//! antes no namespace de usuário do sandbox, de que é dono, o auxiliar tem a
//! capability lá. E `setns` num namespace de usuário exige um processo de uma
//! só thread, por isso o auxiliar nasce antes do gateway.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, OwnedFd};
use std::time::Duration;

use nix::unistd::Pid;

use crate::net;

fn setns(f: &std::fs::File, kind: libc::c_int) -> io::Result<()> {
    // SAFETY: descritor válido de um namespace.
    if unsafe { libc::setns(f.as_raw_fd(), kind) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Responde um pedido: a operação com o descritor, ou `E` sem nada.
fn answer(sock: &OwnedFd, op: u8, fd: Option<&dyn AsRawFd>) {
    match fd {
        Some(s) => {
            let _ = net::send_fd_tagged(sock, &s.as_raw_fd(), op);
        }
        None => {
            let b = *b"E";
            // SAFETY: escrita de um byte.
            unsafe { libc::write(sock.as_raw_fd(), b.as_ptr().cast(), 1) };
        }
    }
}

/// Cria o auxiliar para o sandbox cujo processo 1 é `child`. Devolve o lado
/// do supervisor do par de sockets.
///
/// # Safety
/// O processo precisa ter uma única thread (`fork`).
pub unsafe fn spawn(child: Pid) -> io::Result<OwnedFd> {
    // Abertos aqui, antes de o processo 1 desligar o PR_SET_DUMPABLE: depois
    // disso o /proc dele não entrega os namespaces.
    let pid = child.as_raw();
    let user = std::fs::File::open(format!("/proc/{pid}/ns/user"))?;
    let netns = std::fs::File::open(format!("/proc/{pid}/ns/net"))?;
    let (mine, theirs) = net::pair()?;
    // SAFETY: o chamador garante uma única thread.
    match unsafe { nix::unistd::fork() }.map_err(io::Error::from)? {
        nix::unistd::ForkResult::Parent { .. } => {
            drop(theirs);
            Ok(mine)
        }
        nix::unistd::ForkResult::Child => {
            drop(mine);
            // SAFETY: prctl com argumentos numéricos.
            unsafe {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0);
                libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
            }
            let ok =
                setns(&user, libc::CLONE_NEWUSER).and_then(|_| setns(&netns, libc::CLONE_NEWNET));
            if ok.is_err() {
                // SAFETY: saída imediata do auxiliar.
                unsafe { libc::_exit(1) };
            }
            let mut req = [0u8; 3];
            loop {
                // SAFETY: leitura num buffer válido.
                let n = unsafe { libc::read(theirs.as_raw_fd(), req.as_mut_ptr().cast(), 3) };
                if n != 3 {
                    // SAFETY: fim do supervisor ou pedido inválido.
                    unsafe { libc::_exit(0) };
                }
                let port = u16::from_be_bytes([req[1], req[2]]);
                let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
                match req[0] {
                    b'L' => {
                        let l = TcpListener::bind(addr).ok();
                        answer(&theirs, b'L', l.as_ref().map(|l| l as &dyn AsRawFd));
                    }
                    _ => {
                        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok();
                        answer(&theirs, b'C', s.as_ref().map(|s| s as &dyn AsRawFd));
                    }
                }
            }
        }
    }
}
