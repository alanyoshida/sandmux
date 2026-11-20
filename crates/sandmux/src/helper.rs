//! Auxiliar root (`sandmux-helper`): o protocolo, o servidor e o cliente do
//! daemon.
//!
//! O auxiliar prende ao cgroup do Docker de uma pasta os programas BPF LSM
//! do confinamento (`sandmux-ebpf`) e marca os arquivos que o daemon manda.
//! Ele só atende o cgroup que pertence a quem pede, decidido pelo
//! `SO_PEERCRED`, e nunca abre um caminho: os arquivos chegam como
//! descritores abertos por quem pede (`SCM_RIGHTS`). As regras duram enquanto
//! a conexão estiver aberta.
//!
//! O socket é `SOCK_SEQPACKET`: cada mensagem é um pedido JSON (até 4 KiB),
//! com os descritores dele.

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sandmux_spec::{tr, trf};
use serde_json::{Value, json};

/// Socket do serviço do sistema.
pub const SOCKET: &str = "/run/sandmux-helper.sock";
/// Variável que troca o socket (testes).
pub const SOCKET_ENV: &str = "SANDMUX_HELPER_SOCKET";
/// Teto de uma mensagem.
pub const MAX_MSG: usize = 4096;
/// Teto de descritores numa mensagem.
pub const MAX_FDS: usize = 200;
/// Confinamentos ativos por usuário.
pub const MAX_CONFINE_PER_UID: usize = 16;
/// Marcas por confinamento.
pub const MAX_MARKS: usize = 16384;
/// Observações de execução ativas por usuário (uma por aba).
pub const MAX_OBSERVE_PER_UID: usize = 64;
/// Escopos de sandbox ativos por usuário (um por lançamento, nos kernels sem
/// o Landlock ABI 6).
pub const MAX_SCOPE_PER_UID: usize = 256;

const CGROUP2_SUPER_MAGIC: i64 = 0x6367_7270;

/// O socket que o daemon usa.
pub fn socket_path() -> PathBuf {
    std::env::var_os(SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SOCKET))
}

/// Tipo de marca de um pedido `mark`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Deny,
    Protect,
    GitDir,
}

impl MarkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkKind::Deny => "deny",
            MarkKind::Protect => "protect",
            MarkKind::GitDir => "gitdir",
        }
    }
    fn parse(s: &str) -> Option<MarkKind> {
        Some(match s {
            "deny" => MarkKind::Deny,
            "protect" => MarkKind::Protect,
            "gitdir" => MarkKind::GitDir,
            _ => return None,
        })
    }
    pub fn bits(self) -> u32 {
        match self {
            MarkKind::Deny => sandmux_ebpf::prog::DENY,
            MarkKind::Protect => sandmux_ebpf::prog::PROTECT,
            MarkKind::GitDir => sandmux_ebpf::prog::GITDIR,
        }
    }
}

// ---------------------------------------------------------------- mensagens

/// Manda `msg` com os descritores `fds`.
pub fn send(sock: &impl AsRawFd, msg: &[u8], fds: &[RawFd]) -> io::Result<()> {
    if msg.len() > MAX_MSG || fds.len() > MAX_FDS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            tr!("mensagem grande demais", "message too large"),
        ));
    }
    let mut iov = libc::iovec {
        iov_base: msg.as_ptr() as *mut _,
        iov_len: msg.len(),
    };
    // SAFETY: CMSG_SPACE só calcula.
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of_val(fds) as u32) } as usize;
    let mut cbuf = vec![0u64; space.div_ceil(8)];
    // SAFETY: msghdr com ponteiros para buffers vivos; o cmsg é escrito
    // dentro de `cbuf`, que tem o tamanho de CMSG_SPACE.
    unsafe {
        let mut m: libc::msghdr = std::mem::zeroed();
        m.msg_iov = &mut iov;
        m.msg_iovlen = 1;
        if !fds.is_empty() {
            m.msg_control = cbuf.as_mut_ptr().cast();
            m.msg_controllen = space as _;
            let c = libc::CMSG_FIRSTHDR(&m);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(fds) as u32) as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr(),
                libc::CMSG_DATA(c).cast::<RawFd>(),
                fds.len(),
            );
        }
        if libc::sendmsg(sock.as_raw_fd(), &m, libc::MSG_NOSIGNAL) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Recebe uma mensagem e os descritores dela. `None` no fim da conexão.
pub fn recv(sock: &impl AsRawFd) -> io::Result<Option<(Vec<u8>, Vec<OwnedFd>)>> {
    let mut buf = vec![0u8; MAX_MSG + 1];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    // SAFETY: CMSG_SPACE só calcula.
    let space =
        unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<RawFd>()) as u32) } as usize;
    let mut cbuf = vec![0u64; space.div_ceil(8)];
    let mut fds = Vec::new();
    // SAFETY: msghdr com ponteiros para buffers vivos; os descritores lidos
    // do cmsg são novos (MSG_CMSG_CLOEXEC) e passam a ser nossos.
    let (n, flags) = unsafe {
        let mut m: libc::msghdr = std::mem::zeroed();
        m.msg_iov = &mut iov;
        m.msg_iovlen = 1;
        m.msg_control = cbuf.as_mut_ptr().cast();
        m.msg_controllen = space as _;
        let n = libc::recvmsg(sock.as_raw_fd(), &mut m, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut c = libc::CMSG_FIRSTHDR(&m);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(c).cast::<RawFd>();
                let len = (*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                for i in 0..len / std::mem::size_of::<RawFd>() {
                    fds.push(OwnedFd::from_raw_fd(*data.add(i)));
                }
            }
            c = libc::CMSG_NXTHDR(&m, c);
        }
        (n as usize, m.msg_flags)
    };
    if n == 0 && fds.is_empty() {
        return Ok(None);
    }
    if flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0 || n > MAX_MSG {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            tr!(
                "mensagem ou descritores além do teto",
                "message or descriptors beyond the cap"
            ),
        ));
    }
    buf.truncate(n);
    Ok(Some((buf, fds)))
}

/// O uid de quem está do outro lado do socket.
pub fn peer_uid(sock: &impl AsRawFd) -> io::Result<u32> {
    let mut cred: libc::ucred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt escreve no máximo `len` bytes em `cred`.
    let r = unsafe {
        libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred.uid)
}

// ---------------------------------------------------------------- servidor

/// Um confinamento carregado, do ponto de vista do servidor.
pub trait Confined: Send {
    fn mark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), String>;
    fn unmark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), String>;
    /// Quantos arquivos estão marcados.
    fn marks(&self) -> usize;
    fn attach(&self) -> &'static str;
    /// A fonte dos eventos do confinamento, para a thread que os manda.
    fn events(&self) -> Option<Box<dyn Events>>;
}

/// Eventos de um confinamento: os que chegaram em até `timeout_ms`, como
/// JSON, e quantos foram descartados com a fila cheia.
pub trait Events: Send {
    fn poll(&mut self, timeout_ms: i32) -> (Vec<Value>, u64);
}

/// Um escopo de sandbox registrado: sai do registro quando é solto.
pub trait Scoped: Send {}

