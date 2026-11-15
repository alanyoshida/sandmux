//! Compositor do Wayland falso: anuncia uma lista de globais, responde ao
//! `sync` e, quando anuncia o `wp_security_context_manager_v1`, recebe o
//! socket de escuta do `create_listener` e atende nele os clientes do
//! sandbox, com os mesmos globais.
#![allow(dead_code)]

use std::io::{IoSliceMut, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nix::sys::socket::{ControlMessageOwned, MsgFlags, recvmsg};

pub const MANAGER: &str = "wp_security_context_manager_v1";

#[derive(Default)]
pub struct Seen {
    /// O motor, o app_id e o identificador recebidos.
    pub engine: Option<String>,
    pub app_id: Option<String>,
    pub instance: Option<String>,
    /// Clientes atendidos no socket do security-context.
    pub sandboxed: usize,
}

pub struct Fake {
    pub path: PathBuf,
    pub seen: Arc<Mutex<Seen>>,
}

fn msg(object: u32, opcode: u16, args: &[u8]) -> Vec<u8> {
    let size = 8 + args.len() as u32;
    let mut v = object.to_ne_bytes().to_vec();
    v.extend((size << 16 | opcode as u32).to_ne_bytes());
    v.extend_from_slice(args);
    v
}

fn string(s: &str) -> Vec<u8> {
    let mut v = ((s.len() + 1) as u32).to_ne_bytes().to_vec();
    v.extend(s.as_bytes());
    v.push(0);
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
    v
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(b[at..at + 4].try_into().unwrap())
}

fn str_at(b: &[u8], at: usize) -> (String, usize) {
    let len = u32_at(b, at) as usize;
    let s = String::from_utf8_lossy(&b[at + 4..at + 4 + len - 1]).into_owned();
    (s, at + 4 + len.div_ceil(4) * 4)
}

fn client(mut s: UnixStream, globals: Arc<Vec<String>>, seen: Arc<Mutex<Seen>>, sandboxed: bool) {
    if sandboxed {
        seen.lock().unwrap().sandboxed += 1;
    }
    let mut buf = Vec::new();
    let mut fds: Vec<OwnedFd> = Vec::new();
    let mut manager = None;
    let mut context = None;
    let mut registries: Vec<u32> = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let mut cmsg = nix::cmsg_space!([RawFd; 8]);
        let mut iov = [IoSliceMut::new(&mut chunk)];
        let n = match recvmsg::<()>(s.as_raw_fd(), &mut iov, Some(&mut cmsg), MsgFlags::empty()) {
            Ok(r) => {
                for c in r.cmsgs().unwrap() {
                    if let ControlMessageOwned::ScmRights(v) = c {
                        fds.extend(v.into_iter().map(|fd| unsafe { OwnedFd::from_raw_fd(fd) }));
                    }
                }
                r.bytes
            }
            Err(_) => return,
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        while buf.len() >= 8 {
            let size = (u32_at(&buf, 4) >> 16) as usize;
            if buf.len() < size {
                break;
            }
            let m: Vec<u8> = buf.drain(..size).collect();
            let (obj, op, a) = (u32_at(&m, 0), (u32_at(&m, 4) & 0xffff) as u16, &m[8..]);
            let mut out = Vec::new();
            match (obj, op) {
                (1, 0) => {
                    // sync: done e delete_id.
                    let cb = u32_at(a, 0);
                    out.extend(msg(cb, 0, &0u32.to_ne_bytes()));
                    out.extend(msg(1, 1, &cb.to_ne_bytes()));
                }
                (1, 1) => {
                    let reg = u32_at(a, 0);
                    registries.push(reg);
                    for (i, g) in globals.iter().enumerate() {
                        let mut args = (i as u32 + 1).to_ne_bytes().to_vec();
                        args.extend(string(g));
                        args.extend(1u32.to_ne_bytes());
                        out.extend(msg(reg, 0, &args));
                    }
                }
                (o, 0) if registries.contains(&o) => {
                    // bind no registro (o primeiro id livre do cliente).
                    let (iface, next) = str_at(a, 4);
                    if iface == MANAGER {
                        manager = Some(u32_at(a, next + 4));
                    }
                }
                (o, 1) if Some(o) == manager => {
                    context = Some(u32_at(a, 0));
                    // listen_fd e close_fd, nesta ordem.
                    let listen = fds.remove(0);
                    let close = fds.remove(0);
                    let g = globals.clone();
                    let sn = seen.clone();
                    std::thread::spawn(move || {
                        let _close = close;
                        let l = unsafe { UnixListener::from_raw_fd(listen.as_raw_fd()) };
                        std::mem::forget(listen);
                        for c in l.incoming().flatten() {
                            let (g, sn) = (g.clone(), sn.clone());
                            std::thread::spawn(move || client(c, g, sn, true));
                        }
                    });
                }
                (o, op) if Some(o) == context => {
                    let mut st = seen.lock().unwrap();
                    match op {
                        1 => st.engine = Some(str_at(a, 0).0),
                        2 => st.app_id = Some(str_at(a, 0).0),
                        3 => st.instance = Some(str_at(a, 0).0),
                        _ => {}
                    }
                }
                _ => {}
            }
            if !out.is_empty() && s.write_all(&out).is_err() {
                return;
            }
        }
    }
}

impl Fake {
    /// Sobe o compositor em `dir/wayland-falso`, com `globals` (o
    /// `wp_security_context_manager_v1` entra quando está na lista).
    pub fn start(dir: &Path, globals: &[&str]) -> Fake {
        let path = dir.join("wayland-falso");
        let l = UnixListener::bind(&path).unwrap();
        let globals = Arc::new(globals.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let sn = seen.clone();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let (g, sn) = (globals.clone(), sn.clone());
                std::thread::spawn(move || client(c, g, sn, false));
            }
        });
        Fake { path, seen }
    }
}

/// Cliente mínimo em Python, rodado dentro do sandbox: lista os globais que
/// o registro anuncia.
pub const LIST_GLOBALS: &str = r#"
import os, socket, struct
p = os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"])
s = socket.socket(socket.AF_UNIX); s.connect(p)
def m(o, op, a): return struct.pack("=II", o, (8 + len(a)) << 16 | op) + a
s.sendall(m(1, 1, struct.pack("=I", 2)) + m(1, 0, struct.pack("=I", 3)))
buf = b""; names = []
while True:
    d = s.recv(4096)
    if not d: break
    buf += d
    done = False
    while len(buf) >= 8:
        o, w = struct.unpack("=II", buf[:8]); n = w >> 16
        if len(buf) < n: break
        a = buf[8:n]; buf = buf[n:]
        if o == 2 and (w & 0xffff) == 0:
            l = struct.unpack("=I", a[4:8])[0]; names.append(a[8:8 + l - 1].decode())
        if o == 3: done = True
    if done: break
print("globais:" + ",".join(names) + ":fim")
"#;
