//! Tarefa 3.2: o lançador recusa especificações inválidas sem executar nada.

mod common;
use common::*;

fn marker_spec(marker_dir: &std::path::Path) -> sandmux_spec::LaunchSpec {
    let mut s = base_spec(vec![
        "/bin/sh".into(),
        "-c".into(),
        format!("touch {}/executou", marker_dir.display()),
    ]);
    s.mounts.push(sandmux_spec::Mount::Bind {
        source: marker_dir.into(),
        dest: marker_dir.into(),
        writable: true,
    });
    s.landlock.write.push(marker_dir.into());
    s
}

#[test]
fn especificacao_valida_executa() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_spec(&marker_spec(dir.path()));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.path().join("executou").exists());
}

#[test]
fn versao_desconhecida_nao_executa() {
    let dir = tempfile::tempdir().unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(&marker_spec(dir.path()).to_json()).unwrap();
    v["version"] = serde_json::json!(7);
    let out = run_json(v.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(125));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("recebida 7, suportada 1"), "{err}");
    assert!(!dir.path().join("executou").exists());
}

#[test]
fn campo_desconhecido_nao_executa() {
    let dir = tempfile::tempdir().unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(&marker_spec(dir.path()).to_json()).unwrap();
    v["no_sandbox"] = serde_json::json!(true);
    let out = run_json(v.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(125));
    assert!(!dir.path().join("executou").exists());
}

#[test]
fn json_malformado_nao_executa() {
    let dir = tempfile::tempdir().unwrap();
    let mut json = marker_spec(dir.path()).to_json();
    json.truncate(json.len() / 2);
    let out = run_json(json.as_bytes());
    assert_eq!(out.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&out.stderr).contains("malformada"));
    assert!(!dir.path().join("executou").exists());
}

#[test]
fn descritor_ausente_e_recusado() {
    let out = std::process::Command::new(RUN)
        .args(["--spec-fd", "57"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(125));
}
