//! Auxiliar de montagem: monta e desmonta pastas do host em `/mnt/sandmux`
//! no sandbox em execução, a pedido do supervisor.
//!
//! Ele entra no user namespace do sandbox (fica com as capabilities de lá) e
//! cria um mount namespace próprio, uma cópia da árvore do host que pertence a
//! esse user namespace: ali o `open_tree` vê a pasta do host e pode copiá-la.
//! A árvore copiada é ligada no mount namespace do sandbox com `move_mount`.

use std::io;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::Pid;
use sandmux_spec::{FOLDERS_DIR, LiveFolder, folders_text};
use serde::{Deserialize, Serialize};

use crate::{net, sys};

/// Um pedido ao auxiliar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Add {
        folder: LiveFolder,
        all: Vec<LiveFolder>,
    },
    Remove {
        name: String,
        all: Vec<LiveFolder>,
    },
}

/// A resposta: `None` deu certo, ou a mensagem do erro.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub error: Option<String>,
}

fn setns(f: &impl AsRawFd, kind: libc::c_int) -> io::Result<()> {
    // SAFETY: descritor válido de um namespace.
    if unsafe { libc::setns(f.as_raw_fd(), kind) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn open_path(p: &Path, dir: bool) -> io::Result<OwnedFd> {
    let mut flags = libc::O_PATH | libc::O_NOFOLLOW;
    if dir {
        flags |= libc::O_DIRECTORY;
    }
    Ok(std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(p)?
        .into())
}

fn ctx(what: &str) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// Regrava o `PASTAS.txt`: desmonta a cópia em somente leitura, grava o
/// arquivo do tmpfs e monta de novo em somente leitura por cima dele.
fn write_list(all: &[LiveFolder]) -> Result<(), String> {
    let txt = Path::new(FOLDERS_DIR).join("PASTAS.txt");
    let c = sys::cstr(&txt).map_err(ctx("PASTAS.txt"))?;
    // SAFETY: string C válida.
    while unsafe { libc::umount2(c.as_ptr(), libc::MNT_DETACH) } == 0 {}
    std::fs::write(&txt, folders_text(all)).map_err(ctx(sandmux_spec::tr!(
        "gravar PASTAS.txt",
        "write PASTAS.txt"
    )))?;
    let f = open_path(&txt, false).map_err(ctx("PASTAS.txt"))?;
    let tree = sys::open_tree_clone(f.as_fd(), false).map_err(ctx(sandmux_spec::tr!(
        "copiar PASTAS.txt",
        "copy PASTAS.txt"
    )))?;
    sys::mount_setattr(tree.as_fd(), false, sys::MOUNT_ATTR_RDONLY, 0).map_err(ctx(
        sandmux_spec::tr!("PASTAS.txt em somente leitura", "PASTAS.txt read-only"),
    ))?;
    sys::move_mount(tree.as_fd(), f.as_fd()).map_err(ctx(sandmux_spec::tr!(
        "montar PASTAS.txt",
        "mount PASTAS.txt"
    )))
}

struct State {
    own: OwnedFd,
    sandbox: std::fs::File,
}

impl State {
    /// Roda `f` dentro do mount namespace do sandbox e volta para o próprio.
    fn inside<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        setns(&self.sandbox, libc::CLONE_NEWNS).map_err(ctx(sandmux_spec::tr!(
            "entrar no sandbox",
            "enter the sandbox"
        )))?;
        let r = f();
        setns(&self.own, libc::CLONE_NEWNS).map_err(ctx(sandmux_spec::tr!("voltar", "go back")))?;
        r
    }

    fn add(&self, folder: &LiveFolder, all: &[LiveFolder]) -> Result<(), String> {
        let src = open_path(&folder.path, true)
            .map_err(ctx(sandmux_spec::tr!("abrir a pasta", "open the folder")))?;
        let tree = sys::open_tree_clone(src.as_fd(), true)
            .map_err(ctx(sandmux_spec::tr!("copiar a pasta", "copy the folder")))?;
        let mut set = sys::MOUNT_ATTR_NOSUID | sys::MOUNT_ATTR_NODEV;
        if !folder.rw {
            set |= sys::MOUNT_ATTR_RDONLY;
        }
        sys::mount_setattr(tree.as_fd(), true, set, 0).map_err(ctx(sandmux_spec::tr!(
            "atributos da pasta",
            "folder attributes"
        )))?;
        self.inside(|| {
            let dest = folder.dest();
            match std::fs::create_dir(&dest) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(sandmux_spec::trf!(
                        "criar {}: {e}",
                        "create {}: {e}",
                        dest.display()
                    ));
                }
            }
            let to = open_path(&dest, true).map_err(ctx(sandmux_spec::tr!(
                "abrir o destino",
                "open the destination"
            )))?;
            sys::move_mount(tree.as_fd(), to.as_fd())
                .map_err(ctx(sandmux_spec::tr!("montar", "mount")))?;
            write_list(all)
        })
    }

    fn remove(&self, name: &str, all: &[LiveFolder]) -> Result<(), String> {
        self.inside(|| {
            let dest: PathBuf = Path::new(FOLDERS_DIR).join(name);
            let c = sys::cstr(&dest).map_err(ctx(sandmux_spec::tr!("destino", "destination")))?;
            // SAFETY: string C válida.
            if unsafe { libc::umount2(c.as_ptr(), libc::MNT_DETACH) } != 0 {
                return Err(sandmux_spec::trf!(
                    "desmontar: {}",
                    "unmount: {}",
                    io::Error::last_os_error()
                ));
            }
            let _ = std::fs::remove_dir(&dest);
            write_list(all)
        })
    }
}

