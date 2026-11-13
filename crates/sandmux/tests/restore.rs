//! Reinício de sessão, restauração ao subir o daemon e `daemon restart`.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::Path;

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
fn detached(e: &Env, cwd: &Path, args: &[&str], envs: &[(&str, &str)]) {
    let mut t = term_in(e, cwd, args, envs);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn restart_de_sessao_encerrada_viva_e_sem_variavel() {
    let e = Env::new();
    let mut t = term_in(&e, &e.work, &["run", "shell"], &[]);
    t.write(b"exit 4\r");
    assert_eq!(t.wait_exit(), 4);
    // Reinício de sessão encerrada: sobe, anexa e fica viva.
    let mut t = term_in(&e, &e.work, &["restart", "1"], &[]);
    t.write(b"echo de-novo-$((2+3))\r");
    t.wait_text("de-novo-5");
    t.write(b"\x02d");
    t.wait_exit();
    assert!(line_of(&ls(&e), 1).contains("viva"));
    // Sessão viva não é reiniciada.
    let out = e.run(&e.work, &["restart", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("está viva"), "{}", stderr(&out));

    // Variável de --pass-env ausente impede o reinício.
    let mut t = term_in(
        &e,
        &e.work,
        &["run", "shell", "--pass-env", "CHAVE_X"],
        &[("CHAVE_X", "v")],
    );
    t.write(b"exit 1\r");
    t.wait_exit();
    let out = e.run(&e.work, &["restart", "2"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("CHAVE_X"), "{}", stderr(&out));
    assert!(line_of(&ls(&e), 2).contains("encerrada (1)"));
    // Com a variável, reinicia sem terminal (não anexa).
    let out = e
        .cmd(&e.work, &["restart", "2"])
        .env("CHAVE_X", "v2")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(line_of(&ls(&e), 2).contains("viva"));
}

#[test]
fn restauracao_ao_subir_o_daemon() {
    let e = Env::new();
    let gone = e.home.join("projetos/temporario");
    std::fs::create_dir_all(&gone).unwrap();
    detached(&e, &e.work, &["run", "shell"], &[]); // 1: volta sozinha
    detached(
        &e,
        &e.work,
        &["run", "shell", "--pass-env", "CHAVE_Y"],
        &[("CHAVE_Y", "y")],
    ); // 2: parada
    detached(&e, &gone, &["run", "shell"], &[]); // 3: pasta apagada
    let mut t = term_in(&e, &e.work, &["run", "shell"], &[]);
    t.write(b"exit 6\r");
    assert_eq!(t.wait_exit(), 6); // 4: encerrada

    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    std::fs::remove_dir_all(&gone).unwrap();

    let list = ls(&e); // sobe o daemon de novo
    assert!(line_of(&list, 1).contains("viva"), "{list}");
    let l2 = line_of(&list, 2);
    assert!(
        l2.contains("parada") && l2.contains("CHAVE_Y") && l2.contains("sandmux restart 2"),
        "{list}"
    );
    assert!(line_of(&list, 3).contains("não existe mais"), "{list}");
    assert!(line_of(&list, 4).contains("encerrada (6)"), "{list}");

    // A sessão 1 foi relançada sem cliente e responde ao anexar.
    let mut t = term_in(&e, &e.work, &["attach", "1"], &[]);
    t.write(b"echo restaurada-$((4*4))\r");
    t.wait_text("restaurada-16");
    t.write(b"\x02d");
    t.wait_exit();

    // A parada sobe com a variável.
    let mut t = term_in(&e, &e.work, &["restart", "2"], &[("CHAVE_Y", "y2")]);
    t.write(b"echo Y=$CHAVE_Y\r");
    t.wait_text("Y=y2");
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn daemon_restart_relanca_e_perde_o_historico() {
    let e = Env::new();
    let mut t = term_in(&e, &e.work, &["run", "shell"], &[]);
    t.write(b"echo HIST-$((9*9))\r");
    t.wait_text("HIST-81");
    t.write(b"\x02d");
    t.wait_exit();
    let pid = e.daemon_pid().unwrap();

    let out = e.run(&e.work, &["daemon", "restart"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_ne!(e.daemon_pid().unwrap(), pid, "o daemon não foi trocado");
    assert!(line_of(&ls(&e), 1).contains("viva"));

    let mut t = term_in(&e, &e.work, &["attach", "1"], &[]);
    t.write(b"echo depois-$((1+1))\r");
    t.wait_text("depois-2");
    assert!(
        !t.raw_text().contains("HIST-81"),
        "o histórico sobreviveu ao reinício"
    );
    t.write(b"exit\r");
    t.wait_exit();
}
