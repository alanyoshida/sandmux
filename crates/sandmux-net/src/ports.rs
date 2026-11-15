//! Portas que o agente escuta dentro do sandbox, publicadas no `127.0.0.1`
//! do host. A conexão para dentro do namespace de rede é aberta por um
//! auxiliar que entrou nos namespaces do sandbox (ver o lançador).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Quantos números acima do pedido tentar quando a porta do host está ocupada.
pub const TRIES: u16 = 100;

/// Uma porta publicada: a do sandbox e a do host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Published {
    pub guest: u16,
    pub host: u16,
    /// Endereço do host; `None` é o `127.0.0.1`.
    #[serde(default)]
    pub addr: Option<Ipv4Addr>,
}

/// Portas em escuta numa tabela `/proc/net/tcp` (IPv4) ou `tcp6`, nos
/// endereços que o auxiliar alcança por `127.0.0.1`: `127.0.0.1`, `0.0.0.0` e
/// `::`.
pub fn listening(table: &str, v6: bool) -> BTreeSet<u16> {
    let mut out = BTreeSet::new();
    for line in table.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(local), Some(state)) = (f.get(1), f.get(3)) else {
            continue;
        };
        if *state != "0A" {
            continue;
        }
        let Some((addr, port)) = local.split_once(':') else {
            continue;
        };
        let Ok(port) = u16::from_str_radix(port, 16) else {
            continue;
        };
        let ok = if v6 {
            addr.len() == 32 && addr.chars().all(|c| c == '0')
        } else {
            match u32::from_str_radix(addr, 16) {
                // O kernel escreve os bytes do endereço como um inteiro na
                // ordem da máquina (little-endian em x86 e ARM).
                Ok(a) => {
                    let ip = Ipv4Addr::from(a.to_le_bytes());
                    ip == Ipv4Addr::LOCALHOST || ip == Ipv4Addr::UNSPECIFIED
                }
                Err(_) => false,
            }
        };
        if ok {
            out.insert(port);
        }
    }
    out
}

/// Portas em escuta no namespace de rede do processo `pid`.
pub fn listening_of(pid: i32) -> BTreeSet<u16> {
    let read =
        |n: &str| std::fs::read_to_string(format!("/proc/{pid}/net/{n}")).unwrap_or_default();
    let mut s = listening(&read("tcp"), false);
    s.extend(listening(&read("tcp6"), true));
    s
}

/// Cliente do auxiliar de conexão: pede uma porta e recebe um socket já
/// conectado dentro do sandbox.
pub struct Connector(Mutex<OwnedFd>);

impl Connector {
    pub fn new(sock: OwnedFd) -> Connector {
        Connector(Mutex::new(sock))
    }

    fn ask(&self, op: u8, port: u16) -> io::Result<Option<OwnedFd>> {
        let g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let p = port.to_be_bytes();
        let req = [op, p[0], p[1]];
        // SAFETY: escrita de 3 bytes de um buffer válido.
        let n = unsafe { libc::write(g.as_raw_fd(), req.as_ptr().cast(), 3) };
        if n != 3 {
            return Err(io::Error::last_os_error());
        }
        Ok(match recv_fd(&*g)? {
            Some((t, fd)) if t == op => Some(fd),
            _ => None,
        })
    }

    /// Conecta em `127.0.0.1:port` dentro do sandbox (bloqueante).
    pub fn connect(&self, port: u16) -> io::Result<std::net::TcpStream> {
        self.ask(b'C', port)?
            .map(std::net::TcpStream::from)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    sandmux_spec::trf!(
                        "o sandbox recusou a conexão na porta {port}",
                        "the sandbox refused the connection on port {port}"
                    ),
                )
            })
    }

    /// Escuta em `127.0.0.1:port` dentro do sandbox (bloqueante).
    pub fn listen(&self, port: u16) -> io::Result<std::net::TcpListener> {
        self.ask(b'L', port)?
            .map(std::net::TcpListener::from)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrInUse,
                    sandmux_spec::trf!(
                        "a porta {port} já está em uso dentro do sandbox",
                        "port {port} is already in use inside the sandbox"
                    ),
                )
            })
    }
}

/// Recebe um byte e, se veio junto, um descritor (SCM_RIGHTS). `None` quando
/// não veio descritor.
pub fn recv_fd(sock: &impl AsRawFd) -> io::Result<Option<(u8, OwnedFd)>> {
    let mut byte = [0u8];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut buf = [0u8; 64];
    // SAFETY: msghdr zerado com ponteiros para buffers vivos; o kernel
    // preenche o cmsg dentro do buffer.
    unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = buf.as_mut_ptr().cast();
        msg.msg_controllen = buf.len() as _;
        let n = libc::recvmsg(sock.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                sandmux_spec::tr!(
                    "o auxiliar de conexão terminou",
                    "the connection helper exited"
                ),
            ));
        }
        let c = libc::CMSG_FIRSTHDR(&msg);
        if c.is_null() || (*c).cmsg_type != libc::SCM_RIGHTS {
            return Ok(None);
        }
        let fd = std::ptr::read_unaligned(libc::CMSG_DATA(c).cast::<RawFd>());
        Ok(Some((byte[0], OwnedFd::from_raw_fd(fd))))
    }
}