/// O que carrega os programas. A versão real usa `sandmux-ebpf`; os testes,
/// uma falsa.
pub trait Backend: Send + Sync {
    fn lsm(&self) -> Result<bool, String>;
    /// Liga o confinamento no cgroup. `dev` é o `s_dev` do superbloco da
    /// pasta de trabalho (0 quando não se sabe), que filtra as criações.
    fn confine(&self, cgroup: &OwnedFd, dev: u32) -> Result<Box<dyn Confined>, String>;
    /// Observa as execuções do cgroup, sem regras de arquivos. Devolve a
    /// fonte dos eventos.
    fn observe(&self, cgroup: &OwnedFd) -> Result<Box<dyn Events>, String>;
    /// Confere que o descritor é um cgroup v2 de `uid` (o root pode
    /// qualquer um).
    fn check_cgroup(&self, cgroup: &OwnedFd, uid: u32) -> Result<(), String> {
        check_cgroup(cgroup, uid)
    }
    /// Os programas do escopo carregam neste kernel? `Err` com o motivo.
    fn scope_supported(&self) -> Result<(), String>;
    /// Registra o user namespace `ns` de um sandbox, com as opções e os
    /// dispositivos (`(número, máscara)` no formato do kernel).
    /// `init_pid` é o processo 1 do sandbox, que não recebe sinais dele.
    fn scope(
        &self,
        ns: &OwnedFd,
        opts: u32,
        init_pid: u32,
        devs: &[(u32, u32)],
    ) -> Result<Box<dyn Scoped>, String>;
    /// Confere que o descritor é um user namespace de `uid`.
    fn check_userns(&self, ns: &OwnedFd, uid: u32) -> Result<(), String> {
        check_userns(ns, uid)
    }
}

const NS_GET_NSTYPE: libc::c_ulong = 0xb703;
const NS_GET_OWNER_UID: libc::c_ulong = 0xb704;

/// A conferência real: um user namespace, que não é o do auxiliar, e cujo
/// dono é quem pede (o root pode qualquer um).
pub fn check_userns(ns: &OwnedFd, uid: u32) -> Result<(), String> {
    // SAFETY: ioctl sem argumento de memória.
    let t = unsafe { libc::ioctl(ns.as_raw_fd(), NS_GET_NSTYPE as _) };
    if t != libc::CLONE_NEWUSER {
        return Err(tr!(
            "o descritor não é de um user namespace",
            "the descriptor is not a user namespace"
        )
        .into());
    }
    let own = std::fs::metadata("/proc/self/ns/user")
        .map(|m| m.ino())
        .ok();
    let f = std::fs::File::from(ns.try_clone().map_err(|e| e.to_string())?);
    let ino = f.metadata().map_err(|e| e.to_string())?.ino();
    if Some(ino) == own {
        return Err(tr!(
            "o user namespace é o do próprio auxiliar",
            "the user namespace is the helper's own"
        )
        .into());
    }
    let mut owner: libc::uid_t = 0;
    // SAFETY: o kernel escreve um uid_t em `owner`.
    if unsafe { libc::ioctl(ns.as_raw_fd(), NS_GET_OWNER_UID as _, &mut owner) } < 0 {
        return Err(format!("NS_GET_OWNER_UID: {}", io::Error::last_os_error()));
    }
    if uid != 0 && owner != uid {
        return Err(trf!(
            "o user namespace pertence ao uid {owner}, não a quem pede ({uid})",
            "the user namespace belongs to uid {owner}, not to the requester ({uid})"
        ));
    }
    Ok(())
}

/// A conferência real: cgroup v2, e o dono do diretório é quem pede.
pub fn check_cgroup(cgroup: &OwnedFd, uid: u32) -> Result<(), String> {
    // SAFETY: fstatfs escreve na struct zerada.
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(cgroup.as_raw_fd(), &mut st) } < 0 {
        return Err(format!("fstatfs: {}", io::Error::last_os_error()));
    }
    if st.f_type as i64 != CGROUP2_SUPER_MAGIC {
        return Err(tr!(
            "o descritor não é de um cgroup v2",
            "the descriptor is not a cgroup v2"
        )
        .into());
    }
    let f = std::fs::File::from(cgroup.try_clone().map_err(|e| e.to_string())?);
    let m = f.metadata().map_err(|e| e.to_string())?;
    if !m.is_dir() {
        return Err(tr!(
            "o descritor não é um diretório de cgroup",
            "the descriptor is not a cgroup directory"
        )
        .into());
    }
    if uid != 0 && m.uid() != uid {
        return Err(trf!(
            "o cgroup pertence ao uid {}, não a quem pede ({uid})",
            "the cgroup belongs to uid {}, not to the requester ({uid})",
            m.uid()
        ));
    }
    Ok(())
}

/// Estado comum às conexões: confinamentos ativos por usuário.
pub struct Server {
    backend: Box<dyn Backend>,
    active: Mutex<HashMap<u32, usize>>,
    observing: Mutex<HashMap<u32, usize>>,
    scoping: Mutex<HashMap<u32, usize>>,
}

struct Conn {
    uid: u32,
    confined: Option<Box<dyn Confined>>,
    /// A observação pedida nesta conexão (os eventos vão pela thread).
    observing: bool,
    /// Eventos de uma observação nova, para a thread que os manda.
    new_events: Option<Box<dyn Events>>,
    /// O escopo de sandbox registrado nesta conexão.
    scoped: Option<Box<dyn Scoped>>,
}

fn err(msg: impl Into<String>) -> Value {
    json!({"ok": false, "error": msg.into()})
}

impl Server {
    pub fn new(backend: Box<dyn Backend>) -> Arc<Server> {
        Arc::new(Server {
            backend,
            active: Mutex::new(HashMap::new()),
            observing: Mutex::new(HashMap::new()),
            scoping: Mutex::new(HashMap::new()),
        })
    }

