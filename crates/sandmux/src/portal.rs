//! Portal do desktop: um barramento D-Bus mínimo do sandmux por aba, montado
//! em `/run/user/<uid>/bus` no sandbox. Ele responde ao `Hello` e às
//! consultas de nome, à aparência (`org.freedesktop.portal.Settings`), às
//! notificações (o portal e o `org.freedesktop.Notifications`) e ao abrir
//! link (`org.freedesktop.portal.OpenURI`), filtrado pela política de rede
//! da aba. Nenhuma mensagem do sandbox chega ao D-Bus de sessão do usuário:
//! o daemon faz, do lado de fora, só as chamadas que ele mesmo monta.
//!
//! Cada conexão é um servidor zbus ponto a ponto. O campo de destino não é
//! olhado: o zbus despacha pelo caminho e pela interface, e o resto recebe
//! erro.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use sandmux_policy::compose::PortalMounts;
use sandmux_spec::net::Verdict;
use sandmux_spec::{tr, trf};
use tokio::sync::Mutex;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Guid, interface};

/// O nome único que o barramento diz ser o dono dos serviços.
const OWNER: &str = ":1.0";
/// Os serviços que o barramento atende.
const NAMES: &[&str] = &[
    "org.freedesktop.DBus",
    "org.freedesktop.portal.Desktop",
    "org.freedesktop.Notifications",
];
/// As chaves da aparência.
const APPEARANCE: &str = "org.freedesktop.appearance";
const APPEARANCE_KEYS: &[&str] = &["color-scheme", "accent-color", "contrast"];
/// Limite de notificações por aba.
const NOTIFY_MAX: usize = 5;
const NOTIFY_WINDOW: Duration = Duration::from_secs(10);
/// Validade da aparência lida do host.
const SETTINGS_TTL: Duration = Duration::from_secs(5);

/// A aba dona do barramento.
#[derive(Debug, Clone)]
pub struct Tab {
    /// O nome que vai no título das notificações (`shell:app`).
    pub label: String,
    /// A marca da aba no registro de rede.
    pub tag: String,
    pub workdir: PathBuf,
    /// A política de rede inicial e o arquivo que o daemon regrava; `None`
    /// sem rede.
    pub policy: Option<sandmux_spec::net::NetPolicy>,
    pub policy_file: Option<PathBuf>,
    /// A pasta do registro de rede.
    pub log_dir: PathBuf,
}

/// O barramento de uma aba. Soltá-lo para o servidor e apaga o socket.
#[derive(Debug)]
pub struct PortalSocket {
    pub path: PathBuf,
    pub mounts: PortalMounts,
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Drop for PortalSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// O barramento de uma aba em `<runtime>/portal/<name>.sock`.
pub fn open(name: &str, tab: Tab) -> Result<PortalSocket> {
    let dir = crate::paths::runtime_dir().join("portal");
    if let Some(parent) = dir.parent() {
        crate::paths::ensure_runtime_dir(parent)?;
    }
    crate::paths::ensure_runtime_dir(&dir)?;
    let marker = dir.join("flatpak-info");
    if !marker.exists() {
        std::fs::write(&marker, b"")?;
    }
    let path = dir.join(format!("{name}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path)
        .map_err(|e| anyhow!("{}: {e}", path.display()))?;
    listener.set_nonblocking(true)?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let inner = Arc::new(Inner::new(tab));
    std::thread::Builder::new()
        .name("portal".into())
        .spawn(move || serve(listener, inner, stopped))?;
    Ok(PortalSocket {
        mounts: PortalMounts {
            bus: path.clone(),
            marker,
        },
        path,
        _stop: stop,
    })
}

fn serve(
    listener: std::os::unix::net::UnixListener,
    inner: Arc<Inner>,
    mut stopped: tokio::sync::oneshot::Receiver<()>,
) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    rt.block_on(async move {
        let Ok(l) = tokio::net::UnixListener::from_std(listener) else {
            return;
        };
        let next = Arc::new(AtomicU32::new(1));
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                c = l.accept() => {
                    let Ok((s, _)) = c else { continue };
                    let me = format!(":1.{}", next.fetch_add(1, Ordering::Relaxed));
                    let inner = inner.clone();
                    tokio::spawn(async move {
                        let _ = connection(s, me, inner).await;
                    });
                }
            }
        }
    });
}

