//! O processo de cada conexão de uma aba, para o registro de rede.
//!
//! Primeiro o daemon (o kernel, pelo auxiliar root, registrou o `connect`
//! com a porta de origem); quando ele não sabe, o `/proc` do sandbox: o
//! socket pela porta de origem em `/proc/<pid>/net/{tcp,udp}{,6}` e o
//! processo dono dele entre os processos do namespace de pid do sandbox,
//! como o `ss -p`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sandmux_proto::msg::Msg;
use sandmux_proto::{Frame, PROTOCOL, read_frame, write_control};
use sandmux_spec::VaultAccess;

tokio::task_local! {
    /// A origem (o socket do processo no sandbox) da conexão desta tarefa.
    pub static CONN_SRC: SocketAddr;
}

/// O processo achado e a fonte (`kernel` ou `proc`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub pid: u32,
    pub exe: String,
    pub via: &'static str,
}

/// Quem procura o processo das conexões de uma aba.
pub struct Resolver {
    access: Option<VaultAccess>,
    tag: String,
    sandbox_pid: u32,
    /// O socket de sock_diag no namespace de rede do sandbox (`None`: lê o
    /// `/proc`).
    diag: std::sync::Mutex<Option<OwnedFd>>,
    /// O daemon já disse que a aba está pelo kernel: a busca no `/proc`
    /// no começo das conexões fica dispensada.
    kernel: AtomicBool,
}

impl Resolver {
    pub fn new(
        access: Option<VaultAccess>,
        tag: &str,
        sandbox_pid: u32,
        diag: Option<OwnedFd>,
    ) -> Resolver {
        Resolver {
            access,
            tag: tag.into(),
            sandbox_pid,
            diag: std::sync::Mutex::new(diag),
            kernel: AtomicBool::new(false),
        }
    }

    /// O processo da conexão TCP de `src` a `dst` pelo kernel (o daemon),
    /// com uma segunda pergunta quando a aba está pelo kernel e o evento
    /// ainda não chegou.
    pub fn resolve_kernel(&self, proto: &str, src: SocketAddr, dst: SocketAddr) -> Option<Found> {
        if proto != "tcp" {
            return None;
        }
        let a = self.access.as_ref()?;
        let found = |r: Option<(Option<u32>, Option<String>, bool)>| match r {
            Some((Some(pid), exe, _)) => Some(Found {
                pid,
                exe: exe.unwrap_or_default(),
                via: "kernel",
            }),
            _ => None,
        };
        let first = ask(a, &self.tag, src.port(), dst);
        if matches!(first, Some((_, _, true))) {
            self.kernel.store(true, Ordering::Relaxed);
        }
        if let Some(f) = found(first.clone()) {
            return Some(f);
        }
        if matches!(first, Some((None, _, true))) {
            std::thread::sleep(Duration::from_millis(400));
            return found(ask(a, &self.tag, src.port(), dst));
        }
        None
    }

    /// O processo dono do socket `proto` de `src` a `dst` no `/proc` do
    /// sandbox (precisa do socket aberto).
    pub fn resolve_proc(&self, proto: &str, src: SocketAddr, dst: SocketAddr) -> Option<Found> {
        let inode = self.inode(proto, src, dst)?;
        let pid = self.owner(inode)?;
        let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Some(Found {
            pid,
            exe,
            via: "proc",
        })
    }

    /// O inode do socket: pela consulta exata do sock_diag; sem ele (ou com
    /// erro), pela tabela de `/proc/<pid>/net`, que o kernel monta varrendo a
    /// tabela inteira de conexões (alguns ms por leitura).
    fn inode(&self, proto: &str, src: SocketAddr, dst: SocketAddr) -> Option<u64> {
        let mut g = self.diag.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(fd) = g.as_ref() {
            match diag_inode(fd, proto, src, dst) {
                Ok(i) => return i,
                Err(_) => *g = None,
            }
        }
        drop(g);
        socket_inode(self.sandbox_pid, proto, src.port(), dst)
    }

    /// A aba está pelo kernel (o daemon já respondeu assim uma vez).
    pub fn by_kernel(&self) -> bool {
        self.kernel.load(Ordering::Relaxed)
    }