    fn handle(&self, c: &mut Conn, msg: &[u8], fds: Vec<OwnedFd>) -> Value {
        let Ok(v) = serde_json::from_slice::<Value>(msg) else {
            return err(tr!("pedido não é JSON", "request is not JSON"));
        };
        match v.get("op").and_then(Value::as_str) {
            Some("status") => {
                let (lsm, reason) = match self.backend.lsm() {
                    Ok(b) => (b, Value::Null),
                    Err(e) => (false, Value::String(e)),
                };
                let (scope, scope_reason) = match self.backend.scope_supported() {
                    Ok(()) => (true, Value::Null),
                    Err(e) => (false, Value::String(e)),
                };
                json!({"ok": true, "version": env!("CARGO_PKG_VERSION"), "lsm": lsm, "reason": reason,
                    "scope": scope, "scope_reason": scope_reason})
            }
            Some("confine") => {
                if c.confined.is_some() || c.observing {
                    return err(tr!(
                        "esta conexão já tem um confinamento",
                        "this connection already has a confinement"
                    ));
                }
                let Ok([cg]) = <[OwnedFd; 1]>::try_from(fds) else {
                    return err(tr!(
                        "confine leva um descritor, o do cgroup",
                        "confine takes one descriptor, the cgroup's"
                    ));
                };
                if let Err(e) = self.backend.check_cgroup(&cg, c.uid) {
                    return err(e);
                }
                {
                    let mut a = self.active.lock().unwrap_or_else(|e| e.into_inner());
                    let n = a.entry(c.uid).or_default();
                    if *n >= MAX_CONFINE_PER_UID {
                        return err(trf!(
                            "limite de {MAX_CONFINE_PER_UID} confinamentos ativos por usuário",
                            "limit of {MAX_CONFINE_PER_UID} active confinements per user"
                        ));
                    }
                    *n += 1;
                }
                let dev = v.get("dev").and_then(Value::as_u64).unwrap_or(0) as u32;
                match self.backend.confine(&cg, dev) {
                    Ok(x) => {
                        let attach = x.attach();
                        eprintln!(
                            "{}",
                            trf!(
                                "sandmux-helper: confinamento ligado para o uid {} (anexo {attach})",
                                "sandmux-helper: confinement on for uid {} ({attach} attach)",
                                c.uid
                            )
                        );
                        c.confined = Some(x);
                        json!({"ok": true, "attach": attach})
                    }
                    Err(e) => {
                        self.release(c.uid);
                        eprintln!("sandmux-helper: uid {}: {e}", c.uid);
                        err(e)
                    }
                }
            }
            Some("observe") => {
                if c.observing || c.confined.is_some() {
                    return err(tr!(
                        "esta conexão já tem um confinamento ou uma observação",
                        "this connection already has a confinement or an observation"
                    ));
                }
                let Ok([cg]) = <[OwnedFd; 1]>::try_from(fds) else {
                    return err(tr!(
                        "observe leva um descritor, o do cgroup",
                        "observe takes one descriptor, the cgroup's"
                    ));
                };
                if let Err(e) = self.backend.check_cgroup(&cg, c.uid) {
                    return err(e);
                }
                {
                    let mut a = self.observing.lock().unwrap_or_else(|e| e.into_inner());
                    let n = a.entry(c.uid).or_default();
                    if *n >= MAX_OBSERVE_PER_UID {
                        return err(trf!(
                            "limite de {MAX_OBSERVE_PER_UID} observações ativas por usuário",
                            "limit of {MAX_OBSERVE_PER_UID} active observations per user"
                        ));
                    }
                    *n += 1;
                }
                match self.backend.observe(&cg) {
                    Ok(src) => {
                        c.observing = true;
                        c.new_events = Some(src);
                        json!({"ok": true})
                    }
                    Err(e) => {
                        self.release_observe(c.uid);
                        eprintln!("sandmux-helper: uid {}: {e}", c.uid);
                        err(e)
                    }
                }
            }
            Some(op @ ("mark" | "unmark")) => {
                let Some(kind) = v
                    .get("kind")
                    .and_then(Value::as_str)
                    .and_then(MarkKind::parse)
                else {
                    return err(tr!("marca desconhecida", "unknown mark"));
                };
                let Some(x) = c.confined.as_mut() else {
                    return err(tr!(
                        "mark ou unmark antes de confine",
                        "mark or unmark before confine"
                    ));
                };
                if op == "mark" && x.marks() + fds.len() > MAX_MARKS {
                    return err(trf!(
                        "limite de {MAX_MARKS} marcas por confinamento",
                        "limit of {MAX_MARKS} marks per confinement"
                    ));
                }
                for fd in fds {
                    let r = if op == "mark" {
                        x.mark(fd, kind.bits())
                    } else {
                        x.unmark(fd, kind.bits())
                    };
                    if let Err(e) = r {
                        return err(e);
                    }
                }
                json!({"ok": true, "marks": x.marks()})
            }
            Some("scope") => {
                if c.scoped.is_some() || c.observing || c.confined.is_some() {
                    return err(tr!(
                        "esta conexão já tem um pedido ativo",
                        "this connection already has an active request"
                    ));
                }
                let Ok([ns]) = <[OwnedFd; 1]>::try_from(fds) else {
                    return err(tr!(
                        "scope leva um descritor, o do user namespace",
                        "scope takes one descriptor, the user namespace's"
                    ));
                };
                if let Err(e) = self.backend.check_userns(&ns, c.uid) {
                    return err(e);
                }
                let devs: Vec<(u32, u32)> = v
                    .get("devs")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|d| {
                                Some((d.get(0)?.as_u64()? as u32, d.get(1)?.as_u64()? as u32))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let opts = if v.get("ioctl") == Some(&Value::Bool(true)) {
                    sandmux_ebpf::scope::OPT_IOCTL
                } else {
                    0
                };
                {
                    let mut a = self.scoping.lock().unwrap_or_else(|e| e.into_inner());
                    let n = a.entry(c.uid).or_default();
                    if *n >= MAX_SCOPE_PER_UID {
                        return err(trf!(
                            "limite de {MAX_SCOPE_PER_UID} escopos ativos por usuário",
                            "limit of {MAX_SCOPE_PER_UID} active scopes per user"
                        ));
                    }
                    *n += 1;
                }
                let init_pid = v.get("init_pid").and_then(Value::as_u64).unwrap_or(0) as u32;
                match self.backend.scope(&ns, opts, init_pid, &devs) {
                    Ok(x) => {
                        c.scoped = Some(x);
                        json!({"ok": true})
                    }
                    Err(e) => {
                        self.release_scope(c.uid);
                        eprintln!("sandmux-helper: uid {}: {e}", c.uid);
                        err(e)
                    }
                }
            }
            _ => err(tr!("pedido desconhecido", "unknown request")),
        }
    }

    fn release_scope(&self, uid: u32) {
        let mut a = self.scoping.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = a.get_mut(&uid) {
            *n = n.saturating_sub(1);
        }
    }

    fn release_observe(&self, uid: u32) {
        let mut a = self.observing.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = a.get_mut(&uid) {
            *n = n.saturating_sub(1);
        }
    }

    fn release(&self, uid: u32) {
        let mut a = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = a.get_mut(&uid) {
            *n = n.saturating_sub(1);
        }
    }

    /// Atende uma conexão até ela fechar; depois desfaz o confinamento. Os
    /// eventos do confinamento vão pela mesma conexão, de outra thread.
    pub fn serve(self: &Arc<Server>, sock: OwnedFd) {
        let Ok(uid) = peer_uid(&sock) else { return };
        let sock = Arc::new(sock);
        let send_lock = Arc::new(Mutex::new(()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut c = Conn {
            uid,
            confined: None,
            observing: false,
            new_events: None,
            scoped: None,
        };
        let mut bad = 0;
        loop {
            let reply = match recv(&*sock) {
                Ok(None) => break,
                Ok(Some((msg, fds))) => {
                    let had = c.confined.is_some();
                    let r = self.handle(&mut c, &msg, fds);
                    if !had && let Some(src) = c.confined.as_ref().and_then(|x| x.events()) {
                        forward_events(src, sock.clone(), send_lock.clone(), stop.clone());
                    }
                    if let Some(src) = c.new_events.take() {
                        forward_events(src, sock.clone(), send_lock.clone(), stop.clone());
                    }
                    r
                }
                Err(e) if e.kind() == io::ErrorKind::InvalidData => err(e.to_string()),
                Err(_) => break,
            };
            if reply["ok"] == false {
                bad += 1;
                if bad > 32 {
                    break;
                }
            }
            let _g = send_lock.lock().unwrap_or_else(|e| e.into_inner());
            if send(&*sock, reply.to_string().as_bytes(), &[]).is_err() {
                break;
            }
        }
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if c.confined.take().is_some() {
            self.release(uid);
        }
        if c.observing {
            self.release_observe(uid);
        }
        if c.scoped.take().is_some() {
            self.release_scope(uid);
        }
    }
}

/// Uma thread que lê os eventos do confinamento e os manda em lotes, até a
/// conexão terminar.
fn forward_events(
    mut src: Box<dyn Events>,
    sock: Arc<OwnedFd>,
    send_lock: Arc<Mutex<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    let _ = std::thread::Builder::new()
        .name("helper-eventos".into())
        .spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let (evs, dropped) = src.poll(300);
                if evs.is_empty() && dropped == 0 {
                    continue;
                }
                // Lotes que cabem numa mensagem.
                let mut batch = Vec::new();
                let mut size = 0;
                let flush = |batch: &mut Vec<Value>, dropped: u64| -> bool {
                    let msg = json!({"ev": batch, "dropped": dropped}).to_string();
                    batch.clear();
                    let _g = send_lock.lock().unwrap_or_else(|e| e.into_inner());
                    send(&*sock, msg.as_bytes(), &[]).is_ok()
                };
                let mut d = dropped;
                for e in evs {
                    let n = e.to_string().len() + 1;
                    if size + n > MAX_MSG - 64 && !batch.is_empty() {
                        if !flush(&mut batch, d) {
                            return;
                        }
                        d = 0;
                        size = 0;
                    }
                    size += n;
                    batch.push(e);
                }
                if (!batch.is_empty() || d > 0) && !flush(&mut batch, d) {
                    return;
                }
            }
        });
}

