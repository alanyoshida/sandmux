//! Daemon no boot: a espera do login antes de relançar as abas e o
//! `sandmux daemon boot`. O logind e o `loginctl` são falsos: o sistema de
//! quem roda os testes nunca muda.

mod common;
use common::cli::*;
use common::pty::Term;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

struct Boot {
    logind: PathBuf,
    linger: PathBuf,
    loginctl: PathBuf,
    calls: PathBuf,
}

fn boot(e: &Env) -> Boot {
    let root = e.dir.path().join("boot");
    let linger = root.join("linger");
    std::fs::create_dir_all(&linger).unwrap();
    let logind = root.join("logind-user");
    std::fs::write(&logind, "STATE=active\n").unwrap();
    let calls = root.join("chamadas");
    let loginctl = root.join("loginctl");
    std::fs::write(
        &loginctl,
        format!(
            "#!/bin/sh\necho \"$@\" >> {calls}\nif [ -n \"$FALHA\" ]; then echo 'Access denied' >&2; exit 1; fi\ncase \"$1\" in\n  enable-linger) touch {linger}/$(id -un) ;;\n  disable-linger) rm -f {linger}/$(id -un) ;;\nesac\n",
            calls = calls.display(),
            linger = linger.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&loginctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    Boot {
        logind,
        linger,
        loginctl,
        calls,
    }
}

fn cmd(e: &Env, b: &Boot, args: &[&str]) -> Command {
    let mut c = e.cmd(&e.work, args);
    c.env("SANDMUX_LOGIND_USER", &b.logind)
        .env("SANDMUX_LINGER_DIR", &b.linger)
        .env("SANDMUX_LOGINCTL", &b.loginctl)
        .env("SHELL", "/bin/bash");
    c
}

fn run(e: &Env, b: &Boot, args: &[&str]) -> Output {
    cmd(e, b, args).output().unwrap()
}

fn ls(e: &Env, b: &Boot) -> String {
    let o = run(e, b, &["ls"]);
    assert!(
        o.status.success(),
        "{}\n--- registro do daemon:\n{}",
        stderr(&o),
        std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default()
    );
    stdout(&o)
}

fn sessao(e: &Env, b: &Boot) {
    let mut t = Term::spawn_answering(cmd(e, b, &["run", "shell"]), 100, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn sem_login_as_abas_esperam_e_voltam_no_login() {
    let e = Env::new();
    let b = boot(&e);
    sessao(&e, &b);
    sessao(&e, &b);
    assert_eq!(ls(&e, &b).matches("viva").count(), 2);

    // O daemon sobe de novo sem ninguém logado (o boot).
    std::fs::write(&b.logind, "STATE=lingering\n").unwrap();
    assert!(run(&e, &b, &["daemon", "stop"]).status.success());
    let l = ls(&e, &b);
    assert_eq!(l.matches("aguardando o login").count(), 2, "{l}");
    assert!(!l.contains("viva"), "{l}");

    // Reinício antes do login: continuam esperando.
    assert!(run(&e, &b, &["daemon", "stop"]).status.success());
    let l = ls(&e, &b);
    assert_eq!(l.matches("aguardando o login").count(), 2, "{l}");

    // O login: voltam vivas, sem comando.
    std::fs::write(&b.logind, "STATE=active\nSESSIONS=2\n").unwrap();
    wait_until("as abas vivas depois do login", || {
        ls(&e, &b).matches("viva").count() == 2
    });
    let log = std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default();
    assert!(log.contains("subiu sem login"), "{log}");
    assert!(log.contains("login do usuário; relançando 2"), "{log}");
}

#[test]
fn daemon_boot_liga_desliga_e_mostra() {
    let e = Env::new();
    let b = boot(&e);
    let o = run(&e, &b, &["daemon", "boot"]);
    assert!(stdout(&o).contains("Início no boot: não"), "{}", stdout(&o));

    let o = run(&e, &b, &["daemon", "boot", "on"]);
    assert!(o.status.success(), "{}", stderr(&o));
    // Nos testes o daemon não usa o systemd: falta a unidade habilitada.
    assert!(
        stdout(&o).contains("o linger está ligado, mas a unidade"),
        "{}",
        stdout(&o)
    );
    let wants = e.config.join("systemd/user/default.target.wants");
    std::fs::create_dir_all(&wants).unwrap();
    std::fs::write(wants.join("sandmuxd.service"), "").unwrap();
    let o = run(&e, &b, &["daemon", "boot"]);
    assert!(stdout(&o).contains("Início no boot: sim"), "{}", stdout(&o));
    ls(&e, &b);
    let o = run(&e, &b, &["daemon", "status"]);
    assert!(stdout(&o).contains("início no boot: sim"), "{}", stdout(&o));

    let o = run(&e, &b, &["daemon", "boot", "off"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("Outros serviços systemd"),
        "{}",
        stderr(&o)
    );
    assert!(stdout(&o).contains("Início no boot: não"), "{}", stdout(&o));
    let calls = std::fs::read_to_string(&b.calls).unwrap();
    assert_eq!(calls, "enable-linger\ndisable-linger\n");

    // O loginctl recusa: erro com a mensagem dele.
    let o = cmd(&e, &b, &["daemon", "boot", "on"])
        .env("FALHA", "1")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("loginctl enable-linger falhou: Access denied"),
        "{}",
        stderr(&o)
    );
}
