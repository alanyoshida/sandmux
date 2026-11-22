//! Gateway de rede do sandmux.
//!
//! O supervisor de cada sandbox (`sandmux-run`) recebe o descritor do TUN
//! criado no namespace de rede do sandbox e chama [`spawn`]. O gateway roda
//! uma pilha TCP/IP em espaço de usuário (`ipstack`, sobre tokio): cada
//! conexão TCP é terminada e refeita a partir do host, cada fluxo UDP é
//! repassado, o DNS em `10.0.2.3` é respondido com o resolvedor do host, e
//! tudo é registrado. Os metadados de cloud, o QUIC e o ICMP são bloqueados.

pub mod dns;
pub mod hostports;
pub mod inject;
pub mod log;
pub mod mitm;
pub mod policy;
pub mod ports;
pub mod procinfo;
pub mod sniff;
pub mod tls;
mod tun;
pub mod ws;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use ipstack::{IpStack, IpStackConfig, IpStackStream};
use sandmux_spec::net::Verdict;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};
use tokio::sync::watch;

/// Endereço do sandbox.
pub const GUEST: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 100);
/// Gateway (rota padrão).
pub const GATEWAY: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 2);
/// Servidor de nomes.
pub const DNS: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 3);
/// A máquina (`host.sandmux.internal`), levada ao `127.0.0.1` do host.
pub const HOST: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 254);
/// Nome da máquina.
pub const HOST_NAME: &str = "host.sandmux.internal";
/// Os API servers do Kubernetes (`<nome>.kube.sandmux.internal`): o gateway
/// conecta ao servidor guardado da identidade.
pub const KUBE: Ipv4Addr = Ipv4Addr::new(10, 0, 2, 253);

/// Configuração de um gateway.
pub struct Config {
    pub log_dir: PathBuf,
    pub tag: String,
    pub workdir: PathBuf,
    pub nameservers: Vec<IpAddr>,
    /// Política do lançamento e o arquivo que a substitui quando muda.
    pub policy: sandmux_spec::net::NetPolicy,
    pub policy_file: Option<PathBuf>,
    /// Publicação das portas do sandbox no host (`None` no Docker).
    pub ports: Option<ports::Ports>,
    /// Destinos com nome fixo, decididos e registrados por esse nome (o
    /// espelho do Docker Hub como `registry-1.docker.io`).
    pub aliases: Vec<(SocketAddr, String)>,
    /// CA do lançamento, para decifrar o TLS da lista de decifração.
    pub tls: Option<sandmux_spec::TlsSpec>,
    /// Acesso ao daemon para os segredos (só nas sessões).
    pub vault: Option<sandmux_spec::VaultAccess>,
    /// Harness da aba: os endpoints de login capturados e os segredos de
    /// login trocados são só os dele.
    pub harness: String,
    /// O pid do sandbox da aba, no namespace do host: com ele, cada linha do
    /// registro ganha o processo da conexão (`None` no Docker).
    pub sandbox_pid: Option<u32>,
    /// Um socket de sock_diag no namespace de rede do sandbox, para achar o
    /// socket de cada conexão sem ler `/proc/<pid>/net`.
    pub sock_diag: Option<std::os::fd::OwnedFd>,
}

struct State {
    aliases: Vec<(SocketAddr, String)>,
    log: Arc<log::Log>,
    /// CA do lançamento e o cliente TLS dos destinos, para decifrar.
    mitm: Option<(Arc<tls::Ca>, Arc<rustls::ClientConfig>)>,
    /// A CA adicional dos destinos (`SANDMUX_UPSTREAM_CA`), para os API
    /// servers sem CA própria.
    upstream_ca: Option<String>,
    vault: Option<sandmux_spec::VaultAccess>,
    tag: String,
    harness: String,
    names: policy::Names,
    policy: policy::Policy,
    icmp: Mutex<HashMap<IpAddr, Instant>>,
    nameservers: Vec<IpAddr>,
    /// Vira `true` quando o sandbox termina: as conexões abertas param e
    /// gravam a linha delas.
    stop: watch::Receiver<bool>,
    active: Arc<AtomicUsize>,
}

/// Conta uma tarefa de conexão enquanto ela vive.
struct Active(Arc<AtomicUsize>);

