//! Aberturas de arquivo pelo supervisor (seccomp user notify).
//!
//! O filtro do comando manda cada `open`, `openat`, `openat2` e `creat` ao
//! supervisor. Ele lê o caminho na memória do processo, abre o arquivo dentro
//! da raiz do sandbox, confere as regras do Landlock pelo arquivo que de fato
//! abriu e entrega o descritor ao processo. Trocar o caminho na memória depois
//! da leitura não muda o arquivo entregue.

use std::collections::HashSet;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sandmux_spec::files::{FilePolicy, FileVerdict};

use crate::namer::{Namer, Op};
use sandmux_spec::{FileEntry, FilesSpec, LandlockRules, LaunchSpec};

use crate::sys;

#[repr(C)]
#[derive(Default)]
struct SeccompData {
    nr: i32,
    arch: u32,
    ip: u64,
    args: [u64; 6],
}

#[repr(C)]
#[derive(Default)]
struct Notif {
    id: u64,
    pid: u32,
    flags: u32,
    data: SeccompData,
}

#[repr(C)]
#[derive(Default)]
struct Resp {
    id: u64,
    val: i64,
    error: i32,
    flags: u32,
}

#[repr(C)]
#[derive(Default)]
struct AddFd {
    id: u64,
    flags: u32,
    srcfd: u32,
    newfd: u32,
    newfd_flags: u32,
}

const IOCTL_RECV: libc::c_ulong = 0xc050_2100;
const IOCTL_SEND: libc::c_ulong = 0xc018_2101;
const IOCTL_ID_VALID: libc::c_ulong = 0x4008_2102;
const IOCTL_ADDFD: libc::c_ulong = 0x4018_2103;
const ADDFD_FLAG_SEND: u32 = 2;
const USER_NOTIF_FLAG_CONTINUE: u32 = 1;
const RESOLVE_BENEATH: u64 = 0x08;

/// Threads que atendem as notificações.
pub const THREADS: usize = 4;

/// Um pedido de abertura, lido da notificação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub dirfd: i32,
    pub path: Vec<u8>,
    pub flags: i32,
    pub mode: u32,
    pub resolve: u64,
}

/// O acesso de uma abertura, para o Landlock e o registro.
pub fn access(flags: i32) -> (&'static str, bool) {
    if flags & libc::O_PATH != 0 {
        return ("r", false);
    }
    let write = matches!(flags & libc::O_ACCMODE, libc::O_WRONLY | libc::O_RDWR)
        || flags & (libc::O_TRUNC | libc::O_CREAT) != 0;
    let label = match flags & libc::O_ACCMODE {
        libc::O_WRONLY => "w",
        libc::O_RDWR => "rw",
        _ if write => "w",
        _ => "r",
    };
    (label, write)
}

fn under(p: &Path, root: &Path) -> bool {
    p == root || p.starts_with(root)
}

/// Confere uma abertura contra as regras do Landlock, pelo caminho do
/// arquivo aberto na visão do agente: leitura sob uma regra de leitura ou de
/// escrita; escrita sob uma regra de escrita; `O_PATH` passa.
pub fn landlock_allows(rules: &LandlockRules, path: &Path, flags: i32) -> bool {
    if flags & libc::O_PATH != 0 {
        return true;
    }
    let (_, write) = access(flags);
    if write {
        rules.write.iter().any(|r| under(path, r))
    } else {
        rules
            .read
            .iter()
            .chain(&rules.write)
            .any(|r| under(path, r))
    }
}

/// Como abrir um caminho pedido pelo processo.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// Absoluto na visão do agente: aberto a partir da raiz do sandbox, com
    /// `RESOLVE_IN_ROOT` (links absolutos e `..` ficam dentro dela).
    InRoot(Vec<u8>),
    /// Um link mágico do próprio processo como último componente
    /// (`/proc/self/exe`, `/proc/self/fd/<n>`, `/dev/fd/<n>`): aberto pelo
    /// `/proc` do host, que salta direto para o objeto, sem resolver texto.
    Host(Vec<u8>),
    /// Relativo ao diretório `dirfd` do processo, com `RESOLVE_BENEATH` ou
    /// `RESOLVE_IN_ROOT` pedidos pelo próprio `openat2`.
    Beneath(i32, Vec<u8>),
    /// O `uid_map`, `gid_map`, `projid_map` ou `setgroups` do próprio processo, pelo
    /// `/proc` do host: aberto no user namespace pai do dele
    /// ([`open_idmap`]), como o kernel exige para gravar o mapeamento.
    IdMap(Vec<u8>),
    /// Recusado (links mágicos de outros processos).
    Refused,
}

fn split_first(rest: &[u8]) -> (&[u8], &[u8]) {
    match rest.iter().position(|&c| c == b'/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, &b""[..]),
    }
}

fn digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(u8::is_ascii_digit)
}

fn join(base: &Path, rest: &[u8]) -> io::Result<Vec<u8>> {
    let mut v = base.as_os_str().as_bytes().to_vec();
    if !v.starts_with(b"/") {
        return Err(io::Error::from_raw_os_error(libc::ENOTDIR));
    }
    if v.ends_with(b" (deleted)") {
        return Err(io::Error::from_raw_os_error(libc::ENOENT));
    }
    let rest = rest.strip_prefix(b"/").unwrap_or(rest);
    if !v.ends_with(b"/") && !rest.is_empty() {
        v.push(b'/');
    }
    v.extend_from_slice(rest);
    Ok(v)
}

