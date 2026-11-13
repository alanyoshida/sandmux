//! Montagem da raiz nova do sandbox.
//!
//! Roda no processo 1 do sandbox, dentro dos namespaces novos e ainda com
//! capabilities no user namespace. A sequência segue o desenho:
//!
//! 1. propagação escrava, para nada do sandbox voltar ao host;
//! 2. tmpfs de trabalho em `/tmp` e `pivot_root` para ele, com o host em
//!    `/oldroot`;
//! 3. tmpfs em `/newroot` e cada montagem da especificação, em ordem;
//! 4. `pivot_root` para `/newroot`, desanexando o host e o tmpfs de trabalho;
//! 5. raiz em somente leitura.
//!
//! Os pontos de montagem são abertos componente a componente com
//! `RESOLVE_NO_SYMLINKS`: partes da árvore (o HOME sintético, a pasta de
//! trabalho) são graváveis pelo agente entre uma execução e outra, e um link
//! simbólico plantado ali não pode desviar uma montagem.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use nix::mount::{MntFlags, MsFlags, mount, umount2};
use nix::sys::stat::{Mode, SFlag, fstat, mkdirat};
use nix::unistd::{chdir, fchdir, symlinkat};
use sandmux_spec::{LaunchSpec, Layer, Mount};

use crate::sys::{
    self, FsContext, MOUNT_ATTR_NODEV, MOUNT_ATTR_NOEXEC, MOUNT_ATTR_NOSUID, MOUNT_ATTR_RDONLY,
    RESOLVE_IN_ROOT, RESOLVE_NO_MAGICLINKS, RESOLVE_NO_SYMLINKS,
};

const BASE: &str = "/tmp";
const NEWROOT: &str = "/newroot";
const OLDROOT: &str = "/oldroot";

/// Erro de montagem com o contexto do passo.
#[derive(Debug)]
pub struct MountError(pub String);

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn ctx<T>(r: io::Result<T>, what: impl FnOnce() -> String) -> Result<T, MountError> {
    r.map_err(|e| MountError(format!("{}: {e}", what())))
}

fn nix_ctx<T>(r: nix::Result<T>, what: impl FnOnce() -> String) -> Result<T, MountError> {
    r.map_err(|e| MountError(format!("{}: {e}", what())))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
}

struct Builder {
    newroot: OwnedFd,
    oldroot: OwnedFd,
    counter: usize,
}