/// Uma conexão do sandbox: um servidor ponto a ponto com os objetos do
/// barramento, que vive até o cliente fechar.
async fn connection(s: tokio::net::UnixStream, me: String, inner: Arc<Inner>) -> zbus::Result<()> {
    let c = zbus::connection::Builder::unix_stream(s)
        .server(Guid::generate())?
        .p2p()
        .serve_at("/org/freedesktop/DBus", Bus { me: me.clone() })?
        .serve_at(
            "/org/freedesktop/portal/desktop",
            Settings {
                inner: inner.clone(),
            },
        )?
        .serve_at(
            "/org/freedesktop/portal/desktop",
            PortalNotification {
                inner: inner.clone(),
            },
        )?
        .serve_at(
            "/org/freedesktop/portal/desktop",
            OpenUri {
                inner: inner.clone(),
                me: me.clone(),
                next: AtomicU32::new(1),
            },
        )?
        .serve_at("/org/freedesktop/Notifications", Notifications { inner })?
        .build()
        .await?;
    // A conexão termina quando o cliente fecha.
    let mut s = zbus::MessageStream::from(&c);
    use futures_util::StreamExt;
    while s.next().await.is_some() {}
    Ok(())
}

/// O estado da aba compartilhado pelas conexões.
struct Inner {
    tab: Tab,
    policy: Option<sandmux_net::policy::Policy>,
    host: Mutex<Option<zbus::Connection>>,
    sent: Mutex<VecDeque<Instant>>,
    settings: Mutex<HashMap<String, (Instant, Option<OwnedValue>)>>,
    /// Ids das notificações desta aba no desktop, e as do portal pelo id
    /// que o programa deu.
    ids: Mutex<Vec<u32>>,
    portal_ids: Mutex<HashMap<String, u32>>,
    fake: AtomicU32,
}

impl Inner {
    fn new(tab: Tab) -> Inner {
        let policy = tab
            .policy
            .clone()
            .map(|p| sandmux_net::policy::Policy::new(p, tab.policy_file.clone()));
        Inner {
            tab,
            policy,
            host: Mutex::new(None),
            sent: Mutex::new(VecDeque::new()),
            settings: Mutex::new(HashMap::new()),
            ids: Mutex::new(Vec::new()),
            portal_ids: Mutex::new(HashMap::new()),
            fake: AtomicU32::new(1_000_000),
        }
    }

    /// A conexão ao D-Bus de sessão do usuário (nos testes, ao barramento de
    /// `SANDMUX_NOTIFY_BUS`).
    async fn host(&self) -> zbus::Result<zbus::Connection> {
        let mut h = self.host.lock().await;
        if let Some(c) = h.as_ref() {
            return Ok(c.clone());
        }
        let b = match std::env::var("SANDMUX_NOTIFY_BUS") {
            Ok(a) => zbus::connection::Builder::address(a.as_str())?,
            Err(_) => zbus::connection::Builder::session()?,
        };
        let c = b.build().await?;
        *h = Some(c.clone());
        Ok(c)
    }

    /// Uma chave da aparência do portal do host, com cache.
    async fn appearance(&self, key: &str) -> Option<OwnedValue> {
        if let Some((at, v)) = self.settings.lock().await.get(key)
            && at.elapsed() < SETTINGS_TTL
        {
            return v.as_ref().and_then(dup);
        }
        let v = self.read_host(key).await;
        let v = v.or_else(|| (key == "color-scheme").then(|| OwnedValue::from(0u32)));
        self.settings
            .lock()
            .await
            .insert(key.to_string(), (Instant::now(), v.as_ref().and_then(dup)));
        v
    }