/// Traduz o caminho pedido. `base(dirfd)` dá o caminho do diretório (`cwd`
/// com `AT_FDCWD`, ou o descritor) na visão do agente; `host_pid` e `ns_pid`
/// são o processo no host e no pid namespace do sandbox.
pub fn target(
    req: &Request,
    host_pid: u32,
    ns_pid: impl Fn() -> Option<u32>,
    base: impl Fn(i32) -> io::Result<PathBuf>,
) -> io::Result<Target> {
    if req.resolve & (RESOLVE_BENEATH | sys::RESOLVE_IN_ROOT) != 0 && !req.path.starts_with(b"/") {
        return Ok(Target::Beneath(req.dirfd, req.path.clone()));
    }
    let abs = if req.path.starts_with(b"/") {
        req.path.clone()
    } else {
        join(&base(req.dirfd)?, &req.path)?
    };
    // Links do sistema para o `/proc/self`: o `self` do `/proc` do sandbox
    // não existe para o supervisor, que está fora do pid namespace.
    let abs = {
        let mut abs = abs;
        for (link, to) in [
            (&b"/proc/net"[..], &b"/proc/self/net"[..]),
            (b"/proc/mounts", b"/proc/self/mounts"),
            (b"/etc/mtab", b"/proc/self/mounts"),
        ] {
            if abs == link || (abs.starts_with(link) && abs.get(link.len()) == Some(&b'/')) {
                let mut v = to.to_vec();
                v.extend_from_slice(&abs[link.len()..]);
                abs = v;
                break;
            }
        }
        abs
    };
    // `/dev/fd/<n>` e `/dev/std*` são `/proc/self/fd/<n>`.
    let own_tail: Option<Vec<u8>> = if let Some(rest) = abs.strip_prefix(b"/dev/fd/") {
        let mut v = b"/fd/".to_vec();
        v.extend_from_slice(rest);
        Some(v)
    } else if let Some(n) = [&b"/dev/stdin"[..], b"/dev/stdout", b"/dev/stderr"]
        .iter()
        .position(|a| abs == *a)
    {
        Some(format!("/fd/{n}").into_bytes())
    } else if let Some(rest) = abs.strip_prefix(b"/proc/") {
        let (first, tail) = split_first(rest);
        let own = first == b"self"
            || first == b"thread-self"
            || (digits(first) && ns_pid().is_some_and(|n| first == n.to_string().as_bytes()));
        if own {
            Some(tail.to_vec())
        } else {
            // Outros processos: nada de atravessar ou abrir os links mágicos.
            let magic = |t: &[u8]| {
                let (c, _) = split_first(t.strip_prefix(b"/").unwrap_or(t));
                matches!(c, b"fd" | b"cwd" | b"root" | b"exe" | b"map_files")
            };
            let (_, sub) = split_first(tail.strip_prefix(b"/task/").unwrap_or(b""));
            if digits(first) && (magic(tail) || (tail.starts_with(b"/task/") && magic(sub))) {
                return Ok(Target::Refused);
            }
            None
        }
    } else {
        None
    };
    let Some(tail) = own_tail else {
        return Ok(Target::InRoot(abs));
    };
    let host = |t: &[u8]| {
        let mut v = format!("/proc/{host_pid}").into_bytes();
        v.extend_from_slice(t);
        Target::Host(v)
    };
    let t = tail.strip_prefix(b"/").unwrap_or(&tail);
    let (c, rest) = split_first(t);
    match c {
        // Link mágico como último componente: salta direto para o objeto.
        b"exe" | b"cwd" | b"root" if rest.is_empty() => Ok(host(&tail)),
        b"fd" => {
            let (n, more) = split_first(rest.strip_prefix(b"/").unwrap_or(b""));
            if !digits(n) {
                return Ok(match ns_pid() {
                    Some(p) => Target::InRoot(
                        format!("/proc/{p}{}", String::from_utf8_lossy(&tail)).into_bytes(),
                    ),
                    None => host(&tail),
                });
            }
            if more.is_empty() {
                return Ok(host(&tail));
            }
            // Atravessar o descritor: o caminho dele na visão do agente.
            let fd: i32 = std::str::from_utf8(n)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(-1);
            Ok(Target::InRoot(join(&base(fd)?, more)?))
        }
        b"cwd" => Ok(Target::InRoot(join(&base(libc::AT_FDCWD)?, rest)?)),
        b"root" => Ok(Target::InRoot(if rest.is_empty() {
            b"/".to_vec()
        } else {
            rest.to_vec()
        })),
        b"map_files" | b"task" => Ok(Target::Refused),
        b"uid_map" | b"gid_map" | b"projid_map" | b"setgroups" if rest.is_empty() => {
            let mut v = format!("/proc/{host_pid}").into_bytes();
            v.extend_from_slice(&tail);
            Ok(Target::IdMap(v))
        }
        // O resto do `/proc` do processo: pelo `/proc` do sandbox, com o pid
        // do namespace (o `self` resolveria para o supervisor).
        _ => Ok(match ns_pid() {
            Some(p) => {
                let mut v = format!("/proc/{p}").into_bytes();
                v.extend_from_slice(&tail);
                Target::InRoot(v)
            }
            None => host(&tail),
        }),
    }
}

/// O registro dos acessos às pastas do agente: uma linha por (caminho,
/// acesso) por lançamento, e todo bloqueio.
struct Logger {
    spec: FilesSpec,
    seen: HashSet<(PathBuf, &'static str)>,
    file: Option<(String, File)>,
    /// Caminhos já vistos nesta pasta de trabalho, em qualquer lançamento.
    known: HashSet<PathBuf>,
    known_file: Option<File>,
}

impl Logger {
    fn new(spec: FilesSpec) -> Logger {
        let path = spec.state_dir.as_ref().map(|d| {
            d.join("known")
                .join(format!("{}.txt", dir_hash(&spec.workdir)))
        });
        let known = path
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .map(|b| {
                b.split(|&c| c == b'\n')
                    .filter(|l| !l.is_empty())
                    .map(|l| PathBuf::from(std::ffi::OsStr::from_bytes(l)))
                    .collect()
            })
            .unwrap_or_default();
        let known_file = path.and_then(|f| {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(f)
                .ok()
        });
        Logger {
            spec,
            seen: HashSet::new(),
            file: None,
            known,
            known_file,
        }
    }
}

impl Logger {
    /// Uma execução: toda execução vira uma linha, de qualquer pasta.
    fn record_exec(&mut self, path: &Path, args: Vec<String>) {
        let (day, ts) = sandmux_net::log::now_local();
        let e = FileEntry {
            ts,
            tag: self.spec.tag.clone(),
            workdir: self.spec.workdir.clone(),
            path: path.to_path_buf(),
            access: "exec".into(),
            blocked: None,
            new: false,
            args,
            via: Some("supervisor".into()),
        };
        self.write(&day, &e);
    }

