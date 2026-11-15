//! Notificações do desktop. Cada teste sobe um barramento de sessão próprio
//! e um servidor de notificações falso nele: o desktop de quem roda os testes
//! nunca é tocado. Sem `dbus-daemon`, o teste é pulado.

mod common;
use common::cli::*;
use common::pty::Term;

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// (replaces_id, título, corpo, id devolvido).
type Calls = Arc<Mutex<Vec<(u32, String, String, u32)>>>;

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

fn bus(e: &Env) -> Option<Bus> {
    let ok = Command::new("sh")
        .args(["-c", "command -v dbus-daemon"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("pulado: sem dbus-daemon");
        return None;
    }
    let sock = e.runtime.join("bus");
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

struct Fake {
    calls: Calls,
    next: u32,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Fake {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        _app: String,
        replaces_id: u32,
        _icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next += 1;
            self.next
        };
        self.calls
            .lock()
            .unwrap()
            .push((replaces_id, summary, body, id));
        id
    }
}

/// Servidor de notificações falso no barramento do teste.
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
            let _conn = zbus::connection::Builder::address(addr.as_str())
                .unwrap()
                .name("org.freedesktop.Notifications")
                .unwrap()
                .serve_at("/org/freedesktop/Notifications", Fake { calls: c, next: 0 })
                .unwrap()
                .build()
                .await
                .unwrap();
            tx.send(()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("o servidor falso não subiu");
    calls
}

fn term(e: &Env, b: &Bus, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash")
        .env("SANDMUX_NOTIFY_BUS", &b.addr);
    Term::spawn_answering(cmd, 100, 30)
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn bodies(c: &Calls) -> Vec<String> {
    c.lock().unwrap().iter().map(|x| x.2.clone()).collect()
}

#[test]
fn bloqueio_agrupado_e_aba_encerrada() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    let calls = serve(&b.addr);
    let mut t = term(
        &e,
        &b,
        &[
            "run",
            "shell",
            "--deny",
            "proibido.test",
            "--deny",
            "outro.test",
        ],
    );
    t.write(b"getent hosts proibido.test; echo feito-$((1+1))\r");
    t.wait_text("feito-2");
    wait_until("a notificação do bloqueio", || {
        !calls.lock().unwrap().is_empty()
    });
    let first = calls.lock().unwrap()[0].clone();
    assert_eq!(first.0, 0);
    assert!(first.1.contains("shell:app"), "{first:?}");
    assert!(first.2.starts_with("Bloqueado: proibido.test"), "{first:?}");

    t.write(b"getent hosts outro.test; echo feito-$((2+2))\r");
    t.wait_text("feito-4");
    wait_until("a notificação agrupada", || {
        bodies(&calls).iter().any(|b| b.contains("outro.test"))
    });
    let last = calls
        .lock()
        .unwrap()
        .iter()
        .rfind(|c| c.2.contains("outro.test"))
        .cloned()
        .unwrap();
    assert_eq!(last.0, first.3, "não substituiu a anterior: {last:?}");
    assert!(
        last.2.contains("bloqueios") && last.2.contains("proibido.test"),
        "{last:?}"
    );

    // O harness termina sozinho: avisa com o código.
    t.write(b"exit 3\r");
    assert_eq!(t.wait_exit(), 3);
    wait_until("a notificação da aba encerrada", || {
        bodies(&calls).iter().any(|b| b.contains("código 3"))
    });
}

#[test]
fn kill_e_notify_false_nao_notificam() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    let calls = serve(&b.addr);
    let mut t = term(&e, &b, &["run", "shell"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    t.wait_exit();
    assert!(e.run(&e.work, &["kill", "1"]).status.success());

    e.write_config("[global]\nnotify = false\n");
    let mut t = term(&e, &b, &["run", "shell", "--deny", "proibido.test"]);
    t.write(b"getent hosts proibido.test; echo feito-$((1+1))\r");
    t.wait_text("feito-2");
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        calls.lock().unwrap().is_empty(),
        "{:?}",
        calls.lock().unwrap()
    );
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn tui_em_foco_na_aba_suprime() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    let calls = serve(&b.addr);
    let mut s = term(&e, &b, &["run", "shell", "--deny", "proibido.test"]);
    s.write(b"echo pronto-$((1+1))\r");
    s.wait_text("pronto-2");
    s.write(b"\x02d");
    s.wait_exit();

    // A TUI abre em foco, mostrando a aba 1.
    let mut t = term(&e, &b, &[]);
    t.wait_text("pronto-2");
    t.write(b"getent hosts proibido.test; echo feito-$((3+3))\r");
    t.wait_text("feito-6");
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        calls.lock().unwrap().is_empty(),
        "{:?}",
        calls.lock().unwrap()
    );

    // O terminal perde o foco: a aba passa a ser notificada.
    t.write(b"\x1b[O");
    std::thread::sleep(Duration::from_millis(500));
    t.write(b"getent hosts proibido.test; echo feito-$((4+4))\r");
    t.wait_text("feito-8");
    wait_until("a notificação sem foco", || {
        !calls.lock().unwrap().is_empty()
    });
    assert!(
        bodies(&calls)[0].contains("proibido.test"),
        "{:?}",
        bodies(&calls)
    );
}
