//! Conexões decifradas: o gateway termina o TLS do sandbox com um certificado
//! assinado pela CA do lançamento, abre TLS até o destino real e repassa cada
//! pedido HTTP/1.1, registrando método, caminho e status. É aqui que a chave
//! falsa vira a real (changes 13 e 25), em [`rewrite`].

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{Body, Frame, Incoming, SizeHint};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Fluxo com bytes já lidos à frente (o ClientHello que o gateway leu para
/// achar o SNI), contando o que passa em cada sentido.
pub struct Prefixed<S> {
    pre: Vec<u8>,
    pos: usize,
    inner: S,
    pub read: Arc<AtomicU64>,
    pub written: Arc<AtomicU64>,
}

impl<S> Prefixed<S> {
    pub fn new(pre: Vec<u8>, inner: S) -> Prefixed<S> {
        Prefixed {
            pre,
            pos: 0,
            inner,
            read: Arc::new(AtomicU64::new(0)),
            written: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Prefixed<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.pos < self.pre.len() {
            let n = (self.pre.len() - self.pos).min(buf.remaining());
            let start = self.pos;
            buf.put_slice(&self.pre[start..start + n]);
            self.pos += n;
            self.read.fetch_add(n as u64, Ordering::Relaxed);
            return Poll::Ready(Ok(()));
        }
        let before = buf.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = buf.filled().len() - before;
        self.read.fetch_add(n as u64, Ordering::Relaxed);
        r
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Prefixed<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let r = Pin::new(&mut self.inner).poll_write(cx, data);
        if let Poll::Ready(Ok(n)) = &r {
            self.written.fetch_add(*n as u64, Ordering::Relaxed);
        }
        r
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Corpo que conta os bytes e chama `done` no fim (ou quando é solto antes).
struct Counted {
    inner: Incoming,
    n: u64,
    done: Option<Box<dyn FnOnce(u64) + Send + Sync>>,
}

impl Drop for Counted {
    fn drop(&mut self) {
        if let Some(f) = self.done.take() {
            f(self.n);
        }
    }
}

impl Body for Counted {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, hyper::Error>>> {
        let r = Pin::new(&mut self.inner).poll_frame(cx);
        match &r {
            Poll::Ready(Some(Ok(f))) => {
                if let Some(d) = f.data_ref() {
                    self.n += d.len() as u64;
                }
            }
            Poll::Ready(None) => {
                let n = self.n;
                if let Some(f) = self.done.take() {
                    f(n);
                }
            }
            _ => {}
        }
        r
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

type Boxed = BoxBody<Bytes, hyper::Error>;

/// Ponto de troca: o falso de um segredo deste domínio vira o real, nos
/// cabeçalhos e no caminho da URL. Devolve o nome do segredo usado.
pub fn rewrite(
    req: &mut hyper::Request<Boxed>,
    list: &[sandmux_proto::msg::Injection],
) -> Option<String> {
    let h = crate::inject::rewrite_headers(req.headers_mut(), list);
    let p = crate::inject::rewrite_path(req.uri_mut(), list);
    h.or(p)
}

/// Um pedido de upgrade para WebSocket.
pub fn is_websocket(h: &hyper::HeaderMap) -> bool {
    h.get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// O caminho sem a query.
pub fn path_of(uri: &hyper::Uri) -> String {
    uri.path().to_string()
}

/// O que o proxy precisa para registrar.
pub struct Ctx {
    pub log: Arc<crate::log::Log>,
    /// Protocolo das linhas de pedido: `https` (decifrado) ou `http` (portas
    /// do host).
    pub proto: &'static str,
    pub domain: String,
    pub ip: String,
    pub port: u16,
    /// Segredos deste domínio, só na memória desta conexão.
    pub injections: Vec<sandmux_proto::msg::Injection>,
}

/// Abre TLS até o destino, verificando o certificado pelo SNI.
pub async fn connect_upstream(
    target: std::net::SocketAddr,
    sni: &str,
    client: Arc<rustls::ClientConfig>,
) -> io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let tcp = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::net::TcpStream::connect(target),
    )
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            sandmux_spec::tr!("tempo esgotado", "timed out"),
        )
    })??;
    let name = ServerName::try_from(sni.to_string())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    tokio_rustls::TlsConnector::from(client)
        .connect(name, tcp)
        .await
}

/// Serve a conexão decifrada do sandbox: TLS com o certificado do domínio,
/// HTTP/1.1, cada pedido repassado pelo TLS já aberto até o destino.
pub async fn serve<S>(
    guest: S,
    server: Arc<rustls::ServerConfig>,
    upstream: tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    ctx: Arc<Ctx>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Ok(tls) = tokio_rustls::TlsAcceptor::from(server).accept(guest).await else {
        return;
    };
    serve_http(tls, upstream, ctx).await;
}

/// Serve HTTP/1.1 do sandbox ao destino, pedido a pedido, trocando os
/// falsos dos segredos de `ctx` e registrando cada pedido.
pub async fn serve_http<G, U>(guest: G, upstream: U, ctx: Arc<Ctx>)
where
    G: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    U: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Ok((sender, conn)) =
        hyper::client::conn::http1::handshake::<_, Boxed>(TokioIo::new(upstream)).await
    else {
        return;
    };
    tokio::spawn(conn.with_upgrades());
    let sender = Arc::new(tokio::sync::Mutex::new(sender));
    let service = hyper::service::service_fn(move |mut req: hyper::Request<Incoming>| {
        let sender = sender.clone();
        let ctx = ctx.clone();
        async move {
            let t0 = Instant::now();
            let method = req.method().to_string();
            let path = path_of(req.uri());
            // WebSocket: o lado do sandbox, promovido depois do `101`.
            let guest_up = is_websocket(req.headers()).then(|| hyper::upgrade::on(&mut req));
            let up = Arc::new(AtomicU64::new(0));
            let up2 = up.clone();
            let (parts, body) = req.into_parts();
            let body = Counted {
                inner: body,
                n: 0,
                done: Some(Box::new(move |n| up2.store(n, Ordering::Relaxed))),
            };
            let mut out = hyper::Request::from_parts(parts, body.boxed());
            if guest_up.is_some() {
                // Sem compressão, as mensagens de texto podem ser lidas.
                out.headers_mut().remove("sec-websocket-extensions");
            }
            let secret = rewrite(&mut out, &ctx.injections);
            let resp = {
                let mut s = sender.lock().await;
                match s.ready().await {
                    Ok(()) => s.send_request(out).await,
                    Err(e) => Err(e),
                }
            };
            let resp = match resp {
                Ok(r) => r,
                Err(_) => {
                    ctx.log.write_http(
                        ctx.proto,
                        &ctx.domain,
                        &ctx.ip,
                        ctx.port,
                        up.load(Ordering::Relaxed),
                        0,
                        t0.elapsed().as_millis() as u64,
                        crate::log::Http {
                            method: &method,
                            path: &path,
                            status: 502,
                            secret: secret.as_deref(),
                        },
                    );
                    let mut r = hyper::Response::new(
                        http_body_util::Full::new(Bytes::from_static(
                            b"sandmux: o destino falhou\n",
                        ))
                        .map_err(|n| match n {})
                        .boxed(),
                    );
                    *r.status_mut() = hyper::StatusCode::BAD_GATEWAY;
                    return Ok::<_, hyper::Error>(r);
                }
            };
            let status = resp.status().as_u16();
            if let (Some(guest_up), 101) = (guest_up, status) {
                let mut resp = resp;
                let dest_up = hyper::upgrade::on(&mut resp);
                tokio::spawn(tunnel(
                    guest_up,
                    dest_up,
                    ctx.clone(),
                    t0,
                    method,
                    path,
                    secret,
                ));
                let (parts, _) = resp.into_parts();
                let empty = http_body_util::Empty::<Bytes>::new()
                    .map_err(|n| match n {})
                    .boxed();
                return Ok(hyper::Response::from_parts(parts, empty));
            }
            let (parts, body) = resp.into_parts();
            let ctx2 = ctx.clone();
            let body = Counted {
                inner: body,
                n: 0,
                done: Some(Box::new(move |down| {
                    ctx2.log.write_http(
                        ctx2.proto,
                        &ctx2.domain,
                        &ctx2.ip,
                        ctx2.port,
                        up.load(Ordering::Relaxed),
                        down,
                        t0.elapsed().as_millis() as u64,
                        crate::log::Http {
                            method: &method,
                            path: &path,
                            status,
                            secret: secret.as_deref(),
                        },
                    );
                })),
            };
            Ok(hyper::Response::from_parts(parts, body.boxed()))
        }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(guest), service)
        .with_upgrades()
        .await;
}

/// O WebSocket depois do `101`: o sandbox ao destino pelo leitor de quadros,
/// que troca os falsos; o destino ao sandbox, cópia direta. Quando um lado
/// termina, o outro tem 5 segundos para terminar. Registra no fim.
async fn tunnel(
    guest: hyper::upgrade::OnUpgrade,
    dest: hyper::upgrade::OnUpgrade,
    ctx: Arc<Ctx>,
    t0: Instant,
    method: String,
    path: String,
    secret: Option<String>,
) {
    use tokio::io::AsyncWriteExt;
    let (Ok(g), Ok(d)) = (guest.await, dest.await) else {
        return;
    };
    let (gr, mut gw) = tokio::io::split(TokioIo::new(g));
    let (mut dr, dw) = tokio::io::split(TokioIo::new(d));
    let list = ctx.injections.clone();
    let up = crate::ws::pump_client(gr, dw, &list);
    let down = async {
        let n = tokio::io::copy(&mut dr, &mut gw).await.unwrap_or(0);
        // O ipstack só manda o FIN sem dados pendentes de ACK e não tenta de
        // novo sozinho: o shutdown é repetido, por até 2 segundos.
        for _ in 0..40 {
            let t = std::time::Duration::from_millis(50);
            if tokio::time::timeout(t, gw.shutdown()).await.is_ok() {
                break;
            }
        }
        n
    };
    tokio::pin!(up, down);
    let (mut a, mut b) = (None, None);
    loop {
        tokio::select! {
            r = &mut up, if a.is_none() => a = Some(r),
            r = &mut down, if b.is_none() => b = Some(r),
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)), if a.is_some() || b.is_some() => break,
        }
        if a.is_some() && b.is_some() {
            break;
        }
    }
    let (upn, used) = a.unwrap_or((0, None));
    ctx.log.write_http(
        ctx.proto,
        &ctx.domain,
        &ctx.ip,
        ctx.port,
        upn,
        b.unwrap_or(0),
        t0.elapsed().as_millis() as u64,
        crate::log::Http {
            method: &method,
            path: &path,
            status: 101,
            secret: secret.or(used).as_deref(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminho_sem_query() {
        let u: hyper::Uri = "/v1/models?key=abc".parse().unwrap();
        assert_eq!(path_of(&u), "/v1/models");
    }

    #[tokio::test]
    async fn prefixo_vem_antes() {
        use tokio::io::AsyncReadExt;
        let (mut a, b) = tokio::io::duplex(64);
        tokio::io::AsyncWriteExt::write_all(&mut a, b"mundo")
            .await
            .unwrap();
        drop(a);
        let mut p = Prefixed::new(b"ola ".to_vec(), b);
        let mut s = String::new();
        p.read_to_string(&mut s).await.unwrap();
        assert_eq!(s, "ola mundo");
        assert_eq!(p.read.load(Ordering::Relaxed), 9);
    }
}