    fn write(&mut self, day: &str, e: &FileEntry) {
        if self.file.as_ref().is_none_or(|(d, _)| *d != day) {
            let _ = std::fs::create_dir_all(&self.spec.log_dir);
            let _ = std::fs::set_permissions(
                &self.spec.log_dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            );
            self.file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(self.spec.log_dir.join(format!("{day}.jsonl")))
                .ok()
                .map(|f| (day.to_string(), f));
        }
        if let (Some((_, f)), Ok(mut line)) = (self.file.as_mut(), serde_json::to_string(e)) {
            line.push('\n');
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn record(&mut self, path: &Path, access: &'static str, blocked: Option<&str>) {
        if !self.spec.roots.iter().any(|r| under(path, r))
            || self.spec.exclude.iter().any(|r| under(path, r))
        {
            return;
        }
        if blocked.is_none() && !self.seen.insert((path.to_path_buf(), access)) {
            return;
        }
        let (day, ts) = sandmux_net::log::now_local();
        let e = FileEntry {
            ts,
            tag: self.spec.tag.clone(),
            workdir: self.spec.workdir.clone(),
            path: path.to_path_buf(),
            access: access.into(),
            blocked: blocked.map(String::from),
            new: blocked.is_none() && self.known.insert(path.to_path_buf()),
            args: Vec::new(),
            via: None,
        };
        if e.new
            && let Some(f) = self.known_file.as_mut()
        {
            let mut line = path.as_os_str().as_bytes().to_vec();
            line.push(b'\n');
            let _ = f.write_all(&line);
        }
        if self.file.as_ref().is_none_or(|(d, _)| *d != day) {
            let _ = std::fs::create_dir_all(&self.spec.log_dir);
            let _ = std::fs::set_permissions(
                &self.spec.log_dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            );
            self.file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(self.spec.log_dir.join(format!("{day}.jsonl")))
                .ok()
                .map(|f| (day, f));
        }
        if let (Some((_, f)), Ok(mut line)) = (self.file.as_mut(), serde_json::to_string(&e)) {
            line.push('\n');
            let _ = f.write_all(line.as_bytes());
        }
    }
}

use sandmux_spec::files::dir_hash;

/// A política de arquivos ao vivo, os criados pelo agente e os caminhos
/// conhecidos da pasta de trabalho.
struct Live {
    policy: FilePolicy,
    file: Option<PathBuf>,
    stamp: Option<std::time::SystemTime>,
    checked: std::time::Instant,
    created: HashSet<PathBuf>,
    created_file: Option<PathBuf>,
}

impl Live {
    fn new(spec: Option<&FilesSpec>) -> Live {
        let created_file = spec.and_then(|s| {
            s.state_dir
                .as_ref()
                .map(|d| sandmux_spec::files::created_file(d, &s.workdir))
        });
        let created = spec
            .and_then(|s| {
                s.state_dir
                    .as_ref()
                    .map(|d| sandmux_spec::files::read_created(d, &s.workdir))
            })
            .unwrap_or_default();
        Live {
            policy: spec.map(|s| s.policy.clone()).unwrap_or_default(),
            file: spec.and_then(|s| s.policy_file.clone()),
            stamp: None,
            checked: std::time::Instant::now(),
            created,
            created_file,
        }
    }

    /// Relê o arquivo da política se mudou (no máximo a cada 200 ms).
    fn refresh(&mut self) {
        let Some(f) = &self.file else { return };
        if self.checked.elapsed() < std::time::Duration::from_millis(200) && self.stamp.is_some() {
            return;
        }
        self.checked = std::time::Instant::now();
        let stamp = std::fs::metadata(f).and_then(|m| m.modified()).ok();
        if stamp.is_some()
            && stamp != self.stamp
            && let Some(p) = std::fs::read(f)
                .ok()
                .and_then(|b| serde_json::from_slice::<FilePolicy>(&b).ok())
        {
            self.policy = p;
            self.stamp = stamp;
        }
    }

    fn decide(&self, path: &Path) -> FileVerdict {
        self.policy.decide(path, |q| self.created.contains(q))
    }

    /// Guarda um segredo criado pelo agente (fora do alcance dele).
    fn add_created(&mut self, path: &Path) {
        if !self.created.insert(path.to_path_buf()) {
            return;
        }
        if let Some(f) = &self.created_file {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let mut v: Vec<&PathBuf> = self.created.iter().collect();
            v.sort();
            if let Ok(b) = serde_json::to_vec(&v) {
                let tmp = f.with_extension("tmp");
                if std::fs::write(&tmp, b).is_ok() {
                    let _ = std::fs::rename(&tmp, f);
                }
            }
        }
    }
}

struct Ctx {
    listener: OwnedFd,
    live: Mutex<Live>,
    /// O auxiliar que faz `rename`, `link` e `symlink` dentro do sandbox.
    namer: Option<Arc<Mutex<Namer>>>,
    /// A raiz do sandbox, aberta na primeira notificação (é a mesma para
    /// todos os processos: `chroot` e `pivot_root` são negados).
    root: std::sync::OnceLock<OwnedFd>,
    rules: LandlockRules,
    log: Option<Mutex<Logger>>,
}

/// Começa a atender as notificações de `listener` em [`THREADS`] threads.
pub fn spawn(
    listener: OwnedFd,
    spec: &LaunchSpec,
    namer: Option<Arc<Mutex<Namer>>>,
) -> io::Result<()> {
    let ctx = Arc::new(Ctx {
        namer,
        live: Mutex::new(Live::new(spec.files.as_ref())),
        listener,
        root: std::sync::OnceLock::new(),
        rules: spec.landlock.clone(),
        log: spec.files.clone().map(|f| Mutex::new(Logger::new(f))),
    });
    for i in 0..THREADS {
        let c = ctx.clone();
        std::thread::Builder::new()
            .name(format!("sandmux-files-{i}"))
            .spawn(move || serve(&c))?;
    }
    Ok(())
}

fn serve(ctx: &Ctx) {
    // `umask` própria da thread: a criação usa a do processo atendido.
    // SAFETY: unshare só da estrutura de sistema de arquivos da thread.
    unsafe { libc::unshare(libc::CLONE_FS) };
    let fd = ctx.listener.as_raw_fd();
    loop {
        let mut n = Notif::default();
        // SAFETY: estrutura do tamanho que o ioctl espera.
        if unsafe { libc::ioctl(fd, IOCTL_RECV as _, &mut n) } < 0 {
            let e = io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EINTR) | Some(libc::ENOENT) => continue,
                _ => return,
            }
        }
        handle(ctx, &n);
    }
}

fn respond_error(fd: RawFd, id: u64, errno: i32) {
    let r = Resp {
        id,
        val: 0,
        error: -errno,
        flags: 0,
    };
    // SAFETY: estrutura do tamanho que o ioctl espera.
    unsafe { libc::ioctl(fd, IOCTL_SEND as _, &r) };
}

fn read_cstr(mem: &File, addr: u64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 512];
    let mut off = addr;
    while out.len() <= libc::PATH_MAX as usize {
        let n = mem.read_at(&mut buf, off).ok()?;
        if n == 0 {
            return None;
        }
        if let Some(i) = buf[..n].iter().position(|&b| b == 0) {
            out.extend_from_slice(&buf[..i]);
            return Some(out);
        }
        out.extend_from_slice(&buf[..n]);
        off += n as u64;
    }
    None
}