/// Grava a lista de portas de forma atômica.
fn write_list(file: &Path, list: &[Published]) {
    let tmp = file.with_extension("tmp");
    if let Ok(b) = serde_json::to_vec(list)
        && std::fs::write(&tmp, b).is_ok()
    {
        let _ = std::fs::rename(&tmp, file);
    }
}

/// Lê a lista de portas de um lançamento.
pub fn read_list(file: &Path) -> Vec<Published> {
    std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Ouvinte no `127.0.0.1` do host, no número pedido ou no próximo livre.
fn bind_host(port: u16) -> Option<(std::net::TcpListener, u16)> {
    for p in port..=port.saturating_add(TRIES) {
        if p == 0 {
            continue;
        }
        if let Ok(l) = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, p))) {
            return Some((l, p));
        }
    }
    None
}

/// Publicação das portas de um sandbox.
pub struct Ports {
    pub pid: i32,
    pub file: Option<PathBuf>,
    pub connector: Arc<Connector>,
}

/// As regras de porta atuais (a política relida) e o registro.
pub struct Live {
    pub rules: Box<dyn Fn() -> Vec<sandmux_spec::net::PortRule> + Send + Sync>,
    /// Portas do host: os ouvintes delas no sandbox não são publicados.
    pub host_ports: Box<dyn Fn() -> Vec<u16> + Send + Sync>,
    pub log: Option<Arc<crate::log::Log>>,
}

/// O que publicar: cada porta do sandbox com o endereço extra, pelas regras.
/// Sem regras, todas as em escuta, só no `127.0.0.1`.
pub fn desired(
    rules: &[sandmux_spec::net::PortRule],
    listening: &BTreeSet<u16>,
) -> BTreeMap<u16, Option<Ipv4Addr>> {
    if rules.is_empty() {
        return listening.iter().map(|g| (*g, None)).collect();
    }
    rules
        .iter()
        .filter(|r| listening.contains(&r.port))
        .map(|r| (r.port, r.addr))
        .collect()
}

