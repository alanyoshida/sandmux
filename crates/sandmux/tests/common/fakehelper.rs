//! Um auxiliar root falso para os testes: um socket `SOCK_SEQPACKET` que
//! responde a todo pedido com um `status` da versão dada. Nenhum programa
//! BPF é carregado.
#![allow(dead_code)]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct FakeHelper {
    /// O caminho para `SANDMUX_HELPER_SOCKET`.
    pub socket: PathBuf,
    version: Arc<Mutex<String>>,
}

impl FakeHelper {
    /// Sobe o auxiliar falso num socket dentro de `dir`. Ele dura até o
    /// processo de teste sair.
    pub fn start(dir: &Path, version: &str) -> FakeHelper {
        let socket = dir.join("helper-falso.sock");
        // SAFETY: socket devolve um descritor novo ou -1.
        let fd =
            unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
        assert!(fd >= 0, "socket: {}", std::io::Error::last_os_error());
        // SAFETY: descritor recém-criado.
        let listener = unsafe { OwnedFd::from_raw_fd(fd) };
        // SAFETY: um sockaddr_un zerado é válido.
        let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = socket.as_os_str().as_encoded_bytes();
        assert!(bytes.len() < addr.sun_path.len(), "caminho longo demais");
        for (i, b) in bytes.iter().enumerate() {
            addr.sun_path[i] = *b as libc::c_char;
        }
        let len = (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t;
        // SAFETY: endereço válido do tamanho dado, num socket nosso.
        let bound = unsafe {
            libc::bind(
                listener.as_raw_fd(),
                (&addr as *const libc::sockaddr_un).cast(),
                len,
            )
        };
        assert_eq!(bound, 0, "bind: {}", std::io::Error::last_os_error());
        // SAFETY: listen num socket já ligado ao caminho.
        let listening = unsafe { libc::listen(listener.as_raw_fd(), 16) };
        assert_eq!(listening, 0, "listen: {}", std::io::Error::last_os_error());

        let version = Arc::new(Mutex::new(version.to_string()));
        let shared = version.clone();
        std::thread::spawn(move || {
            loop {
                // SAFETY: accept num socket em escuta, sem pedir o endereço.
                let c = unsafe {
                    libc::accept4(
                        listener.as_raw_fd(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        libc::SOCK_CLOEXEC,
                    )
                };
                if c < 0 {
                    break;
                }
                // SAFETY: descritor recém-aceito.
                let conn = unsafe { OwnedFd::from_raw_fd(c) };
                let version = shared.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 4096];
                    loop {
                        // SAFETY: recv num buffer nosso do tamanho dado.
                        let n = unsafe {
                            libc::recv(conn.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0)
                        };
                        if n <= 0 {
                            break;
                        }
                        let v = version.lock().unwrap().clone();
                        let reply = serde_json::json!({
                            "ok": true,
                            "version": v,
                            "lsm": true,
                            "scope": true,
                            "tab": true,
                        })
                        .to_string();
                        // SAFETY: send de um buffer nosso do tamanho dado.
                        unsafe {
                            libc::send(
                                conn.as_raw_fd(),
                                reply.as_ptr().cast(),
                                reply.len(),
                                libc::MSG_NOSIGNAL,
                            )
                        };
                    }
                });
            }
        });
        FakeHelper { socket, version }
    }

    /// A versão que o auxiliar passa a responder.
    pub fn set_version(&self, v: &str) {
        *self.version.lock().unwrap() = v.to_string();
    }
}
