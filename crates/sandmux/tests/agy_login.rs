//! O login do Google do Antigravity CLI fora do sandbox: o login do host
//! importado para o cofre, um login real gravado no HOME sintético levado ao
//! cofre pelo vigia, e o `*.googleapis.com` decifrado só na aba do `agy`.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

const TOKEN: &str = ".gemini/antigravity-cli/antigravity-oauth-token";

fn token(access: &str, refresh: &str) -> String {
    format!(
        r#"{{"token":{{"access_token":"{access}","token_type":"Bearer","refresh_token":"{refresh}","expiry":"2026-11-16T10:00:00Z","expires_in":3599}},"auth_method":"google","id_token_email":"ana@exemplo.com"}}"#
    )
}

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

fn exe(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `agy` falso: pula as opções do sandmux e executa o resto.
fn fake_agy(e: &Env) {
    exe(
        &e.home.join(".local/bin/agy"),
        "[ \"$1\" = --dangerously-skip-permissions ] && shift\n[ \"$1\" = --settings ] && shift 2\nexec \"$@\"",
    );
}

/// O host com o `agy` instalado e logado.
fn host_login(e: &Env, access: &str, refresh: &str) {
    fake_agy(e);
    let f = e.home.join(TOKEN);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f.parent().unwrap().join("settings.json"), "{}").unwrap();
    std::fs::write(&f, token(access, refresh)).unwrap();
}

fn synthetic(e: &Env) -> PathBuf {
    e.data.join("sandmux/homes/agy")
}

/// Todo o texto do HOME sintético, para procurar tokens reais.
fn all_text(dir: &Path) -> String {
    let mut out = String::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for f in rd.flatten() {
        let p = f.path();
        if p.is_dir() && !p.is_symlink() {
            out.push_str(&all_text(&p));
        } else if let Ok(t) = std::fs::read_to_string(&p) {
            out.push_str(&t);
        }
    }
    out
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "não aconteceu: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn login_do_host_vai_para_o_cofre_e_o_home_tem_os_falsos() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    host_login(&e, "ya29.a0HOSTREAL1", "1//0gHOSTREFRESH1");
    let out = e.run(&e.work, &["import", "agy"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("login do Google levado ao cofre"),
        "{}",
        stderr(&out)
    );
    let home = synthetic(&e);
    let f = home.join(TOKEN);
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.contains("\"access_token\":\"ya29.sandmux"), "{text}");
    assert!(text.contains("\"refresh_token\":\"1//sandmux"), "{text}");
    assert!(text.contains("ana@exemplo.com") && text.contains("2026-11-16T10:00:00Z"));
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let all = all_text(&home);
    assert!(!all.contains("HOSTREAL") && !all.contains("HOSTREFRESH"));
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(list.contains("agy:access  (*.googleapis.com)"), "{list}");
    assert!(
        list.contains("agy:refresh  (oauth2.googleapis.com)"),
        "{list}"
    );
    assert!(!list.contains("HOSTREAL"), "{list}");

    // Sem `--overwrite`, nada muda; com ele, os falsos continuam os mesmos.
    let out = e.run(&e.work, &["import", "agy"]);
    assert!(stderr(&out).contains("--overwrite"), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), text);
    host_login(&e, "ya29.a0HOSTREAL2", "1//0gHOSTREFRESH2");
    let out = e.run(&e.work, &["import", "agy", "--overwrite"]);
    assert!(
        stderr(&out).contains("login do Google levado ao cofre"),
        "{}",
        stderr(&out)
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), text);
    assert!(!all_text(&home).contains("HOSTREAL2"));
    let _ = e.run(&e.work, &["daemon", "stop"]);
}

