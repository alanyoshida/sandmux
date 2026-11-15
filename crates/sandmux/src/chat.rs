//! Assistente de chat: a conversa guardada na pasta da aba, as pastas das
//! abas de chat e o cliente HTTP dos provedores (API compatível com a da
//! OpenAI, com streaming SSE).
//!
//! O cliente roda dentro do sandbox, no `sandmux-chat`, com a chave falsa: o
//! gateway troca pelo valor real no tráfego.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use sandmux_policy::assistant::{PROVIDERS_ENV, Provider};
use sandmux_spec::{tr, trf};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

/// Arquivo da conversa, na pasta da aba.
pub const FILE: &str = "conversa.json";
/// Nome do harness interno das abas de chat.
pub const HARNESS: &str = "chat";
/// Caminho do programa dentro do sandbox.
pub const BIN: &str = "/run/sandmux/bin/sandmux-chat";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Papel {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mensagem {
    pub papel: Papel,
    pub texto: String,
    /// Tempo da resposta do modelo, em milissegundos (as conversas antigas
    /// não têm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
}

/// Variável com as cores do tema ativo para a aba de chat, um objeto JSON de
/// elemento para cor (`#rrggbb`, um índice ou um nome).
pub const THEME_ENV: &str = "SANDMUX_THEME";

/// A conversa de uma aba de chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversa {
    pub versao: u32,
    pub modelo: String,
    /// Texto de sistema, mandado antes das mensagens (o modal de comando).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sistema: Option<String>,
    pub mensagens: Vec<Mensagem>,
}

impl Conversa {
    pub fn new(modelo: &str, primeira: Option<&str>) -> Conversa {
        Conversa {
            versao: 1,
            modelo: modelo.trim().to_string(),
            sistema: None,
            mensagens: primeira
                .filter(|t| !t.trim().is_empty())
                .map(|t| Mensagem {
                    papel: Papel::User,
                    texto: t.to_string(),
                    ms: None,
                })
                .into_iter()
                .collect(),
        }
    }

    /// Lê a conversa da pasta; `None` quando não existe.
    pub fn load(dir: &Path) -> Result<Option<Conversa>> {
        let p = dir.join(FILE);
        match std::fs::read(&p) {
            Ok(b) => Ok(Some(
                serde_json::from_slice(&b)
                    .with_context(|| trf!("ler {}", "read {}", p.display()))?,
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| trf!("ler {}", "read {}", p.display())),
        }
    }

    /// Grava num temporário e renomeia.
    pub fn save(&self, dir: &Path) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let tmp = dir.join(format!(".{FILE}.tmp"));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .with_context(|| trf!("gravar {}", "write {}", tmp.display()))?;
        f.write_all(&serde_json::to_vec_pretty(self)?)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dir.join(FILE))?;
        Ok(())
    }

    /// A última mensagem é do usuário e ainda não tem resposta.
    pub fn pending(&self) -> bool {
        self.mensagens
            .last()
            .is_some_and(|m| m.papel == Papel::User)
    }
}

/// Pasta das abas de chat: `<estado>/chat`.
pub fn root(state_dir: &Path) -> PathBuf {
    state_dir.join("chat")
}

fn random_hex(n: usize) -> String {
    use std::io::Read;
    let mut b = vec![0u8; n];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .is_err()
    {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            ^ u128::from(std::process::id());
        b = t.to_le_bytes()[..n.min(16)].to_vec();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Cria a pasta privada de uma aba de chat, com a conversa inicial.
pub fn new_dir(state_dir: &Path, conversa: &Conversa) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let r = root(state_dir);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&r)
        .with_context(|| trf!("criar {}", "create {}", r.display()))?;
    crate::paths::check_private_dir(&r, unsafe { libc::getuid() })?;
    let dir = r.join(random_hex(8));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .with_context(|| trf!("criar {}", "create {}", dir.display()))?;
    conversa.save(&dir)?;
    Ok(dir)
}