    /// O processo da conexão: o kernel primeiro, o `/proc` depois.
    pub fn resolve(&self, proto: &str, src: SocketAddr, dst: SocketAddr) -> Option<Found> {
        self.resolve_kernel(proto, src, dst)
            .or_else(|| self.resolve_proc(proto, src, dst))
    }

    /// Os processos do sandbox: o processo do sandbox e os descendentes
    /// dele, pelos `children` de cada thread (só a árvore do sandbox, sem
    /// varrer o `/proc` do host). Sem esse arquivo no kernel, pela árvore de
    /// pais de `/proc/*/stat`.
    fn sandbox_pids(&self) -> Vec<u32> {
        children_tree(self.sandbox_pid).unwrap_or_else(|| {
            let parents: Vec<(u32, u32)> = std::fs::read_dir("/proc")
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
                .filter_map(|p| {
                    Some((
                        p,
                        ppid(&std::fs::read_to_string(format!("/proc/{p}/stat")).ok()?)?,
                    ))
                })
                .collect();
            descendants(self.sandbox_pid, &parents)
        })
    }

    /// O processo do sandbox que tem o socket `inode` entre os descritores.
    fn owner(&self, inode: u64) -> Option<u32> {
        let want = format!("socket:[{inode}]");
        self.sandbox_pids().into_iter().find(|p| {
            std::fs::read_dir(format!("/proc/{p}/fd"))
                .into_iter()
                .flatten()
                .flatten()
                .any(|f| std::fs::read_link(f.path()).is_ok_and(|l| l.as_os_str() == want.as_str()))
        })
    }
}

/// O pai de um processo, pela linha de `/proc/<pid>/stat` (o nome, entre
/// parênteses, pode ter espaços e parênteses).
pub fn ppid(stat: &str) -> Option<u32> {
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// `root` e os descendentes dele por `/proc/<pid>/task/<tid>/children`;
/// `None` quando o kernel não tem esse arquivo.
fn children_tree(root: u32) -> Option<Vec<u32>> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let p = out[i];
        i += 1;
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{p}/task")) else {
            continue;
        };
        for t in tasks.flatten() {
            match std::fs::read_to_string(t.path().join("children")) {
                Ok(c) => out.extend(c.split_whitespace().filter_map(|x| x.parse::<u32>().ok())),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && p == root => return None,
                Err(_) => {}
            }
        }
    }
    Some(out)
}

/// `root` e todos os descendentes dele, dados os pares (pid, pai).
pub fn descendants(root: u32, parents: &[(u32, u32)]) -> Vec<u32> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let p = out[i];
        out.extend(parents.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
        i += 1;
    }
    out
}

/// Conecta ao daemon, com o `Hello` aceito, e o prazo de cada leitura.
fn hello(a: &VaultAccess, wait: Duration) -> Option<UnixStream> {
    let mut s = UnixStream::connect(&a.socket).ok()?;
    let _ = s.set_read_timeout(Some(wait));
    let _ = s.set_write_timeout(Some(wait));
    let hello = Msg::Hello {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: a.data_dir.clone(),
        config_dir: a.config_dir.clone(),
        lang: String::new(),
    };
    write_control(&mut s, &hello).ok()?;
    if !matches!(
        read_frame(&mut s),
        Ok(Some(Frame::Control(Msg::Welcome { .. })))
    ) {
        return None;
    }
    Some(s)
}

/// Pede ao daemon os arquivos da aba `tag` pelo kernel (o lançador, antes de
/// liberar o comando). `Err` com o motivo de ficar com o supervisor.
pub fn confine_tab(
    a: &VaultAccess,
    tag: &str,
    mounts: Vec<u32>,
    roots: Vec<std::path::PathBuf>,
    exclude: Vec<std::path::PathBuf>,
) -> Result<(), String> {
    let no_daemon =
        || sandmux_spec::tr!("o daemon não respondeu", "the daemon did not answer").to_string();
    let mut s = hello(a, Duration::from_secs(10)).ok_or_else(no_daemon)?;
    write_control(
        &mut s,
        &Msg::ConfineTab {
            tag: tag.into(),
            mounts,
            roots,
            exclude,
        },
    )
    .map_err(|e| e.to_string())?;
    match read_frame(&mut s) {
        Ok(Some(Frame::Control(Msg::ConfineTabReply { kernel: true, .. }))) => Ok(()),
        Ok(Some(Frame::Control(Msg::ConfineTabReply { reason, .. }))) => {
            Err(reason.unwrap_or_default())
        }
        _ => Err(no_daemon()),
    }
}