/// Monta a raiz descrita pela especificação e troca para ela.
pub fn setup_root(spec: &LaunchSpec) -> Result<(), MountError> {
    nix_ctx(
        mount(
            None::<&str>,
            "/",
            None::<&str>,
            MsFlags::MS_REC | MsFlags::MS_SLAVE,
            None::<&str>,
        ),
        || "propagação escrava na raiz".into(),
    )
    .map_err(|e| {
        MountError(format!(
            "{e} (o user namespace não tem capabilities; rode `sandmux doctor`)"
        ))
    })?;

    let base = ctx(FsContext::open("tmpfs"), || "tmpfs de trabalho".into())?;
    ctx(base.set_string("mode", "0755"), || {
        "tmpfs de trabalho".into()
    })?;
    let base = ctx(base.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV), || {
        "tmpfs de trabalho".into()
    })?;
    let base_target = ctx(open_path_plain(BASE), || format!("abrir {BASE}"))?;
    ctx(sys::move_mount(base.as_fd(), base_target.as_fd()), || {
        format!("montar tmpfs de trabalho em {BASE}")
    })?;
    drop(base_target);

    nix_ctx(chdir(BASE), || format!("entrar em {BASE}"))?;
    for d in ["newroot", "oldroot", "gen", "layers"] {
        ctx(fs::create_dir(d), || format!("criar {d}"))?;
    }
    ctx(sys::pivot_root(c".", c"oldroot"), || {
        "pivot_root para o tmpfs de trabalho".into()
    })?;
    nix_ctx(chdir("/"), || "entrar na raiz de trabalho".into())?;

    let nr = ctx(FsContext::open("tmpfs"), || "raiz nova".into())?;
    ctx(nr.set_string("mode", "0755"), || "raiz nova".into())?;
    let nr = ctx(nr.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV), || {
        "raiz nova".into()
    })?;
    let nr_target = ctx(open_path_plain(NEWROOT), || format!("abrir {NEWROOT}"))?;
    ctx(sys::move_mount(nr.as_fd(), nr_target.as_fd()), || {
        "montar a raiz nova".into()
    })?;
    drop(nr_target);

    let mut b = Builder {
        newroot: ctx(open_path_plain(NEWROOT), || format!("abrir {NEWROOT}"))?,
        oldroot: ctx(open_path_plain(OLDROOT), || format!("abrir {OLDROOT}"))?,
        counter: 0,
    };
    for m in &spec.mounts {
        b.apply(m)
            .map_err(|e| MountError(format!("montagem {}: {e}", m.dest().display())))?;
    }

    let newroot = b.newroot;
    drop(b.oldroot);
    nix_ctx(fchdir(&newroot), || "entrar na raiz nova".into())?;
    ctx(sys::pivot_root(c".", c"."), || {
        "pivot_root para a raiz nova".into()
    })?;
    nix_ctx(umount2(".", MntFlags::MNT_DETACH), || {
        "desanexar o host".into()
    })?;
    nix_ctx(chdir("/"), || "entrar na raiz".into())?;
    let root = ctx(open_path_plain("/"), || "abrir a raiz".into())?;
    ctx(
        sys::mount_setattr(root.as_fd(), false, MOUNT_ATTR_RDONLY, 0),
        || "raiz em somente leitura".into(),
    )?;
    Ok(())
}

fn open_path_plain(p: &str) -> io::Result<OwnedFd> {
    let c = sys::cstr(p)?;
    // SAFETY: string C válida.
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_PATH | libc::O_CLOEXEC | libc::O_DIRECTORY,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descritor recém-aberto.
    Ok(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) })
}