impl Active {
    fn new(a: &Arc<AtomicUsize>) -> Active {
        a.fetch_add(1, Ordering::SeqCst);
        Active(a.clone())
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Fluxo que conta os bytes lidos, para o registro sair certo mesmo quando a
/// cópia é interrompida.
struct Counted<T> {
    inner: T,
    read: Arc<AtomicU64>,
}

impl<T: AsyncRead + Unpin> AsyncRead for Counted<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = buf.filled().len() - before;
        self.read.fetch_add(n as u64, Ordering::Relaxed);
        r
    }
}

/// Espera o sinal de parada.
async fn stopped(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

fn blocked_ip(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v) if v == Ipv4Addr::new(169, 254, 169, 254) => Some("metadados de cloud"),
        IpAddr::V6(v) if v == Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254) => {
            Some("metadados de cloud")
        }
        _ => None,
    }
}

impl State {
    /// Nomes ligados ao IP, do mais recente ao mais antigo (o nome da
    /// máquina para `10.0.2.254`).
    fn names_of(&self, ip: IpAddr) -> Vec<String> {
        match ip {
            IpAddr::V4(v) if v == HOST => vec![HOST_NAME.into()],
            IpAddr::V4(v) => self.names.of(v),
            IpAddr::V6(_) => Vec::new(),
        }
    }

    /// Decide uma conexão pela política; `Some(motivo)` bloqueia.
    fn verdict(&self, ip: IpAddr, sni: Option<&str>, names: &[String]) -> Option<String> {
        let IpAddr::V4(v4) = ip else {
            return Some("IPv6".into());
        };
        match self.policy.get().decide_conn(v4, sni, names) {
            Verdict::Pass => None,
            Verdict::Block(m) => Some(m),
        }
    }
}

impl State {
    /// Decide uma conexão SSH pelo nome dela e pelos nomes ligados ao IP.
    fn ssh_verdict(&self, domain: &str, names: &[String]) -> Option<String> {
        let mut all: Vec<&str> = names.iter().map(String::as_str).collect();
        all.push(domain);
        match self.policy.get().decide_ssh(&all) {
            Verdict::Pass => None,
            Verdict::Block(m) => Some(m),
        }
    }
}

/// O SNI (ou `Host`) e os nomes de uma conexão. Um destino com nome fixo vale
/// por esse nome, sem o SNI nem o `Host`.
fn conn_names(
    aliases: &[(SocketAddr, String)],
    dst: SocketAddr,
    first: &[u8],
    names_of: impl FnOnce() -> Vec<String>,
) -> (Option<String>, Vec<String>) {
    match aliases.iter().find(|(a, _)| *a == dst) {
        Some((_, n)) => (None, vec![n.clone()]),
        None => (
            sniff::tls_sni(first).or_else(|| sniff::http_host(first)),
            names_of(),
        ),
    }
}

/// Destino real de uma conexão do sandbox.
fn target(dst: SocketAddr) -> SocketAddr {
    if dst.ip() == IpAddr::V4(HOST) {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), dst.port())
    } else {
        dst
    }
}