/// Pergunta ao daemon: (pid, executável, a aba está pelo kernel).
fn ask(
    a: &VaultAccess,
    tag: &str,
    sport: u16,
    dst: SocketAddr,
) -> Option<(Option<u32>, Option<String>, bool)> {
    let mut s = hello(a, Duration::from_secs(2))?;
    write_control(
        &mut s,
        &Msg::ConnProcess {
            tag: tag.into(),
            sport,
            ip: dst.ip().to_string(),
            port: dst.port(),
        },
    )
    .ok()?;
    match read_frame(&mut s) {
        Ok(Some(Frame::Control(Msg::ConnProcessReply { pid, exe, kernel }))) => {
            Some((pid, exe, kernel))
        }
        _ => None,
    }
}

/// O inode do socket `proto` com a porta de origem `sport` (e, no TCP, o
/// destino `dst`) no namespace de rede do processo `pid`.
pub fn socket_inode(pid: u32, proto: &str, sport: u16, dst: SocketAddr) -> Option<u64> {
    let files: &[&str] = if proto == "tcp" {
        &["tcp", "tcp6"]
    } else {
        &["udp", "udp6"]
    };
    for f in files {
        let Ok(t) = std::fs::read_to_string(format!("/proc/{pid}/net/{f}")) else {
            continue;
        };
        if let Some(i) = find_inode(&t, sport, (proto == "tcp").then_some(dst)) {
            return Some(i);
        }
    }
    None
}

const SOCK_DIAG_BY_FAMILY: u16 = 20;
const INET_DIAG_NOCOOKIE: u32 = !0;