impl Builder {
    fn apply(&mut self, m: &Mount) -> Result<(), MountError> {
        match m {
            Mount::Tmpfs { dest, mode } => {
                let fsc = ctx(FsContext::open("tmpfs"), || "tmpfs".into())?;
                ctx(fsc.set_string("mode", &format!("{mode:o}")), || {
                    "modo do tmpfs".into()
                })?;
                let mnt = ctx(fsc.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV), || {
                    "tmpfs".into()
                })?;
                self.attach(mnt, dest, Kind::Dir)
            }
            Mount::Bind {
                source,
                dest,
                writable,
            } => {
                let src = self.open_source(source)?;
                let kind = kind_of(src.as_fd())?;
                let tree = ctx(sys::open_tree_clone(src.as_fd(), true), || {
                    format!("copiar a árvore de {}", source.display())
                })?;
                if !writable {
                    ctx(
                        sys::mount_setattr(tree.as_fd(), true, MOUNT_ATTR_RDONLY, 0),
                        || "somente leitura".into(),
                    )?;
                }
                self.attach(tree, dest, kind)
            }
            Mount::Device { source, dest } => {
                let src = self.open_source(source)?;
                let tree = ctx(sys::open_tree_clone(src.as_fd(), false), || {
                    format!("copiar {}", source.display())
                })?;
                self.attach(tree, dest, Kind::File)
            }
            Mount::Overlay { lower, dest, layer } => {
                let (upper, work) = match layer {
                    Layer::Persistent { upper, work } => (host_path(upper), host_path(work)),
                    Layer::Temporary => {
                        self.counter += 1;
                        let d = PathBuf::from(format!("/layers/{}", self.counter));
                        let (u, w) = (d.join("upper"), d.join("work"));
                        ctx(fs::create_dir_all(&u), || "camada temporária".into())?;
                        ctx(fs::create_dir_all(&w), || "camada temporária".into())?;
                        (u, w)
                    }
                };
                let fsc = ctx(FsContext::open("overlay"), || "overlay".into())?;
                let as_str = |p: &Path| p.to_string_lossy().into_owned();
                ctx(
                    fsc.set_string("lowerdir+", &as_str(&host_path(lower))),
                    || "camada de baixo".into(),
                )?;
                ctx(fsc.set_string("upperdir", &as_str(&upper)), || {
                    "camada de escrita".into()
                })?;
                ctx(fsc.set_string("workdir", &as_str(&work)), || {
                    "pasta de trabalho do overlay".into()
                })?;
                let mnt = ctx(fsc.mount(MOUNT_ATTR_NODEV), || "overlay".into())?;
                self.attach(mnt, dest, Kind::Dir)
            }
            Mount::Cover { dest } => {
                // Sem nada no caminho (ou só um link simbólico criado na camada
                // de escrita), não há conteúdo do host a esconder.
                let Some(target) = self.lookup_existing(dest)? else {
                    return Ok(());
                };
                let kind = kind_of(target.as_fd())?;
                let mnt = match kind {
                    Kind::Dir => {
                        let fsc = ctx(FsContext::open("tmpfs"), || "tmpfs vazio".into())?;
                        ctx(fsc.set_string("mode", "0755"), || "tmpfs vazio".into())?;
                        ctx(
                            fsc.mount(
                                MOUNT_ATTR_RDONLY
                                    | MOUNT_ATTR_NOSUID
                                    | MOUNT_ATTR_NODEV
                                    | MOUNT_ATTR_NOEXEC,
                            ),
                            || "tmpfs vazio".into(),
                        )?
                    }
                    Kind::File => self.generated_file("", 0o444)?,
                };
                ctx(sys::move_mount(mnt.as_fd(), target.as_fd()), || {
                    "cobrir".into()
                })
            }
            Mount::File {
                dest,
                contents,
                mode,
            } => {
                let mnt = self.generated_file(contents, *mode)?;
                self.attach(mnt, dest, Kind::File)
            }
            Mount::Symlink { dest, target } => {
                let (parent, name) = split(dest)?;
                let dir = self.open_dest(&parent, Some(Kind::Dir))?;
                match symlinkat(target.as_path(), dir.as_fd(), name.as_os_str()) {
                    Ok(()) => Ok(()),
                    Err(nix::errno::Errno::EEXIST) => {
                        let current = nix::fcntl::readlinkat(dir.as_fd(), name.as_os_str());
                        match current {
                            Ok(c) if Path::new(&c) == target.as_path() => Ok(()),
                            _ => Err(MountError("já existe algo diferente no caminho".into())),
                        }
                    }
                    Err(e) => Err(MountError(format!("criar link simbólico: {e}"))),
                }
            }
            Mount::Proc { dest } => {
                let fsc = ctx(FsContext::open("proc"), || "proc".into())?;
                let mnt = ctx(
                    fsc.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV | MOUNT_ATTR_NOEXEC),
                    || "proc".into(),
                )?;
                self.attach(mnt, dest, Kind::Dir)
            }
            Mount::Devpts { dest } => {
                let fsc = ctx(FsContext::open("devpts"), || "devpts".into())?;
                ctx(fsc.set_string("ptmxmode", "0666"), || "devpts".into())?;
                ctx(fsc.set_string("mode", "0620"), || "devpts".into())?;
                let mnt = ctx(fsc.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NOEXEC), || {
                    "devpts".into()
                })?;
                self.attach(mnt, dest, Kind::Dir)
            }
        }
    }

    fn attach(&self, mnt: OwnedFd, dest: &Path, kind: Kind) -> Result<(), MountError> {
        let target = self.open_dest(dest, Some(kind))?;
        ctx(sys::move_mount(mnt.as_fd(), target.as_fd()), || {
            "ligar a montagem".into()
        })
    }

    /// Abre um caminho do host (sob `/oldroot`), resolvendo links simbólicos
    /// absolutos dentro do host e não na raiz de trabalho.
    fn open_source(&self, source: &Path) -> Result<OwnedFd, MountError> {
        let rel = source.strip_prefix("/").unwrap_or(source);
        let c = ctx(sys::cstr(rel), || "caminho".into())?;
        let rel_c = if rel.as_os_str().is_empty() {
            c".".to_owned()
        } else {
            c
        };
        ctx(
            sys::openat2(
                self.oldroot.as_fd(),
                &rel_c,
                (libc::O_PATH | libc::O_CLOEXEC) as u64,
                RESOLVE_IN_ROOT | RESOLVE_NO_MAGICLINKS,
            ),
            || format!("abrir a origem {}", source.display()),
        )
    }

    /// Cria um arquivo no tmpfs de trabalho e devolve uma montagem dele em
    /// somente leitura.
    fn generated_file(&mut self, contents: &str, mode: u32) -> Result<OwnedFd, MountError> {
        self.counter += 1;
        let path = PathBuf::from(format!("/gen/{}", self.counter));
        let mut f = ctx(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path),
            || "arquivo gerado".into(),
        )?;
        ctx(f.write_all(contents.as_bytes()), || "arquivo gerado".into())?;
        ctx(
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)),
            || "arquivo gerado".into(),
        )?;
        drop(f);
        let fd = ctx(open_any(&path), || "arquivo gerado".into())?;
        let tree = ctx(sys::open_tree_clone(fd.as_fd(), false), || {
            "arquivo gerado".into()
        })?;
        ctx(
            sys::mount_setattr(tree.as_fd(), false, MOUNT_ATTR_RDONLY, 0),
            || "arquivo gerado".into(),
        )?;
        Ok(tree)
    }

    /// Abre um caminho que já existe sob a raiz nova, sem seguir links. Devolve
    /// `None` quando algum componente falta ou quando o último é um link.
    fn lookup_existing(&self, dest: &Path) -> Result<Option<OwnedFd>, MountError> {
        let comps: Vec<&OsStr> = dest
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s),
                _ => None,
            })
            .collect();
        let mut cur = ctx(open_any_at(self.newroot.as_fd(), OsStr::new(".")), || {
            "raiz nova".into()
        })?;
        for (i, name) in comps.iter().enumerate() {
            let fd = match open_nofollow_at(cur.as_fd(), name) {
                Ok(fd) => fd,
                Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
                Err(e) => return Err(MountError(e.to_string())),
            };
            let st = nix_ctx(fstat(fd.as_fd()), || "fstat".into())?;
            if st.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFLNK.bits() {
                if i + 1 == comps.len() {
                    return Ok(None);
                }
                return Err(MountError(
                    "caminho com link simbólico; montagem recusada".into(),
                ));
            }
            cur = fd;
        }
        Ok(Some(cur))
    }

    /// Abre (criando se preciso) o ponto de montagem sob a raiz nova, sem
    /// seguir links simbólicos em nenhum componente. `kind: None` exige que o
    /// caminho já exista.
    fn open_dest(&self, dest: &Path, kind: Option<Kind>) -> Result<OwnedFd, MountError> {
        let comps: Vec<&OsStr> = dest
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s),
                _ => None,
            })
            .collect();
        let mut cur = ctx(open_any_at(self.newroot.as_fd(), OsStr::new(".")), || {
            "raiz nova".into()
        })?;
        if comps.is_empty() {
            return if kind == Some(Kind::Dir) {
                Ok(cur)
            } else {
                Err(MountError("destino é a própria raiz".into()))
            };
        }
        for (i, name) in comps.iter().enumerate() {
            let last = i + 1 == comps.len();
            let want = if last { kind } else { Some(Kind::Dir) };
            cur = step(cur.as_fd(), name, want).map_err(|e| {
                let partial: PathBuf = comps[..=i].iter().collect();
                MountError(format!("/{}: {}", partial.display(), e.0))
            })?;
        }
        Ok(cur)
    }
}

