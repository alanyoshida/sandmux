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

/// Um pedido de upgrade (WebSocket, SPDY…).
pub fn is_upgrade(h: &hyper::HeaderMap) -> bool {
    h.get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| !v.trim().is_empty())
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
    /// Os segredos pedidos de novo durante a conexão (o real de uma
    /// identidade de cloud perto do fim da validade).
    pub refreshed: std::sync::Mutex<Option<Vec<sandmux_proto::msg::Injection>>>,
}

/// A folga mínima do real de uma identidade de cloud antes de um pedido.
const CLOUD_MARGIN: u64 = 300;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Ctx {
    /// Os segredos atuais da conexão.
    pub fn list(&self) -> Vec<sandmux_proto::msg::Injection> {
        self.refreshed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| self.injections.clone())
    }

    /// Os segredos para o próximo pedido: pede de novo ao daemon quando o
    /// real de uma identidade de cloud está a menos de 5 minutos do fim.
    pub async fn fresh(self: &Arc<Self>) -> Vec<sandmux_proto::msg::Injection> {
        let list = self.list();
        let stale = list
            .iter()
            .any(|i| i.expires > 0 && i.expires < now_secs() + CLOUD_MARGIN);
        let Some(access) = self.vault.clone().filter(|_| stale) else {
            return list;
        };
        let (tag, harness, host) = (self.tag.clone(), self.harness.clone(), self.domain.clone());
        let got = tokio::task::spawn_blocking(move || {
            crate::inject::for_host(crate::inject::fetch(&access, &tag, &harness, &host), &host)
        })
        .await
        .unwrap_or_default();
        *self.refreshed.lock().unwrap_or_else(|e| e.into_inner()) = Some(got.clone());
        got
    }
}

/// A identidade de cloud cujo refresh token falso está no corpo de um
/// pedido de token (cru ou num formulário).
pub fn cloud_refresh<'a>(
    body: &[u8],
    list: &'a [sandmux_proto::msg::Injection],
) -> Option<&'a sandmux_proto::msg::Injection> {
    let text = std::str::from_utf8(body).ok()?;
    list.iter().find(|i| {
        !i.refresh_fake.is_empty() && {
            let enc = percent(&i.refresh_fake);
            text.contains(&i.refresh_fake)
                || text.contains(&enc)
                || text.contains(&lower_escapes(&enc))
        }
    })
}

