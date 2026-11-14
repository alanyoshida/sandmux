//! Sessão do gateway do hermes (change 20): `--gateway`, um por vez, reinício
//! com espera crescente, `kill` sem relançar e espera pelo cofre depois de um
//! reinício do daemon. O hermes é falso: no `gateway run`, anota o início na
//! pasta de trabalho, espera um segundo e sai com erro.

mod common;
use common::cli::*;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn exe(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn fake_hermes(e: &Env) {
    std::fs::create_dir_all(e.home.join(".hermes/hermes-agent")).unwrap();
    std::fs::write(e.home.join(".hermes/hermes-agent/main.py"), "codigo").unwrap();
    exe(
        &e.home.join(".local/bin/hermes"),
        "if [ \"$1\" = gateway ]; then echo \"inicio $2\" >> ./inicios; sleep 1; exit 3; fi\nexec \"$@\"",
    );
}

fn ls(e: &Env) -> String {
    stdout(&e.run(&e.work, &["ls"]))
}

fn starts(e: &Env) -> usize {
    std::fs::read_to_string(e.work.join("inicios"))
        .unwrap_or_default()
        .lines()
        .count()
}

fn wait(secs: u64, f: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn gateway_so_no_hermes() {
    let e = Env::new();
    let o = e.run(&e.work, &["run", "opencode", "--gateway"]);
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("só o hermes tem gateway"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn gateway_reinicia_um_por_vez_e_kill_encerra() {
    let e = Env::new();
    fake_hermes(&e);
    // Sem terminal, o gateway vai para o daemon mesmo assim.
    let o = e.run(&e.work, &["run", "hermes", "--gateway", "--no-import"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("sessão 1 criada: hermes-gateway:app"),
        "{}",
        stdout(&o)
    );
    assert!(wait(5, || starts(&e) >= 1), "o gateway não subiu");
    assert_eq!(
        std::fs::read_to_string(e.work.join("inicios")).unwrap(),
        "inicio run\n"
    );
    // Caiu: volta depois de 5 s.
    assert!(
        wait(5, || ls(&e).contains("reiniciando em 5 s")),
        "{}",
        ls(&e)
    );
    assert!(wait(8, || starts(&e) >= 2), "o gateway não voltou");
    // Caiu de novo logo em seguida: a espera dobra.
    assert!(
        wait(5, || ls(&e).contains("reiniciando em 10 s")),
        "{}",
        ls(&e)
    );

    // Um segundo gateway é recusado, mesmo noutra pasta.
    let outra = e.work.parent().unwrap().join("outra");
    std::fs::create_dir_all(&outra).unwrap();
    let o = e.run(&outra, &["run", "hermes", "--gateway", "--no-import"]);
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("já existe a sessão 1"),
        "{}",
        stderr(&o)
    );

    // `kill` encerra sem relançar.
    assert!(e.run(&e.work, &["kill", "1"]).status.success());
    let n = starts(&e);
    std::thread::sleep(Duration::from_secs(11));
    assert_eq!(starts(&e), n, "o gateway voltou depois do kill");
    assert!(!ls(&e).contains("hermes-gateway"), "{}", ls(&e));
}

#[test]
fn gateway_espera_o_cofre_depois_do_reinicio_do_daemon() {
    let e = Env::new();
    fake_hermes(&e);
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    let o = e.run(&e.work, &["run", "hermes", "--gateway", "--no-import"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(wait(5, || starts(&e) >= 1));

    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());
    let l = ls(&e);
    assert!(l.contains("aguardando cofre"), "{l}");
    let n = starts(&e);
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(starts(&e), n, "o gateway subiu com o cofre trancado");

    let mut c = e
        .cmd(&e.work, &["vault", "unlock"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    assert!(
        wait(8, || starts(&e) > n),
        "o gateway não subiu no desbloqueio: {}",
        ls(&e)
    );
    assert!(e.run(&e.work, &["kill", "1"]).status.success());
}
