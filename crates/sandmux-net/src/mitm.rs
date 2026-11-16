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
    /// Acesso ao cofre do daemon, para guardar os tokens capturados.
    pub vault: Option<sandmux_spec::VaultAccess>,
    /// Etiqueta do lançamento.
    pub tag: String,
    /// Harness da aba: só os endpoints de login dele são capturados.
    pub harness: String,
}

/// Os tokens reais de uma resposta de login: (papel, valor) dos campos de
/// primeiro nível conhecidos.
pub fn captured_tokens(body: &[u8]) -> Vec<(String, String)> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    sandmux_spec::net::CAPTURE_FIELDS
        .iter()
        .filter_map(|(field, role)| {
            v.get(*field)
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| (role.to_string(), s.to_string()))
        })
        .collect()
}

/// A resposta com cada token real trocado pelo falso dele.
pub fn with_fakes(body: &[u8], reals: &[(String, String)], fakes: &[String]) -> Option<Vec<u8>> {
    let mut text = std::str::from_utf8(body).ok()?.to_string();
    for ((_, real), fake) in reals.iter().zip(fakes) {
        text = text.replace(real.as_str(), fake);
    }
    Some(text.into_bytes())
}

/// O corpo do pedido com os falsos dos segredos do host trocados pelos
/// reais; e o nome do primeiro segredo usado.
pub fn body_with_reals(
    body: &[u8],
    list: &[sandmux_proto::msg::Injection],
) -> (Vec<u8>, Option<String>) {
    let Ok(text) = std::str::from_utf8(body) else {
        return (body.to_vec(), None);
    };
    let mut out = text.to_string();
    let mut used = None;
    for i in list {
        if i.fake.is_empty() {
            continue;
        }
        if out.contains(&i.fake) {
            out = out.replace(&i.fake, &i.value);
            used.get_or_insert_with(|| i.name.clone());
        }
        // Num formulário (`a=b&c=d`), o falso chega codificado: o refresh do
        // Google (`1//…`) vai como `1%2F%2F…`.
        let enc = percent(&i.fake);
        if enc != i.fake {
            let real = percent(&i.value);
            for form in [lower_escapes(&enc), enc] {
                if out.contains(&form) {
                    out = out.replace(&form, &real);
                    used.get_or_insert_with(|| i.name.clone());
                }
            }
        }
    }
    (out.into_bytes(), used)
}

/// A forma percentual de um valor num formulário: cada byte fora de
/// `[A-Za-z0-9._~-]` como `%XX` maiúsculo (como o `url.Values.Encode` do Go).
pub fn percent(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `%2F` vira `%2f`; o resto fica.
fn lower_escapes(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut rest = v;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let esc = rest.get(i..i + 3).unwrap_or(&rest[i..]);
        out.push_str(&esc.to_ascii_lowercase());
        rest = &rest[i + esc.len()..];
    }
    out.push_str(rest);
    out
}

fn full(b: impl Into<Bytes>) -> Boxed {
    http_body_util::Full::new(b.into())
        .map_err(|n| match n {})
        .boxed()
}

/// Resposta do gateway ao sandbox.
fn refuse(status: u16, msg: &str) -> hyper::Response<Boxed> {
    let mut r = hyper::Response::new(full(format!("{msg}\n")));
    *r.status_mut() = hyper::StatusCode::from_u16(status).unwrap_or(hyper::StatusCode::BAD_GATEWAY);
    r
}