/// A pasta está dentro da pasta das abas de chat (sem links)?
pub fn is_chat_dir(state_dir: &Path, dir: &Path) -> bool {
    let Ok(m) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if m.file_type().is_symlink() || !m.is_dir() {
        return false;
    }
    match (
        std::fs::canonicalize(root(state_dir)),
        std::fs::canonicalize(dir),
    ) {
        (Ok(r), Ok(d)) => d.parent() == Some(r.as_path()),
        _ => false,
    }
}

/// Apaga a pasta de uma aba de chat; qualquer outra pasta fica.
pub fn remove_dir(state_dir: &Path, dir: &Path) -> bool {
    is_chat_dir(state_dir, dir) && std::fs::remove_dir_all(dir).is_ok()
}

/// Os provedores entregues pelo daemon em [`PROVIDERS_ENV`].
pub fn providers_from_env() -> Vec<Provider> {
    std::env::var(PROVIDERS_ENV)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Para onde vai um pedido: a URL base, a chave e o modelo do provedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    pub key: Option<String>,
    pub model: String,
    /// Texto de sistema da conversa.
    pub system: Option<String>,
}

/// Resolve o modelo `<provedor>/<modelo>` entre os provedores.
pub fn target(model: &str, providers: &[Provider]) -> Result<Target, String> {
    let (p, m) = sandmux_policy::assistant::split_model(model)?;
    let prov = providers.iter().find(|x| x.name == p).ok_or_else(|| {
        trf!(
            "provedor desconhecido: {p:?}. Provedores: {}",
            "unknown provider: {p:?}. Providers: {}",
            providers
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let key = (!prov.env.is_empty())
        .then(|| std::env::var(&prov.env).ok())
        .flatten()
        .filter(|k| !k.is_empty());
    Ok(Target {
        url: prov.url.clone(),
        key,
        model: m.to_string(),
        system: None,
    })
}

/// Partes de uma URL base.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Url {
    tls: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<Url, String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(trf!(
            "URL sem http:// ou https://: {url}",
            "URL without http:// or https://: {url}"
        ));
    };
    let (auth, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let default = if tls { 443 } else { 80 };
    let (host, port) = if let Some(h) = auth.strip_prefix('[') {
        let (h, after) = h
            .split_once(']')
            .ok_or(tr!("URL com IPv6 malformado", "URL with malformed IPv6"))?;
        let port = after.strip_prefix(':').map(|p| p.parse::<u16>());
        (h.to_string(), port.transpose().map_err(|e| e.to_string())?)
    } else {
        match auth.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                Some(
                    p.parse::<u16>()
                        .map_err(|e| trf!("porta inválida: {e}", "invalid port: {e}"))?,
                ),
            ),
            None => (auth.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(trf!("URL sem host: {url}", "URL without host: {url}"));
    }
    Ok(Url {
        tls,
        host,
        port: port.unwrap_or(default),
        path: path.trim_end_matches('/').to_string(),
    })
}

/// A mensagem de erro do corpo de uma resposta de erro.
pub fn error_message(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        let m = v
            .pointer("/error/message")
            .or_else(|| v.get("error").filter(|e| e.is_string()))
            .or_else(|| v.get("message"))
            .and_then(|m| m.as_str());
        if let Some(m) = m {
            return m.to_string();
        }
    }
    let t = String::from_utf8_lossy(body);
    let t = t.trim();
    t.chars().take(300).collect()
}

/// Leitor de SSE: junta os pedaços e devolve as linhas `data:` completas.
#[derive(Default)]
pub struct Sse {
    buf: Vec<u8>,
}

/// Um evento de uma resposta em streaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    Text(String),
    Done,
    Error(String),
}

impl Sse {
    pub fn push(&mut self, data: &[u8]) -> Vec<Chunk> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        while let Some(i) = self.buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                out.push(Chunk::Done);
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };
            if v.get("error").is_some() {
                out.push(Chunk::Error(error_message(data.as_bytes())));
                continue;
            }
            if let Some(t) = v
                .pointer("/choices/0/delta/content")
                .and_then(|c| c.as_str())
                .filter(|t| !t.is_empty())
            {
                out.push(Chunk::Text(t.to_string()));
            }
        }
        out
    }
}

