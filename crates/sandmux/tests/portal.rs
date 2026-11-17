//! Portal do desktop. Cada teste sobe um barramento de sessão próprio com um
//! servidor de notificações e um portal falsos: o desktop de quem roda os
//! testes nunca é tocado, e nenhum navegador de verdade abre. Sem
//! `dbus-daemon`, `gdbus` ou `notify-send`, o teste é pulado.

mod common;
use common::cli::*;
use common::pty::Term;

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

/// O que o desktop falso recebeu: (título, corpo) e os links abertos.
#[derive(Default)]
struct Seen {
    notes: Vec<(String, String)>,
    uris: Vec<String>,
}
type Calls = Arc<Mutex<Seen>>;

struct Bus {
    addr: String,
    daemon: Child,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

fn has(bin: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {bin}")])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn bus(e: &Env) -> Option<Bus> {
    for b in ["dbus-daemon", "gdbus", "notify-send", "xdg-open"] {
        if !has(b) {
            eprintln!("pulado: sem {b}");
            return None;
        }
    }
    let sock = e.runtime.join("hostbus");
    let addr = format!("unix:path={}", sock.display());
    let daemon = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile"])
        .arg(format!("--address={addr}"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &e.home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let b = Bus { addr, daemon };
    let end = Instant::now() + Duration::from_secs(5);
    while !sock.exists() {
        if Instant::now() > end {
            eprintln!("pulado: o dbus-daemon do teste não subiu");
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(b)
}

struct FakeNotes {
    calls: Calls,
    next: u32,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FakeNotes {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        _app: String,
        _replaces_id: u32,
        _icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _timeout: i32,
    ) -> u32 {
        self.calls.lock().unwrap().notes.push((summary, body));
        self.next += 1;
        self.next
    }
}

struct FakeSettings;

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl FakeSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue> {
        if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
            Ok(OwnedValue::from(1u32))
        } else {
            Err(zbus::fdo::Error::Failed("não encontrado".into()))
        }
    }
}

struct FakeOpen {
    calls: Calls,
}

#[zbus::interface(name = "org.freedesktop.portal.OpenURI")]
impl FakeOpen {
    #[zbus(name = "OpenURI")]
    fn open_uri(
        &self,
        _parent: &str,
        uri: &str,
        _options: HashMap<String, OwnedValue>,
    ) -> OwnedObjectPath {
        self.calls.lock().unwrap().uris.push(uri.to_string());
        OwnedObjectPath::try_from("/org/freedesktop/portal/desktop/request/x/y").unwrap()
    }
}

/// O desktop falso: notificações e portal no barramento do teste.
fn serve(addr: &str) -> Calls {
    let calls: Calls = Arc::default();
    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_string();
    let c = calls.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let _a = zbus::connection::Builder::address(addr.as_str())
                .unwrap()
                .name("org.freedesktop.Notifications")
                .unwrap()
                .serve_at(
                    "/org/freedesktop/Notifications",
                    FakeNotes {
                        calls: c.clone(),
                        next: 0,
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap();
            let _b = zbus::connection::Builder::address(addr.as_str())
                .unwrap()
                .name("org.freedesktop.portal.Desktop")
                .unwrap()
                .serve_at("/org/freedesktop/portal/desktop", FakeSettings)
                .unwrap()
                .serve_at("/org/freedesktop/portal/desktop", FakeOpen { calls: c })
                .unwrap()
                .build()
                .await
                .unwrap();
            tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("o desktop falso não subiu");
    calls
}

fn term(e: &Env, b: &Bus, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash")
        .env("SANDMUX_NOTIFY_BUS", &b.addr);
    Term::spawn_answering(cmd, 120, 30)
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// As linhas do registro de rede com o protocolo `open-uri`.
fn opens(e: &Env) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for f in std::fs::read_dir(e.state.join("sandmux/net"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if f.path().extension().is_some_and(|x| x == "jsonl") {
            let t = std::fs::read_to_string(f.path()).unwrap_or_default();
            out.extend(
                t.lines()
                    .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                    .filter(|l| l["proto"] == "open-uri"),
            );
        }
    }
    out
}

/// As notificações que vieram do sandbox (título `shell:app: …`).
fn from_tab(c: &Calls) -> Vec<(String, String)> {
    c.lock()
        .unwrap()
        .notes
        .iter()
        .filter(|n| n.0.starts_with("shell:app: "))
        .cloned()
        .collect()
}

#[test]
fn portal_com_aparencia_notificacoes_e_link() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    let calls = serve(&b.addr);
    let mut t = term(&e, &b, &["run", "shell", "--network", "inferencia"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    // O barramento do sandmux, no caminho padrão, com o flatpak-info.
    t.write(b"echo \"bus=$DBUS_SESSION_BUS_ADDRESS gio=$GIO_USE_PORTALS\"; ls $XDG_RUNTIME_DIR | tr '\\n' ' '; echo fim-1\r");
    t.wait_text("fim-1");
    let uid = unsafe { libc::getuid() };
    t.wait_text(&format!("bus=unix:path=/run/user/{uid}/bus gio=1"));
    t.wait_text("bus flatpak-info");
    // A aparência vem do portal do host.
    t.write(b"gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop --method org.freedesktop.portal.Settings.ReadOne org.freedesktop.appearance color-scheme; echo fim-2\r");
    t.wait_text("(<uint32 1>,)");
    // Um serviço que o sandmux não atende recebe erro.
    t.write(b"gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.Secret.Service.OpenSession plain '<\"\">' >/dev/null 2>&1; echo segredo-rc=$?\r");
    t.wait_text("segredo-rc=1");
    // notify-send chega ao desktop com o nome da aba.
    t.write(b"notify-send Pronto 'build <b>ok</b>'; echo fim-3\r");
    t.wait_text("fim-3");
    wait_until("a notificação do notify-send", || {
        !from_tab(&calls).is_empty()
    });
    assert_eq!(
        from_tab(&calls)[0],
        (
            "shell:app: Pronto".to_string(),
            "build &lt;b&gt;ok&lt;/b&gt;".to_string()
        )
    );
    // O limite: 5 a cada 10 segundos.
    t.write(b"for i in $(seq 20); do notify-send n$i; done; echo fim-4\r");
    t.wait_text("fim-4");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(from_tab(&calls).len(), 5, "{:?}", from_tab(&calls));
    // xdg-open de um domínio permitido abre pelo portal do host.
    t.write(b"xdg-open https://openrouter.ai/keys; echo fim-5\r");
    t.wait_text("fim-5");
    wait_until("o link aberto", || {
        calls.lock().unwrap().uris == ["https://openrouter.ai/keys"]
    });
    // Fora da política: nada abre, e o registro tem a recusa.
    t.write(b"gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop --method org.freedesktop.portal.OpenURI.OpenURI '' https://exemplo.com/x {}; echo fim-6\r");
    t.wait_text("fim-6");
    wait_until("as duas linhas no registro", || opens(&e).len() >= 2);
    let log = opens(&e);
    assert!(
        log.iter()
            .any(|l| l["domain"] == "openrouter.ai" && l["blocked"].is_null()),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|l| l["domain"] == "exemplo.com" && l["blocked"].is_string()),
        "{log:?}"
    );
    assert_eq!(calls.lock().unwrap().uris.len(), 1);
    // Um arquivo local não abre.
    t.write(b"gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop --method org.freedesktop.portal.OpenURI.OpenFile '' 0 {} >/dev/null 2>&1 0</etc/hostname; echo arquivo-rc=$?\r");
    t.wait_text("arquivo-rc=1");
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn sem_portal_nada_de_dbus() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    let _calls = serve(&b.addr);
    let mut t = term(&e, &b, &["run", "shell", "--no-portal"]);
    t.write(
        b"echo \"x${DBUS_SESSION_BUS_ADDRESS}x\"; ls /run/user 2>&1 | head -1; echo fim-$((1+1))\r",
    );
    t.wait_text("fim-2");
    t.wait_text("xx");
    t.with_screen(|s| assert!(!s.contents().contains("/bus"), "{}", s.contents()));
    t.write(b"exit\r");
    t.wait_exit();
    // A chave na configuração também desliga.
    e.write_config("[global]\nportal = false\n");
    let mut t = term(&e, &b, &["run", "shell"]);
    t.write(b"echo \"y${DBUS_SESSION_BUS_ADDRESS}y\"; echo fim-$((2+2))\r");
    t.wait_text("fim-4");
    t.wait_text("yy");
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn sem_terminal_o_portal_explicito_e_recusado() {
    let e = Env::new();
    // Sem terminal, o padrão segue sem portal.
    let o = e.shell(&[], "echo \"z${DBUS_SESSION_BUS_ADDRESS}z\"");
    assert!(stdout(&o).contains("zz"), "{}", stderr(&o));
    let o = e.shell(&["--portal"], "true");
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("o portal só funciona nas abas do daemon"),
        "{}",
        stderr(&o)
    );
}