async fn tcp(mut s: ipstack::IpStackTcpStream, st: Arc<State>) {
    let t0 = Instant::now();
    let dst = s.peer_addr();
    st.log.begin("tcp", s.local_addr(), dst).await;
    if let Some(why) = blocked_ip(dst.ip()) {
        st.log.write(
            "tcp",
            log::NO_DOMAIN,
            &dst.ip().to_string(),
            dst.port(),
            0,
            0,
            0,
            Some(why),
        );
        close(&mut s).await;
        return;
    }
    // O primeiro pedaço do cliente, para o SNI ou o Host.
    let mut first = vec![0u8; 16 * 1024];
    let n = match tokio::time::timeout(Duration::from_millis(300), s.read(&mut first)).await {
        Ok(Ok(n)) => n,
        _ => 0,
    };
    first.truncate(n);
    if dst.ip() == IpAddr::V4(KUBE) {
        kube_tcp(s, first, st, t0, dst.port()).await;
        return;
    }
    let (sni, names) = conn_names(&st.aliases, dst, &first, || st.names_of(dst.ip()));
    let domain = sni
        .clone()
        .or_else(|| names.first().cloned())
        .unwrap_or_else(|| log::NO_DOMAIN.into());
    if let Some(why) = st.verdict(dst.ip(), sni.as_deref(), &names) {
        st.log.write(
            "tcp",
            &domain,
            &dst.ip().to_string(),
            dst.port(),
            0,
            0,
            t0.elapsed().as_millis() as u64,
            Some(&why),
        );
        close(&mut s).await;
        return;
    }
    // SSH (a linha `SSH-` do cliente): só para os servidores das chaves.
    let mut proto = "tcp";
    if first.starts_with(b"SSH-") {
        proto = "ssh";
        if let Some(why) = st.ssh_verdict(&domain, &names) {
            st.log.write(
                "ssh",
                &domain,
                &dst.ip().to_string(),
                dst.port(),
                0,
                0,
                t0.elapsed().as_millis() as u64,
                Some(&why),
            );
            close(&mut s).await;
            return;
        }
    }
    // TLS para um domínio da lista de decifração: o proxy do `mitm`.
    if let (Some(name), Some((ca, client))) = (sniff::tls_sni(&first), st.mitm.as_ref())
        && !st.aliases.iter().any(|(a, _)| *a == dst)
        && st.policy.get().decrypts(&name)
    {
        let ip = dst.ip().to_string();
        let up = match mitm::connect_upstream(target(dst), &name, client.clone()).await {
            Ok(u) => u,
            Err(e) => {
                let why = if e.kind() == std::io::ErrorKind::InvalidData {
                    Some(format!("certificado inválido: {e}"))
                } else {
                    None
                };
                st.log.write(
                    "tcp",
                    &name,
                    &ip,
                    dst.port(),
                    0,
                    0,
                    t0.elapsed().as_millis() as u64,
                    why.as_deref(),
                );
                close(&mut s).await;
                return;
            }
        };
        let Ok(server) = ca.server_config(&name) else {
            close(&mut s).await;
            return;
        };
        // Os segredos deste domínio, pedidos ao daemon agora (o cofre pode ter
        // sido trancado ou mudado desde a última conexão).
        let injections = match st.vault.clone() {
            Some(access) => {
                let (tag, harness, host) = (st.tag.clone(), st.harness.clone(), name.clone());
                let all = tokio::task::spawn_blocking(move || {
                    inject::fetch(&access, &tag, &harness, &host)
                })
                .await
                .unwrap_or_default();
                inject::for_host(all, &name)
            }
            None => Vec::new(),
        };
        let ctx = Arc::new(mitm::Ctx {
            log: st.log.clone(),
            proto: "https",
            domain: name.clone(),
            ip: ip.clone(),
            port: dst.port(),
            injections,
            vault: st.vault.clone(),
            tag: st.tag.clone(),
            harness: st.harness.clone(),
            refreshed: Default::default(),
        });
        let guest = mitm::Prefixed::new(first, s);
        let (read, written) = (guest.read.clone(), guest.written.clone());
        tokio::select! {
            _ = mitm::serve(guest, server, up, ctx) => {}
            _ = stopped(st.stop.clone()) => {}
        }
        st.log.write(
            "tcp",
            &name,
            &ip,
            dst.port(),
            read.load(Ordering::Relaxed),
            written.load(Ordering::Relaxed),
            t0.elapsed().as_millis() as u64,
            None,
        );
        return;
    }
    let mut up_bytes = 0u64;
    let mut down_bytes = 0u64;
    let mut failed = false;
    match tokio::time::timeout(
        Duration::from_secs(15),
        tokio::net::TcpStream::connect(target(dst)),
    )
    .await
    {
        Ok(Ok(mut up)) => {
            // Cliente calado: o servidor fala primeiro. Um servidor SSH fora
            // dos servidores das chaves é cortado antes de o cliente ver nada.
            let mut cut = None;
            if n == 0 {
                let mut banner = vec![0u8; 4096];
                if let Ok(Ok(m)) =
                    tokio::time::timeout(Duration::from_secs(2), up.read(&mut banner)).await
                    && m > 0
                {
                    banner.truncate(m);
                    if banner.starts_with(b"SSH-") {
                        proto = "ssh";
                        cut = st.ssh_verdict(&domain, &names);
                    }
                    if cut.is_none() && s.write_all(&banner).await.is_ok() {
                        down_bytes += m as u64;
                    }
                }
            }
            if let Some(why) = cut {
                st.log.write(
                    "ssh",
                    &domain,
                    &dst.ip().to_string(),
                    dst.port(),
                    0,
                    0,
                    t0.elapsed().as_millis() as u64,
                    Some(&why),
                );
                close(&mut s).await;
                return;
            }
            if up.write_all(&first).await.is_ok() {
                let (a, b) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
                tokio::select! {
                    _ = pump(&mut s, &mut up, a.clone(), b.clone()) => {}
                    _ = stopped(st.stop.clone()) => {}
                }
                up_bytes = n as u64 + a.load(Ordering::Relaxed);
                down_bytes += b.load(Ordering::Relaxed);
            }
        }
        _ => failed = true,
    }
    st.log.write(
        proto,
        &domain,
        &dst.ip().to_string(),
        dst.port(),
        up_bytes,
        down_bytes,
        t0.elapsed().as_millis() as u64,
        None,
    );
    // Depois do registro: o sandbox pode terminar assim que a conexão fecha.
    if failed {
        close(&mut s).await;
    }
}

