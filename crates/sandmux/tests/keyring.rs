//! Senha do cofre no keyring do sistema. Cada teste sobe um barramento de
//! sessão e um gnome-keyring próprios, numa pasta temporária: o keyring de
//! quem roda os testes nunca é tocado. Sem `dbus-daemon` ou
//! `gnome-keyring-daemon`, o teste é pulado.

mod common;
use common::cli::*;

use std::io::Write;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Barramento e keyring do teste; mortos no fim.
struct Bus {
    addr: String,
    procs: Vec<Child>,
}

impl Drop for Bus {
    fn drop(&mut self) {
        for c in self.procs.iter_mut().rev() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn has(cmd: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {cmd}")])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn bus(e: &Env) -> Option<Bus> {
    if !has("dbus-daemon") || !has("dbus-send") || !has("gnome-keyring-daemon") {
        eprintln!("pulado: sem dbus-daemon ou gnome-keyring-daemon");
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
    let mut b = Bus {
        addr: addr.clone(),
        procs: vec![daemon],
    };
    let end = Instant::now() + Duration::from_secs(5);
    while !sock.exists() {
        if Instant::now() > end {
            eprintln!("pulado: o dbus-daemon do teste não subiu");
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut kr = Command::new("gnome-keyring-daemon")
        .args(["--foreground", "--unlock", "--components=secrets"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &e.home)
        .env("XDG_RUNTIME_DIR", &e.runtime)
        .env("DBUS_SESSION_BUS_ADDRESS", &addr)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    kr.stdin.take()?.write_all(b"senha-do-keyring").ok()?;
    b.procs.push(kr);
    // Espera o serviço aparecer no barramento.
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let o = Command::new("dbus-send")
            .args([
                "--session",
                "--print-reply",
                "--dest=org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListNames",
            ])
            .env("DBUS_SESSION_BUS_ADDRESS", &addr)
            .output();
        if o.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("org.freedesktop.secrets")) {
            break;
        }
        if Instant::now() > end {
            eprintln!("pulado: o gnome-keyring do teste não apareceu no barramento");
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Some(b)
}

fn run(e: &Env, b: &Bus, args: &[&str], input: &str) -> Output {
    let mut c = e
        .cmd(&e.work, args)
        .env("DBUS_SESSION_BUS_ADDRESS", &b.addr)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

fn status(e: &Env, b: &Bus) -> String {
    stdout(&run(e, b, &["vault", "status"], ""))
}

fn log(e: &Env) -> String {
    std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default()
}

fn restart(e: &Env, b: &Bus) {
    let o = run(e, b, &["daemon", "stop"], "");
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn ligar_abrir_sozinho_trocar_a_senha_e_desligar() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    assert!(run(&e, &b, &["vault", "init"], "s\n").status.success());
    let st = stdout(&run(&e, &b, &["vault", "keyring"], ""));
    assert!(st.contains("Keyring: desligado"), "{st}");

    // Senha errada: recusa e não grava nada.
    let o = run(&e, &b, &["vault", "keyring", "on"], "errada\n");
    assert!(!o.status.success());
    assert!(!e.data_dir().join("vault.keyring").exists());
    assert!(stdout(&run(&e, &b, &["vault", "keyring"], "")).contains("desligado"));

    let o = run(&e, &b, &["vault", "keyring", "on"], "s\n");
    assert!(o.status.success(), "{}", stderr(&o));
    let st = stdout(&run(&e, &b, &["vault", "keyring"], ""));
    assert!(
        st.contains("Keyring: ligado; o daemon abre o cofre sozinho"),
        "{st}"
    );

    // O daemon sobe de novo e abre o cofre sem a senha.
    restart(&e, &b);
    let st = status(&e, &b);
    assert!(st.contains("Cofre: desbloqueado"), "{st}");
    assert!(
        log(&e).contains("cofre aberto com a senha do keyring"),
        "{}",
        log(&e)
    );
    assert!(!log(&e).contains("senha-do-keyring"));

    // A troca de senha regrava o keyring.
    let o = run(&e, &b, &["vault", "passwd"], "s\nnova\n");
    assert!(o.status.success(), "{}", stderr(&o));
    restart(&e, &b);
    let st = status(&e, &b);
    assert!(st.contains("Cofre: desbloqueado"), "{st}");

    // Desligado: o cofre volta a subir trancado, sem consulta ao keyring.
    let o = run(&e, &b, &["vault", "keyring", "off"], "");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!e.data_dir().join("vault.keyring").exists());
    restart(&e, &b);
    let before = log(&e).len();
    let st = status(&e, &b);
    assert!(st.contains("Cofre: trancado"), "{st}");
    std::thread::sleep(Duration::from_millis(300));
    let after = log(&e);
    assert!(!after[before..].contains("keyring"), "{}", &after[before..]);
}

#[test]
fn senha_do_keyring_que_nao_abre_mais_o_cofre() {
    let e = Env::new();
    let Some(b) = bus(&e) else { return };
    assert!(run(&e, &b, &["vault", "init"], "s\n").status.success());
    assert!(
        run(&e, &b, &["vault", "keyring", "on"], "s\n")
            .status
            .success()
    );
    // Troca a senha com o keyring desligado: o item fica com a antiga.
    std::fs::remove_file(e.data_dir().join("vault.keyring")).unwrap();
    assert!(
        run(&e, &b, &["vault", "passwd"], "s\nnova\n")
            .status
            .success()
    );
    std::fs::write(e.data_dir().join("vault.keyring"), "").unwrap();
    restart(&e, &b);
    let st = status(&e, &b);
    assert!(st.contains("Cofre: trancado"), "{st}");
    assert!(
        log(&e).contains("a senha guardada não abre o cofre"),
        "{}",
        log(&e)
    );
}
