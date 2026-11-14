//! Configuração nas sessões: verificações do `run` com terminal, saudação,
//! reinício e restauração com a política resolvida de novo.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::Path;
use std::process::Stdio;

fn term_in(e: &Env, cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> Term {
    let mut cmd = e.cmd(cwd, args);
    cmd.env("SHELL", "/bin/bash");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    Term::spawn(cmd, 100, 30)
}

fn ls(e: &Env) -> String {
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn line_of(list: &str, id: u32) -> String {
    list.lines()
        .find(|l| l.starts_with(&format!("{id} ")))
        .unwrap_or_else(|| panic!("sem a sessão {id}:\n{list}"))
        .to_string()
}

/// Cria uma sessão, espera o prompt e desanexa.
fn detached(e: &Env, args: &[&str], envs: &[(&str, &str)]) {
    let mut t = term_in(e, &e.work, args, envs);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

fn docs(e: &Env) -> std::path::PathBuf {
    let d = e.home.join("docs");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.txt"), "conteudo-docs\n").unwrap();
    d
}

// ---- 5.4 verificações do run com terminal ----

#[test]
fn run_com_terminal_recusa_configuracao_invalida_e_perfil_inexistente() {
    let e = Env::new();
    e.write_config("[global]\nrede = \"aberta\"\n");
    let mut t = term_in(&e, &e.work, &["run", "shell"], &[]);
    assert_eq!(t.wait_exit(), 1);
    let s = t.raw_text();
    assert!(s.contains("linha 2") && s.contains("`rede`"), "{s}");
    assert!(ls(&e).contains("Não há sessões"));

    e.write_config("[profile.vm]\nkvm = true\n");
    let mut t = term_in(&e, &e.work, &["run", "shell", "--profile", "nada"], &[]);
    assert_eq!(t.wait_exit(), 1);
    let s = t.raw_text();
    assert!(s.contains("\"nada\"") && s.contains("vm"), "{s}");
    assert!(ls(&e).contains("Não há sessões"));
}

// ---- 6.1 mesma configuração no cliente e no daemon ----

#[test]
fn pasta_de_configuracao_diferente_recusada() {
    let e = Env::new();
    let other = e.dir.path().join("outra-config");
    std::fs::create_dir_all(&other).unwrap();
    let mut d = e
        .daemon_cmd()
        .env("XDG_CONFIG_HOME", &other)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    e.wait_socket();
    let out = e.run(&e.work, &["ls"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("outra-config/sandmux")
            && err.contains(&e.config.join("sandmux").display().to_string()),
        "{err}"
    );
    // O daemon continua de pé.
    assert!(std::os::unix::net::UnixStream::connect(e.socket()).is_ok());
    let _ = d.kill();
    let _ = d.wait();
}

// ---- 6.2 daemon resolve na criação e no reinício ----

#[test]
fn clipboard_liberado_pelo_perfil_do_projeto() {
    let e = Env::new();
    e.write_config(&format!(
        "[project.\"{}\"]\nclipboard = true\n",
        e.work.display()
    ));
    let mut t = term_in(&e, &e.work, &["run", "shell"], &[]);
    t.write(b"printf '\\033]52;c;b2xh\\007'; echo osc-$((5*2))\r");
    t.wait_text("osc-10");
    assert!(
        t.raw_text().contains("\x1b]52;c;b2xh\x07"),
        "{:?}",
        t.raw_text()
    );
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn restart_resolve_o_perfil_de_novo() {
    let e = Env::new();
    let d = docs(&e);
    e.write_config("[profile.leitura]\nro = [\"~/docs\"]\n");
    let cat = format!("cat {}/a.txt || echo sem-docs-$((3*3))\r", d.display());

    let mut t = term_in(&e, &e.work, &["run", "shell", "--profile", "leitura"], &[]);
    t.write(cat.as_bytes());
    t.wait_text("conteudo-docs");
    t.write(b"exit 3\r");
    assert_eq!(t.wait_exit(), 3);

    // O perfil perde a pasta: o reinício não a monta.
    e.write_config("[profile.leitura]\nkvm = false\n");
    let mut t = term_in(&e, &e.work, &["restart", "1"], &[]);
    t.write(cat.as_bytes());
    t.wait_text("sem-docs-9");
    t.write(b"exit 2\r");
    t.wait_exit();

    // O perfil some: o reinício termina com erro e a sessão fica como estava.
    e.write_config("[global]\n");
    let out = e.run(&e.work, &["restart", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("\"leitura\""), "{}", stderr(&out));
    assert!(line_of(&ls(&e), 1).contains("encerrada (2)"));
}

// ---- 6.3 restart com pass-env vindo do perfil ----

#[test]
fn restart_com_variavel_do_perfil() {
    let e = Env::new();
    e.write_config("[profile.ia]\npass-env = [\"CHAVE_P\"]\n");
    let mut t = term_in(
        &e,
        &e.work,
        &["run", "shell", "--profile", "ia"],
        &[("CHAVE_P", "segredo-p1-xyz")],
    );
    t.write(b"echo P=$CHAVE_P\r");
    t.wait_text("P=segredo-p1-xyz");
    t.write(b"exit 1\r");
    t.wait_exit();
    assert!(
        !std::fs::read_to_string(e.data_dir().join("sessions.json"))
            .unwrap()
            .contains("segredo-p1-xyz")
    );

    let out = e.run(&e.work, &["restart", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("CHAVE_P"), "{}", stderr(&out));

    let mut t = term_in(&e, &e.work, &["restart", "1"], &[("CHAVE_P", "p2")]);
    t.write(b"echo P=$CHAVE_P\r");
    t.wait_text("P=p2");
    t.write(b"exit\r");
    t.wait_exit();
}

// ---- 6.4 restauração ----

#[test]
fn restauracao_com_a_configuracao_do_momento() {
    let e = Env::new();
    e.write_config("[profile.vm]\nkvm = false\n");
    detached(&e, &["run", "shell"], &[]); // 1: o projeto ganha pass-env
    detached(&e, &["run", "shell", "--profile", "vm"], &[]); // 2: perfil apagado
    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));

    e.write_config(&format!(
        "[project.\"{}\"]\npass-env = [\"CHAVE_R\"]\n",
        e.work.display()
    ));
    let list = ls(&e);
    let l1 = line_of(&list, 1);
    assert!(l1.contains("parada") && l1.contains("CHAVE_R"), "{list}");
    let l2 = line_of(&list, 2);
    assert!(l2.contains("parada") && l2.contains("\"vm\""), "{list}");

    // Com a configuração inválida, toda sessão viva fica parada e o daemon
    // continua.
    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    e.write_config("[global]\n");
    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success());
    // Relança a 1 para que volte viva antes do teste do arquivo inválido.
    let mut t = term_in(&e, &e.work, &["restart", "1"], &[]);
    t.write(b"echo ok-$((2*2))\r");
    t.wait_text("ok-4");
    t.write(b"\x02d");
    t.wait_exit();
    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    e.write_config("[global]\nkvm = \"sim\"\n");
    let list = ls(&e);
    let l1 = line_of(&list, 1);
    assert!(
        l1.contains("parada") && l1.contains("config.toml") && l1.contains("linha 2"),
        "{list}"
    );
    let out = e.run(&e.work, &["daemon", "status"]);
    assert!(out.status.success(), "{}", stdout(&out));
}