/// Uma conexão a `KUBE`: só TLS com o nome sintético de uma identidade de
/// Kubernetes. O gateway decifra e conecta ele mesmo ao servidor guardado,
/// com a CA do cluster e o certificado de cliente; sem a identidade ou sem
/// o real, responde ele mesmo com um `Status` do Kubernetes.
async fn kube_tcp(
    mut s: ipstack::IpStackTcpStream,
    first: Vec<u8>,
    st: Arc<State>,
    t0: Instant,
    port: u16,
) {
    let name = sniff::tls_sni(&first).filter(|n| sandmux_spec::net::kube_label(n).is_some());
    let (Some(name), Some((ca, _))) = (name, st.mitm.as_ref()) else {
        st.log.write(
            "tcp",
            log::NO_DOMAIN,
            &KUBE.to_string(),
            port,
            0,
            0,
            t0.elapsed().as_millis() as u64,
            Some(sandmux_spec::tr!(
                "kubernetes: só TLS com o nome de uma identidade",
                "kubernetes: only TLS with an identity name"
            )),
        );
        close(&mut s).await;
        return;
    };
    let Ok(server) = ca.server_config(&name) else {
        close(&mut s).await;
        return;
    };
    let list = match st.vault.clone() {
        Some(access) => {
            let (tag, harness, host) = (st.tag.clone(), st.harness.clone(), name.clone());
            let all =
                tokio::task::spawn_blocking(move || inject::fetch(&access, &tag, &harness, &host))
                    .await
                    .unwrap_or_default();
            inject::for_host(all, &name)
        }
        None => Vec::new(),
    };
    let target = list
        .iter()
        .find(|i| i.error.is_empty())
        .and_then(|i| i.kube.clone())
        .and_then(|t| sandmux_spec::net::kube_server(&t.server).map(|k| (t, k)));
    let (ip, rport) = target
        .as_ref()
        .map(|(_, k)| (k.host.clone(), k.port))
        .unwrap_or_else(|| (KUBE.to_string(), port));
    let ctx = Arc::new(mitm::Ctx {
        log: st.log.clone(),
        proto: "https",
        domain: name.clone(),
        ip: ip.clone(),
        port: rport,
        injections: list.clone(),
        vault: st.vault.clone(),
        tag: st.tag.clone(),
        harness: st.harness.clone(),
        refreshed: Default::default(),
    });
    let guest = mitm::Prefixed::new(first, s);
    let (read, written) = (guest.read.clone(), guest.written.clone());
    let mut why = None;
    match target {
        None => {
            // Sem a identidade, ou sem o real: a mesma resposta que um
            // pedido teria.
            let mut probe = hyper::Request::new(());
            let (code, msg) = mitm::kube_prepare(&mut probe, &list)
                .err()
                .unwrap_or((503, format!("sandmux: {name}")));
            tokio::select! {
                _ = mitm::answer_local(guest, server, ctx, code, msg) => {}
                _ = stopped(st.stop.clone()) => {}
            }
        }
        Some((t, k)) => {
            let up = async {
                let client = tls::kube_client_config(&t, st.upstream_ca.as_deref())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
                let addr = tokio::net::lookup_host((k.host.as_str(), k.port))
                    .await?
                    .next()
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::NotFound, k.host.clone())
                    })?;
                mitm::connect_upstream(addr, &t.server_name, client).await
            };
            match up.await {
                Ok(up) => {
                    tokio::select! {
                        _ = mitm::serve(guest, server, up, ctx) => {}
                        _ = stopped(st.stop.clone()) => {}
                    }
                }
                Err(e) => {
                    let msg = if e.kind() == std::io::ErrorKind::InvalidData {
                        format!("certificado inválido: {e}")
                    } else {
                        e.to_string()
                    };
                    why = Some(msg.clone());
                    let text = format!("sandmux: {}: {msg}", t.server);
                    tokio::select! {
                        _ = mitm::answer_local(guest, server, ctx, 503, text) => {}
                        _ = stopped(st.stop.clone()) => {}
                    }
                }
            }
        }
    }
    st.log.write(
        "tcp",
        &name,
        &ip,
        rport,
        read.load(Ordering::Relaxed),
        written.load(Ordering::Relaxed),
        t0.elapsed().as_millis() as u64,
        why.as_deref(),
    );
}

