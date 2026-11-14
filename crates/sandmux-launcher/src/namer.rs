//! Auxiliar de nomes: um processo dentro dos namespaces de usuário e de
//! montagem do sandbox, sob o mesmo Landlock do comando, que faz `rename`,
//! `link` e `symlink` a pedido do supervisor.
//!
//! O supervisor decide a política, mas não faz essas chamadas ele mesmo: fora
//! do mount namespace do sandbox, o kernel não vê os pontos de montagem de lá
//! (renomear um arquivo coberto daria certo), e o Landlock não vale para ele.
//! Aqui, o kernel decide os dois como para o comando.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use nix::unistd::Pid;
use sandmux_spec::LandlockRules;

use crate::{landlock_rules, net, sys};

/// Uma operação de nome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Rename { flags: u32 },
    Link { flags: i32 },
    Symlink { target: Vec<u8> },
}

fn setns(f: &std::fs::File, kind: libc::c_int) -> io::Result<()> {
    // SAFETY: descritor válido de um namespace.
    if unsafe { libc::setns(f.as_raw_fd(), kind) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn put(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_ne_bytes());
    out.extend_from_slice(b);
}

fn take<'a>(b: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (h, t) = b.split_at_checked(4)?;
    let n = u32::from_ne_bytes(h.try_into().ok()?) as usize;
    let (v, rest) = t.split_at_checked(n)?;
    *b = rest;
    Some(v)
}

/// Codifica um pedido: operação, flags, nomes e alvo.
fn encode(op: &Op, src_name: &[u8], dst_name: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    match op {
        Op::Rename { flags } => {
            out.push(b'R');
            out.extend_from_slice(&flags.to_ne_bytes());
            put(&mut out, b"");
        }
        Op::Link { flags } => {
            out.push(b'L');
            out.extend_from_slice(&(*flags as u32).to_ne_bytes());
            put(&mut out, b"");
        }
        Op::Symlink { target } => {
            out.push(b'S');
            out.extend_from_slice(&0u32.to_ne_bytes());
            put(&mut out, target);
        }
    }
    put(&mut out, src_name);
    put(&mut out, dst_name);
    out
}