/// O corpo do pedido.
pub fn request_body(model: &str, system: Option<&str>, msgs: &[Mensagem], stream: bool) -> Vec<u8> {
    let messages: Vec<serde_json::Value> = system
        .map(|s| serde_json::json!({"role": "system", "content": s}))
        .into_iter()
        .chain(msgs.iter().map(|m| {
            serde_json::json!({
                "role": match m.papel { Papel::User => "user", Papel::Assistant => "assistant" },
                "content": m.texto,
            })
        }))
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    }))
    .unwrap_or_default()
}

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

async fn connect(u: &Url) -> io::Result<Box<dyn Io>> {
    let addr = tokio::net::lookup_host((u.host.as_str(), u.port))
        .await?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                tr!("host sem endereço", "host without address"),
            )
        })?;
    if u.tls {
        let tls = sandmux_net::mitm::connect_upstream(
            addr,
            &u.host,
            sandmux_net::tls::client_config(None),
        )
        .await?;
        Ok(Box::new(tls))
    } else {
        let tcp = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        .map_err(|_| {
            io::Error::new(io::ErrorKind::TimedOut, tr!("tempo esgotado", "timed out"))
        })??;
        Ok(Box::new(tcp))
    }
}

/// Faz o pedido de chat. Com `stream`, chama `on` com cada pedaço de texto.
/// Devolve o texto inteiro, ou a mensagem de erro (com o status, quando
/// houver). `cancel` interrompe a leitura.
pub async fn complete(
    t: &Target,
    msgs: &[Mensagem],
    stream: bool,
    cancel: &AtomicBool,
    on: &mut (dyn FnMut(&str) + Send),
) -> Result<String, String> {
    let u = parse_url(&t.url)?;
    let io = connect(&u).await.map_err(|e| {
        trf!(
            "sem conexão com {}: {e}",
            "no connection to {}: {e}",
            u.host
        )
    })?;
    let (mut send, conn) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
        .await
        .map_err(|e| {
            trf!(
                "falha no HTTP com {}: {e}",
                "HTTP failure with {}: {e}",
                u.host
            )
        })?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let default = if u.tls { 443 } else { 80 };
    let host = if u.port == default {
        u.host.clone()
    } else {
        format!("{}:{}", u.host, u.port)
    };
    let mut req = hyper::Request::post(format!("{}/chat/completions", u.path))
        .header("host", host)
        .header("content-type", "application/json")
        .header("user-agent", "sandmux-chat")
        .header("x-title", "sandmux")
        .header(
            "accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
    if let Some(k) = &t.key {
        req = req.header("authorization", format!("Bearer {k}"));
    }
    let req = req
        .body(Full::new(Bytes::from(request_body(
            &t.model,
            t.system.as_deref(),
            msgs,
            stream,
        ))))
        .map_err(|e| e.to_string())?;
    let resp = send.send_request(req).await.map_err(|e| {
        trf!(
            "falha no pedido a {}: {e}",
            "request to {} failed: {e}",
            u.host
        )
    })?;
    let status = resp.status();
    let mut body = resp.into_body();
    if !status.is_success() {
        let b = body
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();
        return Err(format!("{status}: {}", error_message(&b)));
    }
    let mut text = String::new();
    if !stream {
        let b = body
            .collect()
            .await
            .map_err(|e| trf!("resposta interrompida: {e}", "response interrupted: {e}"))?
            .to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&b).map_err(|_| {
            trf!(
                "resposta inválida: {}",
                "invalid response: {}",
                error_message(&b)
            )
        })?;
        if v.get("error").is_some() {
            return Err(error_message(&b));
        }
        text = v
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        on(&text);
        return Ok(text);
    }
    let mut sse = Sse::default();
    while let Some(frame) = body.frame().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(tr!("resposta cancelada", "response cancelled").into());
        }
        let frame =
            frame.map_err(|e| trf!("resposta interrompida: {e}", "response interrupted: {e}"))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        for c in sse.push(&data) {
            match c {
                Chunk::Text(t) => {
                    on(&t);
                    text.push_str(&t);
                }
                Chunk::Done => return Ok(text),
                Chunk::Error(e) => return Err(e),
            }
        }
    }
    Ok(text)
}

