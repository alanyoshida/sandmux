//! A AWS no gateway: o pedido assinado com o par falso de uma identidade é
//! reassinado com a chave real (`sigv4`), e as credenciais que a AWS emite
//! dentro da aba (STS, SSO, Cognito) voltam como falsos.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::{Body, Frame, SizeHint};
use sandmux_proto::msg::{AwsCreds, Injection};

use crate::sigv4::{self, Auth, ChunkRewriter, ChunkSigner, Scope};

type Boxed = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

/// O maior corpo lido para calcular o hash.
pub const MAX_BODY: usize = 32 << 20;
/// A maior resposta de credenciais lida.
pub const MAX_ANSWER: usize = 1 << 20;

/// As ações do STS que devolvem credenciais.
pub const STS_ACTIONS: &[&str] = &[
    "AssumeRole",
    "AssumeRoleWithSAML",
    "AssumeRoleWithWebIdentity",
    "AssumeRoot",
    "GetSessionToken",
    "GetFederationToken",
];

/// Uma injeção da AWS: a chave real, ou o falso de uma identidade sem o
/// real.
pub fn is_aws(i: &Injection) -> bool {
    i.aws.is_some()
        || i.fake.starts_with(sandmux_spec::net::AWS_ID_PREFIX)
        || i.fake.starts_with(sandmux_spec::net::AWS_CAPTURED_PREFIX)
}

/// Como o pedido foi assinado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Header(Auth),
    /// URL pré-assinada: o ID e o escopo do `X-Amz-Credential`.
    Query(String, Scope),
}

impl Mode {
    pub fn access_key(&self) -> &str {
        match self {
            Mode::Header(a) => &a.access_key,
            Mode::Query(id, _) => id,
        }
    }

    pub fn scope(&self) -> &Scope {
        match self {
            Mode::Header(a) => &a.scope,
            Mode::Query(_, s) => s,
        }
    }
}

fn query_value(q: &str, name: &str) -> Option<String> {
    sigv4::query_pairs(q)
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| String::from_utf8_lossy(&sigv4::percent_decode(&v)).into_owned())
}

/// Como o pedido foi assinado com SigV4, quando foi.
pub fn mode_of(headers: &hyper::HeaderMap, uri: &hyper::Uri) -> Option<Mode> {
    if let Some(a) = headers
        .get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(Auth::parse)
    {
        return Some(Mode::Header(a));
    }
    let q = uri.query()?;
    if query_value(q, "X-Amz-Algorithm")? != sigv4::ALGORITHM {
        return None;
    }
    let (id, scope) = sigv4::parse_credential(&query_value(q, "X-Amz-Credential")?)?;
    Some(Mode::Query(id, scope))
}

/// A identidade cujo ID falso assinou o pedido.
pub fn identity<'a>(mode: &Mode, list: &'a [Injection]) -> Option<&'a Injection> {
    list.iter()
        .find(|i| is_aws(i) && !i.fake.is_empty() && i.fake == mode.access_key())
}

/// O pedido emite credenciais: uma ação do STS que as devolve, o
/// `GetRoleCredentials` do SSO ou o `GetCredentialsForIdentity` do Cognito.
pub fn issues_credentials(
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> bool {
    if method == hyper::Method::GET && uri.path().ends_with("/federation/credentials") {
        return true;
    }
    if let Some(t) = headers.get("x-amz-target").and_then(|v| v.to_str().ok()) {
        let action = t.rsplit('.').next().unwrap_or_default();
        if action == "GetCredentialsForIdentity"
            || (t.contains("SecurityTokenService") && STS_ACTIONS.contains(&action))
        {
            return true;
        }
    }
    let form = std::str::from_utf8(body).unwrap_or_default();
    [uri.query().unwrap_or_default(), form]
        .iter()
        .any(|q| query_value(q, "Action").is_some_and(|a| STS_ACTIONS.contains(&a.as_str())))
}

/// A resposta do gateway sem o real: `403` no formato da AWS (JSON nos
/// protocolos JSON, XML nos outros).
pub fn error_response(json: bool, msg: &str) -> hyper::Response<Boxed> {
    let (ctype, body) = if json {
        (
            "application/x-amz-json-1.1",
            serde_json::json!({"__type": "ExpiredTokenException", "message": msg}).to_string(),
        )
    } else {
        let esc = msg
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        (
            "text/xml",
            format!(
                "<ErrorResponse><Error><Type>Sender</Type><Code>ExpiredToken</Code><Message>{esc}</Message></Error><RequestId>sandmux</RequestId></ErrorResponse>"
            ),
        )
    };
    let mut r = hyper::Response::new(
        http_body_util::Full::new(Bytes::from(body))
            .map_err(|n| match n {})
            .boxed(),
    );
    *r.status_mut() = hyper::StatusCode::FORBIDDEN;
    let h = r.headers_mut();
    h.insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static(ctype),
    );
    h.insert(
        "x-amzn-errortype",
        hyper::header::HeaderValue::from_static("ExpiredTokenException"),
    );
    r
}