fn sendmsg(sock: RawFd, data: &[u8], fds: &[RawFd]) -> io::Result<()> {
    // SAFETY: estruturas montadas com tamanhos válidos durante a chamada.
    unsafe {
        let mut iov = libc::iovec {
            iov_base: data.as_ptr() as *mut _,
            iov_len: data.len(),
        };
        let space = libc::CMSG_SPACE((fds.len() * 4) as u32) as usize;
        let mut cbuf = vec![0u8; space.max(1)];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        if !fds.is_empty() {
            msg.msg_control = cbuf.as_mut_ptr().cast();
            msg.msg_controllen = space as _;
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN((fds.len() * 4) as u32) as _;
            std::ptr::copy_nonoverlapping(fds.as_ptr(), libc::CMSG_DATA(c).cast(), fds.len());
        }
        if libc::sendmsg(sock, &msg, libc::MSG_NOSIGNAL) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn recvmsg(sock: RawFd, buf: &mut [u8]) -> io::Result<(usize, Vec<OwnedFd>)> {
    // SAFETY: estruturas com tamanhos válidos durante a chamada.
    unsafe {
        let mut iov = libc::iovec {
            iov_base: buf.as_mut_ptr().cast(),
            iov_len: buf.len(),
        };
        let mut cbuf = [0u8; 64];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cbuf.as_mut_ptr().cast();
        msg.msg_controllen = cbuf.len() as _;
        let n = libc::recvmsg(sock, &mut msg, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut fds = Vec::new();
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let count = ((*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / 4;
                let p = libc::CMSG_DATA(c) as *const RawFd;
                for i in 0..count {
                    fds.push(OwnedFd::from_raw_fd(*p.add(i)));
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
        Ok((n as usize, fds))
    }
}

/// Lado do supervisor.
pub struct Namer {
    sock: OwnedFd,
}

impl Namer {
    /// Libera o auxiliar quando a raiz do sandbox está pronta: ele aplica o
    /// Landlock e passa a atender.
    pub fn ready(&self) {
        let _ = sendmsg(self.sock.as_raw_fd(), b"G", &[]);
    }

    /// Faz a operação com as pastas `src` (rename e link) e `dst`, abertas
    /// pelo supervisor dentro da raiz, e os últimos componentes.
    pub fn call(
        &self,
        op: &Op,
        src: Option<(&OwnedFd, &[u8])>,
        dst: (&OwnedFd, &[u8]),
    ) -> io::Result<()> {
        let mut fds = Vec::new();
        if let Some((f, _)) = src {
            fds.push(f.as_raw_fd());
        }
        fds.push(dst.0.as_raw_fd());
        let data = encode(op, src.map(|(_, n)| n).unwrap_or(b""), dst.1);
        sendmsg(self.sock.as_raw_fd(), &data, &fds)?;
        let mut r = [0u8; 4];
        let (n, _) = recvmsg(self.sock.as_raw_fd(), &mut r)?;
        if n != 4 {
            return Err(io::Error::from_raw_os_error(libc::EIO));
        }
        match i32::from_ne_bytes(r) {
            0 => Ok(()),
            e => Err(io::Error::from_raw_os_error(e)),
        }
    }
}

fn serve(sock: &OwnedFd) -> ! {
    let mut buf = vec![0u8; 3 * 4096 + 64];
    loop {
        let Ok((n, fds)) = recvmsg(sock.as_raw_fd(), &mut buf) else {
            // SAFETY: fim do auxiliar.
            unsafe { libc::_exit(0) };
        };
        if n == 0 {
            // SAFETY: o supervisor fechou o par.
            unsafe { libc::_exit(0) };
        }
        let err = run_one(&buf[..n], &fds)
            .err()
            .map_or(0, |e| e.raw_os_error().unwrap_or(libc::EIO));
        let _ = sendmsg(sock.as_raw_fd(), &err.to_ne_bytes(), &[]);
    }
}

fn run_one(msg: &[u8], fds: &[OwnedFd]) -> io::Result<()> {
    let inval = || io::Error::from_raw_os_error(libc::EINVAL);
    let (&kind, rest) = msg.split_first().ok_or_else(inval)?;
    let (fl, mut rest) = rest.split_at_checked(4).ok_or_else(inval)?;
    let flags = u32::from_ne_bytes(fl.try_into().map_err(|_| inval())?);
    let target = take(&mut rest).ok_or_else(inval)?;
    let src = take(&mut rest).ok_or_else(inval)?;
    let dst = take(&mut rest).ok_or_else(inval)?;
    let c = |v: &[u8]| CString::new(v).map_err(|_| inval());
    let (dst_name, target) = (c(dst)?, c(target)?);
    // SAFETY: descritores e strings C válidos durante as chamadas.
    let r = unsafe {
        match (kind, fds) {
            (b'R', [s, d]) => {
                let src = c(src)?;
                libc::syscall(
                    libc::SYS_renameat2,
                    s.as_raw_fd(),
                    src.as_ptr(),
                    d.as_raw_fd(),
                    dst_name.as_ptr(),
                    flags,
                )
            }
            (b'L', [s, d]) => {
                let src = c(src)?;
                libc::syscall(
                    libc::SYS_linkat,
                    s.as_raw_fd(),
                    src.as_ptr(),
                    d.as_raw_fd(),
                    dst_name.as_ptr(),
                    flags as i32 & libc::AT_SYMLINK_FOLLOW,
                )
            }
            (b'S', [d]) => libc::syscall(
                libc::SYS_symlinkat,
                target.as_ptr(),
                d.as_raw_fd(),
                dst_name.as_ptr(),
            ),
            _ => return Err(inval()),
        }
    };
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Cria o auxiliar para o sandbox cujo processo 1 é `child`.
///
/// # Safety
/// O processo precisa ter uma única thread (`fork`).
pub unsafe fn spawn(child: Pid, rules: &LandlockRules) -> io::Result<Namer> {
    // Abertos antes de o processo 1 desligar o PR_SET_DUMPABLE.
    let pid = child.as_raw();
    let user = std::fs::File::open(format!("/proc/{pid}/ns/user"))?;
    let mnt = std::fs::File::open(format!("/proc/{pid}/ns/mnt"))?;
    let (mine, theirs) = net::pair()?;
    // SAFETY: o chamador garante uma única thread.
    match unsafe { nix::unistd::fork() }.map_err(io::Error::from)? {
        nix::unistd::ForkResult::Parent { .. } => {
            drop(theirs);
            Ok(Namer { sock: mine })
        }
        nix::unistd::ForkResult::Child => {
            drop(mine);
            // SAFETY: prctl com argumentos numéricos.
            unsafe {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0);
                libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
            }
            let fail = || -> ! {
                // SAFETY: saída imediata do auxiliar.
                unsafe { libc::_exit(1) }
            };
            if setns(&user, libc::CLONE_NEWUSER)
                .and_then(|_| setns(&mnt, libc::CLONE_NEWNS))
                .is_err()
            {
                fail();
            }
            drop((user, mnt));
            // Espera a raiz do sandbox: o `pivot_root` do processo 1 troca a
            // raiz deste processo também.
            let mut b = [0u8; 1];
            if !matches!(recvmsg(theirs.as_raw_fd(), &mut b), Ok((1, _))) {
                fail();
            }
            if nix::unistd::chdir("/").is_err()
                || sys::drop_all_capabilities().is_err()
                || sys::set_no_new_privs().is_err()
                || landlock_rules::apply(rules).is_err()
            {
                fail();
            }
            serve(&theirs)
        }
    }
}