/// Faz o pedido num runtime próprio, na thread atual.
pub fn complete_blocking(
    t: &Target,
    msgs: &[Mensagem],
    stream: bool,
    cancel: &AtomicBool,
    on: &mut (dyn FnMut(&str) + Send),
) -> Result<String, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(complete(t, msgs, stream, cancel, on))
}

/// Os ids de `data[].id` de uma resposta de `/models` (formato da OpenAI).
pub fn model_ids(json: &[u8]) -> Vec<String> {
    crate::discover::ids_of(json)
}

/// Pede `GET <url>/models` a um provedor: os ids dos modelos.
pub async fn list_models(url: &str, key: Option<&str>) -> Result<Vec<String>, String> {
    let u = parse_url(url)?;
    let io = connect(&u).await.map_err(|e| {
        trf!(
            "sem conexão com {}: {e}",
            "no connection to {}: {e}",
            u.host
        )
    })?;
    let (mut send, conn) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
        .await
        .map_err(|e| {
            trf!(
                "falha no HTTP com {}: {e}",
                "HTTP failure with {}: {e}",
                u.host
            )
        })?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let default = if u.tls { 443 } else { 80 };
    let host = if u.port == default {
        u.host.clone()
    } else {
        format!("{}:{}", u.host, u.port)
    };
    let mut req = hyper::Request::get(format!("{}/models", u.path))
        .header("host", host)
        .header("accept", "application/json")
        .header("user-agent", "sandmux-chat");
    if let Some(k) = key {
        req = req.header("authorization", format!("Bearer {k}"));
    }
    let req = req
        .body(Full::new(Bytes::new()))
        .map_err(|e| e.to_string())?;
    let resp = send.send_request(req).await.map_err(|e| {
        trf!(
            "falha no pedido a {}: {e}",
            "request to {} failed: {e}",
            u.host
        )
    })?;
    let status = resp.status();
    let b = resp
        .into_body()
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| trf!("resposta interrompida: {e}", "response interrupted: {e}"))?;
    if !status.is_success() {
        return Err(format!("{status}: {}", error_message(&b)));
    }
    let ids = model_ids(&b);
    if ids.is_empty() {
        return Err(tr!("nenhum modelo na resposta", "no models in the answer").into());
    }
    Ok(ids)
}

/// Os modelos com todas as palavras do filtro, sem caixa.
pub fn filter_models<'a>(all: &'a [String], filter: &str) -> Vec<&'a String> {
    let words: Vec<String> = filter
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    all.iter()
        .filter(|m| {
            let l = m.to_lowercase();
            words.iter().all(|w| l.contains(w.as_str()))
        })
        .collect()
}

/// Os modelos de todos os provedores, como `provedor/modelo`, em ordem, e
/// um aviso por provedor que falhou. Cada provedor tem 10 s.
/// Os provedores conhecidos sem chave ficam de fora: sem ela, não dá para
/// conversar com eles.
pub fn all_models(providers: &[Provider]) -> (Vec<String>, Vec<String>) {
    let has_key =
        |p: &Provider| !p.env.is_empty() && std::env::var(&p.env).is_ok_and(|k| !k.is_empty());
    let handles: Vec<_> = providers
        .iter()
        .filter(|p| !p.is_known() || has_key(p))
        .cloned()
        .map(|p| {
            std::thread::spawn(move || {
                let key = (!p.env.is_empty())
                    .then(|| std::env::var(&p.env).ok())
                    .flatten()
                    .filter(|k| !k.is_empty());
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string());
                let r = rt.and_then(|rt| {
                    rt.block_on(async {
                        tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            list_models(&p.url, key.as_deref()),
                        )
                        .await
                        .unwrap_or_else(|_| {
                            Err(tr!("sem resposta em 10 s", "no answer in 10 s").into())
                        })
                    })
                });
                (p.name, r)
            })
        })
        .collect();
    let mut ids = Vec::new();
    let mut notes = Vec::new();
    for h in handles {
        let Ok((name, r)) = h.join() else { continue };
        match r {
            Ok(list) => ids.extend(list.into_iter().map(|m| format!("{name}/{m}"))),
            Err(e) => notes.push(format!("{name}: {e}")),
        }
    }
    ids.sort();
    ids.dedup();
    (ids, notes)
}

