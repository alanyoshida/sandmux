//! Identidades de cloud (GCP): a conferência das fontes e a obtenção do
//! token de acesso real, sempre fora do sandbox. O real só existe na memória
//! de quem chama; o sandbox recebe falsos, que o gateway troca.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sandmux_proto::msg::CloudAdd;
use sandmux_spec::{tr, trf};

/// Variável que aponta o `gcloud` do host (sem ela, o do `PATH`).
pub const GCLOUD_ENV: &str = "SANDMUX_GCLOUD";
/// Endpoint de token do Google.
pub const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
/// Escopo pedido para uma conta de serviço.
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
/// Prazo do `gcloud` do host.
const GCLOUD_TIMEOUT: Duration = Duration::from_secs(30);

/// O `gcloud` do host.
pub fn gcloud_bin() -> String {
    std::env::var(GCLOUD_ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "gcloud".into())
}

/// O `gcloud` do host existe?
pub fn gcloud_present() -> bool {
    let b = gcloud_bin();
    if b.contains('/') {
        return std::path::Path::new(&b).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(&b).is_file()))
}

/// Um nome de identidade: letras, números, `-` e `_`.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn json(text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text).map_err(|e| {
        trf!(
            "o arquivo não é um JSON válido: {e}",
            "the file is not valid JSON: {e}"
        )
    })
}

fn field<'a>(v: &'a serde_json::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

/// Confere um ADC de usuário (`authorized_user`) e devolve a conta, quando
/// ele diz.
pub fn check_adc(text: &str) -> Result<String, String> {
    let v = json(text)?;
    if field(&v, "type") != "authorized_user" {
        return Err(trf!(
            "o ADC é do tipo {:?}, e só o authorized_user (o do `gcloud auth application-default login`) pode ser copiado",
            "the ADC has type {:?}, and only authorized_user (the one from `gcloud auth application-default login`) can be copied",
            field(&v, "type")
        ));
    }
    for k in ["client_id", "client_secret", "refresh_token"] {
        if field(&v, k).is_empty() {
            return Err(trf!("o ADC não tem {k}", "the ADC lacks {k}"));
        }
    }
    Ok(field(&v, "account").to_string())
}

/// Confere a chave JSON de uma conta de serviço e devolve a conta e o
/// projeto.
pub fn check_key(text: &str) -> Result<(String, String), String> {
    let v = json(text)?;
    if field(&v, "type") != "service_account" {
        return Err(tr!(
            "o arquivo não é a chave de uma conta de serviço (falta \"type\": \"service_account\")",
            "the file is not a service account key (it lacks \"type\": \"service_account\")"
        )
        .into());
    }
    for k in ["client_email", "private_key"] {
        if field(&v, k).is_empty() {
            return Err(trf!("a chave não tem {k}", "the key lacks {k}"));
        }
    }
    use rsa::pkcs8::DecodePrivateKey;
    rsa::RsaPrivateKey::from_pkcs8_pem(field(&v, "private_key")).map_err(|e| {
        trf!(
            "a chave privada da conta de serviço não abre: {e}",
            "the service account private key does not open: {e}"
        )
    })?;
    Ok((
        field(&v, "client_email").to_string(),
        field(&v, "project_id").to_string(),
    ))
}

/// Por que uma CLI do host falhou.
pub(crate) enum GcloudErr {
    /// Não está instalada.
    Missing,
    /// Falhou, com a mensagem.
    Failed(String),
}

impl GcloudErr {
    fn text(&self) -> String {
        match self {
            GcloudErr::Missing => tr!(
                "o gcloud não está instalado no host",
                "gcloud is not installed on the host"
            )
            .into(),
            GcloudErr::Failed(m) => m.clone(),
        }
    }
}

/// Roda o `gcloud` do host com `args`, sem perguntas e com prazo. Devolve a
/// saída padrão, ou o erro.
fn gcloud(args: &[&str]) -> Result<String, GcloudErr> {
    run_host(
        &gcloud_bin(),
        args,
        &[("CLOUDSDK_CORE_DISABLE_PROMPTS".into(), "1".into())],
        GCLOUD_TIMEOUT,
    )
}