/// Um pedido a um endpoint de login: o corpo vai com os reais, e a resposta
/// volta com os tokens trocados pelos falsos, guardados no cofre. Sem cofre
/// que aceite, a resposta é recusada: o token real nunca chega ao sandbox.
async fn capture_exchange(
    req: hyper::Request<Incoming>,
    sender: Arc<tokio::sync::Mutex<hyper::client::conn::http1::SendRequest<Boxed>>>,
    ctx: Arc<Ctx>,
) -> Result<hyper::Response<Boxed>, hyper::Error> {
    use hyper::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};
    let t0 = Instant::now();
    let method = req.method().to_string();
    let path = path_of(req.uri());
    let log = |status: u16, up: u64, down: u64, secret: Option<&str>| {
        ctx.log.write_http(
            ctx.proto,
            &ctx.domain,
            &ctx.ip,
            ctx.port,
            up,
            down,
            t0.elapsed().as_millis() as u64,
            crate::log::Http {
                method: &method,
                path: &path,
                status,
                secret,
            },
        );
    };
    let (mut parts, body) = req.into_parts();
    let Ok(collected) = http_body_util::Limited::new(body, 64 * 1024)
        .collect()
        .await
    else {
        log(413, 0, 0, None);
        return Ok(refuse(
            413,
            sandmux_spec::tr!(
                "sandmux: pedido de login grande demais",
                "sandmux: login request too large"
            ),
        ));
    };
    let raw = collected.to_bytes();
    let (sent, body_secret) = body_with_reals(&raw, &ctx.injections);
    parts.headers.remove(ACCEPT_ENCODING);
    parts.headers.remove(TRANSFER_ENCODING);
    parts.headers.insert(CONTENT_LENGTH, sent.len().into());
    let up = sent.len() as u64;
    let mut out = hyper::Request::from_parts(parts, full(sent));
    let secret = rewrite(&mut out, &ctx.injections).or(body_secret);
    let resp = {
        let mut s = sender.lock().await;
        match s.ready().await {
            Ok(()) => s.send_request(out).await,
            Err(e) => Err(e),
        }
    };
    let Ok(resp) = resp else {
        log(502, up, 0, secret.as_deref());
        return Ok(refuse(502, "sandmux: o destino falhou"));
    };
    let status = resp.status();
    let (mut rparts, rbody) = resp.into_parts();
    let Ok(rcollected) = http_body_util::Limited::new(rbody, 1 << 20).collect().await else {
        log(502, up, 0, secret.as_deref());
        return Ok(refuse(
            502,
            sandmux_spec::tr!(
                "sandmux: resposta de login grande demais",
                "sandmux: login answer too large"
            ),
        ));
    };
    let rbytes = rcollected.to_bytes();
    let encoded = rparts
        .headers
        .get(CONTENT_ENCODING)
        .is_some_and(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"));
    let body = if !status.is_success() {
        rbytes.to_vec()
    } else if encoded {
        // Sem ler, não dá para tirar os tokens: não passa nada.
        log(502, up, rbytes.len() as u64, secret.as_deref());
        return Ok(refuse(
            502,
            sandmux_spec::tr!(
                "sandmux: resposta de login comprimida recusada",
                "sandmux: compressed login answer refused"
            ),
        ));
    } else {
        let found = captured_tokens(&rbytes);
        if found.is_empty() {
            rbytes.to_vec()
        } else {
            let Some(access) = ctx.vault.clone() else {
                log(502, up, rbytes.len() as u64, secret.as_deref());
                return Ok(refuse(
                    502,
                    sandmux_spec::tr!(
                        "sandmux: sem cofre para guardar o token de login",
                        "sandmux: no vault to store the login token"
                    ),
                ));
            };
            let (tag, harness) = (ctx.tag.clone(), ctx.harness.clone());
            let items = found.clone();
            let fakes = tokio::task::spawn_blocking(move || {
                crate::inject::capture(&access, &tag, &harness, items)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            match fakes.ok().and_then(|f| with_fakes(&rbytes, &found, &f)) {
                Some(b) => b,
                None => {
                    log(502, up, rbytes.len() as u64, secret.as_deref());
                    return Ok(refuse(
                        502,
                        sandmux_spec::tr!(
                            "sandmux: o cofre não guardou o token de login; desbloqueie o cofre (sandmux vault unlock) e entre de novo",
                            "sandmux: the vault did not store the login token; unlock the vault (sandmux vault unlock) and log in again"
                        ),
                    ));
                }
            }
        }
    };
    rparts.headers.remove(TRANSFER_ENCODING);
    rparts.headers.insert(CONTENT_LENGTH, body.len().into());
    log(status.as_u16(), up, body.len() as u64, secret.as_deref());
    Ok(hyper::Response::from_parts(rparts, full(body)))
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
            if req.method() == hyper::Method::POST
                && sandmux_spec::net::is_capture(&ctx.harness, &ctx.domain, &path_of(req.uri()))
            {
                return capture_exchange(req, sender, ctx).await;
            }
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

    fn inj(name: &str, fake: &str, value: &str) -> sandmux_proto::msg::Injection {
        sandmux_proto::msg::Injection {
            name: name.into(),
            domains: vec!["platform.claude.com".into()],
            fake: fake.into(),
            value: value.into(),
        }
    }

    #[test]
    fn tokens_da_resposta_e_troca() {
        let body = br#"{"token_type":"Bearer","access_token":"sk-ant-oat01-REALA","refresh_token":"sk-ant-ort01-REALR","expires_in":28800}"#;
        let found = captured_tokens(body);
        assert_eq!(
            found,
            vec![
                ("access".to_string(), "sk-ant-oat01-REALA".to_string()),
                ("refresh".to_string(), "sk-ant-ort01-REALR".to_string())
            ]
        );
        let out = with_fakes(body, &found, &["FA".into(), "FR".into()]).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(
            s.contains("\"FA\"") && s.contains("\"FR\"") && !s.contains("REAL"),
            "{s}"
        );
        assert!(captured_tokens(b"nao e json").is_empty());
        // O pedido de renovação leva o real no corpo.
        let (b, used) = body_with_reals(
            br#"{"grant_type":"refresh_token","refresh_token":"FR"}"#,
            &[inj("claude:refresh", "FR", "sk-ant-ort01-REALR")],
        );
        assert!(String::from_utf8(b).unwrap().contains("REALR"));
        assert_eq!(used.as_deref(), Some("claude:refresh"));
    }

    /// Um servidor de login falso e o proxy no meio: sem cofre, a resposta
    /// com tokens é recusada e o real não chega ao sandbox; com o "daemon",
    /// chegam os falsos, e o destino recebe o real do corpo.
    async fn exchange(vault: Option<sandmux_spec::VaultAccess>) -> (String, String) {
        exchange_with(
            Login {
                harness: "claude",
                domain: "platform.claude.com",
                path: "/v1/oauth/token",
                ctype: "application/json",
                req_body: r#"{"grant_type":"refresh_token","refresh_token":"FR"}"#,
                resp_body: r#"{"access_token":"sk-ant-oat01-NEWA","refresh_token":"sk-ant-ort01-NEWR"}"#,
                injection: inj("claude:refresh", "FR", "sk-ant-ort01-REALR"),
            },
            vault,
        )
        .await
    }

    struct Login {
        harness: &'static str,
        domain: &'static str,
        path: &'static str,
        ctype: &'static str,
        req_body: &'static str,
        resp_body: &'static str,
        injection: sandmux_proto::msg::Injection,
    }

    async fn exchange_with(l: Login, vault: Option<sandmux_spec::VaultAccess>) -> (String, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut guest, guest_side) = tokio::io::duplex(1 << 16);
        let (up_side, mut upstream) = tokio::io::duplex(1 << 16);
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(Ctx {
            log: Arc::new(crate::log::Log::new(dir.path(), "t", dir.path())),
            proto: "https",
            domain: l.domain.into(),
            ip: "1.2.3.4".into(),
            port: 443,
            injections: vec![l.injection],
            vault,
            tag: "t".into(),
            harness: l.harness.into(),
        });
        tokio::spawn(serve_http(guest_side, up_side, ctx));
        // O "destino": lê o pedido e responde com tokens novos.
        let server = tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let mut got = Vec::new();
            loop {
                let n = upstream.read(&mut buf).await.unwrap();
                got.extend_from_slice(&buf[..n]);
                let s = String::from_utf8_lossy(&got).to_string();
                if let Some(i) = s.find("\r\n\r\n") {
                    let len: usize = s[..i]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if got.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            let body = l.resp_body;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            upstream.write_all(resp.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        let req_body = l.req_body;
        let req = format!(
            "POST {} HTTP/1.1\r\nhost: {}\r\ncontent-type: {}\r\naccept-encoding: gzip\r\ncontent-length: {}\r\n\r\n{req_body}",
            l.path,
            l.domain,
            l.ctype,
            req_body.len()
        );
        guest.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let mut buf = vec![0u8; 8192];
        loop {
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), guest.read(&mut buf))
                .await
                .unwrap()
                .unwrap();
            out.extend_from_slice(&buf[..n]);
            let s = String::from_utf8_lossy(&out);
            if let Some(i) = s.find("\r\n\r\n") {
                let len: usize = s[..i]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if out.len() >= i + 4 + len {
                    break;
                }
            }
        }
        let seen = server.await.unwrap();
        (String::from_utf8_lossy(&out).to_string(), seen)
    }

    #[tokio::test]
    async fn sem_cofre_o_token_nao_chega() {
        let (resp, seen) = exchange(None).await;
        assert!(seen.contains("sk-ant-ort01-REALR"), "{seen}");
        assert!(
            !seen.to_ascii_lowercase().contains("accept-encoding"),
            "{seen}"
        );
        assert!(resp.starts_with("HTTP/1.1 502"), "{resp}");
        assert!(!resp.contains("NEWA") && !resp.contains("NEWR"), "{resp}");
    }

    #[tokio::test]
    async fn com_cofre_chegam_os_falsos() {
        use sandmux_proto::{Frame, Msg, read_frame, write_control};
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("d.sock");
        let l = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        // O "daemon": saúda e devolve um falso por item.
        let daemon = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let _ = read_frame(&mut s).unwrap();
            write_control(
                &mut s,
                &Msg::Welcome {
                    protocol: sandmux_proto::PROTOCOL,
                    version: "t".into(),
                    pid: 1,
                    start_mode: "t".into(),
                },
            )
            .unwrap();
            let Some(Frame::Control(Msg::VaultCapture { items, .. })) = read_frame(&mut s).unwrap()
            else {
                panic!("sem pedido de captura");
            };
            let fakes = items.iter().map(|(r, _)| format!("FAKE-{r}")).collect();
            write_control(&mut s, &Msg::Captured { fakes }).unwrap();
            items
        });
        let access = sandmux_spec::VaultAccess {
            socket: sock,
            data_dir: dir.path().into(),
            config_dir: dir.path().into(),
        };
        let (resp, _) = exchange(Some(access)).await;
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(
            resp.contains("FAKE-access") && resp.contains("FAKE-refresh"),
            "{resp}"
        );
        assert!(!resp.contains("NEWA") && !resp.contains("NEWR"), "{resp}");
        let items = daemon.join().unwrap();
        assert_eq!(
            items[0],
            ("access".to_string(), "sk-ant-oat01-NEWA".to_string())
        );
    }

    fn agy_refresh() -> Login {
        Login {
            harness: "agy",
            domain: "oauth2.googleapis.com",
            path: "/token",
            ctype: "application/x-www-form-urlencoded",
            req_body: "client_id=c&grant_type=refresh_token&refresh_token=1%2F%2FsandmuxFR",
            resp_body: r#"{"access_token":"ya29.NEW-A_1","expires_in":3599,"token_type":"Bearer"}"#,
            injection: inj("agy:refresh", "1//sandmuxFR", "1//0gREAL-r_x"),
        }
    }

    #[test]
    fn falso_codificado_no_formulario() {
        assert_eq!(percent("1//abc"), "1%2F%2Fabc");
        assert_eq!(percent("ya29.a-b_c~d"), "ya29.a-b_c~d");
        let list = [inj("agy:refresh", "1//sandmuxFR", "1//0gREAL+r")];
        for body in [
            "refresh_token=1%2F%2FsandmuxFR&x=1",
            "refresh_token=1%2f%2fsandmuxFR&x=1",
        ] {
            let (b, used) = body_with_reals(body.as_bytes(), &list);
            assert_eq!(
                String::from_utf8(b).unwrap(),
                "refresh_token=1%2F%2F0gREAL%2Br&x=1"
            );
            assert_eq!(used.as_deref(), Some("agy:refresh"));
        }
    }

    #[tokio::test]
    async fn renovacao_do_google_na_aba_do_agy() {
        use sandmux_proto::{Frame, Msg, read_frame, write_control};
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("d.sock");
        let l = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let _ = read_frame(&mut s).unwrap();
            write_control(
                &mut s,
                &Msg::Welcome {
                    protocol: sandmux_proto::PROTOCOL,
                    version: "t".into(),
                    pid: 1,
                    start_mode: "t".into(),
                },
            )
            .unwrap();
            let Some(Frame::Control(Msg::VaultCapture { harness, items, .. })) =
                read_frame(&mut s).unwrap()
            else {
                panic!("sem pedido de captura");
            };
            write_control(
                &mut s,
                &Msg::Captured {
                    fakes: vec!["ya29.sandmuxFA".into()],
                },
            )
            .unwrap();
            (harness, items)
        });
        let access = sandmux_spec::VaultAccess {
            socket: sock,
            data_dir: dir.path().into(),
            config_dir: dir.path().into(),
        };
        let (resp, seen) = exchange_with(agy_refresh(), Some(access)).await;
        assert!(seen.contains("refresh_token=1%2F%2F0gREAL-r_x"), "{seen}");
        assert!(!seen.contains("sandmuxFR"), "{seen}");
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(resp.contains("ya29.sandmuxFA"), "{resp}");
        assert!(!resp.contains("NEW-A"), "{resp}");
        let (harness, items) = daemon.join().unwrap();
        assert_eq!(harness, "agy");
        assert_eq!(items, [("access".to_string(), "ya29.NEW-A_1".to_string())]);
    }

    #[tokio::test]
    async fn endpoint_do_google_fora_da_aba_do_agy_passa() {
        // Num shell, o `gcloud` renova o login dele: nada é capturado, e o
        // corpo segue como veio.
        let mut l = agy_refresh();
        l.harness = "shell";
        let (resp, seen) = exchange_with(l, None).await;
        assert!(seen.contains("refresh_token=1%2F%2FsandmuxFR"), "{seen}");
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(resp.contains("ya29.NEW-A_1"), "{resp}");
    }
}
