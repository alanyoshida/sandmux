//! Invólucros finos sobre chamadas de sistema que a crate `libc` só expõe como
//! números: a API nova de montagem, `openat2`, `pivot_root`, `clone` e o
//! descarte de capabilities.

use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

pub const OPEN_TREE_CLONE: libc::c_uint = 1;
pub const AT_RECURSIVE: libc::c_uint = 0x8000;
pub const MOVE_MOUNT_F_EMPTY_PATH: libc::c_uint = 0x4;
pub const MOVE_MOUNT_T_EMPTY_PATH: libc::c_uint = 0x40;
pub const FSOPEN_CLOEXEC: libc::c_uint = 1;
pub const FSMOUNT_CLOEXEC: libc::c_uint = 1;
const FSCONFIG_SET_STRING: libc::c_uint = 1;
const FSCONFIG_CMD_CREATE: libc::c_uint = 6;

pub const MOUNT_ATTR_RDONLY: u64 = 0x1;
pub const MOUNT_ATTR_NOSUID: u64 = 0x2;
pub const MOUNT_ATTR_NODEV: u64 = 0x4;
pub const MOUNT_ATTR_NOEXEC: u64 = 0x8;

pub const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
pub const RESOLVE_NO_SYMLINKS: u64 = 0x04;
pub const RESOLVE_IN_ROOT: u64 = 0x10;

#[repr(C)]
struct MountAttr {
    attr_set: u64,
    attr_clr: u64,
    propagation: u64,
    userns_fd: u64,
}

fn check(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

fn owned(ret: libc::c_long) -> io::Result<OwnedFd> {
    let fd = check(ret)? as RawFd;
    // SAFETY: a chamada acabou de devolver este descritor e ninguém mais o possui.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Converte um caminho em `CString`.
pub fn cstr(p: impl AsRef<Path>) -> io::Result<CString> {
    CString::new(p.as_ref().as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "caminho com byte nulo"))
}

const EMPTY: &CStr = c"";

/// Copia uma árvore de montagens (`OPEN_TREE_CLONE`) a partir de um descritor.
pub fn open_tree_clone(fd: BorrowedFd<'_>, recursive: bool) -> io::Result<OwnedFd> {
    let mut flags =
        OPEN_TREE_CLONE | libc::O_CLOEXEC as libc::c_uint | libc::AT_EMPTY_PATH as libc::c_uint;
    if recursive {
        flags |= AT_RECURSIVE;
    }
    // SAFETY: argumentos válidos; o caminho vazio é uma string C constante.
    owned(unsafe { libc::syscall(libc::SYS_open_tree, fd.as_raw_fd(), EMPTY.as_ptr(), flags) })
}

/// Liga uma montagem desanexada ao ponto indicado por um descritor `O_PATH`.
pub fn move_mount(from: BorrowedFd<'_>, to: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: os dois descritores são válidos durante a chamada.
    check(unsafe {
        libc::syscall(
            libc::SYS_move_mount,
            from.as_raw_fd(),
            EMPTY.as_ptr(),
            to.as_raw_fd(),
            EMPTY.as_ptr(),
            MOVE_MOUNT_F_EMPTY_PATH | MOVE_MOUNT_T_EMPTY_PATH,
        )
    })
    .map(|_| ())
}

/// Altera atributos de uma montagem (por exemplo, somente leitura).
pub fn mount_setattr(fd: BorrowedFd<'_>, recursive: bool, set: u64, clr: u64) -> io::Result<()> {
    let attr = MountAttr {
        attr_set: set,
        attr_clr: clr,
        propagation: 0,
        userns_fd: 0,
    };
    let mut flags = libc::AT_EMPTY_PATH as libc::c_uint;
    if recursive {
        flags |= AT_RECURSIVE;
    }
    // SAFETY: `attr` vive durante a chamada e o tamanho informado é o da estrutura.
    check(unsafe {
        libc::syscall(
            libc::SYS_mount_setattr,
            fd.as_raw_fd(),
            EMPTY.as_ptr(),
            flags,
            &attr as *const MountAttr,
            std::mem::size_of::<MountAttr>(),
        )
    })
    .map(|_| ())
}

/// Contexto de criação de um sistema de arquivos (`fsopen`).
pub struct FsContext {
    fd: OwnedFd,
    fstype: String,
}

impl FsContext {
    pub fn open(fstype: &str) -> io::Result<Self> {
        let name = cstr(fstype)?;
        // SAFETY: `name` é uma string C válida.
        let fd = owned(unsafe { libc::syscall(libc::SYS_fsopen, name.as_ptr(), FSOPEN_CLOEXEC) })?;
        Ok(Self {
            fd,
            fstype: fstype.to_string(),
        })
    }

    pub fn set_string(&self, key: &str, value: &str) -> io::Result<()> {
        let k = cstr(key)?;
        let v = cstr(value)?;
        // SAFETY: chave e valor são strings C válidas durante a chamada.
        check(unsafe {
            libc::syscall(
                libc::SYS_fsconfig,
                self.fd.as_raw_fd(),
                FSCONFIG_SET_STRING,
                k.as_ptr(),
                v.as_ptr(),
                0,
            )
        })
        .map(|_| ())
        .map_err(|e| self.annotate(e, &format!("{key}={value}")))
    }

    /// Cria o sistema de arquivos e devolve uma montagem desanexada.
    pub fn mount(self, attrs: u64) -> io::Result<OwnedFd> {
        // SAFETY: descritor válido; os demais argumentos são números.
        check(unsafe {
            libc::syscall(
                libc::SYS_fsconfig,
                self.fd.as_raw_fd(),
                FSCONFIG_CMD_CREATE,
                std::ptr::null::<libc::c_char>(),
                std::ptr::null::<libc::c_char>(),
                0,
            )
        })
        .map_err(|e| self.annotate(e, "create"))?;
        // SAFETY: descritor válido.
        owned(unsafe {
            libc::syscall(
                libc::SYS_fsmount,
                self.fd.as_raw_fd(),
                FSMOUNT_CLOEXEC,
                attrs,
            )
        })
        .map_err(|e| self.annotate(e, "fsmount"))
    }

    /// Junta ao erro as mensagens que o kernel deixa no contexto.
    fn annotate(&self, err: io::Error, step: &str) -> io::Error {
        let mut log = String::new();
        let mut buf = [0u8; 512];
        loop {
            // SAFETY: `buf` é válido para escrita no tamanho informado.
            let n = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            let line = String::from_utf8_lossy(&buf[..n as usize]);
            if !log.is_empty() {
                log.push_str("; ");
            }
            log.push_str(line.trim());
        }
        let detail = if log.is_empty() {
            String::new()
        } else {
            format!(" ({log})")
        };
        io::Error::new(
            err.kind(),
            format!("{} [{step}]: {err}{detail}", self.fstype),
        )
    }
}

/// `openat2` com controle de resolução de caminho.
pub fn openat2(
    dirfd: BorrowedFd<'_>,
    path: &CStr,
    flags: u64,
    resolve: u64,
) -> io::Result<OwnedFd> {
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = OpenHow {
        flags,
        mode: 0,
        resolve,
    };
    // SAFETY: `how` vive durante a chamada; `path` é uma string C válida.
    owned(unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dirfd.as_raw_fd(),
            path.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    })
}