struct Open {
    host: u16,
    addr: Option<Ipv4Addr>,
    /// O ouvinte do endereço extra abriu.
    extra: bool,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

fn serve(
    l: std::net::TcpListener,
    guest: u16,
    conn: Arc<Connector>,
    log: Option<Arc<crate::log::Log>>,
) -> Option<tokio::task::JoinHandle<()>> {
    let _ = l.set_nonblocking(true);
    let l = tokio::net::TcpListener::from_std(l).ok()?;
    Some(tokio::spawn(async move {
        while let Ok((mut c, from)) = l.accept().await {
            let (conn, log) = (conn.clone(), log.clone());
            tokio::spawn(async move {
                let t0 = std::time::Instant::now();
                let up = tokio::task::spawn_blocking(move || conn.connect(guest)).await;
                let Ok(Ok(up)) = up else { return };
                let _ = up.set_nonblocking(true);
                let Ok(mut up) = tokio::net::TcpStream::from_std(up) else {
                    return;
                };
                let (a, b) = tokio::io::copy_bidirectional(&mut c, &mut up)
                    .await
                    .unwrap_or((0, 0));
                if let Some(l) = log {
                    let ip = from.ip().to_string();
                    l.write(
                        "inbound",
                        &ip,
                        &ip,
                        guest,
                        a,
                        b,
                        t0.elapsed().as_millis() as u64,
                        None,
                    );
                }
            });
        }
    }))
}

/// Vigia as portas do sandbox e publica cada uma, até `stop`.
pub async fn run(ports: Ports, live: Live, mut stop: tokio::sync::watch::Receiver<bool>) {
    let conn = ports.connector.clone();
    let mut open: BTreeMap<u16, Open> = BTreeMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = stop.wait_for(|v| *v) => break,
        }
        let rules = (live.rules)();
        let mut listening = listening_of(ports.pid);
        for p in (live.host_ports)() {
            listening.remove(&p);
        }
        let want = desired(&rules, &listening);
        // Com regras, as conexões de entrada vão para o registro.
        let log = if rules.is_empty() {
            None
        } else {
            live.log.clone()
        };
        let mut changed = false;
        open.retain(|g, o| {
            let keep = want.get(g) == Some(&o.addr);
            if !keep {
                for t in &o.tasks {
                    t.abort();
                }
                changed = true;
            }
            keep
        });
        for (g, addr) in want {
            if open.contains_key(&g) {
                continue;
            }
            // No `0.0.0.0` um ouvinte só cobre também o `127.0.0.1`: o Linux
            // recusa o `0.0.0.0:p` com o `127.0.0.1:p` em escuta.
            let any = addr == Some(Ipv4Addr::UNSPECIFIED);
            let wide = any
                .then(|| std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], g))))
                .and_then(|r| r.ok());
            let (l, h, extra) = match wide {
                Some(l) => (l, g, true),
                None => {
                    let Some((l, h)) = bind_host(g) else { continue };
                    (l, h, false)
                }
            };
            let Some(t) = serve(l, g, conn.clone(), log.clone()) else {
                continue;
            };
            let mut o = Open {
                host: h,
                addr,
                extra,
                tasks: vec![t],
            };
            if let Some(a) = addr
                && !extra
            {
                let r = if any {
                    Err(io::Error::from(io::ErrorKind::AddrInUse))
                } else {
                    std::net::TcpListener::bind(SocketAddr::from((a, g)))
                };
                match r {
                    Ok(l) => {
                        if let Some(t) = serve(l, g, conn.clone(), log.clone()) {
                            o.tasks.push(t);
                            o.extra = true;
                        }
                    }
                    Err(e) => {
                        if let Some(l) = &live.log {
                            l.write(
                                "inbound",
                                &a.to_string(),
                                &a.to_string(),
                                g,
                                0,
                                0,
                                0,
                                Some(&format!("publicar em {a}:{g}: {e}")),
                            );
                        }
                    }
                }
            }
            open.insert(g, o);
            changed = true;
        }
        if changed && let Some(f) = &ports.file {
            let mut list = Vec::new();
            for (g, o) in &open {
                if o.addr == Some(Ipv4Addr::UNSPECIFIED) && o.extra {
                    list.push(Published {
                        guest: *g,
                        host: o.host,
                        addr: o.addr,
                    });
                    continue;
                }
                list.push(Published {
                    guest: *g,
                    host: o.host,
                    addr: None,
                });
                if let (true, Some(a)) = (o.extra, o.addr) {
                    list.push(Published {
                        guest: *g,
                        host: *g,
                        addr: Some(a),
                    });
                }
            }
            write_list(f, &list);
        }
    }
    for (_, o) in open {
        for t in o.tasks {
            t.abort();
        }
    }
    if let Some(f) = &ports.file {
        let _ = std::fs::remove_file(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1 1 0000000000000000 100 0 0 10 0
   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 2 1 0000000000000000 100 0 0 10 0
   2: 6402000A:1F91 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 3 1 0000000000000000 100 0 0 10 0
   3: 0100007F:0BB8 0100007F:A000 01 00000000:00000000 00:00000000 00000000  1000        0 4 1 0000000000000000 100 0 0 10 0
";
    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:1388 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000001000000:1389 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 6 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn portas_em_escuta() {
        // 127.0.0.1:3000 e 0.0.0.0:8080; 10.0.2.100:8081 e a conexão
        // estabelecida ficam de fora.
        assert_eq!(listening(TCP, false), BTreeSet::from([3000, 8080]));
        // [::]:5000 entra; [::1]:5001 fica de fora.
        assert_eq!(listening(TCP6, true), BTreeSet::from([5000]));
        assert!(listening("", false).is_empty());
    }

    #[test]
    fn lista_e_ouvinte() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("p.json");
        write_list(
            &f,
            &[Published {
                guest: 3000,
                host: 3001,
                addr: None,
            }],
        );
        assert_eq!(
            read_list(&f),
            vec![Published {
                guest: 3000,
                host: 3001,
                addr: None,
            }]
        );
        assert!(read_list(&d.path().join("nada")).is_empty());
        // Lista antiga, sem `addr`, ainda é lida.
        std::fs::write(&f, br#"[{"guest":1,"host":2}]"#).unwrap();
        assert_eq!(read_list(&f)[0].addr, None);
        let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = busy.local_addr().unwrap().port();
        let (_l, h) = bind_host(p).unwrap();
        assert!(h > p && h <= p + TRIES);
    }

    #[test]
    fn allowlist_de_portas() {
        use sandmux_spec::net::PortRule;
        let on = BTreeSet::from([3000, 8080, 8642]);
        // Sem regras: todas, no localhost.
        assert_eq!(
            desired(&[], &on),
            BTreeMap::from([(3000, None), (8080, None), (8642, None)])
        );
        // Com regras: só as listadas e em escuta, com o endereço da regra.
        let r: Vec<PortRule> = vec![
            "8642".parse().unwrap(),
            "8080@0.0.0.0".parse().unwrap(),
            "9999".parse().unwrap(),
        ];
        assert_eq!(
            desired(&r, &on),
            BTreeMap::from([(8080, Some(Ipv4Addr::UNSPECIFIED)), (8642, None)])
        );
    }
}