/// A carga real, pelo `sandmux-ebpf`.
pub struct Ebpf {
    layout: Mutex<Option<Result<sandmux_ebpf::prog::Layout, String>>>,
    exec: Mutex<Option<Result<sandmux_ebpf::exec::ExecLayout, String>>>,
    scopes: Arc<Mutex<ScopeState>>,
}

/// Os programas do escopo, carregados no primeiro registro e soltos quando
/// não resta nenhum.
#[derive(Default)]
struct ScopeState {
    layout: Option<Result<sandmux_ebpf::scope::ScopeLayout, String>>,
    loaded: Option<sandmux_ebpf::scope::Scopes>,
    inums: std::collections::HashSet<u32>,
}

impl ScopeState {
    fn layout(&mut self) -> Result<sandmux_ebpf::scope::ScopeLayout, String> {
        self.layout
            .get_or_insert_with(|| {
                sandmux_ebpf::btf::Btf::vmlinux()
                    .and_then(|b| sandmux_ebpf::scope::ScopeLayout::new(&b))
                    .map_err(|e| e.to_string())
            })
            .clone()
    }
}

/// Um escopo registrado no mapa.
struct RealScope {
    st: Arc<Mutex<ScopeState>>,
    inum: u32,
}

impl Scoped for RealScope {}

impl Drop for RealScope {
    fn drop(&mut self) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = &st.loaded {
            let _ = s.unregister(self.inum);
        }
        st.inums.remove(&self.inum);
        if st.inums.is_empty() {
            st.loaded = None;
        }
    }
}

impl Ebpf {
    pub fn new() -> Ebpf {
        Ebpf {
            layout: Mutex::new(None),
            exec: Mutex::new(None),
            scopes: Arc::new(Mutex::new(ScopeState::default())),
        }
    }
}

impl Default for Ebpf {
    fn default() -> Self {
        Ebpf::new()
    }
}

struct Real(
    sandmux_ebpf::prog::Confinement,
    /// O programa de execução, na fila do confinamento (programa e link).
    #[allow(dead_code)]
    Option<(OwnedFd, OwnedFd)>,
);

impl Confined for Real {
    fn mark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), String> {
        self.0.mark(fd, bits).map_err(|e| e.to_string())
    }
    fn unmark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), String> {
        self.0.unmark(&fd, bits).map_err(|e| e.to_string())
    }
    fn marks(&self) -> usize {
        self.0.marks()
    }
    fn events(&self) -> Option<Box<dyn Events>> {
        let ring = self.0.ring().ok()?;
        let drops = self.0.drops().ok()?;
        let reader = sandmux_ebpf::ring::Reader::new(ring, sandmux_ebpf::ring::RING_SIZE).ok()?;
        Some(Box::new(RealEvents { reader, drops }))
    }
    fn attach(&self) -> &'static str {
        match self.0.attach {
            sandmux_ebpf::prog::Attach::Cgroup => "cgroup",
            sandmux_ebpf::prog::Attach::Global { .. } => "global",
        }
    }
}

struct RealEvents {
    reader: sandmux_ebpf::ring::Reader,
    drops: OwnedFd,
}

/// Um evento do kernel como JSON, com o nome do gancho.
pub fn event_json(e: &sandmux_ebpf::ring::Event) -> Value {
    if e.kind == sandmux_ebpf::ring::EV_EXEC {
        return json!({"kind": "exec", "pid": e.ino, "path": e.name, "args": e.args});
    }
    let hook = sandmux_ebpf::prog::HOOKS
        .get(e.hook as usize)
        .map(|h| h.name())
        .unwrap_or("?");
    json!({
        "kind": if e.kind == sandmux_ebpf::ring::EV_CREATE { "create" } else { "deny" },
        "hook": hook,
        "bits": e.bits,
        "write": e.write,
        "ino": e.ino,
        "dev": e.dev,
        "name": e.name,
    })
}

impl Events for RealEvents {
    fn poll(&mut self, timeout_ms: i32) -> (Vec<Value>, u64) {
        let evs = self
            .reader
            .poll(timeout_ms)
            .iter()
            .filter_map(|r| {
                if let Some(c) = sandmux_ebpf::ring::Conn::decode(r) {
                    return Some(json!({
                        "kind": "conn",
                        "pid": c.pid,
                        "sport": c.sport,
                        "dport": c.dport,
                        "ip": c.ip.to_string(),
                    }));
                }
                sandmux_ebpf::ring::Event::decode(r).map(|e| event_json(&e))
            })
            .collect();
        (evs, sandmux_ebpf::prog::take_drops(&self.drops))
    }
}

/// Nível e id de um cgroup v2 pelo descritor (para o anexo global).
fn cgroup_level_id(cg: &OwnedFd) -> Result<(i32, u64), String> {
    let link = std::fs::read_link(format!("/proc/self/fd/{}", cg.as_raw_fd()))
        .map_err(|e| e.to_string())?;
    let mount = cgroup2_mount().ok_or(tr!("sem montagem do cgroup2", "no cgroup2 mount"))?;
    let rel = link.strip_prefix(&mount).map_err(|_| {
        trf!(
            "{} fora de {}",
            "{} outside {}",
            link.display(),
            mount.display()
        )
    })?;
    let level = rel.components().count() as i32;
    let f = std::fs::File::from(cg.try_clone().map_err(|e| e.to_string())?);
    let ino = f.metadata().map_err(|e| e.to_string())?.ino();
    Ok((level, ino))
}

fn cgroup2_mount() -> Option<PathBuf> {
    let s = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    s.lines().find_map(|l| {
        let (pre, post) = l.split_once(" - ")?;
        if post.split_whitespace().next()? != "cgroup2" {
            return None;
        }
        pre.split_whitespace().nth(4).map(PathBuf::from)
    })
}

impl Ebpf {
    fn exec_layout(&self) -> Result<sandmux_ebpf::exec::ExecLayout, String> {
        let mut g = self.exec.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(|| {
            sandmux_ebpf::btf::Btf::vmlinux()
                .and_then(|b| sandmux_ebpf::exec::ExecLayout::new(&b))
                .map_err(|e| e.to_string())
        })
        .clone()
    }

    /// O confinamento com o programa de execução na fila dele. Sem o
    /// programa, o confinamento vale do mesmo jeito (com um aviso no log).
    fn with_exec(&self, c: sandmux_ebpf::prog::Confinement, cg: &OwnedFd) -> Real {
        let exec = (|| -> Result<(OwnedFd, OwnedFd), String> {
            let l = self.exec_layout()?;
            let (level, id) = cgroup_level_id(cg)?;
            let ring = c.ring().map_err(|e| e.to_string())?;
            let drops = c.drops().map_err(|e| e.to_string())?;
            sandmux_ebpf::exec::Observation::attach(&l, level, id, &ring, &drops)
                .map_err(|e| e.to_string())
        })();
        let exec = match exec {
            Ok(x) => Some(x),
            Err(e) => {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmux-helper: confinamento sem o registro de execuções: {e}",
                        "sandmux-helper: confinement without the exec log: {e}"
                    )
                );
                None
            }
        };
        Real(c, exec)
    }
}

struct ObsEvents {
    _obs: sandmux_ebpf::exec::Observation,
    inner: RealEvents,
}