/// Roda uma CLI do host (`bin`), com o ambiente do daemon mais `env`, sem
/// terminal nem entrada, e com prazo. Devolve a saída padrão; numa falha, o
/// erro padrão.
pub(crate) fn run_host(
    bin: &str,
    args: &[&str],
    env: &[(String, String)],
    timeout: Duration,
) -> Result<String, GcloudErr> {
    let mut child = Command::new(bin)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GcloudErr::Missing
            } else {
                GcloudErr::Failed(trf!(
                    "rodar {bin} no host: {e}",
                    "run {bin} on the host: {e}"
                ))
            }
        })?;
    let mut out = child.stdout.take().expect("stdout");
    let mut errp = child.stderr.take().expect("stderr");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let ereader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = errp.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let secs = timeout.as_secs();
                return Err(GcloudErr::Failed(trf!(
                    "{bin} não respondeu em {secs} s",
                    "{bin} did not answer within {secs} s"
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(GcloudErr::Failed(e.to_string())),
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = ereader.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        Err(GcloudErr::Failed(stderr.trim().to_string()))
    }
}

/// A conta e o projeto de uma configuração do `gcloud` do host.
pub fn host_configuration(conf: &str) -> Result<(String, String), String> {
    let out = gcloud(&[
        "config",
        "configurations",
        "describe",
        conf,
        "--format=json",
    ])
    .map_err(|e| match e {
        GcloudErr::Missing => e.text(),
        GcloudErr::Failed(m) => trf!(
            "a configuração {conf:?} não existe no gcloud do host ({m})",
            "the configuration {conf:?} does not exist in the host gcloud ({m})"
        ),
    })?;
    let v = json(&out)?;
    let core = v.pointer("/properties/core").cloned().unwrap_or_default();
    Ok((
        field(&core, "account").to_string(),
        field(&core, "project").to_string(),
    ))
}

/// O token de acesso real e até quando vale (segundos desde 1970). No
/// Kubernetes, também o API server e o certificado de cliente.
#[derive(Clone, Default)]
pub struct Real {
    pub token: String,
    pub expires: u64,
    pub kube: Option<sandmux_proto::msg::KubeTarget>,
}

impl std::fmt::Debug for Real {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Real")
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Segundos desde 1970 de um instante `AAAA-MM-DDTHH:MM:SS[.fff]Z`.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let s = s.trim().strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    // Dias desde 1970 (algoritmo de Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs).ok()
}

/// O real de uma saída do `gcloud config config-helper --format=json`.
pub fn parse_config_helper(out: &str) -> Result<Real, String> {
    let v = json(out)?;
    let c = v.get("credential").cloned().unwrap_or_default();
    let token = field(&c, "access_token");
    if token.is_empty() {
        return Err(tr!(
            "o gcloud do host não deu um token de acesso",
            "the host gcloud gave no access token"
        )
        .into());
    }
    let expires = parse_rfc3339(field(&c, "token_expiry")).unwrap_or_else(|| now() + 3000);
    Ok(Real {
        kube: None,
        token: token.to_string(),
        expires,
    })
}

/// A mensagem do que fazer no host quando o `gcloud` falha.
fn gcloud_failure(conf: &str, e: &GcloudErr) -> String {
    let err = match e {
        GcloudErr::Missing => {
            return trf!(
                "{}; instale o gcloud e rode `gcloud auth login` no host",
                "{}; install gcloud and run `gcloud auth login` on the host",
                e.text()
            );
        }
        GcloudErr::Failed(m) => m.as_str(),
    };
    let low = err.to_ascii_lowercase();
    if low.contains("reauth") || low.contains("login") || low.contains("credential") {
        return trf!(
            "o gcloud do host pede um login novo: rode `gcloud auth login` no host (configuração {conf})",
            "the host gcloud needs a new login: run `gcloud auth login` on the host (configuration {conf})"
        );
    }
    trf!(
        "o gcloud do host falhou na configuração {conf}: {err}",
        "the host gcloud failed on configuration {conf}: {err}"
    )
}

/// Obtém o real de uma identidade de cloud, fora do sandbox.
pub fn resolve(id: &CloudAdd) -> Result<Real, String> {
    if id.info.provider == "kubernetes" {
        return crate::kube::resolve(id);
    }
    match id.info.source.as_str() {
        "host" => {
            let conf = id.info.configuration.as_str();
            let out = gcloud(&[
                "config",
                "config-helper",
                "--format=json",
                "--configuration",
                conf,
            ])
            .map_err(|e| gcloud_failure(conf, &e))?;
            parse_config_helper(&out)
        }
        "adc" => {
            let v = json(&id.secret)?;
            let body = refresh_body(&v);
            token_request(&token_uri(&v), &body)
        }
        "key" => {
            let v = json(&id.secret)?;
            let jwt = assertion(&v, now())?;
            let body = format!(
                "grant_type={}&assertion={jwt}",
                form("urn:ietf:params:oauth:grant-type:jwt-bearer")
            );
            token_request(&token_uri(&v), &body)
        }
        other => Err(trf!(
            "fonte de identidade desconhecida: {other}",
            "unknown identity source: {other}"
        )),
    }
}

fn token_uri(v: &serde_json::Value) -> String {
    let u = field(v, "token_uri");
    if u.is_empty() {
        TOKEN_URI.into()
    } else {
        u.into()
    }
}

/// Um valor num formulário `application/x-www-form-urlencoded`.
pub fn form(v: &str) -> String {
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

/// O corpo do grant `refresh_token` de um ADC de usuário.
pub fn refresh_body(v: &serde_json::Value) -> String {
    format!(
        "grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}",
        form(field(v, "client_id")),
        form(field(v, "client_secret")),
        form(field(v, "refresh_token"))
    )
}

fn b64url(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// A asserção JWT (RS256) de uma conta de serviço, emitida em `iat`.
pub fn assertion(v: &serde_json::Value, iat: u64) -> Result<String, String> {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::signature::{SignatureEncoding, Signer};
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(field(v, "private_key")).map_err(|e| {
        trf!(
            "a chave privada da conta de serviço não abre: {e}",
            "the service account private key does not open: {e}"
        )
    })?;
    let mut header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
    if !field(v, "private_key_id").is_empty() {
        header["kid"] = field(v, "private_key_id").into();
    }
    let claims = serde_json::json!({
        "iss": field(v, "client_email"),
        "scope": SCOPE,
        "aud": token_uri(v),
        "iat": iat,
        "exp": iat + 3600,
    });
    let signing = format!(
        "{}.{}",
        b64url(header.to_string().as_bytes()),
        b64url(claims.to_string().as_bytes())
    );
    let sk = rsa::pkcs1v15::SigningKey::<rsa::sha2::Sha256>::new(key);
    let sig = sk.sign(signing.as_bytes());
    Ok(format!("{signing}.{}", b64url(&sig.to_bytes())))
}

/// Lê a resposta do endpoint de token: o token e a validade.
pub fn parse_token_answer(status: u16, body: &str) -> Result<Real, String> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if status != 200 || field(&v, "access_token").is_empty() {
        let why = [field(&v, "error"), field(&v, "error_description")]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(": ");
        return Err(trf!(
            "o Google recusou a credencial copiada (HTTP {status}{}); copie de novo com `sandmux vault add-cloud`",
            "Google refused the copied credential (HTTP {status}{}); copy it again with `sandmux vault add-cloud`",
            if why.is_empty() {
                String::new()
            } else {
                format!(", {why}")
            }
        ));
    }
    let secs = v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(3599);
    Ok(Real {
        kube: None,
        token: field(&v, "access_token").to_string(),
        expires: now() + secs,
    })
}

/// Um `POST` de formulário ao endpoint de token, do daemon (fora do
/// sandbox). Confia também na CA de `SANDMUX_UPSTREAM_CA` (testes).
fn token_request(url: &str, body: &str) -> Result<Real, String> {
    let (status, text) = post_form(url, body)?;
    parse_token_answer(status, &text)
}

fn post_form(url: &str, body: &str) -> Result<(u16, String), String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(trf!("endereço inválido: {url}", "invalid address: {url}"));
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|e| e.to_string())?),
        None => (hostport.to_string(), if tls { 443 } else { 80 }),
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let body = body.to_string();
    let path = path.to_string();
    rt.block_on(async move {
        use http_body_util::BodyExt;
        let addr = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| e.to_string())?
            .next()
            .ok_or_else(|| tr!("host sem endereço", "host without address").to_string())?;
        let extra =
            std::env::var_os("SANDMUX_UPSTREAM_CA").and_then(|p| std::fs::read_to_string(p).ok());
        let fail = |e: String| {
            trf!(
                "sem conexão com {host}: {e}",
                "no connection to {host}: {e}"
            )
        };
        let req = hyper::Request::post(path.as_str())
            .header(hyper::header::HOST, host.as_str())
            .header(
                hyper::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(http_body_util::Full::new(bytes::Bytes::from(body)))
            .map_err(|e| e.to_string())?;
        let resp = if tls {
            let io = sandmux_net::mitm::connect_upstream(
                addr,
                &host,
                sandmux_net::tls::client_config(extra.as_deref()),
            )
            .await
            .map_err(|e| fail(e.to_string()))?;
            let (mut send, conn) =
                hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
                    .await
                    .map_err(|e| fail(e.to_string()))?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            send.send_request(req)
                .await
                .map_err(|e| fail(e.to_string()))?
        } else {
            let io = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|e| fail(e.to_string()))?;
            let (mut send, conn) =
                hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
                    .await
                    .map_err(|e| fail(e.to_string()))?;
            tokio::spawn(async move {
                let _ = conn.await;
            });
            send.send_request(req)
                .await
                .map_err(|e| fail(e.to_string()))?
        };
        let status = resp.status().as_u16();
        let bytes = http_body_util::Limited::new(resp.into_body(), 1 << 20)
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instante_rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-11-22T14:05:31Z"), Some(1795356331));
        assert_eq!(
            parse_rfc3339("2026-11-22T14:05:31.123456Z"),
            Some(1795356331)
        );
        assert_eq!(parse_rfc3339("2026-11-22 14:05:31"), None);
    }

    #[test]
    fn saida_do_config_helper() {
        let out = r#"{"configuration":{"active_configuration":"trabalho"},
            "credential":{"access_token":"ya29.real","token_expiry":"2026-11-22T14:05:31Z"}}"#;
        let r = parse_config_helper(out).unwrap();
        assert_eq!((r.token.as_str(), r.expires), ("ya29.real", 1795356331));
        assert!(!format!("{r:?}").contains("ya29.real"));
        assert!(parse_config_helper(r#"{"credential":{}}"#).is_err());
    }

    #[test]
    fn falha_do_gcloud_diz_o_que_fazer() {
        let m = gcloud_failure(
            "trabalho",
            &GcloudErr::Failed(
                "ERROR: (gcloud.config.config-helper) Reauthentication required.".into(),
            ),
        );
        assert!(gcloud_failure("x", &GcloudErr::Missing).contains("gcloud auth login"));
        assert!(m.contains("gcloud auth login"), "{m}");
        assert!(m.contains("trabalho"), "{m}");
    }

    #[test]
    fn adc_e_chave() {
        let adc = r#"{"type":"authorized_user","client_id":"c","client_secret":"s","refresh_token":"1//r"}"#;
        assert_eq!(check_adc(adc), Ok(String::new()));
        assert!(check_adc(r#"{"type":"service_account"}"#).is_err());
        assert!(check_adc(r#"{"type":"authorized_user","client_id":"c"}"#).is_err());
        assert!(check_key(adc).unwrap_err().contains("service_account"));
        let body = refresh_body(&serde_json::from_str(adc).unwrap());
        assert_eq!(
            body,
            "grant_type=refresh_token&client_id=c&client_secret=s&refresh_token=1%2F%2Fr"
        );
    }

    #[test]
    fn resposta_do_endpoint_de_token() {
        let r = parse_token_answer(200, r#"{"access_token":"ya29.x","expires_in":3599}"#).unwrap();
        assert_eq!(r.token, "ya29.x");
        assert!(r.expires >= now() + 3590);
        let e = parse_token_answer(
            400,
            r#"{"error":"invalid_grant","error_description":"Bad"}"#,
        )
        .unwrap_err();
        assert!(
            e.contains("invalid_grant") && e.contains("add-cloud"),
            "{e}"
        );
    }

    #[test]
    fn jwt_da_conta_de_servico() {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        use rsa::signature::Verifier;
        let mut rng = rand_core();
        let key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
        let v = serde_json::json!({
            "type": "service_account",
            "client_email": "deploy@app.iam.gserviceaccount.com",
            "private_key": pem,
            "private_key_id": "k1",
            "project_id": "app",
        });
        assert_eq!(
            check_key(&v.to_string()).unwrap(),
            ("deploy@app.iam.gserviceaccount.com".into(), "app".into())
        );
        let jwt = assertion(&v, 1000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        use base64::Engine;
        let d = |s: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(s)
                .unwrap()
        };
        let claims: serde_json::Value = serde_json::from_slice(&d(parts[1])).unwrap();
        assert_eq!(claims["iss"], "deploy@app.iam.gserviceaccount.com");
        assert_eq!(claims["aud"], TOKEN_URI);
        assert_eq!(claims["exp"], 4600);
        let header: serde_json::Value = serde_json::from_slice(&d(parts[0])).unwrap();
        assert_eq!(header["kid"], "k1");
        let vk = rsa::pkcs1v15::VerifyingKey::<rsa::sha2::Sha256>::new(key.to_public_key());
        let sig = rsa::pkcs1v15::Signature::try_from(d(parts[2]).as_slice()).unwrap();
        vk.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .unwrap();
    }

    /// Um gerador do `rsa` para o teste.
    fn rand_core() -> impl rsa::rand_core::CryptoRngCore {
        rsa::rand_core::OsRng
    }
}