#[test]
fn com_o_cofre_trancado_o_login_nao_entra() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    assert!(vault(&e, &["lock"], "").status.success());
    host_login(&e, "ya29.a0TRANCADO", "1//0gTRANCADO");
    let out = e.run(&e.work, &["import", "agy"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("login do Google do host não importado")
            && err.contains("sandmux import agy --overwrite"),
        "{err}"
    );
    let home = synthetic(&e);
    assert!(home.join(".gemini/antigravity-cli/settings.json").exists());
    assert!(!home.join(TOKEN).exists());
    assert!(!all_text(&home).contains("TRANCADO"));
    let _ = e.run(&e.work, &["daemon", "stop"]);
}

#[test]
fn login_real_no_home_sintetico_vai_ao_cofre_pelo_vigia() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    assert!(vault(&e, &["lock"], "").status.success());
    assert!(e.run(&e.work, &["daemon", "stop"]).status.success());
    fake_agy(&e);
    // Um login feito dentro do sandbox antes da captura; o daemon, ao subir,
    // varre os HOMEs sintéticos.
    let f = synthetic(&e).join(TOKEN);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, token("ya29.a0DENTRO", "1//0gDENTRO")).unwrap();
    let cmd = e.cmd(
        &e.work,
        &[
            "run",
            "agy",
            "--no-import",
            "--",
            "sh",
            "-c",
            "echo pronta-$((2+2)); sleep 300",
        ],
    );
    let mut t = Term::spawn(cmd, 120, 30);
    t.wait_text("pronta-4");
    t.write(b"\x02d");
    t.wait_exit();
    wait_until("o login esperando o cofre", || {
        stdout(&e.run(&e.work, &["ls"])).contains("2 chave(s) esperando o cofre")
    });
    assert!(std::fs::read_to_string(&f).unwrap().contains("DENTRO"));
    assert!(vault(&e, &["unlock"], "s\n").status.success());
    wait_until("os falsos no arquivo do token", || {
        let t = std::fs::read_to_string(&f).unwrap_or_default();
        t.contains("ya29.sandmux") && t.contains("1//sandmux") && !t.contains("DENTRO")
    });
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(list.contains("agy:access  (*.googleapis.com)"), "{list}");
    let log = std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default();
    assert!(!log.contains("DENTRO"), "{log}");
    let _ = e.run(&e.work, &["kill", "1"]);
}

/// A política de rede gravada para cada aba viva.
fn policies(e: &Env) -> Vec<String> {
    let dir = e.runtime.join("sandmux/net");
    std::fs::read_dir(&dir)
        .map(|d| {
            d.flatten()
                .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
                .filter_map(|f| std::fs::read_to_string(f.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn googleapis_decifrado_so_na_aba_do_agy() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    host_login(&e, "ya29.a0DECIFRA", "1//0gDECIFRA");
    assert!(e.run(&e.work, &["import", "agy"]).status.success());
    // O preset `shell` passa o resto ao shell; o `agy` falso executa o resto.
    let live = |args: &[&str]| {
        let mut a = vec!["run"];
        a.extend_from_slice(args);
        if args[0] == "agy" {
            a.push("--");
            a.push("sh");
        } else {
            a.push("--");
        }
        a.extend_from_slice(&["-c", "echo pronta-$((3+3)); sleep 300"]);
        let mut cmd = e.cmd(&e.work, &a);
        cmd.env("SHELL", "/bin/sh");
        let mut t = Term::spawn(cmd, 120, 30);
        t.wait_text("pronta-6");
        t.write(b"\x02d");
        t.wait_exit();
    };
    live(&["shell", "--no-import"]);
    let shell = policies(&e);
    assert_eq!(shell.len(), 1, "{shell:?}");
    assert!(!shell[0].contains("googleapis"), "{}", shell[0]);
    live(&["agy"]);
    let all = policies(&e);
    assert_eq!(all.len(), 2, "{all:?}");
    let agy = all.iter().find(|p| p.contains("googleapis")).unwrap();
    assert!(agy.contains("googleapis.com"), "{agy}");
    let _ = e.run(&e.work, &["kill", "1"]);
    let _ = e.run(&e.work, &["kill", "2"]);
}