/// A resposta do gateway a um pedido de token com o refresh falso de uma
/// identidade: o token de acesso falso, ou `invalid_grant` sem o real.
pub fn cloud_token_answer(i: &sandmux_proto::msg::Injection, now: u64) -> (u16, String) {
    if i.value.is_empty() {
        let why = if i.error.is_empty() {
            sandmux_spec::tr!(
                "sandmux: o real desta identidade falta",
                "sandmux: this identity's real token is missing"
            )
            .to_string()
        } else {
            format!("sandmux: {}", i.error)
        };
        let body = serde_json::json!({"error": "invalid_grant", "error_description": why});
        return (400, body.to_string());
    }
    let left = if i.expires == 0 {
        3599
    } else {
        i.expires.saturating_sub(now).min(3599)
    };
    let body = serde_json::json!({
        "access_token": i.fake,
        "expires_in": left,
        "token_type": "Bearer",
        "scope": "https://www.googleapis.com/auth/cloud-platform",
    });
    (200, body.to_string())
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
        if !crate::inject::usable(i) {
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

/// Um `Status` do Kubernetes, a resposta do gateway num API server sem a
/// identidade ou sem o real.
pub fn kube_status(code: u16, msg: &str) -> hyper::Response<Boxed> {
    let reason = match code {
        401 => "Unauthorized",
        404 => "NotFound",
        _ => "ServiceUnavailable",
    };
    let body = serde_json::json!({
        "kind": "Status",
        "apiVersion": "v1",
        "metadata": {},
        "status": "Failure",
        "message": msg,
        "reason": reason,
        "code": code,
    });
    let mut r = hyper::Response::new(full(body.to_string()));
    *r.status_mut() = hyper::StatusCode::from_u16(code).unwrap_or(hyper::StatusCode::BAD_GATEWAY);
    r.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    r
}

/// Um pedido ao API server de uma identidade de Kubernetes: o token falso
/// vira o real (pela troca comum) ou sai, quando a credencial é só o
/// certificado de cliente; o `Host` e o prefixo do caminho vêm do servidor
/// real. Devolve o nome da identidade, ou a resposta local sem o real.
pub fn kube_prepare<B>(
    req: &mut hyper::Request<B>,
    list: &[sandmux_proto::msg::Injection],
) -> Result<String, (u16, String)> {
    // A identidade do nome: com o destino, ou com a falha (sem o real, o
    // daemon não manda o destino).
    let Some(i) = list
        .iter()
        .find(|i| i.kube.is_some() || !i.error.is_empty())
    else {
        return Err((
            404,
            sandmux_spec::tr!(
                "sandmux: nenhuma identidade de Kubernetes com este nome no cofre aberto",
                "sandmux: no Kubernetes identity with this name in the open vault"
            )
            .into(),
        ));
    };
    let Some(t) = i.kube.as_ref().filter(|_| i.error.is_empty()) else {
        return Err((401, format!("sandmux: {}: {}", i.name, i.error)));
    };
    let Some(k) = sandmux_spec::net::kube_server(&t.server) else {
        return Err((503, format!("sandmux: {}: {}", i.name, t.server)));
    };
    if i.value.is_empty() {
        let fake = req
            .headers()
            .get(hyper::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains(&i.fake));
        if fake {
            req.headers_mut().remove(hyper::header::AUTHORIZATION);
        }
    }
    if let Ok(h) = hyper::header::HeaderValue::from_str(&k.authority) {
        req.headers_mut().insert(hyper::header::HOST, h);
    }
    if !k.prefix.is_empty() {
        let pq = req
            .uri()
            .path_and_query()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "/".into());
        if let Ok(u) = format!("{}{pq}", k.prefix).parse() {
            *req.uri_mut() = u;
        }
    }
    Ok(i.name.clone())
}

/// Responde a conexão decifrada do sandbox sem ir a destino nenhum: todo
/// pedido recebe o `Status` do Kubernetes com `code` e `msg`.
pub async fn answer_local<S>(
    guest: S,
    server: Arc<rustls::ServerConfig>,
    ctx: Arc<Ctx>,
    code: u16,
    msg: String,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Ok(tls) = tokio_rustls::TlsAcceptor::from(server).accept(guest).await else {
        return;
    };
    let msg = Arc::new(msg);
    let service = hyper::service::service_fn(move |req: hyper::Request<Incoming>| {
        let (ctx, msg) = (ctx.clone(), msg.clone());
        async move {
            ctx.log.write_http(
                ctx.proto,
                &ctx.domain,
                &ctx.ip,
                ctx.port,
                0,
                0,
                0,
                crate::log::Http {
                    method: req.method().as_str(),
                    path: &path_of(req.uri()),
                    status: code,
                    secret: None,
                },
            );
            Ok::<_, hyper::Error>(kube_status(code, &msg))
        }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(tls), service)
        .await;
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
/// Sem `capture` (um pedido de token que não é de uma identidade de cloud,
/// fora de um endpoint de login), só repassa o pedido já lido.
async fn capture_exchange(
    parts: hyper::http::request::Parts,
    raw: Bytes,
    sender: Arc<tokio::sync::Mutex<hyper::client::conn::http1::SendRequest<Boxed>>>,
    ctx: Arc<Ctx>,
    list: Vec<sandmux_proto::msg::Injection>,
    capture: bool,
) -> Result<hyper::Response<Boxed>, hyper::Error> {
    use hyper::header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};
    let t0 = Instant::now();
    let method = parts.method.to_string();
    let path = path_of(&parts.uri);
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
    let mut parts = parts;
    let (sent, body_secret) = body_with_reals(&raw, &list);
    parts.headers.remove(ACCEPT_ENCODING);
    parts.headers.remove(TRANSFER_ENCODING);
    parts.headers.insert(CONTENT_LENGTH, sent.len().into());
    let up = sent.len() as u64;
    let mut out = hyper::Request::from_parts(parts, full(sent));
    let secret = rewrite(&mut out, &list).or(body_secret);
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
    let body = if !capture || !status.is_success() {
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
            let list = ctx.fresh().await;
            let capture = req.method() == hyper::Method::POST
                && sandmux_spec::net::is_capture(&ctx.harness, &ctx.domain, &path_of(req.uri()));
            // O pedido de token com o refresh falso de uma identidade de cloud
            // é respondido aqui; o de login, capturado.
            let token = req.method() == hyper::Method::POST
                && path_of(req.uri()) == "/token"
                && list.iter().any(|i| !i.refresh_fake.is_empty());
            if capture || token {
                let t0 = Instant::now();
                let method = req.method().to_string();
                let path = path_of(req.uri());
                let (parts, body) = req.into_parts();
                let Ok(collected) = http_body_util::Limited::new(body, 64 * 1024)
                    .collect()
                    .await
                else {
                    ctx.log.write_http(
                        ctx.proto,
                        &ctx.domain,
                        &ctx.ip,
                        ctx.port,
                        0,
                        0,
                        t0.elapsed().as_millis() as u64,
                        crate::log::Http {
                            method: &method,
                            path: &path,
                            status: 413,
                            secret: None,
                        },
                    );
                    return Ok(refuse(
                        413,
                        sandmux_spec::tr!(
                            "sandmux: pedido de token grande demais",
                            "sandmux: token request too large"
                        ),
                    ));
                };
                let raw = collected.to_bytes();
                if token && let Some(i) = cloud_refresh(&raw, &list) {
                    let (status, body) = cloud_token_answer(i, now_secs());
                    ctx.log.write_http(
                        ctx.proto,
                        &ctx.domain,
                        &ctx.ip,
                        ctx.port,
                        raw.len() as u64,
                        body.len() as u64,
                        t0.elapsed().as_millis() as u64,
                        crate::log::Http {
                            method: &method,
                            path: &path,
                            status,
                            secret: Some(&i.name),
                        },
                    );
                    let mut r = hyper::Response::new(full(body));
                    *r.status_mut() = hyper::StatusCode::from_u16(status)
                        .unwrap_or(hyper::StatusCode::BAD_REQUEST);
                    r.headers_mut().insert(
                        hyper::header::CONTENT_TYPE,
                        hyper::header::HeaderValue::from_static("application/json"),
                    );
                    return Ok(r);
                }
                return capture_exchange(parts, raw, sender, ctx, list, capture).await;
            }
            let t0 = Instant::now();
            let method = req.method().to_string();
            let path = path_of(req.uri());
            // O API server de uma identidade de Kubernetes.
            let mut kube = None;
            if sandmux_spec::net::kube_label(&ctx.domain).is_some() {
                match kube_prepare(&mut req, &list) {
                    Ok(name) => kube = Some(name),
                    Err((code, msg)) => {
                        ctx.log.write_http(
                            ctx.proto,
                            &ctx.domain,
                            &ctx.ip,
                            ctx.port,
                            0,
                            0,
                            t0.elapsed().as_millis() as u64,
                            crate::log::Http {
                                method: &method,
                                path: &path,
                                status: code,
                                secret: None,
                            },
                        );
                        return Ok(kube_status(code, &msg));
                    }
                }
            }
            // Upgrade (WebSocket, SPDY): o lado do sandbox, promovido depois
            // do `101`.
            let ws = is_websocket(req.headers());
            let guest_up = is_upgrade(req.headers()).then(|| hyper::upgrade::on(&mut req));
            let up = Arc::new(AtomicU64::new(0));
            let up2 = up.clone();
            let (parts, body) = req.into_parts();
            let body = Counted {
                inner: body,
                n: 0,
                done: Some(Box::new(move |n| up2.store(n, Ordering::Relaxed))),
            };
            let mut out = hyper::Request::from_parts(parts, body.boxed());
            if ws {
                // Sem compressão, as mensagens de texto podem ser lidas.
                out.headers_mut().remove("sec-websocket-extensions");
            }
            let secret = rewrite(&mut out, &list).or(kube);
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
                    ws,
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

/// A conexão depois do `101`. No WebSocket, o sandbox ao destino pelo
/// leitor de quadros, que troca os falsos; nos outros upgrades (SPDY), uma
/// cópia de bytes. O destino ao sandbox, cópia direta. Quando um lado
/// termina, o outro tem 5 segundos para terminar. Registra no fim.
#[allow(clippy::too_many_arguments)]
async fn tunnel(
    guest: hyper::upgrade::OnUpgrade,
    dest: hyper::upgrade::OnUpgrade,
    ctx: Arc<Ctx>,
    t0: Instant,
    method: String,
    path: String,
    secret: Option<String>,
    ws: bool,
) {
    use tokio::io::AsyncWriteExt;
    let (Ok(g), Ok(d)) = (guest.await, dest.await) else {
        return;
    };
    let (mut gr, mut gw) = tokio::io::split(TokioIo::new(g));
    let (mut dr, mut dw) = tokio::io::split(TokioIo::new(d));
    let list = ctx.list();
    let up: std::pin::Pin<
        Box<dyn std::future::Future<Output = (u64, Option<String>)> + Send + '_>,
    > = if ws {
        Box::pin(crate::ws::pump_client(gr, dw, &list))
    } else {
        Box::pin(async move {
            let n = tokio::io::copy(&mut gr, &mut dw).await.unwrap_or(0);
            let _ = dw.shutdown().await;
            (n, None)
        })
    };
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
            ..Default::default()
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
            refreshed: Default::default(),
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

    fn cloud(value: &str, expires: u64, error: &str) -> sandmux_proto::msg::Injection {
        sandmux_proto::msg::Injection {
            name: "gcp:trabalho".into(),
            domains: vec!["*.googleapis.com".into()],
            fake: "ya29.sandmux-fake-A".into(),
            value: value.into(),
            refresh_fake: "1//sandmux-fake-R".into(),
            expires,
            error: error.into(),
            kube: None,
        }
    }

    fn kinj(value: &str, error: &str) -> sandmux_proto::msg::Injection {
        sandmux_proto::msg::Injection {
            name: "kube:dev".into(),
            domains: vec!["dev.kube.sandmux.internal".into()],
            fake: "sandmux-fake-kube-A".into(),
            value: value.into(),
            error: error.into(),
            kube: Some(sandmux_proto::msg::KubeTarget {
                server: "https://10.0.0.1:6443/pre".into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Um pedido do sandbox por `serve_http`, com o destino respondendo
    /// `answer` (vazio: não responde). Devolve o que o sandbox recebeu e o
    /// que chegou ao destino.
    async fn kube_exchange(
        list: Vec<sandmux_proto::msg::Injection>,
        req: &str,
        answer: &str,
    ) -> (String, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut guest, guest_side) = tokio::io::duplex(1 << 16);
        let (up_side, mut upstream) = tokio::io::duplex(1 << 16);
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(Ctx {
            log: Arc::new(crate::log::Log::new(dir.path(), "t", dir.path())),
            proto: "https",
            domain: "dev.kube.sandmux.internal".into(),
            ip: "10.0.0.1".into(),
            port: 6443,
            injections: list,
            vault: None,
            tag: "t".into(),
            harness: "shell".into(),
            refreshed: Default::default(),
        });
        tokio::spawn(serve_http(guest_side, up_side, ctx));
        guest.write_all(req.as_bytes()).await.unwrap();
        let mut seen = Vec::new();
        let mut buf = vec![0u8; 4096];
        if !answer.is_empty() {
            while !String::from_utf8_lossy(&seen).contains("\r\n\r\n") {
                let n = upstream.read(&mut buf).await.unwrap();
                seen.extend_from_slice(&buf[..n]);
            }
            upstream.write_all(answer.as_bytes()).await.unwrap();
        }
        let mut out = Vec::new();
        while !String::from_utf8_lossy(&out).contains("\r\n\r\n") {
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), guest.read(&mut buf))
                .await
                .unwrap()
                .unwrap();
            out.extend_from_slice(&buf[..n]);
        }
        // O corpo curto da resposta vem junto ou logo depois.
        if let Ok(Ok(n)) =
            tokio::time::timeout(std::time::Duration::from_millis(200), guest.read(&mut buf)).await
        {
            out.extend_from_slice(&buf[..n]);
        }
        if answer.is_empty() {
            let r = tokio::time::timeout(
                std::time::Duration::from_millis(300),
                upstream.read(&mut buf),
            )
            .await;
            if let Ok(Ok(n)) = r {
                seen.extend_from_slice(&buf[..n]);
            }
        }
        (
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&seen).into_owned(),
        )
    }

    const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";

    #[tokio::test]
    async fn kubernetes_com_token_real() {
        let (resp, seen) = kube_exchange(
            vec![kinj("k8s-REAL", "")],
            "GET /api/v1/pods?watch=1 HTTP/1.1\r\nhost: dev.kube.sandmux.internal\r\nauthorization: Bearer sandmux-fake-kube-A\r\n\r\n",
            OK,
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(seen.starts_with("GET /pre/api/v1/pods?watch=1 "), "{seen}");
        assert!(seen.contains("Bearer k8s-REAL"), "{seen}");
        assert!(seen.contains("host: 10.0.0.1:6443"), "{seen}");
        assert!(!seen.contains("sandmux-fake"), "{seen}");
    }

    #[tokio::test]
    async fn kubernetes_so_com_certificado() {
        let (resp, seen) = kube_exchange(
            vec![kinj("", "")],
            "GET /api HTTP/1.1\r\nhost: dev.kube.sandmux.internal\r\nauthorization: Bearer sandmux-fake-kube-A\r\n\r\n",
            OK,
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(
            !seen.to_ascii_lowercase().contains("authorization"),
            "{seen}"
        );
    }

    #[tokio::test]
    async fn kubernetes_sem_real_responde_401() {
        let (resp, seen) = kube_exchange(
            vec![sandmux_proto::msg::Injection {
                kube: None,
                ..kinj("", "rode `aws sso login` no host")
            }],
            "GET /api HTTP/1.1\r\nhost: dev.kube.sandmux.internal\r\nauthorization: Bearer sandmux-fake-kube-A\r\n\r\n",
            "",
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 401"), "{resp}");
        assert!(
            resp.contains(r#""kind":"Status""#) && resp.contains("aws sso login"),
            "{resp}"
        );
        assert!(seen.is_empty(), "o destino recebeu: {seen}");
        let (resp, _) = kube_exchange(Vec::new(), "GET /api HTTP/1.1\r\nhost: x\r\n\r\n", "").await;
        assert!(resp.starts_with("HTTP/1.1 404"), "{resp}");
    }

    #[tokio::test]
    async fn upgrade_spdy_vira_tunel() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut guest, guest_side) = tokio::io::duplex(1 << 16);
        let (up_side, mut upstream) = tokio::io::duplex(1 << 16);
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(Ctx {
            log: Arc::new(crate::log::Log::new(dir.path(), "t", dir.path())),
            proto: "https",
            domain: "a.test".into(),
            ip: "1.2.3.4".into(),
            port: 443,
            injections: Vec::new(),
            vault: None,
            tag: "t".into(),
            harness: "shell".into(),
            refreshed: Default::default(),
        });
        tokio::spawn(serve_http(guest_side, up_side, ctx));
        guest
            .write_all(b"POST /exec HTTP/1.1\r\nhost: a.test\r\nconnection: Upgrade\r\nupgrade: SPDY/3.1\r\n\r\n")
            .await
            .unwrap();
        let mut buf = vec![0u8; 4096];
        let mut seen = Vec::new();
        while !String::from_utf8_lossy(&seen).contains("\r\n\r\n") {
            let n = upstream.read(&mut buf).await.unwrap();
            seen.extend_from_slice(&buf[..n]);
        }
        assert!(
            String::from_utf8_lossy(&seen)
                .to_ascii_lowercase()
                .contains("upgrade: spdy/3.1")
        );
        upstream
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\nupgrade: SPDY/3.1\r\n\r\n")
            .await
            .unwrap();
        let mut out = Vec::new();
        while !String::from_utf8_lossy(&out).contains("\r\n\r\n") {
            let n = guest.read(&mut buf).await.unwrap();
            out.extend_from_slice(&buf[..n]);
        }
        assert!(String::from_utf8_lossy(&out).starts_with("HTTP/1.1 101"));
        // Bytes crus nos dois sentidos.
        guest.write_all(b"\x80\x03ping").await.unwrap();
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), upstream.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf[..n], b"\x80\x03ping");
        upstream.write_all(b"pong").await.unwrap();
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), guest.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf[..n], b"pong");
    }

    #[test]
    fn refresh_falso_no_pedido_de_token() {
        let list = vec![cloud("ya29.REAL", 0, "")];
        assert!(cloud_refresh(b"refresh_token=1%2F%2Fsandmux-fake-R&x=1", &list).is_some());
        assert!(cloud_refresh(b"refresh_token=1%2f%2fsandmux-fake-R", &list).is_some());
        assert!(cloud_refresh(br#"{"refresh_token":"1//sandmux-fake-R"}"#, &list).is_some());
        assert!(cloud_refresh(b"refresh_token=1%2F%2Foutro", &list).is_none());
        // A resposta: o falso de acesso, com a validade do real.
        let (st, body) = cloud_token_answer(&cloud("ya29.REAL", 1000 + 600, ""), 1000);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(st, 200);
        assert_eq!(v["access_token"], "ya29.sandmux-fake-A");
        assert_eq!(v["expires_in"], 600);
        assert!(!body.contains("REAL"), "{body}");
        // Sem o real: invalid_grant com o que fazer no host.
        let (st, body) = cloud_token_answer(&cloud("", 0, "rode `gcloud auth login` no host"), 0);
        assert_eq!(st, 400);
        assert!(
            body.contains("invalid_grant") && body.contains("gcloud auth login"),
            "{body}"
        );
    }

    #[test]
    fn identidade_sem_real_fica_com_o_falso() {
        let list = vec![cloud("", 0, "sem gcloud")];
        let mut h = hyper::HeaderMap::new();
        h.insert(
            "authorization",
            "Bearer ya29.sandmux-fake-A".parse().unwrap(),
        );
        assert_eq!(crate::inject::rewrite_headers(&mut h, &list), None);
        assert_eq!(h["authorization"], "Bearer ya29.sandmux-fake-A");
        let mut h = hyper::HeaderMap::new();
        h.insert(
            "authorization",
            "Bearer ya29.sandmux-fake-A".parse().unwrap(),
        );
        let list = vec![cloud("ya29.REAL", 0, "")];
        assert_eq!(
            crate::inject::rewrite_headers(&mut h, &list).as_deref(),
            Some("gcp:trabalho")
        );
        assert_eq!(h["authorization"], "Bearer ya29.REAL");
    }

    #[tokio::test]
    async fn pedido_de_token_respondido_sem_ir_ao_destino() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut guest, guest_side) = tokio::io::duplex(1 << 16);
        let (up_side, mut upstream) = tokio::io::duplex(1 << 16);
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(Ctx {
            log: Arc::new(crate::log::Log::new(dir.path(), "t", dir.path())),
            proto: "https",
            domain: "oauth2.googleapis.com".into(),
            ip: "1.2.3.4".into(),
            port: 443,
            injections: vec![cloud("ya29.REAL", 0, "")],
            vault: None,
            tag: "t".into(),
            harness: "shell".into(),
            refreshed: Default::default(),
        });
        tokio::spawn(serve_http(guest_side, up_side, ctx));
        let body = "grant_type=refresh_token&client_id=c&refresh_token=1%2F%2Fsandmux-fake-R";
        let req = format!(
            "POST /token HTTP/1.1\r\nhost: oauth2.googleapis.com\r\ncontent-type: application/x-www-form-urlencoded\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        guest.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let mut buf = vec![0u8; 4096];
        while !String::from_utf8_lossy(&out).contains("Bearer") {
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), guest.read(&mut buf))
                .await
                .unwrap()
                .unwrap();
            out.extend_from_slice(&buf[..n]);
        }
        let resp = String::from_utf8_lossy(&out);
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(
            resp.contains("ya29.sandmux-fake-A") && !resp.contains("REAL"),
            "{resp}"
        );
        // Nada foi ao destino.
        let r = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            upstream.read(&mut buf),
        )
        .await;
        assert!(r.is_err(), "o destino recebeu algo");
    }

    #[tokio::test]
    async fn pedido_de_token_de_outro_passa_sem_captura() {
        let (resp, seen) = exchange_with(
            Login {
                harness: "shell",
                domain: "oauth2.googleapis.com",
                path: "/token",
                ctype: "application/x-www-form-urlencoded",
                req_body: "grant_type=refresh_token&refresh_token=1%2F%2Fdo-usuario",
                resp_body: r#"{"access_token":"ya29.DO-USUARIO","expires_in":3599}"#,
                injection: cloud("ya29.REAL", 0, ""),
            },
            None,
        )
        .await;
        assert!(seen.contains("1%2F%2Fdo-usuario"), "{seen}");
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(resp.contains("ya29.DO-USUARIO"), "{resp}");
    }

    #[tokio::test]
    async fn real_perto_do_fim_pedido_de_novo() {
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
            let Some(Frame::Control(Msg::VaultInject { host, .. })) = read_frame(&mut s).unwrap()
            else {
                panic!("sem pedido de injeção");
            };
            let secrets = vec![cloud("ya29.NOVO", super::now_secs() + 3600, "")];
            write_control(&mut s, &Msg::Injections { secrets }).unwrap();
            host
        });
        let ctx = Arc::new(Ctx {
            log: Arc::new(crate::log::Log::new(dir.path(), "t", dir.path())),
            proto: "https",
            domain: "storage.googleapis.com".into(),
            ip: "1.2.3.4".into(),
            port: 443,
            injections: vec![cloud("ya29.VELHO", super::now_secs() + 60, "")],
            vault: Some(sandmux_spec::VaultAccess {
                socket: sock,
                data_dir: dir.path().into(),
                config_dir: dir.path().into(),
            }),
            tag: "t".into(),
            harness: "shell".into(),
            refreshed: Default::default(),
        });
        let list = ctx.fresh().await;
        assert_eq!(list[0].value, "ya29.NOVO");
        assert_eq!(daemon.join().unwrap(), "storage.googleapis.com");
        // Com folga, não pede de novo.
        assert_eq!(ctx.fresh().await[0].value, "ya29.NOVO");
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