fn serve(sock: &OwnedFd, st: &State) -> ! {
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        // SAFETY: leitura num buffer válido.
        let n = unsafe { libc::read(sock.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            // SAFETY: o supervisor fechou o par.
            unsafe { libc::_exit(0) };
        }
        let reply = match serde_json::from_slice::<Request>(&buf[..n as usize]) {
            Ok(Request::Add { folder, all }) => st.add(&folder, &all),
            Ok(Request::Remove { name, all }) => st.remove(&name, &all),
            Err(e) => Err(sandmux_spec::trf!(
                "pedido inválido: {e}",
                "invalid request: {e}"
            )),
        };
        let out = serde_json::to_vec(&Reply { error: reply.err() }).unwrap_or_default();
        // SAFETY: escrita de um buffer válido.
        unsafe { libc::write(sock.as_raw_fd(), out.as_ptr().cast(), out.len()) };
    }
}

/// Lado do supervisor.
pub struct Mounter {
    sock: std::sync::Mutex<OwnedFd>,
}

impl Mounter {
    pub fn call(&self, req: &Request) -> Result<(), String> {
        let sock = self.sock.lock().unwrap_or_else(|e| e.into_inner());
        let data = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        // SAFETY: escrita de um buffer válido.
        if unsafe { libc::write(sock.as_raw_fd(), data.as_ptr().cast(), data.len()) } < 0 {
            return Err(sandmux_spec::trf!(
                "auxiliar de montagem: {}",
                "mount helper: {}",
                io::Error::last_os_error()
            ));
        }
        let mut buf = vec![0u8; 64 * 1024];
        // SAFETY: leitura num buffer válido.
        let n = unsafe { libc::read(sock.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            return Err(sandmux_spec::tr!(
                "o auxiliar de montagem parou",
                "the mount helper stopped"
            )
            .into());
        }
        let r: Reply = serde_json::from_slice(&buf[..n as usize]).map_err(|e| e.to_string())?;
        r.error.map_or(Ok(()), Err)
    }
}

/// Cria o auxiliar para o sandbox cujo processo 1 é `child`.
///
/// # Safety
/// O processo precisa ter uma única thread (`fork`).
pub unsafe fn spawn(child: Pid) -> io::Result<Mounter> {
    let pid = child.as_raw();
    let user = std::fs::File::open(format!("/proc/{pid}/ns/user"))?;
    let mnt = std::fs::File::open(format!("/proc/{pid}/ns/mnt"))?;
    let (mine, theirs) = net::pair()?;
    // SAFETY: o chamador garante uma única thread.
    match unsafe { nix::unistd::fork() }.map_err(io::Error::from)? {
        nix::unistd::ForkResult::Parent { .. } => {
            drop(theirs);
            Ok(Mounter {
                sock: std::sync::Mutex::new(mine),
            })
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
            if setns(&user, libc::CLONE_NEWUSER).is_err() {
                fail();
            }
            // SAFETY: unshare do mount namespace, com uma só thread.
            if unsafe { libc::unshare(libc::CLONE_NEWNS) } != 0 {
                fail();
            }
            let Ok(own) = std::fs::File::open("/proc/self/ns/mnt") else {
                fail()
            };
            drop(user);
            serve(
                &theirs,
                &State {
                    own: own.into(),
                    sandbox: mnt,
                },
            )
        }
    }
}