/// O pedido usa um protocolo JSON (o erro vai em JSON).
pub fn json_protocol(headers: &hyper::HeaderMap) -> bool {
    headers.contains_key("x-amz-target")
        || headers
            .get(hyper::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|c| c.starts_with("application/x-amz-json") || c.contains("json"))
}

/// O hash do corpo: o `x-amz-content-sha256` do pedido, quando há.
pub fn declared_hash(headers: &hyper::HeaderMap) -> Option<String> {
    headers
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Os cabeçalhos do pedido, com o nome em minúsculas.
fn header_list(headers: &hyper::HeaderMap, uri: &hyper::Uri) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    // Um pedido HTTP/2 traz a autoridade no `:authority`, sem o `Host`: o
    // SDK assinou `host` com ela.
    if !out.iter().any(|(k, _)| k == "host")
        && let Some(a) = uri.authority()
    {
        out.push(("host".into(), a.to_string()));
    }
    out
}

/// Reassina o pedido com a chave real `c`. `body_hash` é o hash do corpo
/// lido (sem o `x-amz-content-sha256`). Num `aws-chunked` assinado,
/// devolve o assinador dos pedaços, semeado com a assinatura nova.
pub fn resign(
    parts: &mut hyper::http::request::Parts,
    mode: &Mode,
    c: &AwsCreds,
    body_hash: Option<&str>,
) -> Option<ChunkSigner> {
    let scope = mode.scope().clone();
    let s3 = scope.service == "s3";
    let key = scope.key(&c.secret);
    match mode {
        Mode::Header(auth) => {
            let mut signed = auth.signed.clone();
            let token = "x-amz-security-token";
            parts.headers.remove(token);
            signed.retain(|h| h != token);
            if !c.session_token.is_empty()
                && let Ok(v) = hyper::header::HeaderValue::from_str(&c.session_token)
            {
                parts.headers.insert(token, v);
                signed.push(token.into());
                signed.sort();
            }
            let amz_date = parts
                .headers
                .get("x-amz-date")
                .and_then(|v| v.to_str().ok())?
                .to_string();
            let payload = declared_hash(&parts.headers)
                .or(body_hash.map(str::to_string))
                .unwrap_or_else(|| sigv4::EMPTY_HASH.into());
            let headers = header_list(&parts.headers, &parts.uri);
            let r = sigv4::Request {
                method: parts.method.as_str(),
                path: parts.uri.path(),
                query: parts.uri.query().unwrap_or_default(),
                headers: &headers,
                payload_hash: &payload,
            };
            let canonical = sigv4::canonical_request(&r, &signed, s3, None);
            let sig = sigv4::sign(&key, &amz_date, &scope, &canonical);
            let new = Auth {
                access_key: c.access_key_id.clone(),
                scope: scope.clone(),
                signed,
                signature: sig.clone(),
            };
            if let Ok(v) = hyper::header::HeaderValue::from_str(&new.header()) {
                parts.headers.insert(hyper::header::AUTHORIZATION, v);
            }
            (payload == sigv4::STREAMING || payload == sigv4::STREAMING_TRAILER)
                .then(|| ChunkSigner::new(key, &amz_date, scope, &sig))
        }
        Mode::Query(_, _) => {
            let q = parts.uri.query().unwrap_or_default();
            let amz_date = query_value(q, "X-Amz-Date")?;
            let signed: Vec<String> = query_value(q, "X-Amz-SignedHeaders")?
                .split(';')
                .map(|s| s.to_ascii_lowercase())
                .collect();
            let mut pairs: Vec<(String, String)> = sigv4::query_pairs(q)
                .into_iter()
                .filter(|(k, _)| {
                    !k.eq_ignore_ascii_case("X-Amz-Signature")
                        && !k.eq_ignore_ascii_case("X-Amz-Security-Token")
                })
                .collect();
            for (k, v) in pairs.iter_mut() {
                if k.eq_ignore_ascii_case("X-Amz-Credential") {
                    *v = sigv4::uri_encode(
                        format!("{}/{}", c.access_key_id, scope.text()).as_bytes(),
                        true,
                    );
                }
            }
            if !c.session_token.is_empty() {
                pairs.push((
                    "X-Amz-Security-Token".into(),
                    sigv4::uri_encode(c.session_token.as_bytes(), true),
                ));
            }
            let newq = pairs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");
            let payload = if s3 {
                "UNSIGNED-PAYLOAD".to_string()
            } else {
                body_hash.unwrap_or(sigv4::EMPTY_HASH).to_string()
            };
            let headers = header_list(&parts.headers, &parts.uri);
            let r = sigv4::Request {
                method: parts.method.as_str(),
                path: parts.uri.path(),
                query: &newq,
                headers: &headers,
                payload_hash: &payload,
            };
            let canonical = sigv4::canonical_request(&r, &signed, s3, None);
            let sig = sigv4::sign(&key, &amz_date, &scope, &canonical);
            let uri = format!("{}?{newq}&X-Amz-Signature={sig}", parts.uri.path());
            if let Ok(u) = uri.parse() {
                parts.uri = u;
            }
            None
        }
    }
}