/// Copia nos dois sentidos, contando os bytes lidos de cada lado.
async fn pump(
    s: &mut ipstack::IpStackTcpStream,
    up: &mut tokio::net::TcpStream,
    from_guest: Arc<AtomicU64>,
    from_host: Arc<AtomicU64>,
) {
    let (gr, mut gw) = tokio::io::split(s);
    let (ur, mut uw) = up.split();
    let mut gr = Counted {
        inner: gr,
        read: from_guest,
    };
    let mut ur = Counted {
        inner: ur,
        read: from_host,
    };
    let to_host = async {
        let _ = tokio::io::copy(&mut gr, &mut uw).await;
        let _ = uw.shutdown().await;
    };
    let to_guest = async {
        let _ = tokio::io::copy(&mut ur, &mut gw).await;
        // O ipstack só manda o FIN quando não há dados sem ACK e não volta a
        // tentar sozinho: o shutdown é repetido até dar certo.
        for _ in 0..100 {
            if tokio::time::timeout(Duration::from_millis(50), gw.shutdown())
                .await
                .is_ok()
            {
                break;
            }
        }
    };
    tokio::join!(to_host, to_guest);
}

/// Fecha uma conexão do sandbox na hora (FIN), em vez de deixá-la esperar.
async fn close(s: &mut ipstack::IpStackTcpStream) {
    let _ = tokio::time::timeout(Duration::from_secs(1), s.shutdown()).await;
}

/// Resposta de uma consulta: a do gateway ou a do host, o nome, o primeiro
/// IP e o bloqueio.
struct Answer {
    bytes: Vec<u8>,
    name: String,
    ip: String,
    blocked: Option<String>,
}

/// Resolve uma consulta: nega pela política (NXDOMAIN), responde o nome da
/// máquina aqui e repassa o resto ao host.
/// Os API servers do Kubernetes (`<nome>.kube.sandmux.internal`) vão a
/// `KUBE`, em qualquer modo da política: a identidade guardada é o
/// consentimento.
fn kube_answer(q: &[u8], parsed: &dns::Query) -> Option<Answer> {
    sandmux_spec::net::kube_label(&parsed.name)?;
    Some(Answer {
        bytes: dns::answer_a(q, parsed, KUBE),
        name: parsed.name.clone(),
        ip: KUBE.to_string(),
        blocked: None,
    })
}

async fn resolve(q: &[u8], st: &State) -> Option<Answer> {
    let parsed = dns::parse_query(q)?;
    if let Some(a) = kube_answer(q, &parsed) {
        return Some(a);
    }
    if let Verdict::Block(why) = st.policy.get().decide_name(&parsed.name) {
        return Some(Answer {
            bytes: dns::nxdomain(q, &parsed),
            name: parsed.name,
            ip: String::new(),
            blocked: Some(why),
        });
    }
    let found = forward(q, &parsed, st).await?;
    Some(Answer {
        bytes: found.0,
        name: parsed.name,
        ip: found.1,
        blocked: None,
    })
}

