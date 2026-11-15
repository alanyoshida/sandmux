//! Grupos de abas: `run --group`, `ls`, `sandmux group`, a restauração e a
//! TUI por grupo.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::Path;

fn term_in(e: &Env, cwd: &Path, args: &[&str]) -> Term {
    let mut cmd = e.cmd(cwd, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn_answering(cmd, 100, 30)
}

fn ls(e: &Env, args: &[&str]) -> String {
    let mut a = vec!["ls"];
    a.extend_from_slice(args);
    let out = e.run(&e.work, &a);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

/// Cria uma sessão do shell na pasta, no grupo dado, e desanexa.
fn sessao(e: &Env, cwd: &Path, group: Option<&str>) {
    let mut args = vec!["run", "shell"];
    if let Some(g) = group {
        args.extend(["--group", g]);
    }
    let mut t = term_in(e, cwd, &args);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

fn api(e: &Env) -> std::path::PathBuf {
    let p = e.home.join("projetos/api");
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn run_ls_mover_renomear_e_restaurar() {
    let e = Env::new();
    sessao(&e, &e.work, None);
    sessao(&e, &api(&e), Some("estudos"));
    let all = ls(&e, &[]);
    let head = all.lines().next().unwrap();
    assert!(head.contains("GRUPO"), "{all}");
    let line = |id: &str, text: &str| {
        text.lines()
            .find(|l| l.starts_with(&format!("{id} ")))
            .unwrap_or_default()
            .to_string()
    };
    assert!(line("1", &all).contains("principal"), "{all}");
    assert!(line("2", &all).contains("estudos"), "{all}");
    let only = ls(&e, &["--group", "estudos"]);
    assert!(
        only.contains("shell:api") && !only.contains("shell:app"),
        "{only}"
    );

    let list = stdout(&e.run(&e.work, &["group", "list"]));
    assert!(
        list.contains("estudos") && list.contains("principal"),
        "{list}"
    );

    // Nome inválido: recusa com a regra, no `run` e no `move`.
    let o = e.run(&e.work, &["run", "shell", "--group", "a b"]);
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("nome de grupo inválido"),
        "{}",
        stderr(&o)
    );
    let o = e.run(&e.work, &["group", "move", "1", "a/b"]);
    assert!(!o.status.success());

    assert!(
        e.run(&e.work, &["group", "move", "1", "estudos"])
            .status
            .success()
    );
    assert!(line("1", &ls(&e, &[])).contains("estudos"));
    assert!(
        e.run(&e.work, &["group", "rename", "estudos", "rust"])
            .status
            .success()
    );
    let all = ls(&e, &[]);
    assert!(
        line("1", &all).contains("rust") && line("2", &all).contains("rust"),
        "{all}"
    );
    let o = e.run(&e.work, &["group", "rename", "inexistente", "x"]);
    assert!(!o.status.success());

    // Restauração: o grupo volta com a sessão.
    assert!(e.run(&e.work, &["daemon", "stop"]).status.success());
    let all = ls(&e, &[]);
    assert!(
        line("1", &all).contains("rust") && line("2", &all).contains("rust"),
        "{all}"
    );
    let list = stdout(&e.run(&e.work, &["group", "list"]));
    assert!(
        list.contains("principal") && !list.contains("estudos"),
        "{list}"
    );
}

#[test]
fn tui_mostra_o_grupo_troca_cria_e_move() {
    let e = Env::new();
    sessao(&e, &e.work, None);
    sessao(&e, &api(&e), Some("estudos"));

    // Sem `--group`: o principal, só com a aba dele.
    let mut t = term_in(&e, &e.work, &[]);
    t.wait_text(&tab(1, "shell:app"));
    t.wait_text("principal");
    assert!(!t.screen().contains("shell:api"), "{}", t.screen());

    // Próximo grupo: estudos.
    t.write(b"\x02)");
    t.wait_text("shell:api");
    t.wait_text("estudos");
    assert!(!t.screen().contains("shell:app"), "{}", t.screen());
    // As abas do outro grupo continuam vivas.
    assert!(ls(&e, &[]).lines().filter(|l| l.contains("viva")).count() == 2);

    // Lista de grupos: leva a aba ativa (shell:api) ao principal.
    t.write(b"\x02G");
    t.wait_text("Grupos");
    t.write(b"\x1b[B");
    t.write(b"m");
    t.wait_text("levada ao grupo principal");
    let all = ls(&e, &[]);
    assert!(
        all.lines().filter(|l| l.contains("principal")).count() == 2,
        "{all}"
    );

    // Grupo novo pela lista: vazio, abre o modal de nova aba no grupo.
    t.write(b"\x02G");
    t.wait_text("Grupos");
    t.write(b"n");
    t.write(b"novo\r");
    t.wait_text("grupo novo");
    t.write(b"\x02c");
    t.wait_text("Nova aba");
    t.write(b"\r");
    t.wait_text(&tab(1, "shell:app"));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    let novo = ls(&e, &["--group", "novo"]);
    assert_eq!(novo.lines().count(), 2, "{novo}");

    // Duas TUIs: `--group novo` mostra só a aba do grupo novo.
    let mut t = term_in(&e, &e.work, &["tui", "--group", "novo"]);
    t.wait_text("novo");
    t.wait_text(&tab(1, "shell:app"));
    assert!(!t.screen().contains("shell:api"), "{}", t.screen());
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