fn step(dir: BorrowedFd<'_>, name: &OsStr, want: Option<Kind>) -> Result<OwnedFd, MountError> {
    for _ in 0..3 {
        match open_nofollow_at(dir, name) {
            Ok(fd) => {
                let st = nix_ctx(fstat(fd.as_fd()), || "fstat".into())?;
                let fmt = SFlag::from_bits_truncate(st.st_mode & SFlag::S_IFMT.bits());
                if fmt == SFlag::S_IFLNK {
                    return Err(MountError("é um link simbólico; montagem recusada".into()));
                }
                match want {
                    Some(Kind::Dir) if fmt != SFlag::S_IFDIR => {
                        return Err(MountError("esperava uma pasta".into()));
                    }
                    Some(Kind::File) if fmt == SFlag::S_IFDIR => {
                        return Err(MountError(
                            "esperava um arquivo, encontrou uma pasta".into(),
                        ));
                    }
                    _ => {}
                }
                return Ok(fd);
            }
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {
                let created = match want {
                    None => return Err(MountError("não existe".into())),
                    Some(Kind::Dir) => match mkdirat(dir, name, Mode::from_bits_truncate(0o755)) {
                        Ok(()) | Err(nix::errno::Errno::EEXIST) => Ok(()),
                        Err(e) => Err(io::Error::from(e)),
                    },
                    Some(Kind::File) => create_file_at(dir, name),
                };
                created.map_err(|e| MountError(format!("criar ponto de montagem: {e}")))?;
            }
            Err(e) => return Err(MountError(e.to_string())),
        }
    }
    Err(MountError("o caminho mudou durante a montagem".into()))
}