impl Events for ObsEvents {
    fn poll(&mut self, timeout_ms: i32) -> (Vec<Value>, u64) {
        self.inner.poll(timeout_ms)
    }
}

impl Backend for Ebpf {
    fn lsm(&self) -> Result<bool, String> {
        sandmux_ebpf::lsm_active()
    }

    fn scope_supported(&self) -> Result<(), String> {
        if !sandmux_ebpf::lsm_active()? {
            return Err(tr!(
                "o BPF LSM não está ativo no kernel",
                "the BPF LSM is not active in the kernel"
            )
            .into());
        }
        let mut st = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        st.layout().map(|_| ())
    }

    fn scope(
        &self,
        ns: &OwnedFd,
        opts: u32,
        init_pid: u32,
        devs: &[(u32, u32)],
    ) -> Result<Box<dyn Scoped>, String> {
        self.scope_supported()?;
        let f = std::fs::File::from(ns.try_clone().map_err(|e| e.to_string())?);
        let inum = f.metadata().map_err(|e| e.to_string())?.ino() as u32;
        let devs: Vec<sandmux_ebpf::scope::Dev> = devs
            .iter()
            .map(|&(dev, mask)| sandmux_ebpf::scope::Dev { dev, mask })
            .collect();
        let mut st = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        if st.inums.contains(&inum) {
            return Err(tr!(
                "esse user namespace já tem um escopo",
                "that user namespace already has a scope"
            )
            .into());
        }
        if st.loaded.is_none() {
            let l = st.layout()?;
            st.loaded = Some(sandmux_ebpf::scope::Scopes::load(&l).map_err(|e| e.to_string())?);
        }
        let s = st.loaded.as_ref().expect("carregado acima");
        if let Err(e) = s.register(inum, opts, init_pid, &devs) {
            if st.inums.is_empty() {
                st.loaded = None;
            }
            return Err(e.to_string());
        }
        st.inums.insert(inum);
        Ok(Box::new(RealScope {
            st: self.scopes.clone(),
            inum,
        }))
    }

    fn observe(&self, cg: &OwnedFd) -> Result<Box<dyn Events>, String> {
        let l = self.exec_layout()?;
        let (level, id) = cgroup_level_id(cg)?;
        let obs =
            sandmux_ebpf::exec::Observation::load(&l, level, id).map_err(|e| e.to_string())?;
        let reader = sandmux_ebpf::ring::Reader::new(
            obs.ring().map_err(|e| e.to_string())?,
            sandmux_ebpf::ring::RING_SIZE,
        )
        .map_err(|e| e.to_string())?;
        let drops = obs.drops().map_err(|e| e.to_string())?;
        Ok(Box::new(ObsEvents {
            _obs: obs,
            inner: RealEvents { reader, drops },
        }))
    }

    fn confine(&self, cg: &OwnedFd, dev: u32) -> Result<Box<dyn Confined>, String> {
        if !sandmux_ebpf::lsm_active()? {
            return Err(tr!(
                "o BPF LSM não está ativo no kernel",
                "the BPF LSM is not active in the kernel"
            )
            .into());
        }
        let mut g = self.layout.lock().unwrap_or_else(|e| e.into_inner());
        let l = g
            .get_or_insert_with(|| {
                sandmux_ebpf::btf::Btf::vmlinux()
                    .and_then(|b| sandmux_ebpf::prog::Layout::new(&b))
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)?;
        use sandmux_ebpf::prog::{Attach, Confinement};
        match Confinement::load(l, cg.as_raw_fd(), Attach::Cgroup, dev) {
            Ok(c) => Ok(Box::new(self.with_exec(c, cg))),
            Err(first) => {
                let (level, cgroup_id) = cgroup_level_id(cg)?;
                Confinement::load(l, -1, Attach::Global { level, cgroup_id }, dev)
                    .map(|c| Box::new(self.with_exec(c, cg)) as Box<dyn Confined>)
                    .map_err(|e| {
                        trf!(
                            "{first}\nrecuo global: {e}",
                            "{first}\nglobal fallback: {e}"
                        )
                    })
            }
        }
    }
}

// ---------------------------------------------------------------- cliente

/// A conexão do daemon com o auxiliar. Uma thread lê tudo o que chega:
/// as respostas vão para quem fez o pedido, e os eventos para o canal de
/// [`Client::take_events`]. O canal fecha quando a conexão termina.
pub struct Client {
    sock: Arc<OwnedFd>,
    call_lock: Mutex<()>,
    replies: Mutex<std::sync::mpsc::Receiver<Value>>,
    events: Mutex<Option<std::sync::mpsc::Receiver<Value>>>,
}

/// Abre o caminho com `O_PATH | O_NOFOLLOW`, sem precisar de permissão de
/// leitura no arquivo. Pelo `open` direto: no musl (os binários das releases),
/// o `O_ACCMODE` inclui o `O_PATH`, e o `custom_flags` do `OpenOptions` o tira;
/// o arquivo era aberto para leitura, e um `/etc/gshadow` dava `EACCES`.
pub fn open_path(p: &Path) -> io::Result<OwnedFd> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: caminho terminado em NUL; o descritor devolvido é nosso.
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descritor recém-aberto.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Resposta do `status`.
#[derive(Clone, Debug)]
pub struct Status {
    pub version: String,
    pub lsm: bool,
    pub reason: Option<String>,
    /// Os programas do escopo de sandbox carregam (o pedido `scope`).
    pub scope: bool,
    pub scope_reason: Option<String>,
}

fn seqpacket() -> io::Result<OwnedFd> {
    // SAFETY: socket devolve um descritor novo ou -1.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descritor recém-criado.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn sockaddr(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: struct zerada é válida.
    let mut a: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    a.sun_family = libc::AF_UNIX as _;
    let b = path.as_os_str().as_bytes();
    if b.len() >= a.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            tr!("caminho longo demais", "path too long"),
        ));
    }
    for (d, s) in a.sun_path.iter_mut().zip(b) {
        *d = *s as _;
    }
    let len = std::mem::size_of::<libc::sa_family_t>() + b.len() + 1;
    Ok((a, len as libc::socklen_t))
}