/// Confere a conversa antes de mandar: precisa de uma pergunta pendente.
pub fn ready(c: &Conversa) -> Result<()> {
    if !c.pending() {
        bail!(tr!(
            "a conversa não tem pergunta sem resposta",
            "the conversation has no unanswered question"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn conversa_gravada_e_lida() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Conversa::load(d.path()).unwrap(), None);
        let mut c = Conversa::new("openrouter/x", Some("oi"));
        assert!(c.pending());
        c.save(d.path()).unwrap();
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        assert!(text.contains("\"papel\": \"user\""), "{text}");
        c.mensagens.push(Mensagem {
            papel: Papel::Assistant,
            texto: "olá".into(),
            ms: Some(1200),
        });
        c.save(d.path()).unwrap();
        let l = Conversa::load(d.path()).unwrap().unwrap();
        assert_eq!(l, c);
        assert!(!l.pending());
        assert!(Conversa::new("a/b", Some("  ")).mensagens.is_empty());
    }

    #[test]
    fn pasta_da_aba() {
        let st = tempfile::tempdir().unwrap();
        let dir = new_dir(st.path(), &Conversa::new("a/b", Some("x"))).unwrap();
        assert!(dir.join(FILE).is_file());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        // Fora da pasta das abas, nada é apagado.
        let fora = tempfile::tempdir().unwrap();
        assert!(!remove_dir(st.path(), fora.path()));
        assert!(fora.path().is_dir());
        // Um link dentro da pasta também não.
        let link = root(st.path()).join("link");
        std::os::unix::fs::symlink(fora.path(), &link).unwrap();
        assert!(!remove_dir(st.path(), &link));
        assert!(fora.path().is_dir());
        assert!(remove_dir(st.path(), &dir));
        assert!(!dir.exists());
    }

    #[test]
    fn url_dividida() {
        let u = parse_url("https://openrouter.ai/api/v1/").unwrap();
        assert_eq!(
            u,
            Url {
                tls: true,
                host: "openrouter.ai".into(),
                port: 443,
                path: "/api/v1".into()
            }
        );
        let u = parse_url("http://host.sandmux.internal:11434/v1").unwrap();
        assert_eq!((u.tls, u.port, u.path.as_str()), (false, 11434, "/v1"));
        assert_eq!(parse_url("http://[::1]:8/x").unwrap().host, "::1");
        assert!(parse_url("ftp://x").is_err());
    }

    #[test]
    fn sse_em_pedacos() {
        let mut s = Sse::default();
        let a = s.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Ol");
        assert!(a.is_empty());
        let b = s.push(b"\xc3\xa1\"}}]}\r\n\r\n: comentario\ndata: {\"choices\":[{\"delta\":{}}]}\ndata: [DONE]\n");
        assert_eq!(b, vec![Chunk::Text("Olá".into()), Chunk::Done]);
        let e = s.push(b"data: {\"error\":{\"message\":\"sem saldo\",\"code\":402}}\n");
        assert_eq!(e, vec![Chunk::Error("sem saldo".into())]);
    }

    #[test]
    fn mensagem_de_erro() {
        assert_eq!(
            error_message(br#"{"error":{"message":"No auth credentials found","code":401}}"#),
            "No auth credentials found"
        );
        assert_eq!(error_message(br#"{"error":"ruim"}"#), "ruim");
        assert_eq!(error_message(b" texto "), "texto");
    }

    #[test]
    fn alvo_pelo_provedor() {
        let provs = vec![Provider {
            name: "local".into(),
            url: "http://x/v1".into(),
            env: "SANDMUX_TESTE_CHAVE_INEXISTENTE".into(),
        }];
        let t = target("local/llama3", &provs).unwrap();
        assert_eq!(t.model, "llama3");
        assert_eq!(t.key, None);
        assert!(target("outro/x", &provs).unwrap_err().contains("local"));
    }

    /// Servidor HTTP de uma conexão: devolve o pedido lido e responde `resp`.
    fn server(resp: &'static str) -> (u16, std::thread::JoinHandle<String>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = c.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&req).to_string();
                if let Some(i) = t.find("\r\n\r\n") {
                    let len = t
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            c.write_all(resp.as_bytes()).unwrap();
            String::from_utf8_lossy(&req).to_string()
        });
        (port, h)
    }

    #[test]
    fn pedido_com_streaming() {
        let (port, h) = server(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"4\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"2\"}}]}\n\ndata: [DONE]\n\n",
        );
        let t = Target {
            url: format!("http://127.0.0.1:{port}/api/v1"),
            key: Some("sandmux-fake-abc".into()),
            model: "deepseek/x".into(),
            system: Some("só comandos".into()),
        };
        let msgs = Conversa::new("p/m", Some("quanto é 40+2?")).mensagens;
        let mut parts = Vec::new();
        let r = complete_blocking(&t, &msgs, true, &AtomicBool::new(false), &mut |s| {
            parts.push(s.to_string())
        });
        assert_eq!(r.unwrap(), "42");
        assert_eq!(parts, vec!["4", "2"]);
        let req = h.join().unwrap();
        assert!(
            req.starts_with("POST /api/v1/chat/completions HTTP/1.1"),
            "{req}"
        );
        assert!(
            req.contains("authorization: Bearer sandmux-fake-abc"),
            "{req}"
        );
        assert!(req.contains("\"model\":\"deepseek/x\""), "{req}");
        assert!(req.contains("\"stream\":true"), "{req}");
        assert!(
            req.contains("\"messages\":[{\"role\":\"system\",\"content\":\"só comandos\"}"),
            "{req}"
        );
        assert!(req.contains("quanto é 40+2?"), "{req}");
    }

    #[test]
    fn pedido_sem_streaming_e_erro() {
        let (port, h) = server(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 48\r\n\r\n{\"choices\":[{\"message\":{\"content\":\"ok, 42\"}}]}  ",
        );
        let t = Target {
            url: format!("http://127.0.0.1:{port}/v1"),
            key: None,
            model: "m".into(),
            system: None,
        };
        let msgs = Conversa::new("p/m", Some("x")).mensagens;
        let r = complete_blocking(&t, &msgs, false, &AtomicBool::new(false), &mut |_| {});
        assert_eq!(r.unwrap(), "ok, 42");
        assert!(!h.join().unwrap().contains("authorization"));
        let (port, h) = server(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 50\r\n\r\n{\"error\":{\"message\":\"chave inválida\",\"code\":401}}",
        );
        let t = Target {
            url: format!("http://127.0.0.1:{port}/v1"),
            ..t
        };
        let e =
            complete_blocking(&t, &msgs, true, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.contains("401") && e.contains("chave inválida"), "{e}");
        h.join().unwrap();
    }

    #[test]
    fn conversa_antiga_sem_tempo_abre() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join(FILE),
            "{\"versao\":1,\"modelo\":\"a/b\",\"mensagens\":[{\"papel\":\"user\",\"texto\":\"oi\"},{\"papel\":\"assistant\",\"texto\":\"olá\"}]}",
        )
        .unwrap();
        let c = Conversa::load(d.path()).unwrap().unwrap();
        assert_eq!(c.mensagens[1].ms, None);
        // O tempo, quando há, vai para o arquivo; sem ele, a chave não aparece.
        let mut c = c;
        c.mensagens[1].ms = Some(1200);
        c.save(d.path()).unwrap();
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        assert!(text.contains("\"ms\": 1200"), "{text}");
        assert_eq!(text.matches("\"ms\"").count(), 1, "{text}");
    }
}