async fn forward(q: &[u8], parsed: &dns::Query, st: &State) -> Option<(Vec<u8>, String)> {
    if parsed.name == HOST_NAME {
        let r = dns::answer_a(q, parsed, HOST);
        return Some((r, HOST.to_string()));
    }
    for ns in &st.nameservers {
        let bind: SocketAddr = if ns.is_ipv4() {
            "0.0.0.0:0".parse().ok()?
        } else {
            "[::]:0".parse().ok()?
        };
        let Ok(sock) = tokio::net::UdpSocket::bind(bind).await else {
            continue;
        };
        if sock.send_to(q, SocketAddr::new(*ns, 53)).await.is_err() {
            continue;
        }
        let mut buf = vec![0u8; 4096];
        if let Ok(Ok((m, _))) =
            tokio::time::timeout(Duration::from_secs(3), sock.recv_from(&mut buf)).await
        {
            buf.truncate(m);
            let answers = dns::parse_a_answers(&buf);
            let first = answers
                .first()
                .map(|(_, ip)| ip.to_string())
                .unwrap_or_default();
            st.names.remember(&answers);
            return Some((buf, first));
        }
    }
    None
}

async fn udp(mut u: ipstack::IpStackUdpStream, st: Arc<State>) {
    let t0 = Instant::now();
    let dst = u.peer_addr();
    st.log.begin("udp", u.local_addr(), dst).await;
    if let Some(why) = blocked_ip(dst.ip()) {
        st.log.write(
            "udp",
            log::NO_DOMAIN,
            &dst.ip().to_string(),
            dst.port(),
            0,
            0,
            0,
            Some(why),
        );
        return;
    }
    if dst.ip() == IpAddr::V4(DNS) && dst.port() == 53 {
        let mut q = vec![0u8; 4096];
        loop {
            let n = tokio::select! {
                r = tokio::time::timeout(Duration::from_secs(10), u.read(&mut q)) => match r {
                    Ok(Ok(n)) if n > 0 => n,
                    _ => return,
                },
                _ = stopped(st.stop.clone()) => return,
            };
            let t = Instant::now();
            match resolve(&q[..n], &st).await {
                Some(a) => {
                    let _ = u.write_all(&a.bytes).await;
                    st.log.write(
                        "dns",
                        &a.name,
                        &a.ip,
                        53,
                        n as u64,
                        a.bytes.len() as u64,
                        t.elapsed().as_millis() as u64,
                        a.blocked.as_deref(),
                    );
                }
                None => {
                    let name = dns::parse_query(&q[..n])
                        .map(|p| p.name)
                        .unwrap_or_default();
                    st.log.write(
                        "dns",
                        &name,
                        "",
                        53,
                        n as u64,
                        0,
                        t.elapsed().as_millis() as u64,
                        None,
                    );
                }
            }
        }
    }
    let names = st.names_of(dst.ip());
    let domain = names
        .first()
        .cloned()
        .unwrap_or_else(|| log::NO_DOMAIN.into());
    if dst.port() == 443 {
        st.log.write(
            "udp",
            &domain,
            &dst.ip().to_string(),
            443,
            0,
            0,
            0,
            Some("QUIC"),
        );
        return;
    }
    if let Some(why) = st.verdict(dst.ip(), None, &names) {
        st.log.write(
            "udp",
            &domain,
            &dst.ip().to_string(),
            dst.port(),
            0,
            0,
            0,
            Some(&why),
        );
        return;
    }
    let bind: SocketAddr = if dst.is_ipv4() {
        "0.0.0.0:0".parse().expect("endereço")
    } else {
        "[::]:0".parse().expect("endereço")
    };
    let Ok(sock) = tokio::net::UdpSocket::bind(bind).await else {
        return;
    };
    if sock.connect(target(dst)).await.is_err() {
        return;
    }
    let (mut up_b, mut down_b) = (0u64, 0u64);
    let mut a = vec![0u8; 65536];
    let mut b = vec![0u8; 65536];
    loop {
        tokio::select! {
            r = tokio::time::timeout(Duration::from_secs(30), u.read(&mut a)) => match r {
                Ok(Ok(n)) if n > 0 => {
                    up_b += n as u64;
                    if sock.send(&a[..n]).await.is_err() { break; }
                }
                _ => break,
            },
            r = sock.recv(&mut b) => match r {
                Ok(n) => {
                    down_b += n as u64;
                    if u.write_all(&b[..n]).await.is_err() { break; }
                }
                Err(_) => break,
            },
            _ = stopped(st.stop.clone()) => break,
        }
    }
    st.log.write(
        "udp",
        &domain,
        &dst.ip().to_string(),
        dst.port(),
        up_b,
        down_b,
        t0.elapsed().as_millis() as u64,
        None,
    );
}