/// O corpo de um `aws-chunked` assinado, com cada pedaço reassinado.
pub struct ChunkBody {
    inner: Boxed,
    rw: ChunkRewriter,
    done: bool,
}

impl ChunkBody {
    pub fn new(inner: Boxed, signer: ChunkSigner) -> ChunkBody {
        ChunkBody {
            inner,
            rw: ChunkRewriter::new(signer),
            done: false,
        }
    }
}

impl Body for ChunkBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, hyper::Error>>> {
        loop {
            if self.done {
                return Poll::Ready(None);
            }
            match Pin::new(&mut self.inner).poll_frame(cx) {
                Poll::Ready(Some(Ok(f))) => match f.into_data() {
                    Ok(d) => {
                        let out = self.rw.push(&d);
                        if !out.is_empty() {
                            return Poll::Ready(Some(Ok(Frame::data(Bytes::from(out)))));
                        }
                    }
                    Err(f) => return Poll::Ready(Some(Ok(f))),
                },
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    self.done = true;
                    let rest = self.rw.finish();
                    if !rest.is_empty() {
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(rest)))));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

fn full(b: impl Into<Bytes>) -> Boxed {
    http_body_util::Full::new(b.into())
        .map_err(|n| match n {})
        .boxed()
}

/// O que segue para o destino, depois da AWS: o pedido reassinado (o nome
/// da identidade) e, num pedido que emite credenciais, a identidade que
/// pediu.
pub struct Prepared {
    pub parts: hyper::http::request::Parts,
    pub body: Boxed,
    pub name: Option<String>,
    pub capture: Option<String>,
}