    async fn read_host(&self, key: &str) -> Option<OwnedValue> {
        let c = self.host().await.ok()?;
        let reply = c
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Settings"),
                "ReadOne",
                &(APPEARANCE, key),
            )
            .await
            .ok()?;
        reply.body().deserialize::<OwnedValue>().ok()
    }

    /// Repassa uma notificação ao desktop; `None` quando o limite descarta
    /// ou o desktop não responde.
    async fn notify(
        &self,
        replaces: u32,
        title: &str,
        body: &str,
        urgency: Option<u8>,
    ) -> Option<u32> {
        {
            let mut sent = self.sent.lock().await;
            while sent.front().is_some_and(|t| t.elapsed() > NOTIFY_WINDOW) {
                sent.pop_front();
            }
            if sent.len() >= NOTIFY_MAX {
                return None;
            }
            sent.push_back(Instant::now());
        }
        let replaces = if self.ids.lock().await.contains(&replaces) {
            replaces
        } else {
            0
        };
        let summary = cut(&format!("{}: {}", self.tab.label, title), 120);
        let body = escape(&cut(body, 1000));
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        if let Some(u) = urgency {
            hints.insert("urgency", Value::U8(u.min(1)));
        }
        let c = self.host().await.ok()?;
        let reply = c
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &(
                    "sandmux",
                    replaces,
                    "",
                    summary.as_str(),
                    body.as_str(),
                    Vec::<&str>::new(),
                    hints,
                    -1i32,
                ),
            )
            .await
            .ok()?;
        let id: u32 = reply.body().deserialize().ok()?;
        let mut ids = self.ids.lock().await;
        if !ids.contains(&id) {
            ids.push(id);
        }
        Some(id)
    }

    async fn close(&self, id: u32) {
        if !self.ids.lock().await.contains(&id) {
            return;
        }
        if let Ok(c) = self.host().await {
            let _ = c
                .call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "CloseNotification",
                    &(id,),
                )
                .await;
        }
    }

    /// Um id para uma notificação que não chegou ao desktop.
    fn fake_id(&self) -> u32 {
        self.fake.fetch_add(1, Ordering::Relaxed)
    }

    /// Decide e abre um link; devolve se abriu.
    async fn open_uri(&self, uri: &str) -> bool {
        let (host, path) = match parse_uri(uri) {
            Some(x) => x,
            None => {
                self.log(
                    uri_host_or(uri),
                    String::new(),
                    Some(
                        tr!(
                            "abrir link: só http e https",
                            "open link: only http and https"
                        )
                        .into(),
                    ),
                );
                return false;
            }
        };
        let verdict = match &self.policy {
            Some(p) => p.get().decide_name(&host),
            None => Verdict::Block(tr!("sem rede", "no network").into()),
        };
        if let Verdict::Block(why) = verdict {
            self.log(host, path, Some(why));
            return false;
        }
        let opened = self.open_on_host(uri).await;
        self.log(
            host,
            path,
            (!opened).then(|| {
                tr!(
                    "o desktop não abriu o link",
                    "the desktop did not open the link"
                )
                .into()
            }),
        );
        opened
    }

    async fn open_on_host(&self, uri: &str) -> bool {
        let empty: HashMap<&str, Value<'_>> = HashMap::new();
        if let Ok(c) = self.host().await
            && c.call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.OpenURI"),
                "OpenURI",
                &("", uri, empty),
            )
            .await
            .is_ok()
        {
            return true;
        }
        // Sem portal no host: o `xdg-open`, com a URL como argumento. Nos
        // testes (barramento próprio), nunca o navegador de verdade.
        if std::env::var_os("SANDMUX_NOTIFY_BUS").is_some() {
            return false;
        }
        tokio::process::Command::new("xdg-open")
            .arg(uri)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok_and(|s| s.success())
    }

    fn log(&self, host: String, path: String, blocked: Option<String>) {
        let mut e = sandmux_net::log::Entry::ssh_sign(
            &self.tab.tag,
            &self.tab.workdir,
            "",
            &host,
            blocked.as_deref(),
        );
        e.proto = OPEN_URI.into();
        e.path = (!path.is_empty()).then_some(path);
        sandmux_net::log::append(&self.tab.log_dir, &e);
    }
}

/// O protocolo da linha de um link aberto pelo portal no registro de rede.
pub const OPEN_URI: &str = "open-uri";

/// O host e o caminho (sem a query) de uma URL `http` ou `https`.
fn parse_uri(uri: &str) -> Option<(String, String)> {
    let lower = uri.get(..8).unwrap_or(uri).to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    let normalized = if lower.starts_with("https://") {
        format!("https://{}", &uri[8..])
    } else {
        format!("http://{}", &uri[7..])
    };
    let host = sandmux_policy::assistant::url_host(&normalized)?.to_ascii_lowercase();
    let rest = normalized.split_once("://")?.1;
    let path = rest
        .find('/')
        .map(|i| &rest[i..])
        .unwrap_or("/")
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .to_string();
    Some((host, path))
}

/// O texto de uma variante, quando é texto.
fn text(v: &OwnedValue) -> Option<String> {
    <&str>::try_from(v).ok().map(String::from)
}

/// Uma cópia da variante (sem descritores).
fn dup(v: &OwnedValue) -> Option<OwnedValue> {
    v.try_clone().ok()
}