fn open_nofollow_at(dir: BorrowedFd<'_>, name: &OsStr) -> io::Result<OwnedFd> {
    let c = sys::cstr(Path::new(name))?;
    sys::openat2(
        dir,
        &c,
        (libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC) as u64,
        RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    )
}

fn open_any_at(dir: BorrowedFd<'_>, name: &OsStr) -> io::Result<OwnedFd> {
    let c = sys::cstr(Path::new(name))?;
    sys::openat2(
        dir,
        &c,
        (libc::O_PATH | libc::O_CLOEXEC) as u64,
        RESOLVE_NO_SYMLINKS,
    )
}

fn open_any(p: &Path) -> io::Result<OwnedFd> {
    let c = sys::cstr(p)?;
    // SAFETY: string C válida.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descritor recém-aberto.
    Ok(unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) })
}

fn create_file_at(dir: BorrowedFd<'_>, name: &OsStr) -> io::Result<()> {
    let c = sys::cstr(Path::new(name))?;
    // SAFETY: descritor e string válidos.
    let fd = unsafe {
        libc::openat(
            std::os::fd::AsRawFd::as_raw_fd(&dir),
            c.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o644 as libc::c_uint,
        )
    };
    if fd < 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EEXIST) {
            return Ok(());
        }
        return Err(e);
    }
    // SAFETY: descritor recém-aberto, fechado aqui.
    unsafe { libc::close(fd) };
    Ok(())
}

fn kind_of(fd: BorrowedFd<'_>) -> Result<Kind, MountError> {
    let st = nix_ctx(fstat(fd), || "fstat".into())?;
    Ok(
        if st.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits() {
            Kind::Dir
        } else {
            Kind::File
        },
    )
}

fn host_path(p: &Path) -> PathBuf {
    Path::new(OLDROOT).join(p.strip_prefix("/").unwrap_or(p))
}

fn split(dest: &Path) -> Result<(PathBuf, PathBuf), MountError> {
    let name = dest
        .file_name()
        .ok_or_else(|| MountError("caminho sem nome".into()))?;
    let parent = dest.parent().unwrap_or(Path::new("/")).to_path_buf();
    Ok((parent, PathBuf::from(OsStr::from_bytes(name.as_bytes()))))
}
