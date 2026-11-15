//! Histórico de tela cifrado em disco (`[global] screen-history`).

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::path::{Path, PathBuf};
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

fn term(e: &Env, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, 100, 30)
}

fn hist_file(e: &Env, id: u32) -> PathBuf {
    e.state.join(format!("sandmux/history/{id}.bin"))
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Algum arquivo abaixo de `dir` contém `needle` em claro?
fn anywhere(dir: &Path, needle: &[u8]) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if let Some(found) = anywhere(&p, needle) {
                return Some(found);
            }
        } else if ft.is_file()
            && let Ok(b) = std::fs::read(&p)
            && b.windows(needle.len()).any(|w| w == needle)
        {
            return Some(p);
        }
    }
    None
}

#[test]
fn historico_cifrado_volta_depois_do_reinicio() {
    let e = Env::new();
    e.write_config("[global]\nscreen-history = true\n");
    assert!(vault(&e, &["init"], "s\n").status.success());
    let mut t = term(&e, &["run", "shell"]);
    t.write(b"echo marca-$((40+2))\r");
    t.wait_text("marca-42");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // Gravado em poucos segundos, sem o texto em claro em nenhum arquivo.
    wait_until("o arquivo do histórico", || {
        std::fs::metadata(hist_file(&e, 1)).is_ok_and(|m| m.len() > 0)
    });
    std::thread::sleep(Duration::from_secs(3));
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(hist_file(&e, 1)).unwrap().permissions(),
    ) & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(anywhere(e.dir.path(), b"marca-42"), None);

    // O daemon volta com o cofre trancado: o histórico espera o cofre.
    assert!(e.run(&e.work, &["daemon", "stop"]).status.success());
    let mut t = term(&e, &["attach", "1"]);
    t.wait_text("$");
    std::thread::sleep(Duration::from_millis(500));
    assert!(!t.raw_text().contains("marca-42"), "{}", t.raw_text());
    t.write(b"\x02d");
    t.wait_exit();
    assert!(vault(&e, &["unlock"], "s\n").status.success());
    std::thread::sleep(Duration::from_secs(3));
    let mut t = term(&e, &["attach", "1"]);
    t.wait_raw("marca-42");
    t.wait_raw("a aba voltou");
    // A aba termina e é relançada com o cofre aberto: o histórico vem junto.
    t.write(b"exit\r");
    t.wait_exit();
    let o = e.run(&e.work, &["restart", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let mut t = term(&e, &["attach", "1"]);
    t.wait_raw("marca-42");
    t.write(b"\x02d");
    t.wait_exit();

    // Fechar a aba apaga o arquivo.
    assert!(e.run(&e.work, &["kill", "1"]).status.success());
    wait_until("o arquivo apagado", || !hist_file(&e, 1).exists());
}

#[test]
fn desligar_apaga_os_arquivos() {
    let e = Env::new();
    e.write_config("[global]\nscreen-history = true\n");
    assert!(vault(&e, &["init"], "s\n").status.success());
    let mut t = term(&e, &["run", "shell"]);
    t.write(b"echo outra-$((1+1))\r");
    t.wait_text("outra-2");
    t.write(b"\x02d");
    t.wait_exit();
    wait_until("o arquivo do histórico", || hist_file(&e, 1).exists());
    e.write_config("[global]\nscreen-history = false\n");
    wait_until("a pasta do histórico apagada", || {
        !e.state.join("sandmux/history").exists()
    });
}
