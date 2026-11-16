//! Reiniciar uma aba viva: `sandmux restart --force`, Ctrl+b r com
//! confirmação e o aviso das abas vivas ao mudar a configuração.

mod common;
use common::cli::*;
use common::pty::Term;

fn term(e: &Env, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, 120, 30)
}

fn tui(e: &Env) -> Term {
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn_answering(cmd, 160, 30)
}

fn sessao(e: &Env) {
    let mut t = term(e, &["run", "shell"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

fn docs(e: &Env) {
    std::fs::create_dir_all(e.home.join("docs")).unwrap();
    std::fs::write(e.home.join("docs/a.txt"), "conteudo-docs\n").unwrap();
}

#[test]
fn restart_force_relanca_a_sessao_viva_com_a_configuracao_nova() {
    let e = Env::new();
    docs(&e);
    sessao(&e);
    // Sem --force, a sessão viva é recusada e a mensagem aponta o caminho.
    let out = e.run(&e.work, &["restart", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("--force"), "{}", stderr(&out));

    // A mudança na configuração avisa das sessões vivas.
    let out = e.run(
        &e.work,
        &[
            "config",
            "add",
            "--global",
            "ro",
            &e.home.join("docs").display().to_string(),
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("sessões vivas (1)"),
        "{}",
        stderr(&out)
    );

    let mut t = term(&e, &["restart", "1", "--force"]);
    t.write(b"cat ~/docs/a.txt || echo sem-docs\r");
    t.wait_text("conteudo-docs");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    let ls = stdout(&e.run(&e.work, &["ls"]));
    assert!(
        ls.lines()
            .any(|l| l.starts_with("1 ") && l.contains("viva")),
        "{ls}"
    );
}

#[test]
fn ctrl_b_r_pergunta_e_reinicia_a_aba_viva() {
    let e = Env::new();
    e.write_config("[tui]\npanel = false\n");
    docs(&e);
    sessao(&e);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    // Não: a aba continua.
    t.write(b"\x02r");
    t.wait_text("Reiniciar a aba 1?");
    t.write(b"n");
    t.wait_for("o modal fechado", |s| !s.contains("Reiniciar a aba 1?"));
    t.write(b"echo continua-$((2+2))\r");
    t.wait_text("continua-4");
    // A configuração muda; sim: a aba volta com ela.
    e.write_config(&format!(
        "[tui]\npanel = false\n[global]\nro = [\"{}\"]\n",
        e.home.join("docs").display()
    ));
    t.write(b"\x02r");
    t.wait_text("Reiniciar a aba 1?");
    t.write(b"s");
    t.wait_text("aba 1 reiniciada");
    t.write(b"cat ~/docs/a.txt || echo sem-docs\r");
    t.wait_text("conteudo-docs");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
