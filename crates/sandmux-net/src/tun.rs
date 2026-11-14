//! O TUN do sandbox como dispositivo assíncrono: cada leitura é um pacote.

use std::os::fd::{AsRawFd, OwnedFd};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct Tun(AsyncFd<OwnedFd>);

impl Tun {
    pub fn new(fd: OwnedFd) -> std::io::Result<Tun> {
        // SAFETY: fcntl sobre um descritor válido.
        unsafe {
            let fl = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
        }
        // SAFETY: o descritor é nosso (OwnedFd) e só é fechado quando o AsyncFd
        // é solto, depois de sair do reactor.
        Ok(Tun(unsafe { AsyncFd::register(fd)? }))
    }
}

impl AsyncRead for Tun {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            let mut g = ready!(self.0.poll_read_ready(cx))?;
            let un = buf.initialize_unfilled();
            let r = g.try_io(|fd| {
                // SAFETY: leitura num buffer válido do tamanho dado.
                let n = unsafe { libc::read(fd.as_raw_fd(), un.as_mut_ptr().cast(), un.len()) };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match r {
                Ok(Ok(n)) => {
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_) => continue,
            }
        }
    }
}

impl AsyncWrite for Tun {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        loop {
            let mut g = ready!(self.0.poll_write_ready(cx))?;
            let r = g.try_io(|fd| {
                // SAFETY: escrita de um buffer válido.
                let n = unsafe { libc::write(fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match r {
                Ok(r) => return Poll::Ready(r),
                Err(_) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