pub fn pivot_root(new_root: &CStr, put_old: &CStr) -> io::Result<()> {
    // SAFETY: strings C válidas.
    check(unsafe { libc::syscall(libc::SYS_pivot_root, new_root.as_ptr(), put_old.as_ptr()) })
        .map(|_| ())
}

/// `clone` com semântica de `fork` (sem pilha nova), com flags de namespace.
/// Devolve o pid do filho no pai e `0` no filho.
///
/// # Safety
/// O processo precisa ter uma única thread, como em qualquer `fork`.
pub unsafe fn clone_fork(flags: libc::c_int) -> io::Result<libc::pid_t> {
    // SAFETY: sem pilha nova o filho continua na cópia da pilha do pai, como no fork.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_clone,
            (flags | libc::SIGCHLD) as libc::c_ulong,
            0usize,
            0usize,
            0usize,
            0usize,
        )
    };
    check(ret).map(|p| p as libc::pid_t)
}

/// Marca todos os descritores a partir de `first` para fechar no `exec`.
pub fn cloexec_from(first: libc::c_uint) -> io::Result<()> {
    const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
    // SAFETY: só números.
    check(unsafe {
        libc::syscall(
            libc::SYS_close_range,
            first,
            libc::c_uint::MAX,
            CLOSE_RANGE_CLOEXEC,
        )
    })
    .map(|_| ())
}

/// Descarta todas as capabilities: conjunto limite, ambiente e os três conjuntos
/// do processo.
pub fn drop_all_capabilities() -> io::Result<()> {
    let last = std::fs::read_to_string("/proc/sys/kernel/cap_last_cap")
        .ok()
        .and_then(|s| s.trim().parse::<libc::c_ulong>().ok())
        .unwrap_or(63);
    for cap in 0..=last {
        // SAFETY: prctl com argumentos numéricos.
        let r = unsafe { libc::prctl(libc::PR_CAPBSET_DROP, cap, 0, 0, 0) };
        if r != 0 {
            let e = io::Error::last_os_error();
            // EINVAL: capability que o kernel não conhece; pode ser ignorada.
            if e.raw_os_error() != Some(libc::EINVAL) {
                return Err(e);
            }
        }
    }
    // SAFETY: prctl com argumentos numéricos.
    unsafe {
        libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        )
    };
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    const VERSION_3: u32 = 0x2008_0522;
    let header = Header {
        version: VERSION_3,
        pid: 0,
    };
    let data = [Data {
        effective: 0,
        permitted: 0,
        inheritable: 0,
    }; 2];
    // SAFETY: cabeçalho e dados com o layout da versão 3 da interface.
    check(unsafe { libc::syscall(libc::SYS_capset, &header as *const Header, data.as_ptr()) })
        .map(|_| ())
}

pub fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: prctl com argumentos numéricos.
    let r = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if r != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Versão da ABI do Landlock no kernel, ou um número negativo se não houver.
pub fn landlock_abi() -> i32 {
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;
    // SAFETY: consulta de versão: ponteiro nulo e tamanho zero.
    let r = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    r as i32
}
