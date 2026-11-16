//! Portas do host: para cada porta de `host-ports`, um ouvinte no
//! `127.0.0.1` do sandbox (aberto pelo auxiliar de conexão, dentro do
//! namespace de rede de lá) repassa cada conexão ao `127.0.0.1` do host. A
//! lista é relida a cada segundo; só a denylist recusa, e cada conexão vai
//! para o registro. Com segredos no cofre para `host.sandmux.internal`, uma
//! conexão que começa com um pedido HTTP passa pelo proxy que troca o falso
//! pelo real (servidores locais de modelo, como o LM Studio e o Unsloth).

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sandmux_spec::net::NetPolicy;

use crate::log::Log;
use crate::ports::Connector;

/// O nome com que as conexões aparecem no registro e na denylist.
pub const NAME: &str = crate::HOST_NAME;

/// Os primeiros bytes são o começo de um pedido HTTP/1.x?
pub fn looks_http(head: &[u8]) -> bool {
    const METHODS: &[&[u8]] = &[
        b"GET ",
        b"POST ",
        b"PUT ",
        b"PATCH ",
        b"DELETE ",
        b"HEAD ",
        b"OPTIONS ",
    ];
    METHODS.iter().any(|m| head.starts_with(m))
}

/// Acesso ao cofre do lançamento, para os segredos de cada conexão.
#[derive(Clone)]
pub struct Vault {
    pub access: sandmux_spec::VaultAccess,
    pub tag: String,
    pub harness: String,
}

fn serve(
    l: std::net::TcpListener,
    port: u16,
    policy: Arc<dyn Fn() -> Arc<NetPolicy> + Send + Sync>,
    log: Arc<Log>,
    vault: Option<Vault>,
) -> Option<tokio::task::JoinHandle<()>> {
    let _ = l.set_nonblocking(true);
    let l = tokio::net::TcpListener::from_std(l).ok()?;
    Some(tokio::spawn(async move {
        while let Ok((mut c, _)) = l.accept().await {
            let (policy, log, vault) = (policy.clone(), log.clone(), vault.clone());
            tokio::spawn(async move {
                let t0 = Instant::now();
                let ip = Ipv4Addr::LOCALHOST.to_string();
                if let Some(why) = policy().denied(&[NAME], Some(Ipv4Addr::LOCALHOST)) {
                    log.write("tcp", NAME, &ip, port, 0, 0, 0, Some(&why));
                    return;
                }
                let up = tokio::time::timeout(
                    Duration::from_secs(10),
                    tokio::net::TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))),
                )
                .await;
                let mut up = match up {
                    Ok(Ok(u)) => u,
                    Ok(Err(e)) => {
                        let why = format!("o host recusou 127.0.0.1:{port}: {e}");
                        log.write("tcp", NAME, &ip, port, 0, 0, 0, Some(&why));
                        return;
                    }
                    Err(_) => {
                        let why = format!("tempo esgotado em 127.0.0.1:{port} do host");
                        log.write("tcp", NAME, &ip, port, 0, 0, 0, Some(&why));
                        return;
                    }
                };
                // Segredos do host no cofre e um pedido HTTP: pelo proxy, que
                // troca o falso pelo real e registra cada pedido.
                let injections = match vault {
                    Some(v) => tokio::task::spawn_blocking(move || {
                        crate::inject::for_host(
                            crate::inject::fetch(&v.access, &v.tag, &v.harness),
                            NAME,
                        )
                    })
                    .await
                    .unwrap_or_default(),
                    None => Vec::new(),
                };
                if !injections.is_empty() {
                    let mut head = [0u8; 8];
                    let peeked =
                        tokio::time::timeout(Duration::from_millis(300), c.peek(&mut head))
                            .await
                            .ok()
                            .and_then(|r| r.ok())
                            .unwrap_or(0);
                    if looks_http(&head[..peeked]) {
                        let ctx = Arc::new(crate::mitm::Ctx {
                            log: log.clone(),
                            proto: "http",
                            domain: NAME.into(),
                            ip: ip.clone(),
                            port,
                            injections,
                            vault: None,
                            tag: String::new(),
                            harness: String::new(),
                        });
                        crate::mitm::serve_http(c, up, ctx).await;
                        let ms = t0.elapsed().as_millis() as u64;
                        log.write("tcp", NAME, &ip, port, 0, 0, ms, None);
                        return;
                    }
                }
                let (a, b) = tokio::io::copy_bidirectional(&mut c, &mut up)
                    .await
                    .unwrap_or((0, 0));
                log.write(
                    "tcp",
                    NAME,
                    &ip,
                    port,
                    a,
                    b,
                    t0.elapsed().as_millis() as u64,
                    None,
                );
            });
        }
    }))
}

/// Mantém os ouvintes das portas do host em dia com a política, até `stop`.
pub async fn run(
    conn: Arc<Connector>,
    policy: Arc<dyn Fn() -> Arc<NetPolicy> + Send + Sync>,
    log: Arc<Log>,
    vault: Option<Vault>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let mut open: BTreeMap<u16, tokio::task::JoinHandle<()>> = BTreeMap::new();
    // Portas que não abriram, com a hora da última tentativa registrada.
    let mut failed: BTreeMap<u16, Instant> = BTreeMap::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = stop.wait_for(|v| *v) => break,
        }
        let want = policy().host_ports.clone();
        open.retain(|p, t| {
            let keep = want.contains(p);
            if !keep {
                t.abort();
            }
            keep
        });
        failed.retain(|p, _| want.contains(p));
        for p in want {
            if open.contains_key(&p) {
                continue;
            }
            let c = conn.clone();
            let l = tokio::task::spawn_blocking(move || c.listen(p)).await;
            match l {
                Ok(Ok(l)) => {
                    failed.remove(&p);
                    if let Some(t) = serve(l, p, policy.clone(), log.clone(), vault.clone()) {
                        open.insert(p, t);
                    }
                }
                Ok(Err(e)) => {
                    // Um aviso por minuto, não um por segundo.
                    let again = failed
                        .get(&p)
                        .is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
                    if again {
                        failed.insert(p, Instant::now());
                        let why = format!("porta do host {p}: {e}");
                        log.write("tcp", NAME, "127.0.0.1", p, 0, 0, 0, Some(&why));
                    }
                }
                Err(_) => {}
            }
        }
    }
    for t in open.values() {
        t.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::looks_http;

    #[test]
    fn pedido_http_reconhecido() {
        assert!(looks_http(b"POST /v1"));
        assert!(looks_http(b"GET / HT"));
        assert!(!looks_http(b"\x16\x03\x01"));
        assert!(!looks_http(b"SSH-2.0-"));
        assert!(!looks_http(b""));
    }
}