fn read_request(n: &Notif, mem: &File) -> Option<Request> {
    let a = n.data.args;
    let nr = n.data.nr as i64;
    #[cfg(target_arch = "x86_64")]
    {
        if nr == libc::SYS_open {
            return Some(Request {
                dirfd: libc::AT_FDCWD,
                path: read_cstr(mem, a[0])?,
                flags: a[1] as i32,
                mode: a[2] as u32,
                resolve: 0,
            });
        }
        if nr == libc::SYS_creat {
            return Some(Request {
                dirfd: libc::AT_FDCWD,
                path: read_cstr(mem, a[0])?,
                flags: libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
                mode: a[1] as u32,
                resolve: 0,
            });
        }
    }
    if nr == libc::SYS_openat {
        return Some(Request {
            dirfd: a[0] as i32,
            path: read_cstr(mem, a[1])?,
            flags: a[2] as i32,
            mode: a[3] as u32,
            resolve: 0,
        });
    }
    if nr == libc::SYS_openat2 {
        let mut how = [0u8; 24];
        if a[3] < 24 || mem.read_at(&mut how, a[2]).ok()? != 24 {
            return None;
        }
        let u = |i: usize| u64::from_ne_bytes(how[i * 8..i * 8 + 8].try_into().unwrap_or_default());
        return Some(Request {
            dirfd: a[0] as i32,
            path: read_cstr(mem, a[1])?,
            flags: u(0) as i32,
            mode: u(1) as u32,
            resolve: u(2),
        });
    }
    None
}

/// Valor de um campo de `/proc/<pid>/status`.
fn status_field(pid: u32, key: &str) -> Option<String> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    s.lines()
        .find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
}

fn open_how(
    dir: BorrowedFd<'_>,
    path: &[u8],
    flags: i32,
    mode: u32,
    resolve: u64,
) -> io::Result<OwnedFd> {
    #[repr(C)]
    struct How {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let c = CString::new(path).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    let mode = if flags & (libc::O_CREAT | libc::O_TMPFILE) != 0 {
        mode as u64 & 0o7777
    } else {
        0
    };
    let how = How {
        flags: flags as u32 as u64,
        mode,
        resolve,
    };
    // Com `RESOLVE_IN_ROOT`, uma montagem ou um rename em qualquer lugar
    // durante a resolução dá `EAGAIN`: repete.
    let mut tries = 0;
    loop {
        // SAFETY: `how` e `c` vivem durante a chamada.
        let r = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                dir.as_raw_fd(),
                c.as_ptr(),
                &how as *const How,
                std::mem::size_of::<How>(),
            )
        };
        if r >= 0 {
            // SAFETY: descritor novo.
            return Ok(unsafe { OwnedFd::from_raw_fd(r as i32) });
        }
        let e = io::Error::last_os_error();
        tries += 1;
        if e.raw_os_error() != Some(libc::EAGAIN) || tries >= 1000 {
            return Err(e);
        }
    }
}

fn open_proc(path: &str) -> io::Result<OwnedFd> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_DIRECTORY)
        .open(path)?;
    Ok(f.into())
}

/// Abre o pedido como o processo abriria, dentro da raiz do sandbox.
fn open_for(ctx: &Ctx, pid: u32, req: &Request, flags: i32) -> io::Result<OwnedFd> {
    // O pid no namespace do sandbox, o do `/proc` montado lá dentro: o
    // segundo do `NSpid` (o primeiro é o do supervisor). Um processo num pid
    // namespace aninhado (o sandbox do Chrome) tem mais números depois.
    let ns_pid = || {
        status_field(pid, "NSpid:")
            .and_then(|v| v.split_whitespace().nth(1).and_then(|n| n.parse().ok()))
    };
    let t = target(req, pid, ns_pid, |dirfd| {
        let link = if dirfd == libc::AT_FDCWD {
            format!("/proc/{pid}/cwd")
        } else {
            format!("/proc/{pid}/fd/{dirfd}")
        };
        std::fs::read_link(link)
    })?;
    let user_resolve = req.resolve & !(RESOLVE_BENEATH | sys::RESOLVE_IN_ROOT);
    match t {
        Target::InRoot(p) => {
            let root = match ctx.root.get() {
                Some(r) => r,
                None => {
                    let r = open_proc(&format!("/proc/{pid}/root"))?;
                    ctx.root.get_or_init(|| r)
                }
            };
            open_how(
                root.as_fd(),
                &p,
                flags,
                req.mode,
                user_resolve | sys::RESOLVE_IN_ROOT,
            )
        }
        Target::Host(p) => {
            // SAFETY: AT_FDCWD é sempre válido.
            let cwd = unsafe { BorrowedFd::borrow_raw(libc::AT_FDCWD) };
            open_how(cwd, &p, flags, req.mode, user_resolve)
        }
        Target::Beneath(dirfd, p) => {
            let link = if dirfd == libc::AT_FDCWD {
                format!("/proc/{pid}/cwd")
            } else {
                format!("/proc/{pid}/fd/{dirfd}")
            };
            let dir = open_proc(&link)?;
            open_how(dir.as_fd(), &p, flags, req.mode, req.resolve)
        }
        Target::IdMap(p) => open_idmap(pid, &p, flags),
        Target::Refused => Err(io::Error::from_raw_os_error(libc::EACCES)),
    }
}

/// `NS_GET_PARENT`: o user namespace pai, como descritor.
const NS_GET_PARENT: libc::c_ulong = 0xb702;