fn uri_host_or(uri: &str) -> String {
    cut(uri.split(['?', '#']).next().unwrap_or(uri), 80)
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// O texto sem marcação: `<`, `>` e `&` escapados.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---- os objetos do barramento ----

/// O `org.freedesktop.DBus` mínimo.
struct Bus {
    me: String,
}

#[interface(name = "org.freedesktop.DBus")]
impl Bus {
    fn hello(&self) -> String {
        self.me.clone()
    }

    fn request_name(&self, _name: &str, _flags: u32) -> u32 {
        // Responde como dono, sem efeito: nenhum programa do sandbox oferece
        // serviço a outro.
        1
    }

    fn release_name(&self, _name: &str) -> u32 {
        1
    }

    fn add_match(&self, _rule: &str) {}

    fn remove_match(&self, _rule: &str) {}

    fn get_name_owner(&self, name: &str) -> zbus::fdo::Result<String> {
        if NAMES.contains(&name) {
            Ok(OWNER.into())
        } else if name == self.me {
            Ok(self.me.clone())
        } else {
            Err(zbus::fdo::Error::NameHasNoOwner(name.into()))
        }
    }

    fn name_has_owner(&self, name: &str) -> bool {
        NAMES.contains(&name) || name == self.me
    }

    fn list_names(&self) -> Vec<String> {
        let mut v: Vec<String> = NAMES.iter().map(|s| s.to_string()).collect();
        v.push(self.me.clone());
        v
    }

    fn list_activatable_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn start_service_by_name(&self, name: &str, _flags: u32) -> zbus::fdo::Result<u32> {
        if NAMES.contains(&name) {
            // Já rodando.
            Ok(2)
        } else {
            Err(zbus::fdo::Error::ServiceUnknown(name.into()))
        }
    }

    fn get_id(&self) -> String {
        "73616e646d75782d706f7274616c2d30".into()
    }
}

/// A aparência pelo portal.
struct Settings {
    inner: Arc<Inner>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.portal.Error")]
enum PortalError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotFound(String),
}

impl Settings {
    async fn one(&self, ns: &str, key: &str) -> Result<OwnedValue, PortalError> {
        if ns != APPEARANCE || !APPEARANCE_KEYS.contains(&key) {
            return Err(PortalError::NotFound(format!("{ns} {key}")));
        }
        self.inner
            .appearance(key)
            .await
            .ok_or_else(|| PortalError::NotFound(format!("{ns} {key}")))
    }
}

#[interface(name = "org.freedesktop.portal.Settings")]
impl Settings {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        2
    }

    async fn read_one(&self, namespace: &str, key: &str) -> Result<OwnedValue, PortalError> {
        self.one(namespace, key).await
    }

    /// A forma antiga: o valor dentro de outra variante.
    async fn read(&self, namespace: &str, key: &str) -> Result<OwnedValue, PortalError> {
        let v = self.one(namespace, key).await?;
        let inner: Value<'_> = Value::from(v);
        OwnedValue::try_from(Value::Value(Box::new(inner)))
            .map_err(|e| PortalError::ZBus(zbus::Error::Variant(e)))
    }

    async fn read_all(
        &self,
        namespaces: Vec<String>,
    ) -> HashMap<String, HashMap<String, OwnedValue>> {
        let wants = namespaces.is_empty()
            || namespaces.iter().any(|n| {
                n == APPEARANCE
                    || n.strip_suffix('*')
                        .is_some_and(|p| APPEARANCE.starts_with(p))
            });
        let mut out = HashMap::new();
        if wants {
            let mut keys = HashMap::new();
            for k in APPEARANCE_KEYS {
                if let Some(v) = self.inner.appearance(k).await {
                    keys.insert(k.to_string(), v);
                }
            }
            out.insert(APPEARANCE.to_string(), keys);
        }
        out
    }
}

/// As notificações pelo portal.
struct PortalNotification {
    inner: Arc<Inner>,
}

#[interface(name = "org.freedesktop.portal.Notification")]
impl PortalNotification {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    async fn add_notification(&self, id: &str, notification: HashMap<String, OwnedValue>) {
        let field = |k: &str| notification.get(k).and_then(text).unwrap_or_default();
        let urgent = notification
            .get("priority")
            .and_then(text)
            .is_some_and(|p| p == "high" || p == "urgent");
        let replaces = self
            .inner
            .portal_ids
            .lock()
            .await
            .get(id)
            .copied()
            .unwrap_or(0);
        if let Some(n) = self
            .inner
            .notify(
                replaces,
                &field("title"),
                &field("body"),
                urgent.then_some(1),
            )
            .await
        {
            self.inner.portal_ids.lock().await.insert(id.to_string(), n);
        }
    }