/// O inode do socket `proto` de `src` (a ponta do sandbox) a `dst` pela
/// consulta exata do sock_diag (`inet_diag_req_v2` sem `NLM_F_DUMP`), no
/// namespace de rede em que `fd` foi criado. `Ok(None)`: o kernel não achou
/// o socket. `Err`: a consulta falhou (o chamador lê o `/proc`).
pub fn diag_inode(
    fd: &OwnedFd,
    proto: &str,
    src: SocketAddr,
    dst: SocketAddr,
) -> std::io::Result<Option<u64>> {
    let tcp = proto == "tcp";
    // No UDP o kernel troca origem e destino na consulta exata ("por razões
    // históricas", udp_diag.c): a origem é a outra ponta.
    let (a, b) = if tcp { (src, dst) } else { (dst, src) };
    // Um socket IPv6 com endereços IPv4 mapeados também responde pela
    // consulta IPv4; a IPv6 fica para destinos IPv6.
    let family = match (a.ip(), b.ip()) {
        (IpAddr::V4(_), IpAddr::V4(_)) => libc::AF_INET,
        _ => libc::AF_INET6,
    };
    let words = |ip: IpAddr| -> [u8; 16] {
        match ip {
            IpAddr::V4(v) if family == libc::AF_INET => {
                let mut o = [0u8; 16];
                o[..4].copy_from_slice(&v.octets());
                o
            }
            IpAddr::V4(v) => v.to_ipv6_mapped().octets(),
            IpAddr::V6(v) => v.octets(),
        }
    };
    let mut req = [0u8; 16 + 56];
    req[0..4].copy_from_slice(&(16u32 + 56).to_ne_bytes());
    req[4..6].copy_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
    req[6..8].copy_from_slice(&(libc::NLM_F_REQUEST as u16).to_ne_bytes());
    req[8..12].copy_from_slice(&1u32.to_ne_bytes());
    let r = &mut req[16..];
    r[0] = family as u8;
    r[1] = if tcp {
        libc::IPPROTO_TCP
    } else {
        libc::IPPROTO_UDP
    } as u8;
    r[4..8].copy_from_slice(&u32::MAX.to_ne_bytes());
    // inet_diag_sockid: as portas e os endereços na ordem da rede.
    r[8..10].copy_from_slice(&a.port().to_be_bytes());
    r[10..12].copy_from_slice(&b.port().to_be_bytes());
    r[12..28].copy_from_slice(&words(a.ip()));
    r[28..44].copy_from_slice(&words(b.ip()));
    r[48..52].copy_from_slice(&INET_DIAG_NOCOOKIE.to_ne_bytes());
    r[52..56].copy_from_slice(&INET_DIAG_NOCOOKIE.to_ne_bytes());
    // SAFETY: o buffer vive até o fim da chamada; endereço nulo é o kernel.
    let n = unsafe { libc::send(fd.as_raw_fd(), req.as_ptr().cast(), req.len(), 0) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut buf = [0u8; 8192];
    // SAFETY: o kernel escreve no máximo buf.len() bytes.
    let n = unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let buf = &buf[..n as usize];
    if buf.len() < 16 {
        return Err(std::io::Error::other("sock_diag: resposta curta"));
    }
    let typ = u16::from_ne_bytes([buf[4], buf[5]]);
    if typ == libc::NLMSG_ERROR as u16 {
        let errno = i32::from_ne_bytes(buf[16..20].try_into().unwrap_or_default());
        return match -errno {
            libc::ENOENT => Ok(None),
            e => Err(std::io::Error::from_raw_os_error(e)),
        };
    }
    if typ != SOCK_DIAG_BY_FAMILY || buf.len() < 16 + 72 {
        return Err(std::io::Error::other("sock_diag: resposta inesperada"));
    }
    // inet_diag_msg: o inode fica depois da família, do estado, do sockid e
    // de quatro u32.
    let inode = u32::from_ne_bytes(buf[16 + 68..16 + 72].try_into().unwrap_or_default());
    Ok((inode != 0).then_some(inode as u64))
}

/// Procura numa tabela de `/proc/net/{tcp,udp}{,6}` a linha com a porta
/// local `sport` (e o destino `dst`, quando dado) e devolve o inode.
pub fn find_inode(table: &str, sport: u16, dst: Option<SocketAddr>) -> Option<u64> {
    for l in table.lines().skip(1) {
        let f: Vec<&str> = l.split_whitespace().collect();
        let (Some(local), Some(rem), Some(inode)) = (f.get(1), f.get(2), f.get(9)) else {
            continue;
        };
        let Some((_, lp)) = parse_addr(local) else {
            continue;
        };
        if lp != sport {
            continue;
        }
        if let Some(d) = dst {
            match parse_addr(rem) {
                Some((ip, port)) if port == d.port() && same_ip(ip, d.ip()) => {}
                _ => continue,
            }
        }
        if let Ok(i) = inode.parse::<u64>()
            && i != 0
        {
            return Some(i);
        }
    }
    None
}

fn same_ip(a: IpAddr, b: IpAddr) -> bool {
    match (a, b) {
        (IpAddr::V6(x), IpAddr::V4(y)) | (IpAddr::V4(y), IpAddr::V6(x)) => {
            x.to_ipv4_mapped() == Some(y)
        }
        _ => a == b,
    }
}

/// `0100007F:1F90` (IPv4, palavras na ordem da máquina) ou os 32 dígitos do
/// IPv6 (quatro palavras na ordem da máquina), com a porta em hexa.
fn parse_addr(s: &str) -> Option<(IpAddr, u16)> {
    let (ip, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match ip.len() {
        8 => {
            let w = u32::from_str_radix(ip, 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(w.to_le_bytes()))
        }
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                let w = u32::from_str_radix(&ip[i * 8..i * 8 + 8], 16).ok()?;
                b[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            IpAddr::V6(Ipv6Addr::from(b))
        }
        _ => return None,
    };
    Some((ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:9C40 0101A8C0:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 12345 1 0000000000000000 20 4 30 10 -1
   1: 0100007F:9C41 0101A8C0:0050 01 00000000:00000000 00:00000000 00000000  1000        0 67890 1 0000000000000000 20 4 30 10 -1
";

    #[test]
    fn inode_pela_porta_e_destino() {
        let d: SocketAddr = "192.168.1.1:443".parse().unwrap();
        assert_eq!(find_inode(TCP, 40000, Some(d)), Some(12345));
        let d80: SocketAddr = "192.168.1.1:80".parse().unwrap();
        assert_eq!(find_inode(TCP, 40001, Some(d80)), Some(67890));
        // Porta certa, destino errado.
        assert_eq!(find_inode(TCP, 40000, Some(d80)), None);
        // UDP: só a porta.
        assert_eq!(find_inode(TCP, 40001, None), Some(67890));
    }

    #[test]
    fn arvore_de_processos() {
        assert_eq!(ppid("123 (a b) c) S 45 1 1"), Some(45));
        let pairs = [(2, 1), (3, 2), (4, 3), (5, 1), (6, 5)];
        let mut d = descendants(2, &pairs);
        d.sort();
        assert_eq!(d, [2, 3, 4]);
    }

    #[test]
    fn enderecos_do_proc() {
        assert_eq!(
            parse_addr("0100007F:1F90"),
            Some((IpAddr::from([127, 0, 0, 1]), 8080))
        );
        let (ip, p) = parse_addr("0000000000000000FFFF00000100007F:0050").unwrap();
        assert_eq!(p, 80);
        assert!(same_ip(ip, IpAddr::from([127, 0, 0, 1])));
    }

    #[test]
    fn processo_de_um_socket_real() {
        // Um socket do próprio teste: o dono é este processo.
        let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let c = std::net::TcpStream::connect(srv.local_addr().unwrap()).unwrap();
        let src = c.local_addr().unwrap();
        let me = std::process::id();
        // O processo do teste como "sandbox": ele mesmo é o dono.
        let r = Resolver::new(None, "t", me, None);
        let f = r.resolve("tcp", src, srv.local_addr().unwrap()).unwrap();
        assert_eq!((f.pid, f.via), (me, "proc"));
        assert!(!f.exe.is_empty());
        // O mesmo pelo sock_diag.
        let r = Resolver::new(None, "t", me, Some(diag()));
        let f = r.resolve("tcp", src, srv.local_addr().unwrap()).unwrap();
        assert_eq!(f.pid, me);
    }

    fn diag() -> OwnedFd {
        use std::os::fd::FromRawFd;
        // SAFETY: socket() com argumentos numéricos; o descritor é nosso.
        let fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
                libc::NETLINK_SOCK_DIAG,
            )
        };
        assert!(fd >= 0);
        // SAFETY: descritor recém-criado, nosso.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    fn ino(f: &impl AsRawFd) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(format!("/proc/self/fd/{}", f.as_raw_fd()))
            .unwrap()
            .ino()
    }

    #[test]
    fn inode_pelo_sock_diag() {
        let d = diag();
        // TCP IPv4.
        let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let c = std::net::TcpStream::connect(srv.local_addr().unwrap()).unwrap();
        let got = diag_inode(
            &d,
            "tcp",
            c.local_addr().unwrap(),
            srv.local_addr().unwrap(),
        );
        assert_eq!(got.unwrap(), Some(ino(&c)));
        // Um destino sem conexão: o kernel não acha.
        let other: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert_eq!(
            diag_inode(&d, "tcp", c.local_addr().unwrap(), other).unwrap(),
            None
        );
        // TCP IPv6.
        if let Ok(srv6) = std::net::TcpListener::bind("[::1]:0") {
            let c6 = std::net::TcpStream::connect(srv6.local_addr().unwrap()).unwrap();
            let got = diag_inode(
                &d,
                "tcp",
                c6.local_addr().unwrap(),
                srv6.local_addr().unwrap(),
            );
            assert_eq!(got.unwrap(), Some(ino(&c6)));
        }
        // UDP conectado e sem conexão (o `sendto` de um resolvedor).
        let peer = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let u = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        u.connect(peer.local_addr().unwrap()).unwrap();
        let got = diag_inode(
            &d,
            "udp",
            u.local_addr().unwrap(),
            peer.local_addr().unwrap(),
        );
        assert_eq!(got.unwrap(), Some(ino(&u)));
        let free = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        free.send_to(b"x", peer.local_addr().unwrap()).unwrap();
        let src: SocketAddr = format!("127.0.0.1:{}", free.local_addr().unwrap().port())
            .parse()
            .unwrap();
        let got = diag_inode(&d, "udp", src, peer.local_addr().unwrap());
        assert_eq!(got.unwrap(), Some(ino(&free)));
    }
}