/// Abre o mapeamento de ids do processo `pid` (já traduzido para o `/proc`
/// do host) a partir do user namespace pai do dele.
///
/// O kernel só aceita gravar no `uid_map` por um descritor aberto no
/// namespace do processo ou no pai, e o supervisor está acima dos dois.
/// Um processo auxiliar, filho do supervisor, entra no namespace pai (o
/// supervisor é dono da cadeia, e tem as capabilities dela), abre o arquivo e
/// devolve o descritor. As regras da gravação continuam as do kernel: o
/// processo só mapeia ids que o namespace pai já tem, como faria sem o
/// sandmux. Um processo que não criou namespace próprio continua recusado.
fn open_idmap(pid: u32, path: &[u8], flags: i32) -> io::Result<OwnedFd> {
    use std::os::unix::fs::MetadataExt;
    let ns = File::open(format!("/proc/{pid}/ns/user"))?;
    // SAFETY: ioctl sem argumento num descritor de namespace válido.
    let raw = unsafe { libc::ioctl(ns.as_raw_fd(), NS_GET_PARENT as _) };
    if raw < 0 {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    // SAFETY: descritor recém-devolvido pelo kernel, nosso.
    let parent = unsafe { OwnedFd::from_raw_fd(raw) };
    let own = std::fs::metadata("/proc/self/ns/user")?;
    let par = File::from(parent.try_clone()?).metadata()?;
    if (par.dev(), par.ino()) == (own.dev(), own.ino()) {
        // O pai é o namespace do supervisor: o processo não criou um
        // namespace próprio, e o kernel recusaria de qualquer jeito.
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    let c = CString::new(path).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    let flags = (flags & !(libc::O_CREAT | libc::O_TMPFILE | libc::O_PATH)) | libc::O_CLOEXEC;
    let (ours, theirs) = std::os::unix::net::UnixStream::pair()?;
    // Tudo o que o filho usa fica pronto antes do fork: depois dele, num
    // processo com várias threads, só chamadas de sistema.
    let mut byte = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    // SAFETY: CMSG_SPACE só calcula.
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut cbuf = vec![0u64; space.div_ceil(8)];
    // SAFETY: msghdr zerada é válida.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cbuf.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    // SAFETY: o buffer de controle tem espaço para um cabeçalho.
    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    // SAFETY: `cmsg` aponta para dentro de `cbuf`.
    let data = unsafe {
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        libc::CMSG_DATA(cmsg) as *mut RawFd
    };
    let (pfd, sock) = (parent.as_raw_fd(), theirs.as_raw_fd());
    // SAFETY: o filho só faz chamadas de sistema e termina com `_exit`.
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(io::Error::last_os_error());
    }
    if child == 0 {
        // SAFETY: chamadas de sistema com ponteiros preparados antes do fork.
        unsafe {
            if libc::setns(pfd, libc::CLONE_NEWUSER) != 0 {
                libc::_exit(*libc::__errno_location());
            }
            let fd = libc::open(c.as_ptr(), flags);
            if fd < 0 {
                libc::_exit(*libc::__errno_location());
            }
            *data = fd;
            if libc::sendmsg(sock, &msg, libc::MSG_NOSIGNAL) < 0 {
                libc::_exit(*libc::__errno_location());
            }
            libc::_exit(0);
        }
    }
    drop(theirs);
    let mut status = 0;
    // SAFETY: espera o filho que acabamos de criar.
    unsafe { libc::waitpid(child, &mut status, 0) };
    let code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        libc::EPERM
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code));
    }
    let mut buf = [0u8; 1];
    let mut cspace = nix::cmsg_space!(RawFd);
    let mut iov = [io::IoSliceMut::new(&mut buf)];
    let r = nix::sys::socket::recvmsg::<()>(
        ours.as_raw_fd(),
        &mut iov,
        Some(&mut cspace),
        nix::sys::socket::MsgFlags::MSG_CMSG_CLOEXEC,
    )
    .map_err(io::Error::from)?;
    for c in r.cmsgs().map_err(io::Error::from)? {
        if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = c
            && let Some(&fd) = fds.first()
        {
            // SAFETY: descritor recebido agora, nosso.
            return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
        }
    }
    Err(io::Error::from_raw_os_error(libc::EPERM))
}

/// (operação, origem (dirfd, caminho), destino (dirfd, caminho)).
type NameCall = (Op, Option<(i32, Vec<u8>)>, (i32, Vec<u8>));

fn read_name_call(n: &Notif, mem: &File) -> Option<Option<NameCall>> {
    let a = n.data.args;
    let nr = n.data.nr as i64;
    let at = libc::AT_FDCWD;
    let s = |addr: u64| read_cstr(mem, addr);
    #[cfg(target_arch = "x86_64")]
    {
        if nr == libc::SYS_rename {
            return Some(Some((
                Op::Rename { flags: 0 },
                Some((at, s(a[0])?)),
                (at, s(a[1])?),
            )));
        }
        if nr == libc::SYS_link {
            return Some(Some((
                Op::Link { flags: 0 },
                Some((at, s(a[0])?)),
                (at, s(a[1])?),
            )));
        }
        if nr == libc::SYS_symlink {
            return Some(Some((
                Op::Symlink { target: s(a[0])? },
                None,
                (at, s(a[1])?),
            )));
        }
    }
    if nr == libc::SYS_renameat || nr == libc::SYS_renameat2 {
        let flags = if nr == libc::SYS_renameat2 {
            a[4] as u32
        } else {
            0
        };
        return Some(Some((
            Op::Rename { flags },
            Some((a[0] as i32, s(a[1])?)),
            (a[2] as i32, s(a[3])?),
        )));
    }
    if nr == libc::SYS_linkat {
        return Some(Some((
            Op::Link { flags: a[4] as i32 },
            Some((a[0] as i32, s(a[1])?)),
            (a[2] as i32, s(a[3])?),
        )));
    }
    if nr == libc::SYS_symlinkat {
        return Some(Some((
            Op::Symlink { target: s(a[0])? },
            None,
            (a[1] as i32, s(a[2])?),
        )));
    }
    Some(None)
}

/// Separa a pasta e o último componente (sem as barras do fim).
fn split_parent(path: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut p = path;
    while p.len() > 1 && p.ends_with(b"/") {
        p = &p[..p.len() - 1];
    }
    let (dir, name) = match p.iter().rposition(|&c| c == b'/') {
        Some(0) => (b"/".to_vec(), p[1..].to_vec()),
        Some(i) => (p[..i].to_vec(), p[i + 1..].to_vec()),
        None => (b".".to_vec(), p.to_vec()),
    };
    (!name.is_empty() && name != b"." && name != b"..").then_some((dir, name))
}

