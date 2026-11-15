//! Vigia de chaves reais do daemon: as chaves gravadas pelo harness no HOME
//! sintético vão sozinhas para o cofre, com os falsos no lugar.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

fn vault(e: &Env, args: &[&str], input: &str) -> Output {
    let mut a = vec!["vault"];
    a.extend_from_slice(args);
    let mut c = e
        .cmd(&e.work, &a)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

fn ls(e: &Env) -> String {
    stdout(&e.run(&e.work, &["ls"]))
}

fn daemon_log(e: &Env) -> String {
    std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default()
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// opencode falso e uma aba viva dele, desanexada.
fn live_opencode(e: &Env) {
    let oc = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(oc.parent().unwrap()).unwrap();
    std::fs::write(&oc, "#!/bin/sh\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&oc, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let mut cmd = e.cmd(
        &e.work,
        &[
            "run",
            "opencode",
            "--no-import",
            "--",
            "sh",
            "-c",
            "echo pronta-$((2+2)); sleep 300",
        ],
    );
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 100, 30);
    t.wait_text("pronta-4");
    t.write(b"\x02d");
    t.wait_exit();
}

fn auth(e: &Env) -> PathBuf {
    e.data_dir()
        .join("homes/opencode/.local/share/opencode/auth.json")
}

fn write_auth(e: &Env, provider: &str, key: &str) {
    let p = auth(e);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        format!(
            "{{\n  \"{provider}\": {{\n    \"type\": \"api\",\n    \"key\": \"{key}\"\n  }}\n}}\n"
        ),
    )
    .unwrap();
}

fn auth_text(e: &Env) -> String {
    std::fs::read_to_string(auth(e)).unwrap_or_default()
}

#[test]
fn chave_gravada_vai_para_o_cofre_com_o_cofre_aberto() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    live_opencode(&e);
    write_auth(&e, "openrouter", "sk-or-v1-REALOPENROUTER99");
    wait_until("o falso no auth.json", || {
        let a = auth_text(&e);
        a.contains("sandmux-fake-") && !a.contains("REALOPENROUTER")
    });
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("opencode:.local/share/opencode/auth.json:openrouter.key  (openrouter.ai)"),
        "{list}"
    );
    wait_until("o aviso na lista de sessões", || {
        ls(&e).contains("1 chave(s) movida(s) ao cofre")
    });
    let log = daemon_log(&e);
    assert!(log.contains("levada(s) ao cofre (key)"), "{log}");
    assert!(!log.contains("REALOPENROUTER"), "{log}");
    let _ = e.run(&e.work, &["kill", "1"]);
}

#[test]
fn com_o_cofre_trancado_espera_e_move_ao_abrir() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    assert!(vault(&e, &["lock"], "").status.success());
    live_opencode(&e);
    write_auth(&e, "openrouter", "sk-or-v1-REALTRANCADO42");
    wait_until("a chave esperando o cofre", || {
        ls(&e).contains("1 chave(s) esperando o cofre")
    });
    assert!(auth_text(&e).contains("REALTRANCADO"));
    assert!(vault(&e, &["unlock"], "s\n").status.success());
    wait_until("o falso no auth.json depois de abrir o cofre", || {
        let a = auth_text(&e);
        a.contains("sandmux-fake-") && !a.contains("REALTRANCADO")
    });
    wait_until("o aviso de reinício", || {
        let l = ls(&e);
        l.contains("1 chave(s) movida(s) ao cofre") && !l.contains("esperando o cofre")
    });
    let _ = e.run(&e.work, &["kill", "1"]);
}

#[test]
fn chave_sem_dominio_espera_o_doctor() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    live_opencode(&e);
    write_auth(&e, "minhaempresa", "sk-x-REALMINHAEMPRESA77");
    wait_until("a chave esperando um domínio", || {
        ls(&e).contains("1 chave(s) sem domínio: sandmux doctor --fix")
    });
    assert!(auth_text(&e).contains("REALMINHAEMPRESA"));
    let _ = e.run(&e.work, &["kill", "1"]);
}

#[test]
fn gravada_com_o_daemon_parado_vai_ao_abrir_o_cofre() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    assert!(e.run(&e.work, &["daemon", "stop"]).status.success());
    write_auth(&e, "openrouter", "sk-or-v1-REALANTES31");
    // O daemon sobe de novo com o cofre trancado; abrir acorda a varredura.
    assert!(vault(&e, &["unlock"], "s\n").status.success());
    wait_until("o falso no auth.json", || {
        let a = auth_text(&e);
        a.contains("sandmux-fake-") && !a.contains("REALANTES")
    });
}

#[test]
fn varredura_periodica_sem_aviso_do_kernel() {
    let e = Env::new();
    // O daemon sobe sem inotify e com a varredura a cada segundo.
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .env("SANDMUX_SECRET_INOTIFY", "0")
        .env("SANDMUX_SECRET_SCAN_SECS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s\n").unwrap();
    assert!(c.wait_with_output().unwrap().status.success());
    // Depois da varredura que o cofre novo acordou.
    std::thread::sleep(Duration::from_millis(1500));
    write_auth(&e, "openrouter", "sk-or-v1-REALPERIODICA8");
    wait_until("o falso no auth.json pela varredura periódica", || {
        let a = auth_text(&e);
        a.contains("sandmux-fake-") && !a.contains("REALPERIODICA")
    });
}
