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
    /// Segredo de uma aba (`ENOENT`).
    Secret,
    /// Negado pela política de arquivos de uma aba (`EACCES`).
    FileDeny,
}

impl MarkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkKind::Deny => "deny",
            MarkKind::Protect => "protect",
            MarkKind::GitDir => "gitdir",
            MarkKind::Secret => "secret",
            MarkKind::FileDeny => "fdeny",
        }
    }
    fn parse(s: &str) -> Option<MarkKind> {
        Some(match s {
            "deny" => MarkKind::Deny,
            "protect" => MarkKind::Protect,
            "gitdir" => MarkKind::GitDir,
            "secret" => MarkKind::Secret,
            "fdeny" => MarkKind::FileDeny,
            _ => return None,
        })
    }
    pub fn bits(self) -> u32 {
        match self {
            MarkKind::Deny => sandmux_ebpf::prog::DENY,
            MarkKind::Protect => sandmux_ebpf::prog::PROTECT,
            MarkKind::GitDir => sandmux_ebpf::prog::GITDIR,
            MarkKind::Secret => sandmux_ebpf::prog::SECRET,
            MarkKind::FileDeny => sandmux_ebpf::prog::FDENY,
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
    /// Os bytes que estavam na fila do kernel no último `poll` e o tamanho
    /// dela; `(0, 0)` sem fila do kernel.
    fn backlog(&self) -> (u64, u64) {
        (0, 0)
    }
}

/// Um escopo de sandbox registrado: sai do registro quando é solto.
pub trait Scoped: Send {}

/// O que carrega os programas. A versão real usa `sandmux-ebpf`; os testes,
/// uma falsa.
pub trait Backend: Send + Sync {
    fn lsm(&self) -> Result<bool, String>;
    /// Liga o confinamento no cgroup. `dev` é o `s_dev` do superbloco da
    /// pasta de trabalho (0 quando não se sabe), que filtra as criações.
    /// `open_mnts` são as montagens (`mnt_id`) cujas aberturas vão para os
    /// eventos (o registro de arquivos de uma aba); vazio no Docker.
    fn confine(
        &self,
        cgroup: &OwnedFd,
        dev: u32,
        open_mnts: &[u32],
    ) -> Result<Box<dyn Confined>, String>;
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
    /// Na subida: carrega já o que o primeiro pedido carregaria (os
    /// programas do escopo, nos kernels em que os sandboxes os pedem).
    fn preload(&self) {}
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
    /// As conexões abertas, para a saída limpa ([`Server::close_all`]).
    conns: Mutex<HashMap<u64, Arc<OwnedFd>>>,
    next_conn: std::sync::atomic::AtomicU64,
    /// A saída limpa começou: as conexões aceitas daqui em diante fecham sem
    /// ser atendidas, e quem pediu tenta no auxiliar seguinte.
    closing: std::sync::atomic::AtomicBool,
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
    /// O cgroup de uma aba confinada (com montagens a registrar), que a
    /// thread dos eventos congela com a fila cheia.
    freeze_cg: Option<OwnedFd>,
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
            conns: Mutex::new(HashMap::new()),
            next_conn: std::sync::atomic::AtomicU64::new(0),
            closing: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Carrega já o que o backend pode carregar na subida.
    pub fn preload(&self) {
        self.backend.preload();
    }

    /// A saída limpa (`SIGTERM`): cada conexão recebe o fim (`SHUT_WR`)
    /// enquanto as regras dela ainda valem, e quem pediu reage antes de elas
    /// saírem. Espera as conexões terminarem, até `wait`.
    pub fn close_all(&self, wait: std::time::Duration) {
        self.closing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for fd in self
            .conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
        {
            // SAFETY: shutdown num descritor vivo (o `Arc` o segura).
            unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_WR) };
        }
        let deadline = std::time::Instant::now() + wait;
        while std::time::Instant::now() < deadline
            && !self
                .conns
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
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
                // `tab`: as marcas de segredo e de `file-deny` e os eventos
                // de abertura (change 78); um auxiliar antigo não manda, e a
                // aba fica com o supervisor.
                json!({"ok": true, "version": env!("CARGO_PKG_VERSION"), "lsm": lsm, "reason": reason,
                    "scope": scope, "scope_reason": scope_reason, "tab": true})
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
                let open_mnts: Vec<u32> = v
                    .get("open_mnts")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_u64().map(|n| n as u32))
                            .collect()
                    })
                    .unwrap_or_default();
                match self.backend.confine(&cg, dev, &open_mnts) {
                    Ok(x) => {
                        // Uma aba: a thread dos eventos congela o cgroup com
                        // a fila cheia.
                        if !open_mnts.is_empty() {
                            c.freeze_cg = cg.try_clone().ok();
                        }
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
        let id = self
            .next_conn
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, sock.clone());
        let send_lock = Arc::new(Mutex::new(()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut c = Conn {
            uid,
            confined: None,
            observing: false,
            new_events: None,
            scoped: None,
            freeze_cg: None,
        };
        // Marcado antes de soltar o confinamento, sob a trava: depois disso a
        // thread dos eventos não descongela a aba.
        let ended = Arc::new(Mutex::new(false));
        let srv = self.clone();
        let closing = move || srv.closing.load(std::sync::atomic::Ordering::SeqCst);
        let mut bad = 0;
        loop {
            let reply = match recv(&*sock) {
                Ok(None) => break,
                Ok(Some((msg, fds))) => {
                    let had = c.confined.is_some();
                    let r = self.handle(&mut c, &msg, fds);
                    if !had && let Some(src) = c.confined.as_ref().and_then(|x| x.events()) {
                        let freeze = c.freeze_cg.take().map(|cg| Freeze {
                            cg,
                            uid,
                            ended: ended.clone(),
                            closing: Box::new(closing.clone()),
                        });
                        forward_events(src, sock.clone(), send_lock.clone(), stop.clone(), freeze);
                    }
                    if let Some(src) = c.new_events.take() {
                        forward_events(src, sock.clone(), send_lock.clone(), stop.clone(), None);
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
        *ended.lock().unwrap_or_else(|e| e.into_inner()) = true;
        if c.confined.take().is_some() {
            self.release(uid);
        }
        if c.observing {
            self.release_observe(uid);
        }
        if c.scoped.take().is_some() {
            self.release_scope(uid);
        }
        drop(c);
        self.conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

/// Acima disto na fila do auxiliar (eventos lidos e ainda não mandados), a
/// aba congela.
pub const QUEUE_HIGH: usize = 32768;
/// Abaixo disto, e com a fila do kernel abaixo de 1/4, a aba descongela.
pub const QUEUE_LOW: usize = 4096;

/// O congelamento de uma aba com a fila cheia: o cgroup, de quem é, a trava
/// do fim da conexão (com `true`, nada de descongelar) e se a saída limpa do
/// auxiliar começou.
pub struct Freeze {
    pub cg: OwnedFd,
    pub uid: u32,
    pub ended: Arc<Mutex<bool>>,
    pub closing: Box<dyn Fn() -> bool + Send>,
}

/// A fila passou do limite: a fila do kernel acima de 3/4 no último
/// esvaziamento, a do auxiliar acima de [`QUEUE_HIGH`], ou um descarte.
pub fn should_freeze(queued: usize, backlog: (u64, u64), dropped: u64) -> bool {
    dropped > 0 || queued > QUEUE_HIGH || (backlog.1 > 0 && backlog.0 * 4 > backlog.1 * 3)
}

/// As duas filas esvaziaram o bastante para a aba seguir.
pub fn may_thaw(queued: usize, backlog: (u64, u64)) -> bool {
    queued < QUEUE_LOW && backlog.0 * 4 <= backlog.1
}

impl Freeze {
    /// Escreve no `cgroup.freeze` do cgroup.
    fn write(&self, on: bool) -> io::Result<()> {
        // SAFETY: openat relativo ao descritor do cgroup, com um nome fixo.
        let fd = unsafe {
            libc::openat(
                self.cg.as_raw_fd(),
                c"cgroup.freeze".as_ptr(),
                libc::O_WRONLY | libc::O_TRUNC | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: descritor recém-aberto, nosso.
        let f = unsafe { OwnedFd::from_raw_fd(fd) };
        let v: &[u8] = if on { b"1" } else { b"0" };
        // SAFETY: escrita de um buffer válido num descritor nosso.
        if unsafe { libc::write(f.as_raw_fd(), v.as_ptr().cast(), v.len()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Descongela, se a conexão continua aberta e a saída limpa não
    /// começou: com a trava do fim tomada, o confinamento não sai no meio.
    /// Devolve se descongelou.
    fn thaw(&self) -> bool {
        let ended = self.ended.lock().unwrap_or_else(|e| e.into_inner());
        if *ended || (self.closing)() {
            return false;
        }
        match self.write(false) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("sandmux-helper: uid {}: cgroup.freeze: {e}", self.uid);
                false
            }
        }
    }
}

/// Os eventos lidos e ainda não mandados, entre a thread que esvazia a fila
/// do kernel e a que manda ao daemon.
#[derive(Default)]
struct Queue {
    items: std::collections::VecDeque<Value>,
    dropped: u64,
    /// Tirados da fila pela que manda e ainda não mandados.
    sending: usize,
    /// A leitura terminou: a que manda esvazia o que sobrou e sai.
    done: bool,
}

/// Duas threads por confinamento, até a conexão terminar: uma esvazia a fila
/// do kernel sem esperar o envio, e outra manda os eventos em lotes. Com
/// `freeze` (uma aba), a primeira congela o cgroup quando as filas passam do
/// limite ([`should_freeze`]) e o descongela quando esvaziam.
fn forward_events(
    mut src: Box<dyn Events>,
    sock: Arc<OwnedFd>,
    send_lock: Arc<Mutex<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    freeze: Option<Freeze>,
) {
    let q = Arc::new((Mutex::new(Queue::default()), std::sync::Condvar::new()));
    let qs = q.clone();
    let _ = std::thread::Builder::new()
        .name("helper-envio".into())
        .spawn(move || send_events(&qs, &sock, &send_lock));
    let _ = std::thread::Builder::new()
        .name("helper-eventos".into())
        .spawn(move || {
            let mut frozen: Option<std::time::Instant> = None;
            // Uma escrita no `cgroup.freeze` falhou (o cgroup sumiu): sem
            // insistir a cada volta.
            let mut broken = false;
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let (evs, dropped) = src.poll(if frozen.is_some() { 50 } else { 300 });
                let backlog = src.backlog();
                let queued = {
                    let (m, cv) = &*q;
                    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
                    if g.done {
                        break;
                    }
                    if !evs.is_empty() || dropped > 0 {
                        g.items.extend(evs);
                        g.dropped += dropped;
                        cv.notify_one();
                    }
                    g.items.len() + g.sending
                };
                let Some(f) = freeze.as_ref().filter(|_| !broken) else {
                    continue;
                };
                match frozen {
                    None if should_freeze(queued, backlog, dropped) => match f.write(true) {
                        Ok(()) => frozen = Some(std::time::Instant::now()),
                        Err(e) => {
                            broken = true;
                            eprintln!("sandmux-helper: uid {}: cgroup.freeze: {e}", f.uid);
                        }
                    },
                    Some(t) if may_thaw(queued, backlog) && f.thaw() => {
                        frozen = None;
                        let ms = t.elapsed().as_millis();
                        eprintln!(
                            "{}",
                            trf!(
                                "sandmux-helper: uid {}: aba congelada por {ms} ms (fila de eventos cheia)",
                                "sandmux-helper: uid {}: tab frozen for {ms} ms (event queue full)",
                                f.uid
                            )
                        );
                    }
                    _ => {}
                }
            }
            let (m, cv) = &*q;
            m.lock().unwrap_or_else(|e| e.into_inner()).done = true;
            cv.notify_one();
        });
}

/// A thread que manda: tira tudo o que está na fila e manda em lotes que
/// cabem numa mensagem, com os descartes no primeiro.
fn send_events(q: &(Mutex<Queue>, std::sync::Condvar), sock: &OwnedFd, send_lock: &Mutex<()>) {
    let (m, cv) = q;
    loop {
        let (evs, dropped) = {
            let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
            while g.items.is_empty() && g.dropped == 0 && !g.done {
                g = cv
                    .wait_timeout(g, std::time::Duration::from_millis(300))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            if g.items.is_empty() && g.dropped == 0 {
                return;
            }
            let evs: Vec<Value> = g.items.drain(..).collect();
            g.sending = evs.len();
            (evs, std::mem::take(&mut g.dropped))
        };
        let fail = |m: &Mutex<Queue>| {
            let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
            g.done = true;
            g.sending = 0;
        };
        let flush = |batch: &mut Vec<Value>, dropped: u64| -> bool {
            let msg = json!({"ev": batch, "dropped": dropped}).to_string();
            let n = batch.len();
            batch.clear();
            let ok = {
                let _g = send_lock.lock().unwrap_or_else(|e| e.into_inner());
                send(sock, msg.as_bytes(), &[]).is_ok()
            };
            let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
            g.sending = g.sending.saturating_sub(n);
            ok
        };
        let mut batch = Vec::new();
        let mut size = 0;
        let mut d = dropped;
        for e in evs {
            let n = e.to_string().len() + 1;
            if size + n > MAX_MSG - 64 && !batch.is_empty() {
                if !flush(&mut batch, d) {
                    return fail(m);
                }
                d = 0;
                size = 0;
            }
            size += n;
            batch.push(e);
        }
        if (!batch.is_empty() || d > 0) && !flush(&mut batch, d) {
            return fail(m);
        }
    }
}

/// A carga real, pelo `sandmux-ebpf`.
pub struct Ebpf {
    layout: Mutex<Option<Result<sandmux_ebpf::prog::Layout, String>>>,
    exec: Mutex<Option<Result<sandmux_ebpf::exec::ExecLayout, String>>>,
    scopes: Arc<Mutex<ScopeState>>,
}

/// Os programas do escopo, carregados no primeiro registro (ou na subida,
/// [`Backend::preload`]) e soltos depois de [`SCOPE_IDLE`] sem nenhum.
#[derive(Default)]
struct ScopeState {
    layout: Option<Result<sandmux_ebpf::scope::ScopeLayout, String>>,
    loaded: Option<sandmux_ebpf::scope::Scopes>,
    inums: std::collections::HashSet<u32>,
    /// Desde quando não resta escopo nenhum, com os programas carregados.
    idle_since: Option<std::time::Instant>,
    /// A thread que solta os programas parados já roda.
    reaper: bool,
}

/// Quanto os programas do escopo ficam carregados sem escopo nenhum. No 6.8,
/// carregar e soltar os quatro programas leva segundos (o `ftrace` espera o
/// RCU Tasks a cada um); ficar carregado por um tempo poupa a troca de
/// auxiliar e o relançamento de uma aba.
const SCOPE_IDLE: std::time::Duration = std::time::Duration::from_secs(60);

impl ScopeState {
    /// Carrega os programas, se ainda não estão, e liga a thread que os solta
    /// depois de parados.
    fn ensure_loaded(&mut self, st: &Arc<Mutex<ScopeState>>) -> Result<(), String> {
        if self.loaded.is_none() {
            let l = self.layout()?;
            self.loaded = Some(sandmux_ebpf::scope::Scopes::load(&l).map_err(|e| e.to_string())?);
            if self.inums.is_empty() {
                self.idle_since = Some(std::time::Instant::now());
            }
        }
        if !self.reaper {
            self.reaper = true;
            let st = st.clone();
            let _ = std::thread::Builder::new()
                .name("helper-escopo-parado".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        let mut s = st.lock().unwrap_or_else(|e| e.into_inner());
                        if s.inums.is_empty()
                            && s.idle_since.is_some_and(|t| t.elapsed() >= SCOPE_IDLE)
                        {
                            s.loaded = None;
                            s.idle_since = None;
                        }
                    }
                });
        }
        Ok(())
    }
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
            st.idle_since = Some(std::time::Instant::now());
        }
    }
}

/// A ABI do Landlock do kernel (0 sem o Landlock).
fn landlock_abi() -> i64 {
    // SAFETY: a consulta da versão não usa os ponteiros.
    let v = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0usize,
            1u32,
        )
    };
    v.max(0)
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
        let reader = sandmux_ebpf::ring::Reader::new(ring, self.0.ring_size()).ok()?;
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
    if e.kind == sandmux_ebpf::ring::EV_OPEN {
        return json!({"kind": "open", "write": e.write, "ino": e.ino, "mnt": e.dev, "path": e.name});
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
    fn backlog(&self) -> (u64, u64) {
        (self.reader.backlog(), self.reader.size())
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
    fn backlog(&self) -> (u64, u64) {
        self.inner.backlog()
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

    fn preload(&self) {
        // Só onde os lançadores pedem o escopo: o Landlock abaixo do ABI 6.
        if !(1..6).contains(&landlock_abi()) || self.scope_supported().is_err() {
            return;
        }
        let mut st = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = st.ensure_loaded(&self.scopes) {
            eprintln!(
                "{}",
                trf!(
                    "sandmux-helper: os programas do escopo não carregaram: {e}",
                    "sandmux-helper: the scope programs did not load: {e}"
                )
            );
        }
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
        st.ensure_loaded(&self.scopes)?;
        let s = st.loaded.as_ref().expect("carregado acima");
        s.register(inum, opts, init_pid, &devs)
            .map_err(|e| e.to_string())?;
        st.inums.insert(inum);
        st.idle_since = None;
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

    fn confine(
        &self,
        cg: &OwnedFd,
        dev: u32,
        open_mnts: &[u32],
    ) -> Result<Box<dyn Confined>, String> {
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
        match Confinement::load_with(l, cg.as_raw_fd(), Attach::Cgroup, dev, open_mnts) {
            Ok(c) => Ok(Box::new(self.with_exec(c, cg))),
            Err(first) => {
                let (level, cgroup_id) = cgroup_level_id(cg)?;
                Confinement::load_with(l, -1, Attach::Global { level, cgroup_id }, dev, open_mnts)
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
    /// Fecha quando a leitura da conexão termina.
    closed: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
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
    /// Confina os arquivos de uma aba: as marcas de segredo e de
    /// `file-deny` e os eventos de abertura.
    pub tab: bool,
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
        // Na saída limpa, a conexão nova fecha sem resposta.
        if server.closing.load(std::sync::atomic::Ordering::SeqCst) {
            continue;
        }
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
        let (closed_tx, closed_rx) = std::sync::mpsc::channel::<()>();
        let r = sock.clone();
        std::thread::Builder::new()
            .name("helper-leitura".into())
            .spawn(move || {
                // Solto no fim da leitura: a queda da conexão chega a quem
                // espera em `wait_closed`, sem esperar a fila de eventos.
                let _closed = closed_tx;
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
            closed: Mutex::new(Some(closed_rx)),
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
            tab: v["tab"] == true,
        })
    }

    /// Registra o user namespace `ns` de um sandbox: o escopo vale até esta
    /// conexão fechar.
    pub fn scope(&self, ns: &OwnedFd, ioctl: bool, devs: &[(u32, u32)]) -> Result<(), String> {
        self.scope_with(ns, ioctl, 0, devs)
    }

    /// Como [`Client::scope`], com o processo 1 do sandbox (o sinal a ele
    /// também é negado): o registro de novo de uma aba pelo daemon.
    pub fn scope_with(
        &self,
        ns: &OwnedFd,
        ioctl: bool,
        init_pid: u32,
        devs: &[(u32, u32)],
    ) -> Result<(), String> {
        self.call(
            json!({"op": "scope", "ioctl": ioctl, "init_pid": init_pid, "devs": devs}),
            &[ns.as_raw_fd()],
        )
        .map(|_| ())
    }

    /// Liga o confinamento no cgroup do diretório `dir`. Devolve o anexo
    /// usado (`cgroup` ou `global`).
    pub fn confine(&self, dir: &Path, dev: u32) -> Result<String, String> {
        self.confine_with(dir, dev, &[])
    }

    /// Como [`Client::confine`], com as montagens (`mnt_id`) cujas aberturas
    /// vêm nos eventos (o registro de arquivos de uma aba).
    pub fn confine_with(&self, dir: &Path, dev: u32, open_mnts: &[u32]) -> Result<String, String> {
        let f = std::fs::File::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let v = self.call(
            json!({"op": "confine", "dev": dev, "open_mnts": open_mnts}),
            &[f.as_raw_fd()],
        )?;
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

impl Client {
    /// Espera a conexão cair (ou fechar). Só uma thread pode esperar.
    pub fn wait_closed(&self) {
        let rx = self.closed.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(rx) = rx {
            let _ = rx.recv();
        }
    }

    /// Fecha a conexão já, mesmo com outra thread segurando o cliente: o
    /// auxiliar solta o confinamento, e o canal dos eventos fecha.
    pub fn close(&self) {
        // SAFETY: shutdown num descritor nosso.
        unsafe { libc::shutdown(self.sock.as_raw_fd(), libc::SHUT_RDWR) };
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
    let st = status_with(lsm, socket)?;
    Ok(sandmux_spec::trf!(
        "auxiliar root {} em {}, BPF LSM ativo",
        "root helper {} at {}, BPF LSM active",
        st.version,
        socket.display()
    ))
}

/// Os arquivos das abas podem ir ao kernel (o BPF LSM e um auxiliar que
/// confina abas); `Err` com o motivo de ficarem com o supervisor.
pub fn tab_availability() -> Result<String, String> {
    tab_availability_with(sandmux_ebpf::lsm_active(), &socket_path())
}

pub fn tab_availability_with(lsm: Result<bool, String>, socket: &Path) -> Result<String, String> {
    use sandmux_spec::{tr, trf};
    let st = status_with(lsm, socket)?;
    if !st.tab {
        return Err(trf!(
            "o auxiliar root {} não confina os arquivos de uma aba; atualize o auxiliar",
            "the root helper {} does not confine a tab's files; update the helper",
            st.version
        ));
    }
    Ok(tr!(
        "segredos e file-deny negados pelo kernel, aberturas pela fila do kernel (com o escopo próprio da aba)",
        "secrets and file-deny denied by the kernel, opens through the kernel queue (with the tab's own scope)"
    )
    .into())
}

fn status_with(lsm: Result<bool, String>, socket: &Path) -> Result<Status, String> {
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
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        fn confine(
            &self,
            _cg: &OwnedFd,
            _dev: u32,
            open_mnts: &[u32],
        ) -> Result<Box<dyn Confined>, String> {
            self.loaded.fetch_add(1, Ordering::SeqCst);
            // As montagens das aberturas, somadas (o teste confere).
            if !open_mnts.is_empty() {
                *self.asked.lock().unwrap() = Some(open_mnts.iter().sum());
            }
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
    fn confina_a_aba_com_montagens_e_marcas_novas() {
        let (d, p, loaded, asked) = start(true);
        let c = Client::connect(&p).unwrap();
        assert!(c.status().unwrap().tab);
        assert_eq!(c.confine_with(d.path(), 0, &[7, 9]).unwrap(), "cgroup");
        assert_eq!(*asked.lock().unwrap(), Some(16));
        let env = d.path().join(".env");
        let sql = d.path().join("dados.sql");
        std::fs::write(&env, "").unwrap();
        std::fs::write(&sql, "").unwrap();
        assert_eq!(
            c.mark(MarkKind::Secret, std::slice::from_ref(&env))
                .unwrap(),
            1
        );
        assert_eq!(
            c.mark(MarkKind::FileDeny, std::slice::from_ref(&sql))
                .unwrap(),
            1
        );
        assert_eq!(
            c.unmark(MarkKind::Secret, std::slice::from_ref(&env))
                .unwrap(),
            1
        );
        // Um tipo de marca desconhecido é recusado.
        let e = c
            .call(json!({"op": "mark", "kind": "outra"}), &[])
            .unwrap_err();
        assert!(!e.is_empty());
        drop(c);
        wait_for(|| loaded.load(Ordering::SeqCst) == 0);
    }

    #[test]
    fn limites_do_congelamento() {
        assert!(!should_freeze(10, (0, 0), 0));
        assert!(should_freeze(10, (0, 0), 1));
        assert!(should_freeze(QUEUE_HIGH + 1, (0, 0), 0));
        assert!(should_freeze(0, (800, 1000), 0));
        assert!(!should_freeze(0, (700, 1000), 0));
        assert!(may_thaw(0, (0, 0)));
        assert!(may_thaw(QUEUE_LOW - 1, (250, 1000)));
        assert!(!may_thaw(QUEUE_LOW, (0, 1000)));
        assert!(!may_thaw(0, (300, 1000)));
    }

    /// Eventos de um roteiro: cada volta devolve o próximo passo (a quantidade
    /// de eventos, os descartes e o backlog) e anota o `cgroup.freeze`.
    struct Roteiro {
        passos: std::collections::VecDeque<(usize, u64, (u64, u64))>,
        backlog: (u64, u64),
        freeze: PathBuf,
        visto: Arc<Mutex<Vec<String>>>,
    }
    impl Events for Roteiro {
        fn poll(&mut self, _timeout_ms: i32) -> (Vec<Value>, u64) {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let v = std::fs::read_to_string(&self.freeze).unwrap_or_default();
            let mut visto = self.visto.lock().unwrap();
            if visto.last() != Some(&v) {
                visto.push(v);
            }
            let (n, dropped, backlog) = self.passos.pop_front().unwrap_or((0, 0, (0, 0)));
            self.backlog = backlog;
            let evs = (0..n)
                .map(
                    |i| json!({"kind": "open", "write": false, "ino": i, "mnt": 1, "path": "/w/a"}),
                )
                .collect();
            (evs, dropped)
        }
        fn backlog(&self) -> (u64, u64) {
            self.backlog
        }
    }

    struct Congelamento {
        _d: tempfile::TempDir,
        freeze: PathBuf,
        visto: Arc<Mutex<Vec<String>>>,
        ended: Arc<Mutex<bool>>,
        closing: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        /// A outra ponta da conexão, que recebe os eventos.
        peer: OwnedFd,
    }

    fn congelamento(passos: Vec<(usize, u64, (u64, u64))>) -> Congelamento {
        let d = tempfile::tempdir().unwrap();
        let freeze = d.path().join("cgroup.freeze");
        std::fs::write(&freeze, "0").unwrap();
        let cg = open_path(d.path()).unwrap();
        let mut fds = [0; 2];
        // SAFETY: socketpair escreve dois descritores novos no vetor.
        assert_eq!(
            unsafe {
                libc::socketpair(
                    libc::AF_UNIX,
                    libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                    0,
                    fds.as_mut_ptr(),
                )
            },
            0
        );
        // SAFETY: descritores recém-criados, nossos.
        let (a, b) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let visto = Arc::new(Mutex::new(Vec::new()));
        let ended = Arc::new(Mutex::new(false));
        let closing = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let cl = closing.clone();
        forward_events(
            Box::new(Roteiro {
                passos: passos.into(),
                backlog: (0, 0),
                freeze: freeze.clone(),
                visto: visto.clone(),
            }),
            Arc::new(a),
            Arc::new(Mutex::new(())),
            stop.clone(),
            Some(Freeze {
                cg,
                uid: 1000,
                ended: ended.clone(),
                closing: Box::new(move || cl.load(Ordering::SeqCst)),
            }),
        );
        Congelamento {
            _d: d,
            freeze,
            visto,
            ended,
            closing,
            stop,
            peer: b,
        }
    }

    impl Congelamento {
        fn estado(&self) -> String {
            std::fs::read_to_string(&self.freeze).unwrap()
        }
        /// Lê da outra ponta até somar `n` eventos.
        fn recebe(&self, n: usize) -> u64 {
            let mut vistos = 0;
            let mut dropped = 0;
            while vistos < n {
                let (msg, _) = recv(&self.peer).unwrap().unwrap();
                let v: Value = serde_json::from_slice(&msg).unwrap();
                vistos += v["ev"].as_array().unwrap().len();
                dropped += v["dropped"].as_u64().unwrap_or(0);
            }
            dropped
        }
    }

    impl Drop for Congelamento {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn congela_com_descarte_e_descongela_ao_esvaziar() {
        let c = congelamento(vec![(10, 3, (0, 0))]);
        assert_eq!(c.recebe(10), 3);
        wait_for(|| c.visto.lock().unwrap().len() >= 3);
        assert_eq!(&c.visto.lock().unwrap()[..3], ["0", "1", "0"]);
    }

    #[test]
    fn congela_com_a_fila_do_kernel_quase_cheia() {
        let c = congelamento(vec![(5, 0, (900, 1000)), (0, 0, (0, 1000))]);
        assert_eq!(c.recebe(5), 0);
        wait_for(|| c.visto.lock().unwrap().len() >= 3);
        assert_eq!(&c.visto.lock().unwrap()[..3], ["0", "1", "0"]);
    }

    #[test]
    fn congela_com_a_fila_do_auxiliar_cheia() {
        // Ninguém lê a outra ponta: o envio para, e a fila passa do teto.
        let c = congelamento(vec![(QUEUE_HIGH + 2000, 0, (0, 0))]);
        wait_for(|| c.estado() == "1");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(c.estado(), "1", "não descongela com a fila cheia");
        assert_eq!(c.recebe(QUEUE_HIGH + 2000), 0);
        wait_for(|| c.estado() == "0");
    }

    #[test]
    fn nao_descongela_depois_do_fim_nem_na_saida_limpa() {
        for fim in [true, false] {
            let c = congelamento(vec![(QUEUE_HIGH + 2000, 0, (0, 0))]);
            wait_for(|| c.estado() == "1");
            if fim {
                *c.ended.lock().unwrap() = true;
            } else {
                c.closing.store(true, Ordering::SeqCst);
            }
            assert_eq!(c.recebe(QUEUE_HIGH + 2000), 0);
            std::thread::sleep(std::time::Duration::from_millis(300));
            assert_eq!(c.estado(), "1", "fim: {fim}");
        }
    }

    #[test]
    fn espera_a_conexao_fechar() {
        let (_d, p, _, _) = start(true);
        let c = Arc::new(Client::connect(&p).unwrap());
        let c2 = c.clone();
        let t = std::thread::spawn(move || c2.wait_closed());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!t.is_finished());
        c.close();
        wait_for(|| t.is_finished());
    }

    #[test]
    fn saida_limpa_avisa_antes_de_soltar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.sock");
        let loaded = Arc::new(AtomicUsize::new(0));
        let server = Server::new(Box::new(Fake {
            loaded: loaded.clone(),
            cgroup_ok: true,
            asked: Arc::new(Mutex::new(None)),
        }));
        let l = listen(&path).unwrap();
        let s = server.clone();
        std::thread::spawn(move || accept_loop(s, l));
        let c = Client::connect(&path).unwrap();
        c.confine(dir.path(), 0).unwrap();
        let s = server.clone();
        let t = std::thread::spawn(move || s.close_all(std::time::Duration::from_secs(5)));
        // O cliente fica sabendo com o confinamento ainda carregado.
        c.wait_closed();
        assert_eq!(loaded.load(Ordering::SeqCst), 1);
        drop(c);
        t.join().unwrap();
        assert_eq!(loaded.load(Ordering::SeqCst), 0);
        // Uma conexão nova depois da saída começar fecha sem resposta.
        let c = Client::connect(&path).unwrap();
        assert!(c.status().is_err());
    }

    #[test]
    fn evento_de_abertura_em_json() {
        let e = sandmux_ebpf::ring::Event {
            kind: sandmux_ebpf::ring::EV_OPEN,
            hook: 0,
            bits: 0,
            write: true,
            ino: 42,
            dev: 31,
            name: "/home/u/p/novo.txt".into(),
            args: Vec::new(),
        };
        assert_eq!(
            event_json(&e),
            json!({"kind": "open", "write": true, "ino": 42, "mnt": 31, "path": "/home/u/p/novo.txt"})
        );
        assert_eq!(MarkKind::Secret.bits(), sandmux_ebpf::prog::SECRET);
        assert_eq!(MarkKind::FileDeny.bits(), sandmux_ebpf::prog::FDENY);
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