/// Um gateway em execução.
pub struct Gateway {
    stop: watch::Sender<bool>,
    active: Arc<AtomicUsize>,
}

impl Gateway {
    /// Para as conexões abertas e espera, até `wait`, que gravem as linhas
    /// delas. Chamado pelo supervisor quando o sandbox termina.
    pub fn shutdown(self, wait: Duration) {
        let _ = self.stop.send(true);
        let t = Instant::now();
        while self.active.load(Ordering::SeqCst) > 0 && t.elapsed() < wait {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// Sobe o gateway numa thread própria, com um runtime tokio de duas threads.
pub fn spawn(tun: OwnedFd, cfg: Config) -> std::io::Result<Gateway> {
    let (tx, rx) = watch::channel(false);
    let active = Arc::new(AtomicUsize::new(0));
    let a = active.clone();
    std::thread::Builder::new()
        .name("sandmux-gateway".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return,
            };
            rt.block_on(async move {
                // O AsyncFd precisa ser criado dentro do runtime.
                if let Ok(device) = tun::Tun::new(tun) {
                    run(device, cfg, rx, a).await;
                }
            });
        })?;
    Ok(Gateway { stop: tx, active })
}

async fn run(
    device: tun::Tun,
    mut cfg: Config,
    stop: watch::Receiver<bool>,
    active: Arc<AtomicUsize>,
) {
    let st = Arc::new(State {
        aliases: cfg.aliases.clone(),
        log: Arc::new(match cfg.sandbox_pid {
            Some(pid) => log::Log::new(&cfg.log_dir, &cfg.tag, &cfg.workdir).with_resolver(
                procinfo::Resolver::new(cfg.vault.clone(), &cfg.tag, pid, cfg.sock_diag.take()),
                active.clone(),
            ),
            None => log::Log::new(&cfg.log_dir, &cfg.tag, &cfg.workdir),
        }),
        vault: cfg.vault.clone(),
        tag: cfg.tag.clone(),
        harness: cfg.harness.clone(),
        mitm: cfg.tls.as_ref().and_then(|t| {
            let ca = tls::Ca::from_pem(&t.ca_cert_pem, &t.ca_key_pem).ok()?;
            Some((
                Arc::new(ca),
                tls::client_config(t.upstream_ca_pem.as_deref()),
            ))
        }),
        upstream_ca: cfg.tls.as_ref().and_then(|t| t.upstream_ca_pem.clone()),
        names: policy::Names::default(),
        policy: policy::Policy::new(cfg.policy.clone(), cfg.policy_file.clone()),
        icmp: Mutex::new(HashMap::new()),
        nameservers: if cfg.nameservers.is_empty() {
            vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 53))]
        } else {
            cfg.nameservers.clone()
        },
        stop,
        active,
    });
    if let Some(p) = cfg.ports.take() {
        let (st2, st3, st4) = (st.clone(), st.clone(), st.clone());
        tokio::spawn(hostports::run(
            p.connector.clone(),
            Arc::new(move || st4.policy.get()),
            st.log.clone(),
            st.vault.clone().map(|access| hostports::Vault {
                access,
                tag: st.tag.clone(),
                harness: st.harness.clone(),
            }),
            st.stop.clone(),
        ));
        let live = ports::Live {
            rules: Box::new(move || st2.policy.get().ports.clone()),
            host_ports: Box::new(move || st3.policy.get().host_ports.clone()),
            log: Some(st.log.clone()),
        };
        tokio::spawn(ports::run(p, live, st.stop.clone()));
    }
    let mut conf = IpStackConfig::default();
    let _ = conf.mtu(1500);
    let mut stack = IpStack::new(conf, device);
    loop {
        match stack.accept().await {
            Ok(IpStackStream::Tcp(s)) => {
                let guard = Active::new(&st.active);
                let st = st.clone();
                let src = s.local_addr();
                tokio::spawn(procinfo::CONN_SRC.scope(src, async move {
                    tcp(s, st).await;
                    drop(guard);
                }));
            }
            Ok(IpStackStream::Udp(u)) => {
                let guard = Active::new(&st.active);
                let st = st.clone();
                let src = u.local_addr();
                tokio::spawn(procinfo::CONN_SRC.scope(src, async move {
                    udp(u, st).await;
                    drop(guard);
                }));
            }
            Ok(IpStackStream::UnknownTransport(p)) => {
                let dst = p.dst_addr();
                // Sem IPv6 no sandbox: os pacotes do próprio kernel (descoberta
                // de roteador, MLD) são descartados sem registro.
                if dst.is_ipv6() {
                    continue;
                }
                let now = Instant::now();
                let mut g = st.icmp.lock().unwrap_or_else(|e| e.into_inner());
                let recent = g
                    .get(&dst)
                    .is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60));
                if !recent {
                    g.insert(dst, now);
                    drop(g);
                    st.log.write(
                        "icmp",
                        log::NO_DOMAIN,
                        &dst.to_string(),
                        0,
                        0,
                        0,
                        0,
                        Some("ICMP"),
                    );
                }
            }
            Ok(IpStackStream::UnknownNetwork(_)) => {}
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_do_api_server_do_kubernetes() {
        let q = dns::build_query(9, "kind-dev.kube.sandmux.internal", 1);
        let parsed = dns::parse_query(&q).unwrap();
        let a = kube_answer(&q, &parsed).unwrap();
        assert_eq!(a.ip, "10.0.2.253");
        assert!(a.blocked.is_none());
        assert_eq!(
            dns::parse_a_answers(&a.bytes),
            vec![("kind-dev.kube.sandmux.internal".into(), KUBE)]
        );
        let q = dns::build_query(9, "kube.sandmux.internal", 1);
        assert!(kube_answer(&q, &dns::parse_query(&q).unwrap()).is_none());
        let q = dns::build_query(9, "exemplo.com", 1);
        assert!(kube_answer(&q, &dns::parse_query(&q).unwrap()).is_none());
    }

    #[test]
    fn nome_fixo_vence_o_host() {
        let mirror = SocketAddr::from((HOST, 5000));
        let aliases = vec![(mirror, "registry-1.docker.io".to_string())];
        let get = b"GET /v2/ HTTP/1.1\r\nHost: 10.0.2.254:5000\r\n\r\n";
        let (sni, names) = conn_names(&aliases, mirror, get, || vec!["x".into()]);
        assert_eq!(
            (sni, names),
            (None, vec!["registry-1.docker.io".to_string()])
        );
        let other = SocketAddr::from((HOST, 5001));
        let (sni, names) = conn_names(&aliases, other, get, || {
            vec!["host.sandmux.internal".into()]
        });
        assert_eq!(sni.as_deref(), Some("10.0.2.254"));
        assert_eq!(names, vec!["host.sandmux.internal".to_string()]);
        // Com o grupo docker liberado no custom, o espelho passa; sem ele, não.
        use sandmux_spec::net::{NetPolicy, PolicyMode, Rule, RuleKind};
        let pol = |allow: &[&str]| NetPolicy {
            mode: PolicyMode::Custom,
            rules: allow
                .iter()
                .map(|p| Rule {
                    pattern: p.parse().unwrap(),
                    kind: RuleKind::Allow,
                    origin: "x".into(),
                    enabled: true,
                })
                .collect(),
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
        };
        let (sni, names) = conn_names(&aliases, mirror, get, Vec::new);
        assert_eq!(
            pol(&["registry-1.docker.io"]).decide_conn(HOST, sni.as_deref(), &names),
            Verdict::Pass
        );
        assert!(matches!(
            pol(&["host.sandmux.internal"]).decide_conn(HOST, sni.as_deref(), &names),
            Verdict::Block(_)
        ));
    }
}