/// Socket de escuta `SOCK_SEQPACKET` em `path`, com o modo 0666 (quem pode o
/// quê decide o `SO_PEERCRED`).
pub fn listen(path: &Path) -> io::Result<OwnedFd> {
    let _ = std::fs::remove_file(path);
    let s = seqpacket()?;
    let (a, len) = sockaddr(path)?;
    // SAFETY: endereço válido do tamanho dado.
    if unsafe { libc::bind(s.as_raw_fd(), (&a as *const libc::sockaddr_un).cast(), len) } < 0 {
        return Err(io::Error::last_os_error());
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
    // SAFETY: socket ligado.
    if unsafe { libc::listen(s.as_raw_fd(), 64) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(s)
}

/// Aceita conexões e atende cada uma numa thread.
pub fn accept_loop(server: Arc<Server>, listener: OwnedFd) -> io::Result<()> {
    loop {
        // SAFETY: accept4 devolve um descritor novo ou -1.
        let fd = unsafe {
            libc::accept4(
                listener.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };
        if fd < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        // SAFETY: descritor recém-aceito.
        let conn = unsafe { OwnedFd::from_raw_fd(fd) };
        let s = server.clone();
        let _ = std::thread::Builder::new()
            .name("helper-conn".into())
            .spawn(move || s.serve(conn));
    }
}

impl Client {
    pub fn connect(path: &Path) -> io::Result<Client> {
        let s = seqpacket()?;
        let (a, len) = sockaddr(path)?;
        // SAFETY: endereço válido do tamanho dado.
        if unsafe { libc::connect(s.as_raw_fd(), (&a as *const libc::sockaddr_un).cast(), len) } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let sock = Arc::new(s);
        let (rep_tx, rep_rx) = std::sync::mpsc::channel();
        let (ev_tx, ev_rx) = std::sync::mpsc::channel();
        let r = sock.clone();
        std::thread::Builder::new()
            .name("helper-leitura".into())
            .spawn(move || {
                while let Ok(Some((msg, _))) = recv(&*r) {
                    let Ok(v) = serde_json::from_slice::<Value>(&msg) else {
                        continue;
                    };
                    if v.get("ev").is_some() {
                        let _ = ev_tx.send(v);
                    } else if rep_tx.send(v).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Client {
            sock,
            call_lock: Mutex::new(()),
            replies: Mutex::new(rep_rx),
            events: Mutex::new(Some(ev_rx)),
        })
    }

    fn call(&self, req: Value, fds: &[RawFd]) -> Result<Value, String> {
        let _g = self.call_lock.lock().unwrap_or_else(|e| e.into_inner());
        send(&*self.sock, req.to_string().as_bytes(), fds).map_err(|e| e.to_string())?;
        let v = self
            .replies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| {
                tr!(
                    "o auxiliar fechou a conexão ou não respondeu",
                    "the helper closed the connection or did not answer"
                )
                .to_string()
            })?;
        if v["ok"] == true {
            Ok(v)
        } else {
            Err(v["error"]
                .as_str()
                .unwrap_or(tr!("erro sem texto", "error without text"))
                .to_string())
        }
    }

    /// Liga a observação das execuções no cgroup do diretório `dir`.
    pub fn observe(&self, dir: &Path) -> Result<(), String> {
        let f = std::fs::File::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.call(json!({"op": "observe"}), &[f.as_raw_fd()])?;
        Ok(())
    }

    /// O canal dos eventos (só uma vez). Ele fecha quando a conexão com o
    /// auxiliar termina.
    pub fn take_events(&self) -> Option<std::sync::mpsc::Receiver<Value>> {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn status(&self) -> Result<Status, String> {
        let v = self.call(json!({"op": "status"}), &[])?;
        Ok(Status {
            version: v["version"].as_str().unwrap_or("").to_string(),
            lsm: v["lsm"] == true,
            reason: v["reason"].as_str().map(str::to_string),
            scope: v["scope"] == true,
            scope_reason: v["scope_reason"].as_str().map(str::to_string),
        })
    }

    /// Registra o user namespace `ns` de um sandbox: o escopo vale até esta
    /// conexão fechar.
    pub fn scope(&self, ns: &OwnedFd, ioctl: bool, devs: &[(u32, u32)]) -> Result<(), String> {
        self.call(
            json!({"op": "scope", "ioctl": ioctl, "devs": devs}),
            &[ns.as_raw_fd()],
        )
        .map(|_| ())
    }

    /// Liga o confinamento no cgroup do diretório `dir`. Devolve o anexo
    /// usado (`cgroup` ou `global`).
    pub fn confine(&self, dir: &Path, dev: u32) -> Result<String, String> {
        let f = std::fs::File::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let v = self.call(json!({"op": "confine", "dev": dev}), &[f.as_raw_fd()])?;
        Ok(v["attach"].as_str().unwrap_or("").to_string())
    }

    /// Marca os caminhos, abertos aqui com `O_PATH`, em lotes. Os que não
    /// existem são pulados. Devolve quantos foram marcados.
    pub fn mark(&self, kind: MarkKind, paths: &[PathBuf]) -> Result<usize, String> {
        self.send_paths("mark", kind, paths)
    }

    /// Tira as marcas `kind` dos caminhos. Os que não existem são pulados.
    pub fn unmark(&self, kind: MarkKind, paths: &[PathBuf]) -> Result<usize, String> {
        self.send_paths("unmark", kind, paths)
    }

    fn send_paths(&self, op: &str, kind: MarkKind, paths: &[PathBuf]) -> Result<usize, String> {
        let mut n = 0;
        for chunk in paths.chunks(MAX_FDS) {
            let mut files = Vec::new();
            for p in chunk {
                match open_path(p) {
                    Ok(f) => files.push(f),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("{}: {e}", p.display())),
                }
            }
            if files.is_empty() {
                continue;
            }
            let fds: Vec<RawFd> = files.iter().map(|f| f.as_raw_fd()).collect();
            self.call(json!({"op": op, "kind": kind.as_str()}), &fds)?;
            n += files.len();
        }
        Ok(n)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // A thread de leitura segura uma cópia do socket: sem o shutdown, a
        // conexão ficaria aberta, e as regras do confinamento junto.
        // SAFETY: shutdown num descritor nosso.
        unsafe { libc::shutdown(self.sock.as_raw_fd(), libc::SHUT_RDWR) };
    }
}

/// O `s_dev` do superbloco de `path`, no formato do kernel (`major << 20 |
/// minor`), pelo `/proc/self/mountinfo`: o campo `major:minor` da montagem
/// mais longa que contém o caminho. No btrfs, o `stat` dá o dispositivo do
/// subvolume, e não o do superbloco que o kernel vê. 0 quando não acha.
pub fn kernel_dev(path: &Path) -> u32 {
    let Ok(s) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return 0;
    };
    kernel_dev_in(&s, path)
}

pub fn kernel_dev_in(mountinfo: &str, path: &Path) -> u32 {
    let mut best: Option<(usize, u32)> = None;
    for l in mountinfo.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let (Some(dev), Some(mp)) = (f.get(2), f.get(4)) else {
            continue;
        };
        let mp = mp.replace("\\040", " ");
        if !path.starts_with(&mp) {
            continue;
        }
        let Some((ma, mi)) = dev.split_once(':') else {
            continue;
        };
        let (Ok(ma), Ok(mi)) = (ma.parse::<u32>(), mi.parse::<u32>()) else {
            continue;
        };
        if best.is_none_or(|(n, _)| mp.len() >= n) {
            best = Some((mp.len(), (ma << 20) | mi));
        }
    }
    best.map(|b| b.1).unwrap_or(0)
}

/// O confinamento do Docker por eBPF está disponível nesta máquina? `Ok`
/// com o resumo, `Err` com o motivo (para o `sandmux doctor`).
pub fn availability() -> Result<String, String> {
    availability_with(sandmux_ebpf::lsm_active(), &socket_path())
}

pub fn availability_with(lsm: Result<bool, String>, socket: &Path) -> Result<String, String> {
    use sandmux_spec::{tr, trf};
    match lsm {
        Ok(true) => {}
        Ok(false) => {
            return Err(tr!(
                "o BPF LSM não está ativo no kernel (falta `bpf` no parâmetro lsm=)",
                "the BPF LSM is not active in the kernel (`bpf` missing from the lsm= parameter)"
            )
            .into());
        }
        Err(e) => return Err(e),
    }
    let c = Client::connect(socket).map_err(|e| {
        trf!(
            "o auxiliar root não responde em {}: {e}",
            "the root helper does not answer at {}: {e}",
            socket.display()
        )
    })?;
    let st = c.status()?;
    if !st.lsm {
        return Err(st.reason.unwrap_or_else(|| {
            tr!(
                "o auxiliar diz que o BPF LSM não está ativo",
                "the helper says the BPF LSM is not active"
            )
            .into()
        }));
    }
    Ok(trf!(
        "auxiliar root {} em {}, BPF LSM ativo",
        "root helper {} at {}, BPF LSM active",
        st.version,
        socket.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        loaded: Arc<AtomicUsize>,
        cgroup_ok: bool,
        asked: Arc<Mutex<Option<u32>>>,
    }
    struct FakeConfined {
        loaded: Arc<AtomicUsize>,
        marks: Arc<Mutex<Vec<u32>>>,
    }
    impl Drop for FakeConfined {
        fn drop(&mut self) {
            self.loaded.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl Confined for FakeConfined {
        fn mark(&mut self, _fd: OwnedFd, bits: u32) -> Result<(), String> {
            self.marks.lock().unwrap().push(bits);
            Ok(())
        }
        fn unmark(&mut self, _fd: OwnedFd, bits: u32) -> Result<(), String> {
            let mut m = self.marks.lock().unwrap();
            if let Some(i) = m.iter().position(|b| *b == bits) {
                m.remove(i);
            }
            Ok(())
        }
        fn marks(&self) -> usize {
            self.marks.lock().unwrap().len()
        }
        fn attach(&self) -> &'static str {
            "cgroup"
        }
        fn events(&self) -> Option<Box<dyn Events>> {
            Some(Box::new(FakeEvents(0)))
        }
    }
    /// Uma execução na primeira volta; depois, nada.
    struct ExecEvents(u32);
    impl Events for ExecEvents {
        fn poll(&mut self, _timeout_ms: i32) -> (Vec<Value>, u64) {
            self.0 += 1;
            if self.0 == 1 {
                (
                    vec![
                        json!({"kind": "exec", "pid": 7, "path": "/usr/bin/ls", "args": ["ls", "-la"]}),
                    ],
                    0,
                )
            } else {
                std::thread::sleep(std::time::Duration::from_millis(20));
                (vec![], 0)
            }
        }
    }
    /// Um bloqueio na primeira volta, com 2 descartados; depois, nada.
    struct FakeEvents(u32);
    impl Events for FakeEvents {
        fn poll(&mut self, _timeout_ms: i32) -> (Vec<Value>, u64) {
            self.0 += 1;
            if self.0 == 1 {
                (vec![json!({"kind": "deny", "ino": 42})], 2)
            } else {
                std::thread::sleep(std::time::Duration::from_millis(20));
                (vec![], 0)
            }
        }
    }
    impl Backend for Fake {
        fn lsm(&self) -> Result<bool, String> {
            Ok(true)
        }
        fn observe(&self, _cg: &OwnedFd) -> Result<Box<dyn Events>, String> {
            Ok(Box::new(ExecEvents(0)))
        }
        fn confine(&self, _cg: &OwnedFd, _dev: u32) -> Result<Box<dyn Confined>, String> {
            self.loaded.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeConfined {
                loaded: self.loaded.clone(),
                marks: Arc::new(Mutex::new(Vec::new())),
            }))
        }
        fn check_cgroup(&self, _cg: &OwnedFd, uid: u32) -> Result<(), String> {
            *self.asked.lock().unwrap() = Some(uid);
            if self.cgroup_ok {
                Ok(())
            } else {
                Err("o cgroup pertence a outro uid".into())
            }
        }
        fn scope_supported(&self) -> Result<(), String> {
            Ok(())
        }
        fn scope(
            &self,
            _ns: &OwnedFd,
            opts: u32,
            _init_pid: u32,
            devs: &[(u32, u32)],
        ) -> Result<Box<dyn Scoped>, String> {
            self.loaded.fetch_add(1, Ordering::SeqCst);
            *self.asked.lock().unwrap() = Some(opts * 1000 + devs.len() as u32);
            Ok(Box::new(FakeScope(self.loaded.clone())))
        }
    }
    struct FakeScope(Arc<AtomicUsize>);
    impl Scoped for FakeScope {}
    impl Drop for FakeScope {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Um processo num user namespace novo, do usuário do teste.
    fn child_userns() -> Option<(std::process::Child, OwnedFd)> {
        let mut c = std::process::Command::new("unshare")
            .args(["-U", "sleep", "30"])
            .spawn()
            .ok()?;
        let own = std::fs::metadata("/proc/self/ns/user").ok()?.ino();
        for _ in 0..200 {
            if let Ok(f) = std::fs::File::open(format!("/proc/{}/ns/user", c.id()))
                && f.metadata().ok()?.ino() != own
            {
                return Some((c, OwnedFd::from(f)));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = c.kill();
        None
    }

    #[test]
    fn marca_abre_arquivo_sem_leitura() {
        // O `O_PATH` não exige leitura: um arquivo 000 abre (o `/etc/gshadow`
        // das marcas do Docker). No musl, o `OpenOptions` perdia o `O_PATH`.
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("fechado");
        std::fs::write(&f, "x").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&f).is_ok() {
            return; // root lê tudo; o teste não diz nada
        }
        let fd = open_path(&f).unwrap();
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::fstat(fd.as_raw_fd(), &mut st) }, 0);
        // E um link simbólico não é seguido.
        let l = d.path().join("link");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        let fd = open_path(&l).unwrap();
        assert_eq!(unsafe { libc::fstat(fd.as_raw_fd(), &mut st) }, 0);
        assert_eq!(st.st_mode & libc::S_IFMT, libc::S_IFLNK);
    }

    #[test]
    fn escopo_de_um_user_namespace() {
        let (_d, p, loaded, asked) = start(true);
        let c = Client::connect(&p).unwrap();
        assert!(c.status().unwrap().scope);
        // O user namespace do próprio auxiliar (e do teste) é recusado.
        let own = OwnedFd::from(std::fs::File::open("/proc/self/ns/user").unwrap());
        let e = c.scope(&own, true, &[]).unwrap_err();
        assert!(e.contains("próprio auxiliar"), "{e}");
        // Um arquivo qualquer não é user namespace.
        let f = OwnedFd::from(std::fs::File::open("/proc/self/stat").unwrap());
        let e = c.scope(&f, true, &[]).unwrap_err();
        assert!(e.contains("não é de um user namespace"), "{e}");
        let Some((mut kid, ns)) = child_userns() else {
            return;
        };
        c.scope(
            &ns,
            true,
            &[(1 << 20 | 3, u32::MAX), (136 << 20, 0xfff0_0000)],
        )
        .unwrap();
        assert_eq!(loaded.load(Ordering::SeqCst), 1);
        assert_eq!(*asked.lock().unwrap(), Some(1002));
        // Um por conexão.
        let e = c.scope(&ns, false, &[]).unwrap_err();
        assert!(e.contains("já tem"), "{e}");
        drop(c);
        wait_for(|| loaded.load(Ordering::SeqCst) == 0);
        let _ = kid.kill();
        let _ = kid.wait();
    }

    type Started = (
        tempfile::TempDir,
        PathBuf,
        Arc<AtomicUsize>,
        Arc<Mutex<Option<u32>>>,
    );

    fn start(cgroup_ok: bool) -> Started {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sock");
        let loaded = Arc::new(AtomicUsize::new(0));
        let asked = Arc::new(Mutex::new(None));
        let server = Server::new(Box::new(Fake {
            loaded: loaded.clone(),
            cgroup_ok,
            asked: asked.clone(),
        }));
        let l = listen(&path).unwrap();
        std::thread::spawn(move || accept_loop(server, l));
        (dir, path, loaded, asked)
    }

    fn wait_for(f: impl Fn() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("não aconteceu a tempo");
    }

    #[test]
    fn status_e_pedido_desconhecido() {
        let (_d, p, _, _) = start(true);
        let c = Client::connect(&p).unwrap();
        let s = c.status().unwrap();
        assert!(s.lsm);
        assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
        let e = c.call(json!({"op": "root"}), &[]).unwrap_err();
        assert!(e.contains("desconhecido"), "{e}");
        send(&c.sock, b"lixo", &[]).unwrap();
        let m = c
            .replies
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(m["error"].as_str().unwrap().contains("JSON"));
        // A conexão continua de pé.
        assert!(c.status().is_ok());
    }

    #[test]
    fn mark_antes_de_confine_e_cgroup_alheio() {
        let (d, p, loaded, _) = start(false);
        let c = Client::connect(&p).unwrap();
        let f = d.path().join("x");
        std::fs::write(&f, "").unwrap();
        let e = c
            .mark(MarkKind::Deny, std::slice::from_ref(&f))
            .unwrap_err();
        assert!(e.contains("antes de confine"), "{e}");
        let e = c.confine(d.path(), 0).unwrap_err();
        assert!(e.contains("outro uid"), "{e}");
        assert_eq!(loaded.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn confine_marca_e_solta_no_fim() {
        let (d, p, loaded, _) = start(true);
        let c = Client::connect(&p).unwrap();
        assert_eq!(c.confine(d.path(), 0).unwrap(), "cgroup");
        assert!(c.confine(d.path(), 0).unwrap_err().contains("já tem"));
        let mut paths = Vec::new();
        for i in 0..250 {
            let f = d.path().join(format!("f{i}"));
            std::fs::write(&f, "").unwrap();
            paths.push(f);
        }
        paths.push(d.path().join("nao-existe"));
        assert_eq!(c.mark(MarkKind::Protect, &paths).unwrap(), 250);
        assert_eq!(loaded.load(Ordering::SeqCst), 1);
        drop(c);
        wait_for(|| loaded.load(Ordering::SeqCst) == 0);
    }

    #[test]
    fn limite_de_confinamentos_por_usuario() {
        let (d, p, _, _) = start(true);
        let mut keep = Vec::new();
        for _ in 0..MAX_CONFINE_PER_UID {
            let c = Client::connect(&p).unwrap();
            c.confine(d.path(), 0).unwrap();
            keep.push(c);
        }
        let c = Client::connect(&p).unwrap();
        let e = c.confine(d.path(), 0).unwrap_err();
        assert!(e.contains("limite"), "{e}");
        // Fechar um libera a vaga.
        drop(keep.pop());
        wait_for(|| Client::connect(&p).unwrap().confine(d.path(), 0).is_ok());
    }

    #[test]
    fn uid_do_pedido_e_ignorado() {
        let (d, p, _, asked) = start(true);
        let c = Client::connect(&p).unwrap();
        let f = std::fs::File::open(d.path()).unwrap();
        c.call(json!({"op": "confine", "uid": 0}), &[f.as_raw_fd()])
            .unwrap();
        assert_eq!(*asked.lock().unwrap(), Some(unsafe { libc::geteuid() }));
    }

    #[test]
    fn conferencia_real_do_cgroup() {
        let d = tempfile::tempdir().unwrap();
        let f: OwnedFd = std::fs::File::open(d.path()).unwrap().into();
        let e = check_cgroup(&f, 1000).unwrap_err();
        assert!(e.contains("cgroup v2"), "{e}");
        // A raiz do cgroup2 é do root: um usuário comum é recusado.
        if let Some(m) = cgroup2_mount() {
            let f: OwnedFd = std::fs::File::open(&m).unwrap().into();
            let uid = unsafe { libc::geteuid() };
            if uid != 0 {
                let e = check_cgroup(&f, uid).unwrap_err();
                assert!(e.contains("pertence ao uid 0"), "{e}");
            }
        }
    }

    #[test]
    fn disponibilidade_para_o_diagnostico() {
        let e = availability_with(Ok(false), Path::new("/nao")).unwrap_err();
        assert!(e.contains("lsm="), "{e}");
        let e = availability_with(Ok(true), Path::new("/nonexistent/h.sock")).unwrap_err();
        assert!(
            e.contains("não responde") || e.contains("does not answer"),
            "{e}"
        );
        let (_d, p, _, _) = start(true);
        let ok = availability_with(Ok(true), &p).unwrap();
        assert!(ok.contains(env!("CARGO_PKG_VERSION")), "{ok}");
    }

    #[test]
    fn desmarcar_e_eventos_no_meio_dos_pedidos() {
        let (d, p, _, _) = start(true);
        let c = Client::connect(&p).unwrap();
        let ev = c.take_events().unwrap();
        assert!(c.take_events().is_none());
        c.confine(d.path(), 0).unwrap();
        let f = d.path().join("x");
        std::fs::write(&f, "").unwrap();
        assert_eq!(c.mark(MarkKind::Deny, std::slice::from_ref(&f)).unwrap(), 1);
        // O evento chega pelo canal, sem atrapalhar as respostas.
        for _ in 0..5 {
            assert!(c.status().is_ok());
        }
        let v = ev.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(v["dropped"], 2);
        assert_eq!(v["ev"][0]["ino"], 42);
        let r = c
            .call(
                json!({"op": "unmark", "kind": "deny"}),
                &[std::fs::File::open(&f).unwrap().as_raw_fd()],
            )
            .unwrap();
        assert_eq!(r["marks"], 0);
        // Desmarcar o que não está marcado não é erro.
        assert_eq!(
            c.unmark(MarkKind::Deny, std::slice::from_ref(&f)).unwrap(),
            1
        );
        // Fechar a conexão fecha o canal dos eventos.
        drop(c);
        let fim = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match ev.recv_timeout(std::time::Duration::from_millis(100)) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                _ => assert!(std::time::Instant::now() < fim, "o canal não fechou"),
            }
        }
    }

    #[test]
    fn observacao_das_execucoes() {
        let (d, p, _, _) = start(true);
        let c = Client::connect(&p).unwrap();
        let ev = c.take_events().unwrap();
        c.observe(d.path()).unwrap();
        // Uma por conexão, e sem confinamento junto.
        assert!(c.observe(d.path()).unwrap_err().contains("já tem"));
        assert!(c.confine(d.path(), 0).is_err());
        let v = ev.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(v["ev"][0]["kind"], "exec");
        assert_eq!(v["ev"][0]["args"][1], "-la");
        // O limite por usuário.
        let mut keep = Vec::new();
        for _ in 1..MAX_OBSERVE_PER_UID {
            let x = Client::connect(&p).unwrap();
            x.observe(d.path()).unwrap();
            keep.push(x);
        }
        let x = Client::connect(&p).unwrap();
        assert!(x.observe(d.path()).unwrap_err().contains("limite"));
        // Um cgroup alheio é recusado antes de qualquer carga.
        let (d2, p2, _, _) = start(false);
        let y = Client::connect(&p2).unwrap();
        assert!(y.observe(d2.path()).unwrap_err().contains("outro uid"));
    }

    #[test]
    fn dispositivo_do_kernel_pelo_mountinfo() {
        let mi = "22 1 259:2 / / rw - ext4 /dev/nvme0n1p2 rw\n\
                  40 22 0:35 /@home /home rw - btrfs /dev/sda1 rw\n\
                  41 40 0:36 / /home/u/montado\\040x rw - tmpfs t rw\n";
        assert_eq!(kernel_dev_in(mi, Path::new("/etc/x")), (259 << 20) | 2);
        assert_eq!(kernel_dev_in(mi, Path::new("/home/u/proj")), 35);
        assert_eq!(kernel_dev_in(mi, Path::new("/home/u/montado x/a")), 36);
        assert_eq!(kernel_dev_in("", Path::new("/x")), 0);
    }

    #[test]
    fn mensagem_grande_demais_e_recusada() {
        let (_d, p, _, _) = start(true);
        let c = Client::connect(&p).unwrap();
        assert!(send(&c.sock, &vec![b' '; MAX_MSG + 1], &[]).is_err());
    }
}