/// Prepara um pedido numa aba com identidades da AWS: reassina o que veio
/// com o par falso de uma identidade (lendo o corpo só sem o hash
/// declarado) e marca o pedido que emite credenciais. `Err` é a resposta
/// do gateway (sem o real, ou corpo grande demais) e o status dela.
pub async fn prepare(
    mut parts: hyper::http::request::Parts,
    body: Boxed,
    list: &[Injection],
) -> Result<Prepared, Box<(hyper::Response<Boxed>, u16)>> {
    use hyper::header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
    if !list.iter().any(is_aws) {
        return Ok(Prepared {
            parts,
            body,
            name: None,
            capture: None,
        });
    }
    let mode = mode_of(&parts.headers, &parts.uri);
    let ident = mode.as_ref().and_then(|m| identity(m, list));
    if let Some(i) = ident
        && i.aws.is_none()
    {
        let why = if i.error.is_empty() {
            sandmux_spec::tr!("o real falta", "the real one is missing").to_string()
        } else {
            i.error.clone()
        };
        let msg = format!("sandmux: {}: {why}", i.name);
        return Err(Box::new((
            error_response(json_protocol(&parts.headers), &msg),
            403,
        )));
    }
    let need_hash = match (&mode, ident) {
        (Some(Mode::Header(_)), Some(_)) => declared_hash(&parts.headers).is_none(),
        (Some(Mode::Query(_, s)), Some(_)) => s.service != "s3",
        _ => false,
    };
    let form = parts.method == hyper::Method::POST
        && parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|c| c.contains("x-www-form-urlencoded"));
    let mut body = body;
    let mut raw = None;
    if need_hash || form {
        let limit = if need_hash { MAX_BODY } else { 64 * 1024 };
        let Ok(c) = http_body_util::Limited::new(body, limit).collect().await else {
            let mut r = hyper::Response::new(full(sandmux_spec::tr!(
                "sandmux: corpo grande demais para reassinar\n",
                "sandmux: body too large to re-sign\n"
            )));
            *r.status_mut() = hyper::StatusCode::PAYLOAD_TOO_LARGE;
            return Err(Box::new((r, 413)));
        };
        let b = c.to_bytes();
        parts.headers.remove(TRANSFER_ENCODING);
        parts.headers.insert(CONTENT_LENGTH, b.len().into());
        body = full(b.clone());
        raw = Some(b);
    }
    let capture = issues_credentials(
        &parts.method,
        &parts.uri,
        &parts.headers,
        raw.as_deref().unwrap_or_default(),
    )
    .then(|| ident.map_or_else(|| "aws".to_string(), |i| i.name.clone()));
    if capture.is_some() {
        parts.headers.remove(ACCEPT_ENCODING);
    }
    let mut name = None;
    if let (Some(m), Some(i)) = (&mode, ident)
        && let Some(c) = i.aws.as_ref()
    {
        let hash = raw.as_ref().map(|b| sigv4::sha256_hex(b));
        if let Some(signer) = resign(&mut parts, m, c, hash.as_deref()) {
            body = ChunkBody::new(body, signer).boxed();
        }
        name = Some(i.name.clone());
    }
    Ok(Prepared {
        parts,
        body,
        name,
        capture,
    })
}

