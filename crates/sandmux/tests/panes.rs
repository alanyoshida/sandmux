//! Painéis na aba: dividir, foco, ampliar, fechar e o layout guardado.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::Path;

fn term_in(e: &Env, cwd: &Path, args: &[&str]) -> Term {
    let mut cmd = e.cmd(cwd, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn_answering(cmd, 120, 30)
}

fn sessao(e: &Env) {
    let mut t = term_in(e, &e.work, &["run", "shell"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

/// A coluna do separador vertical e a coluna em que `text` aparece.
fn sep_and(t: &Term, text: &str) -> (u16, u16) {
    t.with_screen(|s| {
        let (rows, cols) = (s.size().0.get(), s.size().1.get());
        let mut sep = None;
        let mut at = None;
        for r in 0..rows {
            let line = s.contents_between(r, 0, r, cols);
            if sep.is_none()
                && let Some(c) = line.find('│')
            {
                sep = Some(line[..c].chars().count() as u16);
            }
            if at.is_none()
                && let Some(c) = line.find(text)
            {
                at = Some(line[..c].chars().count() as u16);
            }
        }
        (
            sep.unwrap_or_else(|| panic!("sem separador: {}", s.contents())),
            at.unwrap_or_else(|| panic!("sem {text}: {}", s.contents())),
        )
    })
}

fn wait_for(t: &Term, what: &str, f: impl Fn(&str) -> bool) {
    t.wait_for(what, f);
}

#[test]
fn dividir_focar_ampliar_fechar_e_voltar() {
    let e = Env::new();
    // Sem o painel lateral: as linhas dele não se confundem com os
    // separadores.
    e.write_config("[tui]\npanel = false\n");
    sessao(&e);
    let mut t = term_in(&e, &e.work, &[]);
    t.wait_text(&tab(1, "shell:app"));

    // Lado a lado: o modal de nova aba, com a pasta do painel.
    t.write(b"\x02%");
    t.wait_text("Nova aba");
    t.wait_text(&e.work.display().to_string());
    t.write(b"\r");
    t.wait_text("shell:app +1");
    t.write(b"echo dir-$((1+1))\r");
    t.wait_text("dir-2");
    let (sep, at) = sep_and(&t, "dir-2");
    assert!(at > sep, "dir-2 em {at}, separador em {sep}");

    // Ctrl+b ← põe o foco no painel da esquerda.
    t.write(b"\x02\x1b[D");
    t.write(b"echo esq-$((2+2))\r");
    t.wait_text("esq-4");
    let (sep, at) = sep_and(&t, "esq-4");
    assert!(at < sep, "esq-4 em {at}, separador em {sep}");

    // Ampliar: só o painel ativo, sem separador; de novo, o layout.
    t.write(b"\x02z");
    wait_for(&t, "sem separador", |s| {
        !s.contains('│') && s.contains("esq-4")
    });
    t.write(b"\x02z");
    wait_for(&t, "o separador de volta", |s| s.contains('│'));

    // O layout volta ao reabrir a TUI e depois de reiniciar o daemon.
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    let mut t = term_in(&e, &e.work, &[]);
    t.wait_text("shell:app +1");
    wait_for(&t, "o separador", |s| s.contains('│'));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    assert!(e.run(&e.work, &["daemon", "stop"]).status.success());
    let mut t = term_in(&e, &e.work, &[]);
    t.wait_text("shell:app +1");
    wait_for(&t, "o separador depois do reinício", |s| s.contains('│'));

    // Em cima e embaixo, no painel da esquerda.
    t.write(b"\x02\"");
    t.wait_text("Nova aba");
    t.write(b"\r");
    t.wait_text("shell:app +2");
    wait_for(&t, "o separador horizontal", |s| s.contains('─'));

    // Fechar o painel ativo: o vizinho fica com o espaço.
    t.write(b"\x02x");
    t.wait_text("s/n");
    t.write(b"s");
    t.wait_text("shell:app +1");
    wait_for(&t, "sem o separador horizontal", |s| !s.contains('─'));
    t.write(b"\x02x");
    t.wait_text("s/n");
    t.write(b"s");
    wait_for(&t, "uma aba sem painéis", |s| {
        !s.contains('│') && !s.contains(" +1")
    });
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn fechar_um_painel_mantem_o_foco_na_mesma_aba() {
    let e = Env::new();
    e.write_config("[tui]\npanel = false\n");
    sessao(&e);
    // Uma segunda aba, com outro texto na tela.
    let mut t = term_in(&e, &e.work, &["run", "shell"]);
    t.write(b"echo aba-$((1+1))-dois\r");
    t.wait_text("aba-2-dois");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    let mut t = term_in(&e, &e.work, &[]);
    t.wait_text(&tab(2, "shell:app"));
    t.write(b"\x021");
    t.write(b"echo aba-$((0+1))-um\r");
    t.wait_text("aba-1-um");
    // Divide a aba 1 e fecha o painel novo, que está em foco.
    t.write(b"\x02%");
    t.wait_text("Nova aba");
    t.write(b"\r");
    t.wait_text("shell:app +1");
    t.write(b"\x02x");
    t.wait_text("s/n");
    t.write(b"s");
    wait_for(&t, "a aba 1 sem painéis", |s| {
        !s.contains('│') && !s.contains(" +1")
    });
    // O foco ficou no painel que sobrou, na aba 1, e não na aba 2.
    t.write(b"echo ficou-$((2+3))\r");
    t.wait_text("ficou-5");
    let s = t.screen();
    assert!(s.contains("aba-1-um") && !s.contains("aba-2-dois"), "{s}");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