/// Faz `rename`, `link` ou `symlink` no lugar do processo, com as pastas
/// resolvidas dentro da raiz e as regras conferidas.
fn handle_name(ctx: &Ctx, n: &Notif, call: NameCall) {
    let fd = ctx.listener.as_raw_fd();
    let (op, src, dst) = call;
    let parent = |(dirfd, path): &(i32, Vec<u8>)| -> io::Result<(OwnedFd, PathBuf, Vec<u8>)> {
        let (dir, name) =
            split_parent(path).ok_or_else(|| io::Error::from_raw_os_error(libc::EBUSY))?;
        let req = Request {
            dirfd: *dirfd,
            path: dir,
            flags: libc::O_PATH | libc::O_DIRECTORY,
            mode: 0,
            resolve: 0,
        };
        // O `O_PATH` aqui é do próprio supervisor, não do processo.
        let f = open_for(ctx, n.pid, &req, libc::O_PATH | libc::O_DIRECTORY)?;
        let p = std::fs::read_link(format!("/proc/self/fd/{}", f.as_raw_fd()))?;
        Ok((f, p, name))
    };
    let result = (|| -> io::Result<Option<PathBuf>> {
        let (dfd, dpath, dname) = parent(&dst)?;
        let dest = dpath.join(std::ffi::OsStr::from_bytes(&dname));
        let s = src.as_ref().map(parent).transpose()?;
        let deny = |e| Err(io::Error::from_raw_os_error(e));
        let mut live = ctx.live.lock().unwrap_or_else(|e| e.into_inner());
        live.refresh();
        // O `.git/commondir` da pasta de trabalho faria o git do host usar
        // outro diretório git.
        let wd = &live.policy.workdir;
        if !wd.as_os_str().is_empty() && dest.starts_with(wd) && dest.ends_with(".git/commondir") {
            return deny(libc::EACCES);
        }
        if let Some((_, sp, sn)) = &s {
            let source = sp.join(std::ffi::OsStr::from_bytes(sn));
            // Renomear ou ligar um segredo o mostraria com outro nome.
            if live.decide(&source) != FileVerdict::Pass {
                return deny(libc::EACCES);
            }
        }
        let dest_exists = std::fs::symlink_metadata(format!(
            "/proc/self/fd/{}/{}",
            dfd.as_raw_fd(),
            String::from_utf8_lossy(&dname)
        ))
        .is_ok();
        let new_secret = match live.decide(&dest) {
            FileVerdict::Deny(_) => return deny(libc::EACCES),
            FileVerdict::Secret if dest_exists => return deny(libc::EACCES),
            FileVerdict::Secret => true,
            FileVerdict::Pass => false,
        };
        drop(live);
        if let Op::Link { flags } = &op
            && flags & libc::AT_EMPTY_PATH != 0
        {
            return deny(libc::EPERM);
        }
        // Dentro do sandbox, sob o Landlock: o kernel decide pontos de
        // montagem e as regras de escrita.
        let namer = ctx
            .namer
            .as_ref()
            .ok_or_else(|| io::Error::from_raw_os_error(libc::EPERM))?;
        namer.lock().unwrap_or_else(|e| e.into_inner()).call(
            &op,
            s.as_ref().map(|(f, _, n)| (f, n.as_slice())),
            (&dfd, &dname),
        )?;
        if new_secret {
            ctx.live
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .add_created(&dest);
        }
        Ok(Some(dest))
    })();
    match result {
        Ok(dest) => {
            if let (Some(d), Some(l)) = (dest, &ctx.log) {
                let label = match op {
                    Op::Rename { .. } => "rename",
                    Op::Link { .. } => "link",
                    Op::Symlink { .. } => "symlink",
                };
                l.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .record(&d, label, None);
            }
            let r = Resp {
                id: n.id,
                val: 0,
                error: 0,
                flags: 0,
            };
            // SAFETY: estrutura do tamanho que o ioctl espera.
            unsafe { libc::ioctl(fd, IOCTL_SEND as _, &r) };
        }
        Err(e) => respond_error(fd, n.id, e.raw_os_error().unwrap_or(libc::EIO)),
    }
}

/// Teto dos argumentos de uma execução no registro.
const EXEC_ARGS_MAX: usize = 1024;
const EXEC_ARGC_MAX: usize = 64;

/// Lê o `argv` (vetor de ponteiros terminado em nulo) na memória do
/// processo, até [`EXEC_ARGC_MAX`] argumentos e [`EXEC_ARGS_MAX`] bytes.
pub fn read_argv(mem: &impl std::os::unix::fs::FileExt, addr: u64) -> Vec<String> {
    let mut out = Vec::new();
    let mut total = 0;
    for i in 0..EXEC_ARGC_MAX as u64 {
        let mut p = [0u8; 8];
        if addr == 0 || mem.read_at(&mut p, addr + i * 8).ok() != Some(8) {
            break;
        }
        let ptr = u64::from_ne_bytes(p);
        if ptr == 0 {
            break;
        }
        let mut s = Vec::new();
        let mut buf = [0u8; 256];
        let mut off = ptr;
        while let Ok(n) = mem.read_at(&mut buf, off) {
            if n == 0 {
                break;
            }
            match buf[..n].iter().position(|&b| b == 0) {
                Some(z) => {
                    s.extend_from_slice(&buf[..z]);
                    break;
                }
                None => s.extend_from_slice(&buf[..n]),
            }
            off += n as u64;
            if s.len() > EXEC_ARGS_MAX {
                break;
            }
        }
        let room = EXEC_ARGS_MAX.saturating_sub(total);
        if room == 0 {
            break;
        }
        s.truncate(room);
        total += s.len() + 1;
        out.push(String::from_utf8_lossy(&s).into_owned());
    }
    out
}

/// Uma execução (`execve` ou `execveat`): lê o caminho e os argumentos,
/// grava a linha quando a política ao vivo deixa, e deixa o kernel seguir.
/// A leitura não decide nada: outra thread pode trocar a memória antes da
/// execução, e a linha diz que a fonte é o supervisor.
fn handle_exec(ctx: &Ctx, n: &Notif, mem: Option<&File>) {
    let fd = ctx.listener.as_raw_fd();
    let r = Resp {
        id: n.id,
        val: 0,
        error: 0,
        flags: USER_NOTIF_FLAG_CONTINUE,
    };
    let Some(mem) = mem else {
        // SAFETY: estrutura do tamanho que o ioctl espera.
        unsafe { libc::ioctl(fd, IOCTL_SEND as _, &r) };
        return;
    };
    let a = n.data.args;
    let (path, argv) = if n.data.nr as i64 == libc::SYS_execve {
        (read_cstr(mem, a[0]), a[1])
    } else {
        let p = read_cstr(mem, a[1]);
        // Com `AT_EMPTY_PATH` e caminho vazio, o programa é o próprio
        // descritor.
        let p = match p {
            Some(p) if p.is_empty() => {
                std::fs::read_link(format!("/proc/{}/fd/{}", n.pid, a[0] as i32))
                    .ok()
                    .map(|l| std::os::unix::ffi::OsStringExt::into_vec(l.into_os_string()))
            }
            x => x,
        };
        (p, a[2])
    };
    let args = read_argv(mem, argv);
    let allowed = {
        let mut live = ctx.live.lock().unwrap_or_else(|e| e.into_inner());
        live.refresh();
        live.policy.exec_log != Some(false)
    };
    if allowed && let (Some(log), Some(path)) = (ctx.log.as_ref(), path) {
        log.lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_exec(Path::new(std::ffi::OsStr::from_bytes(&path)), args);
    }
    // SAFETY: estrutura do tamanho que o ioctl espera.
    unsafe { libc::ioctl(fd, IOCTL_SEND as _, &r) };
}

