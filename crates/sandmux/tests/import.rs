//! Grupo 9: importação pela linha de comando e oferta na primeira execução.

mod common;
use common::cli::*;
use common::*;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

const REAL_KEY: &str = "sk-or-v1-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// HOME falso com um opencode falso instalado e configurado.
fn with_opencode() -> Env {
    let e = Env::new();
    let exe = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "#!/bin/sh\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let w = |rel: &str, s: &str| {
        let p = e.home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    };
    w(
        ".config/opencode/opencode.json",
        r#"{"model": "openrouter/x"}"#,
    );
    w(
        ".local/share/opencode/auth.json",
        &format!(r#"{{"openrouter": {{"type": "api", "key": "{REAL_KEY}"}}}}"#),
    );
    w(".local/share/opencode/opencode.db", "historico");
    e
}

fn synth(e: &Env) -> std::path::PathBuf {
    e.data_dir().join("homes/opencode")
}

#[test]
fn import_pela_linha_de_comando_nao_mostra_valores() {
    let e = with_opencode();
    let out = e.run(&e.work, &["import", "opencode"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("auth.json: key"), "{err}");
    assert!(
        err.contains("opencode.db"),
        "o relatório deveria listar o ignorado: {err}"
    );
    assert!(!err.contains(REAL_KEY));
    let manifest = std::fs::read_to_string(e.data_dir().join("manifests/opencode.json")).unwrap();
    assert!(!manifest.contains(REAL_KEY) && manifest.contains("sandmux-fake-"));
    let auth = std::fs::read_to_string(synth(&e).join(".local/share/opencode/auth.json")).unwrap();
    assert!(!auth.contains(REAL_KEY));
    // O manifesto fica fora do alcance do sandbox.
    let v = e.probe(
        &[],
        &["exists", e.data_dir().join("manifests").to_str().unwrap()],
    );
    assert!(!ok(&v));
}

#[test]
fn import_de_preset_sem_importacao_e_harness_sem_estado() {
    let e = Env::new();
    let out = e.run(&e.work, &["import", "shell"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("não tem importação"));
    let out = e.run(&e.work, &["import", "opencode"]);
    assert!(out.status.success());
    assert!(stderr(&out).contains("não há configuração"));
}

#[test]
fn primeira_execucao_sem_terminal_exige_opcao() {
    let e = with_opencode();
    let out = e.run(&e.work, &["run", "opencode", "--", "true"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("--import") && err.contains("--no-import"),
        "{err}"
    );
    assert!(!synth(&e).exists(), "criou o HOME sintético sem decisão");
}

#[test]
fn no_import_cria_vazio_e_nao_pergunta_de_novo() {
    let e = with_opencode();
    let out = e.run(&e.work, &["run", "opencode", "--no-import", "--", "true"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(synth(&e).exists() && !synth(&e).join(".local/share/opencode/auth.json").exists());
    let out = e.run(&e.work, &["run", "opencode", "--", "true"]);
    assert!(
        out.status.success(),
        "a segunda execução não deveria exigir opção: {}",
        stderr(&out)
    );
    // A opção numa execução posterior é ignorada.
    let out = e.run(&e.work, &["run", "opencode", "--import", "--", "true"]);
    assert!(out.status.success());
    assert!(!synth(&e).join(".local/share/opencode/auth.json").exists());
    // A importação continua possível pelo comando.
    assert!(e.run(&e.work, &["import", "opencode"]).status.success());
    assert!(synth(&e).join(".local/share/opencode/auth.json").exists());
}

#[test]
fn import_na_primeira_execucao_por_opcao() {
    let e = with_opencode();
    let out = e.run(
        &e.work,
        &[
            "run",
            "opencode",
            "--import",
            "--",
            "sh",
            "-c",
            "cat ~/.local/share/opencode/auth.json",
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("sandmux-fake-") && !stdout(&out).contains(REAL_KEY));
    assert!(stderr(&out).contains("segredos trocados"));
}

#[test]
fn primeira_importacao_do_hermes_traz_o_config_real_com_a_chave() {
    let e = Env::new();
    let w = |rel: &str, s: &str| {
        let p = e.home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    };
    w(".hermes/hermes-agent/main.py", "x");
    w(
        ".hermes/config.yaml",
        "model:\n  default: anthropic/claude\nupdates:\n  check: true\n",
    );
    let exe = e.home.join(".local/bin/hermes");
    w(".local/bin/hermes", "#!/bin/sh\nexec \"$@\"\n");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = e.run(
        &e.work,
        &[
            "run",
            "hermes",
            "--import",
            "--",
            "cat",
            e.home.join(".hermes/config.yaml").to_str().unwrap(),
        ],
    );
    assert_eq!(
        stdout(&out),
        "model:\n  default: anthropic/claude\nupdates:\n  check: false\n",
        "{}",
        stderr(&out)
    );
    assert!(
        !stderr(&out).contains("diferentes do host"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn harness_sem_estado_nao_pergunta() {
    let e = Env::new();
    let exe = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "#!/bin/sh\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = e.run(&e.work, &["run", "opencode", "--", "true"]);
    assert!(out.status.success(), "{}", stderr(&out));
}

/// Roda o `sandmux` num pseudoterminal (via `script`), respondendo à pergunta.
fn interactive(e: &Env, answer: &str, cmd: &str) -> String {
    let full = format!(
        "env -i HOME={} XDG_DATA_HOME={} PATH=/usr/bin:/bin TERM=xterm {} {cmd}",
        e.home.display(),
        e.data.display(),
        SANDMUX
    );
    let mut child = Command::new("script")
        .args(["-qec", &full, "/dev/null"])
        .current_dir(&e.work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(answer.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn pergunta_na_primeira_execucao_aceite_e_recusa() {
    if !Path::new("/usr/bin/script").exists() {
        return;
    }
    let e = with_opencode();
    let screen = interactive(&e, "s\n", "run opencode -- true");
    assert!(screen.contains("Importar a configuração"), "{screen}");
    assert!(
        synth(&e).join(".local/share/opencode/auth.json").exists(),
        "{screen}"
    );

    let e = with_opencode();
    let screen = interactive(&e, "n\n", "run opencode -- true");
    assert!(screen.contains("Importar a configuração"), "{screen}");
    assert!(synth(&e).exists() && !synth(&e).join(".local/share/opencode/auth.json").exists());
    let screen = interactive(&e, "", "run opencode -- true");
    assert!(
        !screen.contains("Importar a configuração"),
        "perguntou de novo: {screen}"
    );
}