    async fn remove_notification(&self, id: &str) {
        let n = self.inner.portal_ids.lock().await.remove(id);
        if let Some(n) = n {
            self.inner.close(n).await;
        }
    }
}

/// O `org.freedesktop.Notifications` direto (o do `notify-send`).
struct Notifications {
    inner: Arc<Inner>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let urgency = hints.get("urgency").and_then(|v| u8::try_from(v).ok());
        match self.inner.notify(replaces_id, summary, body, urgency).await {
            Some(id) => id,
            None => self.inner.fake_id(),
        }
    }

    async fn close_notification(&self, id: u32) {
        self.inner.close(id).await;
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "sandmux".into(),
            "sandmux".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }
}

/// O abrir link pelo portal.
struct OpenUri {
    inner: Arc<Inner>,
    me: String,
    next: AtomicU32,
}

#[interface(name = "org.freedesktop.portal.OpenURI")]
impl OpenUri {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }

    #[zbus(name = "OpenURI")]
    async fn open_uri(
        &self,
        #[zbus(connection)] conn: &zbus::Connection,
        _parent_window: &str,
        uri: &str,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = options
            .get("handle_token")
            .and_then(text)
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or_else(|| format!("sandmux{}", self.next.fetch_add(1, Ordering::Relaxed)));
        let sender = self.me.trim_start_matches(':').replace('.', "_");
        let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let handle = OwnedObjectPath::try_from(path.clone())
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let opened = self.inner.open_uri(uri).await;
        // O `Response` sai depois da resposta, para quem assina o sinal só
        // depois de receber o caminho.
        let conn = conn.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let results: HashMap<&str, Value<'_>> = HashMap::new();
            let code: u32 = if opened { 0 } else { 2 };
            if let Ok(m) = zbus::message::Message::signal(
                path.as_str(),
                "org.freedesktop.portal.Request",
                "Response",
            )
            .and_then(|b| b.sender(OWNER))
            .and_then(|b| b.build(&(code, results)))
            {
                let _ = conn.send(&m).await;
            }
        });
        Ok(handle)
    }

    #[zbus(name = "OpenFile")]
    fn open_file(
        &self,
        _parent_window: &str,
        _fd: zbus::zvariant::OwnedFd,
        _options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        Err(zbus::fdo::Error::AccessDenied(
            tr!(
                "o portal do sandmux só abre links http e https",
                "the sandmux portal only opens http and https links"
            )
            .into(),
        ))
    }

    #[zbus(name = "OpenDirectory")]
    fn open_directory(
        &self,
        _parent_window: &str,
        _fd: zbus::zvariant::OwnedFd,
        _options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        Err(zbus::fdo::Error::AccessDenied(
            tr!(
                "o portal do sandmux só abre links http e https",
                "the sandmux portal only opens http and https links"
            )
            .into(),
        ))
    }
}

/// Aviso de um portal que não subiu: a aba segue sem ele.
pub fn warning(e: &anyhow::Error) -> String {
    trf!(
        "a aba segue sem o portal do desktop: {e:#}",
        "the tab runs without the desktop portal: {e:#}"
    )
}

/// O nome da aba nas notificações: `harness:pasta`.
pub fn label(harness: &str, workdir: &Path) -> String {
    format!(
        "{harness}:{}",
        workdir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".into())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_http_e_https() {
        assert_eq!(
            parse_uri("https://openrouter.ai/keys?x=1"),
            Some(("openrouter.ai".into(), "/keys".into()))
        );
        assert_eq!(
            parse_uri("HTTP://Exemplo.COM"),
            Some(("exemplo.com".into(), "/".into()))
        );
        assert_eq!(parse_uri("file:///etc/passwd"), None);
        assert_eq!(parse_uri("javascript:alert(1)"), None);
        assert_eq!(parse_uri("https://user:pw@exemplo.com/"), None);
        assert_eq!(parse_uri("https://"), None);
    }

    #[test]
    fn cortes_e_marcacao() {
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("abc", 4), "abc");
        assert_eq!(escape("<b>a & b</b>"), "&lt;b&gt;a &amp; b&lt;/b&gt;");
    }
}