fn handle(ctx: &Ctx, n: &Notif) {
    let fd = ctx.listener.as_raw_fd();
    let nr = n.data.nr as i64;
    // Uma execução segue sempre, mesmo sem dar para ler a memória do
    // processo (o primeiro `exec` do lançador): o registro nunca a impede.
    if nr == libc::SYS_execve || nr == libc::SYS_execveat {
        let mem = File::open(format!("/proc/{}/mem", n.pid)).ok();
        return handle_exec(ctx, n, mem.as_ref());
    }
    let Ok(mem) = File::open(format!("/proc/{}/mem", n.pid)) else {
        return respond_error(fd, n.id, libc::ESRCH);
    };
    match read_name_call(n, &mem) {
        Some(Some(call)) => {
            drop(mem);
            // SAFETY: ponteiro para um u64 válido.
            if unsafe { libc::ioctl(fd, IOCTL_ID_VALID as _, &n.id) } < 0 {
                return;
            }
            return handle_name(ctx, n, call);
        }
        None => return respond_error(fd, n.id, libc::EFAULT),
        Some(None) => {}
    }
    let Some(req) = read_request(n, &mem) else {
        return respond_error(fd, n.id, libc::EFAULT);
    };
    drop(mem);
    // A notificação ainda vale (o processo pode ter morrido e o pid, mudado).
    // SAFETY: ponteiro para um u64 válido.
    if unsafe { libc::ioctl(fd, IOCTL_ID_VALID as _, &n.id) } < 0 {
        return;
    }
    // O `ADDFD` não entrega descritores `O_PATH` (o kernel os ignora). Um
    // `O_PATH` não dá acesso ao conteúdo: ler exige outra abertura, que passa
    // por aqui. Então ele segue pelo kernel, sob o Landlock.
    if req.flags & libc::O_PATH != 0 {
        let r = Resp {
            id: n.id,
            val: 0,
            error: 0,
            flags: USER_NOTIF_FLAG_CONTINUE,
        };
        // SAFETY: estrutura do tamanho que o ioctl espera.
        unsafe { libc::ioctl(fd, IOCTL_SEND as _, &r) };
        return;
    }
    let cloexec = req.flags & libc::O_CLOEXEC != 0;
    let flags = req.flags & !libc::O_CLOEXEC;
    let creating = flags & libc::O_CREAT != 0;
    if creating {
        let mask = status_field(n.pid, "Umask:")
            .and_then(|v| u32::from_str_radix(&v, 8).ok())
            .unwrap_or(0o022);
        // SAFETY: umask da thread (estrutura própria pelo unshare).
        unsafe { libc::umask(mask) };
    }
    let (label, write) = access(flags);
    let record = |p: &Path, label: &'static str, why: Option<&str>| {
        if let Some(l) = &ctx.log {
            l.lock()
                .unwrap_or_else(|e| e.into_inner())
                .record(p, label, why);
        }
    };
    let verdict = |p: &Path| {
        let mut live = ctx.live.lock().unwrap_or_else(|e| e.into_inner());
        live.refresh();
        live.decide(p)
    };
    // Escrita e criação: decide antes de abrir, porque abrir com `O_TRUNC`
    // ou `O_CREAT` já muda o arquivo.
    let before = if write {
        open_for(ctx, n.pid, &req, libc::O_PATH | (flags & libc::O_NOFOLLOW))
            .ok()
            .and_then(|f| std::fs::read_link(format!("/proc/self/fd/{}", f.as_raw_fd())).ok())
    } else {
        None
    };
    let existed = before.is_some();
    if let Some(p) = before.as_deref().filter(|p| p.starts_with("/")) {
        match verdict(p) {
            FileVerdict::Deny(pat) => {
                record(p, label, Some(&format!("file-deny: {pat}")));
                return respond_error(fd, n.id, libc::EACCES);
            }
            FileVerdict::Secret => {
                record(p, label, Some("segredo"));
                return respond_error(fd, n.id, libc::EACCES);
            }
            FileVerdict::Pass => {}
        }
    }
    let opened = match open_for(ctx, n.pid, &req, flags) {
        Ok(f) => f,
        Err(e) => return respond_error(fd, n.id, e.raw_os_error().unwrap_or(libc::EIO)),
    };
    // O caminho do arquivo aberto, na visão do agente.
    let seen = std::fs::read_link(format!("/proc/self/fd/{}", opened.as_raw_fd())).ok();
    let created_now = creating && !existed;
    let label = if created_now { "create" } else { label };
    // Pipes e sockets (`pipe:[...]`) não têm caminho.
    if let Some(p) = seen.as_deref().filter(|p| p.starts_with("/")) {
        match verdict(p) {
            FileVerdict::Deny(pat) => {
                record(p, label, Some(&format!("file-deny: {pat}")));
                return respond_error(fd, n.id, libc::EACCES);
            }
            // Um segredo criado agora pelo agente continua dele.
            FileVerdict::Secret if created_now => {
                ctx.live
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .add_created(p);
            }
            FileVerdict::Secret => {
                record(p, label, Some("segredo"));
                let e = if write { libc::EACCES } else { libc::ENOENT };
                return respond_error(fd, n.id, e);
            }
            FileVerdict::Pass => {}
        }
        if !landlock_allows(&ctx.rules, p, flags) {
            record(p, label, Some("landlock"));
            return respond_error(fd, n.id, libc::EACCES);
        }
        record(p, label, None);
    }
    let add = AddFd {
        id: n.id,
        flags: ADDFD_FLAG_SEND,
        srcfd: opened.as_raw_fd() as u32,
        newfd: 0,
        newfd_flags: if cloexec { libc::O_CLOEXEC as u32 } else { 0 },
    };
    // SAFETY: estrutura do tamanho que o ioctl espera.
    if unsafe { libc::ioctl(fd, IOCTL_ADDFD as _, &add) } < 0 {
        // Sem resposta, o processo esperaria para sempre.
        let e = io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO);
        if e != libc::ENOENT {
            respond_error(fd, n.id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chave_do_registro_de_execucoes_ao_vivo() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("p.json");
        let spec = FilesSpec {
            log_dir: d.path().into(),
            tag: "t".into(),
            workdir: d.path().into(),
            roots: vec![],
            exclude: vec![],
            policy: FilePolicy::default(),
            policy_file: Some(f.clone()),
            state_dir: None,
            control_socket: None,
            folders: vec![],
            exec_log: true,
        };
        let mut live = Live::new(Some(&spec));
        assert_eq!(live.policy.exec_log, None);
        let p = FilePolicy {
            exec_log: Some(false),
            ..Default::default()
        };
        std::fs::write(&f, serde_json::to_vec(&p).unwrap()).unwrap();
        live.refresh();
        assert_eq!(live.policy.exec_log, Some(false));
    }

    #[test]
    fn argumentos_da_execucao() {
        // A memória de um processo: o vetor de ponteiros e as strings.
        struct Mem(Vec<u8>);
        impl std::os::unix::fs::FileExt for Mem {
            fn read_at(&self, buf: &mut [u8], off: u64) -> io::Result<usize> {
                let o = off as usize;
                if o >= self.0.len() {
                    return Ok(0);
                }
                let n = buf.len().min(self.0.len() - o);
                buf[..n].copy_from_slice(&self.0[o..o + n]);
                Ok(n)
            }
            fn write_at(&self, _: &[u8], _: u64) -> io::Result<usize> {
                Ok(0)
            }
        }
        // O vetor começa no endereço 8 (o 0 é o ponteiro nulo).
        let mut m = vec![0u8; 64];
        let strs: &[&[u8]] = &[b"ls\0", b"-la\0", b"/tmp\0"];
        let mut at = 64u64;
        for (i, s) in strs.iter().enumerate() {
            m[8 + i * 8..16 + i * 8].copy_from_slice(&at.to_ne_bytes());
            at += s.len() as u64;
        }
        for s in strs {
            m.extend_from_slice(s);
        }
        assert_eq!(read_argv(&Mem(m), 8), ["ls", "-la", "/tmp"]);
        assert!(read_argv(&Mem(vec![0; 8]), 0).is_empty());
    }

    fn req(dirfd: i32, path: &str) -> Request {
        Request {
            dirfd,
            path: path.as_bytes().to_vec(),
            flags: libc::O_RDONLY,
            mode: 0,
            resolve: 0,
        }
    }

    fn t(r: &Request) -> Target {
        target(r, 900, || Some(7), |_| Ok(PathBuf::from("/w/sub"))).unwrap()
    }

    #[test]
    fn traduz_os_caminhos() {
        let ir = |p: &str| Target::InRoot(p.as_bytes().to_vec());
        let h = |p: &str| Target::Host(p.as_bytes().to_vec());
        assert_eq!(t(&req(libc::AT_FDCWD, "/etc/x")), ir("/etc/x"));
        assert_eq!(t(&req(libc::AT_FDCWD, "a/../b")), ir("/w/sub/a/../b"));
        assert_eq!(t(&req(5, "c")), ir("/w/sub/c"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/self/status")),
            ir("/proc/7/status")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/7/maps")), ir("/proc/7/maps"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/net/dev")),
            ir("/proc/7/net/dev")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/etc/mtab")), ir("/proc/7/mounts"));
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/netx")), ir("/proc/netx"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/self/exe")),
            h("/proc/900/exe")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/dev/fd/63")), h("/proc/900/fd/63"));
        assert_eq!(t(&req(libc::AT_FDCWD, "/dev/stdout")), h("/proc/900/fd/1"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/self/fd/3")),
            h("/proc/900/fd/3")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/self/fd/3/x")), ir("/w/sub/x"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/self/cwd/link")),
            ir("/w/sub/link")
        );
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/thread-self/root/etc/x")),
            ir("/etc/x")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/self/fd")), ir("/proc/7/fd"));
        let idmap = |p: &str| Target::IdMap(p.as_bytes().to_vec());
        for name in ["uid_map", "gid_map", "projid_map", "setgroups"] {
            assert_eq!(
                t(&req(libc::AT_FDCWD, &format!("/proc/self/{name}"))),
                idmap(&format!("/proc/900/{name}"))
            );
        }
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/7/uid_map")),
            idmap("/proc/900/uid_map")
        );
        // De outro processo, o caminho comum (o kernel decide).
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/8/uid_map")),
            ir("/proc/8/uid_map")
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/8/fd/3")), Target::Refused);
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/8/cwd/x")), Target::Refused);
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/8/task/9/root/x")),
            Target::Refused
        );
        assert_eq!(t(&req(libc::AT_FDCWD, "/proc/8/stat")), ir("/proc/8/stat"));
        assert_eq!(
            t(&req(libc::AT_FDCWD, "/proc/8/fdinfo/3")),
            ir("/proc/8/fdinfo/3")
        );
        let mut b = req(3, "x");
        b.resolve = RESOLVE_BENEATH;
        assert_eq!(t(&b), Target::Beneath(3, b"x".to_vec()));
        let deleted = target(
            &req(libc::AT_FDCWD, "x"),
            1,
            || None,
            |_| Ok(PathBuf::from("/w (deleted)")),
        );
        assert!(deleted.is_err());
    }

    #[test]
    fn regras_do_landlock() {
        let rules = LandlockRules {
            read: vec!["/".into()],
            write: vec!["/w".into(), "/tmp".into()],
        };
        assert!(landlock_allows(
            &rules,
            Path::new("/etc/hostname"),
            libc::O_RDONLY
        ));
        assert!(!landlock_allows(
            &rules,
            Path::new("/etc/hostname"),
            libc::O_WRONLY
        ));
        assert!(!landlock_allows(
            &rules,
            Path::new("/etc/hostname"),
            libc::O_RDONLY | libc::O_TRUNC
        ));
        assert!(landlock_allows(
            &rules,
            Path::new("/w/a"),
            libc::O_RDWR | libc::O_CREAT
        ));
        assert!(!landlock_allows(&rules, Path::new("/wx/a"), libc::O_WRONLY));
        assert!(landlock_allows(
            &rules,
            Path::new("/etc"),
            libc::O_PATH | libc::O_WRONLY
        ));
        let none = LandlockRules {
            read: vec!["/usr".into()],
            write: vec![],
        };
        assert!(!landlock_allows(&none, Path::new("/etc/x"), libc::O_RDONLY));
        assert_eq!(access(libc::O_RDWR).0, "rw");
        assert_eq!(access(libc::O_WRONLY | libc::O_CREAT), ("w", true));
    }
}