/// A resposta de um pedido que emite credenciais, com a credencial real
/// trocada pelos falsos que o daemon guarda. `Err` recusa a resposta (o
/// real nunca chega ao sandbox): o status e o motivo.
pub async fn swap_response(
    resp: hyper::Response<hyper::body::Incoming>,
    ctx: &crate::mitm::Ctx,
    parent: &str,
) -> Result<hyper::Response<Boxed>, (u16, String)> {
    use hyper::header::{CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING};
    let (mut parts, body) = resp.into_parts();
    let encoded = parts
        .headers
        .get(CONTENT_ENCODING)
        .is_some_and(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"));
    if encoded {
        return Err((
            502,
            sandmux_spec::tr!(
                "sandmux: resposta de credenciais comprimida recusada",
                "sandmux: compressed credentials answer refused"
            )
            .into(),
        ));
    }
    let Ok(c) = http_body_util::Limited::new(body, MAX_ANSWER)
        .collect()
        .await
    else {
        return Err((
            502,
            sandmux_spec::tr!(
                "sandmux: resposta de credenciais grande demais",
                "sandmux: credentials answer too large"
            )
            .into(),
        ));
    };
    let raw = c.to_bytes();
    let out = match find_creds(&raw) {
        None => raw.to_vec(),
        Some(f) => {
            let Some(access) = ctx.vault.clone() else {
                return Err((
                    502,
                    sandmux_spec::tr!(
                        "sandmux: sem cofre para guardar a credencial",
                        "sandmux: no vault to store the credential"
                    )
                    .into(),
                ));
            };
            let creds = AwsCreds {
                access_key_id: f.id.clone(),
                secret: f.secret.clone(),
                session_token: f.token.clone(),
                session_fake: String::new(),
            };
            let (tag, parent, expires) = (ctx.tag.clone(), parent.to_string(), f.expires);
            let fakes = tokio::task::spawn_blocking(move || {
                crate::inject::cloud_capture(&access, &tag, &parent, creds, expires)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            match fakes.map(|x| with_fakes(&raw, &f, &x)) {
                Ok(Some(b)) => b,
                Ok(None) => {
                    return Err((
                        502,
                        sandmux_spec::tr!(
                            "sandmux: a credencial não pôde ser trocada",
                            "sandmux: the credential could not be swapped"
                        )
                        .into(),
                    ));
                }
                Err(e) => return Err((502, format!("sandmux: {e}"))),
            }
        }
    };
    parts.headers.remove(TRANSFER_ENCODING);
    parts.headers.insert(CONTENT_LENGTH, out.len().into());
    Ok(hyper::Response::from_parts(parts, full(out)))
}

/// Uma credencial achada numa resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub secret: String,
    pub token: String,
    pub expires: u64,
}

fn xml_tag(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = text.find(&open)? + open.len();
    let j = text[i..].find(&close)? + i;
    Some(text[i..j].trim().to_string())
}

fn json_creds(v: &serde_json::Value) -> Option<Found> {
    let get = |o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]| {
        keys.iter().find_map(|k| o.get(*k)).cloned()
    };
    match v {
        serde_json::Value::Object(o) => {
            let id = get(o, &["accessKeyId", "AccessKeyId"]);
            let secret = get(o, &["secretAccessKey", "SecretAccessKey", "SecretKey"]);
            if let (Some(serde_json::Value::String(id)), Some(serde_json::Value::String(secret))) =
                (id, secret)
            {
                let token = get(o, &["sessionToken", "SessionToken"])
                    .and_then(|t| t.as_str().map(str::to_string))
                    .unwrap_or_default();
                let expires = match get(o, &["expiration", "Expiration"]) {
                    Some(serde_json::Value::Number(n)) => {
                        let x = n.as_f64().unwrap_or(0.0);
                        // Milissegundos (SSO) ou segundos (Cognito).
                        if x > 1e11 {
                            (x / 1000.0) as u64
                        } else {
                            x as u64
                        }
                    }
                    Some(serde_json::Value::String(s)) => {
                        sandmux_spec::net::parse_rfc3339(&s).unwrap_or(0)
                    }
                    _ => 0,
                };
                return Some(Found {
                    id,
                    secret,
                    token,
                    expires,
                });
            }
            o.values().find_map(json_creds)
        }
        serde_json::Value::Array(a) => a.iter().find_map(json_creds),
        _ => None,
    }
}

/// A credencial de uma resposta do STS (XML ou JSON), do SSO ou do Cognito.
pub fn find_creds(body: &[u8]) -> Option<Found> {
    let text = std::str::from_utf8(body).ok()?;
    if let (Some(id), Some(secret)) = (
        xml_tag(text, "AccessKeyId"),
        xml_tag(text, "SecretAccessKey"),
    ) {
        return Some(Found {
            id,
            secret,
            token: xml_tag(text, "SessionToken").unwrap_or_default(),
            expires: xml_tag(text, "Expiration")
                .and_then(|e| sandmux_spec::net::parse_rfc3339(&e))
                .unwrap_or(0),
        });
    }
    json_creds(&serde_json::from_str(text).ok()?)
}

/// A resposta com a credencial real trocada pelos falsos; `None` se algo
/// real sobrou.
pub fn with_fakes(body: &[u8], f: &Found, fakes: &(String, String, String)) -> Option<Vec<u8>> {
    let mut text = std::str::from_utf8(body).ok()?.to_string();
    text = text.replace(&f.secret, &fakes.1);
    if !f.token.is_empty() {
        text = text.replace(&f.token, &fakes.2);
    }
    text = text.replace(&f.id, &fakes.0);
    let leaked = text.contains(&f.secret) || (!f.token.is_empty() && text.contains(&f.token));
    (!leaked).then(|| text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aws_inj(fake: &str, id: &str, secret: &str, token: &str) -> Injection {
        Injection {
            name: "aws:dev".into(),
            domains: vec!["*.amazonaws.com".into()],
            fake: fake.into(),
            aws: Some(AwsCreds {
                access_key_id: id.into(),
                secret: secret.into(),
                session_token: token.into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    const FAKE: &str = "AKIASANDMUXAAAAAAAAA";

    #[test]
    fn reassinado_confere_com_a_chave_real() {
        let req = hyper::Request::builder()
            .method("GET")
            .uri("/?list-type=2&prefix=a%20b")
            .header("host", "bucket.s3.sa-east-1.amazonaws.com")
            .header("x-amz-date", "20260101T000000Z")
            .header("x-amz-content-sha256", sigv4::EMPTY_HASH)
            .header(
                "authorization",
                format!("AWS4-HMAC-SHA256 Credential={FAKE}/20260101/sa-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=00"),
            )
            .body(())
            .unwrap();
        let list = vec![aws_inj(FAKE, "AKIAREAL", "SEGREDO", "TOKEN-REAL")];
        let mode = mode_of(req.headers(), req.uri()).unwrap();
        let i = identity(&mode, &list).unwrap();
        let (mut parts, _) = req.into_parts();
        assert!(resign(&mut parts, &mode, i.aws.as_ref().unwrap(), None).is_none());
        let auth = Auth::parse(parts.headers["authorization"].to_str().unwrap()).unwrap();
        assert_eq!(auth.access_key, "AKIAREAL");
        assert_eq!(
            auth.signed,
            vec![
                "host",
                "x-amz-content-sha256",
                "x-amz-date",
                "x-amz-security-token"
            ]
        );
        assert_eq!(parts.headers["x-amz-security-token"], "TOKEN-REAL");
        // Quem tem a chave real confere a assinatura.
        let headers = header_list(&parts.headers, &parts.uri);
        let r = sigv4::Request {
            method: "GET",
            path: "/",
            query: "list-type=2&prefix=a%20b",
            headers: &headers,
            payload_hash: sigv4::EMPTY_HASH,
        };
        let c = sigv4::canonical_request(&r, &auth.signed, true, None);
        assert_eq!(
            sigv4::sign(
                &auth.scope.key("SEGREDO"),
                "20260101T000000Z",
                &auth.scope,
                &c
            ),
            auth.signature
        );
        // Sem token real, o falso sai do pedido e da lista.
        let req = hyper::Request::builder()
            .uri("/")
            .header("host", "sts.amazonaws.com")
            .header("x-amz-date", "20260101T000000Z")
            .header("x-amz-security-token", "falso")
            .header(
                "authorization",
                format!("AWS4-HMAC-SHA256 Credential={FAKE}/20260101/us-east-1/sts/aws4_request, SignedHeaders=host;x-amz-date;x-amz-security-token, Signature=00"),
            )
            .body(())
            .unwrap();
        let mode = mode_of(req.headers(), req.uri()).unwrap();
        let (mut parts, _) = req.into_parts();
        let c = AwsCreds {
            access_key_id: "AKIAREAL".into(),
            secret: "S".into(),
            ..Default::default()
        };
        resign(&mut parts, &mode, &c, Some(sigv4::EMPTY_HASH));
        assert!(!parts.headers.contains_key("x-amz-security-token"));
        assert!(
            !parts.headers["authorization"]
                .to_str()
                .unwrap()
                .contains("security-token")
        );
    }

    #[test]
    fn url_pre_assinada_reassinada() {
        let uri: hyper::Uri = format!(
            "/obj.txt?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={FAKE}%2F20260101%2Fsa-east-1%2Fs3%2Faws4_request&X-Amz-Date=20260101T000000Z&X-Amz-Expires=300&X-Amz-SignedHeaders=host&X-Amz-Signature=00"
        )
        .parse()
        .unwrap();
        let req = hyper::Request::builder()
            .uri(uri)
            .header("host", "b.s3.amazonaws.com")
            .body(())
            .unwrap();
        let mode = mode_of(req.headers(), req.uri()).unwrap();
        assert!(matches!(mode, Mode::Query(ref id, _) if id == FAKE));
        let (mut parts, _) = req.into_parts();
        let c = AwsCreds {
            access_key_id: "AKIAREAL".into(),
            secret: "SEGREDO".into(),
            session_token: "T/1+2".into(),
            ..Default::default()
        };
        resign(&mut parts, &mode, &c, None);
        let q = parts.uri.query().unwrap().to_string();
        assert!(
            q.contains("X-Amz-Credential=AKIAREAL%2F20260101%2Fsa-east-1%2Fs3%2Faws4_request"),
            "{q}"
        );
        assert!(q.contains("X-Amz-Security-Token=T%2F1%2B2"), "{q}");
        let sig = query_value(&q, "X-Amz-Signature").unwrap();
        let without = q.split("&X-Amz-Signature=").next().unwrap();
        let headers = vec![("host".to_string(), "b.s3.amazonaws.com".to_string())];
        let r = sigv4::Request {
            method: "GET",
            path: "/obj.txt",
            query: without,
            headers: &headers,
            payload_hash: "UNSIGNED-PAYLOAD",
        };
        let scope = mode.scope().clone();
        let canonical = sigv4::canonical_request(&r, &["host".to_string()], true, None);
        assert_eq!(
            sigv4::sign(
                &scope.key("SEGREDO"),
                "20260101T000000Z",
                &scope,
                &canonical
            ),
            sig
        );
    }

    #[test]
    fn pedidos_que_emitem_credenciais() {
        let get = hyper::Method::GET;
        let post = hyper::Method::POST;
        let none = hyper::HeaderMap::new();
        let u = |s: &str| s.parse::<hyper::Uri>().unwrap();
        assert!(issues_credentials(
            &post,
            &u("/"),
            &none,
            b"Action=AssumeRole&Version=2011-06-15"
        ));
        assert!(issues_credentials(
            &get,
            &u("/?Action=GetSessionToken"),
            &none,
            b""
        ));
        assert!(!issues_credentials(
            &post,
            &u("/"),
            &none,
            b"Action=GetCallerIdentity"
        ));
        assert!(issues_credentials(
            &get,
            &u("/federation/credentials?role_name=x"),
            &none,
            b""
        ));
        let mut h = hyper::HeaderMap::new();
        h.insert(
            "x-amz-target",
            "AWSCognitoIdentityService.GetCredentialsForIdentity"
                .parse()
                .unwrap(),
        );
        assert!(issues_credentials(&post, &u("/"), &h, b"{}"));
        h.insert(
            "x-amz-target",
            "AWSSecurityTokenServiceV20110615.AssumeRole"
                .parse()
                .unwrap(),
        );
        assert!(issues_credentials(&post, &u("/"), &h, b"{}"));
    }

    #[test]
    fn credenciais_das_respostas() {
        let xml = b"<AssumeRoleResponse><AssumeRoleResult><Credentials><AccessKeyId>ASIAREAL</AccessKeyId><SecretAccessKey>SEG/REAL+1</SecretAccessKey><SessionToken>TOK-REAL</SessionToken><Expiration>2026-11-22T18:00:00Z</Expiration></Credentials></AssumeRoleResult></AssumeRoleResponse>";
        let f = find_creds(xml).unwrap();
        assert_eq!(
            (f.id.as_str(), f.secret.as_str(), f.token.as_str()),
            ("ASIAREAL", "SEG/REAL+1", "TOK-REAL")
        );
        assert_eq!(
            Some(f.expires),
            sandmux_spec::net::parse_rfc3339("2026-11-22T18:00:00Z")
        );
        let fakes = (
            "ASIASANDMUXF".to_string(),
            "fakesecret".to_string(),
            "faketoken".to_string(),
        );
        let out = String::from_utf8(with_fakes(xml, &f, &fakes).unwrap()).unwrap();
        assert!(!out.contains("REAL"), "{out}");
        assert!(out.contains("<AccessKeyId>ASIASANDMUXF</AccessKeyId>"));
        // SSO (milissegundos) e Cognito (segundos).
        let sso = br#"{"roleCredentials":{"accessKeyId":"ASIA1","secretAccessKey":"S1","sessionToken":"T1","expiration":1800000000000}}"#;
        assert_eq!(find_creds(sso).unwrap().expires, 1_800_000_000);
        let cog = br#"{"Credentials":{"AccessKeyId":"ASIA2","SecretKey":"S2","SessionToken":"T2","Expiration":1.8e9},"IdentityId":"x"}"#;
        let f = find_creds(cog).unwrap();
        assert_eq!((f.secret.as_str(), f.expires), ("S2", 1_800_000_000));
        assert!(find_creds(b"<GetCallerIdentityResponse/>").is_none());
    }

    #[test]
    fn erro_sem_o_real() {
        let r = error_response(false, "sandmux: rode `aws sso login` no host <x>");
        assert_eq!(r.status(), 403);
        let mut h = hyper::HeaderMap::new();
        assert!(!json_protocol(&h));
        h.insert(
            "x-amz-target",
            "DynamoDB_20120810.ListTables".parse().unwrap(),
        );
        assert!(json_protocol(&h));
    }

    fn signed_parts(extra: &[(&str, &str)]) -> hyper::http::request::Parts {
        let mut b = hyper::Request::builder()
            .method("POST")
            .uri("/")
            .header("host", "sts.amazonaws.com")
            .header("x-amz-date", "20260101T000000Z")
            .header(
                "authorization",
                format!("AWS4-HMAC-SHA256 Credential={FAKE}/20260101/us-east-1/sts/aws4_request, SignedHeaders=host;x-amz-date, Signature=00"),
            );
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        b.body(()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn preparo_sem_o_real_e_corpo_grande() {
        // Sem identidade da AWS, passa sem mudar.
        let r = prepare(signed_parts(&[]), full("x"), &[])
            .await
            .ok()
            .unwrap();
        assert!(r.name.is_none() && r.capture.is_none());
        // A identidade sem o real: 403 do gateway.
        let sem = Injection {
            name: "aws:dev".into(),
            fake: FAKE.into(),
            error: "rode `aws sso login` no host".into(),
            ..Default::default()
        };
        let Err(e) = prepare(signed_parts(&[]), full("x"), &[sem]).await else {
            panic!("sem o 403");
        };
        let (resp, status) = *e;
        assert_eq!(status, 403);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("aws sso login"));
        // Sem o hash declarado, um corpo acima do limite: 413.
        let list = vec![aws_inj(FAKE, "AKIAREAL", "S", "")];
        let big = vec![b'a'; MAX_BODY + 1];
        let Err(e) = prepare(signed_parts(&[]), full(big), &list).await else {
            panic!("sem o 413");
        };
        assert_eq!(e.1, 413);
        // Com o corpo lido, o GetSessionToken é marcado para a troca.
        let r = prepare(
            signed_parts(&[
                ("content-type", "application/x-www-form-urlencoded"),
                ("accept-encoding", "gzip"),
            ]),
            full("Action=GetSessionToken&Version=2011-06-15"),
            &list,
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(r.name.as_deref(), Some("aws:dev"));
        assert_eq!(r.capture.as_deref(), Some("aws:dev"));
        assert!(!r.parts.headers.contains_key("accept-encoding"));
        assert!(
            r.parts.headers["authorization"]
                .to_str()
                .unwrap()
                .contains("Credential=AKIAREAL/")
        );
    }

    #[test]
    fn host_do_authority_num_pedido_http2() {
        let req = hyper::Request::builder()
            .uri("https://bucket.s3.amazonaws.com/?list-type=2")
            .header("x-amz-date", "20260101T000000Z")
            .header("x-amz-content-sha256", sigv4::EMPTY_HASH)
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let h = header_list(&parts.headers, &parts.uri);
        assert!(
            h.contains(&("host".to_string(), "bucket.s3.amazonaws.com".to_string())),
            "{h:?}"
        );
    }
}
