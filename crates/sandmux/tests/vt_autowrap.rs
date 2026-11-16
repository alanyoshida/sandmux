//! A quebra automática de linha (DECAWM, `CSI ? 7 l`/`h`) no emulador das
//! abas, com a semântica do xterm (conferida no tmux): o renderizador do
//! Antigravity CLI desliga a quebra para escrever até a última coluna.

mod common;
use common::cli::*;
use common::pty::Term;

fn nz(v: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(v).unwrap()
}

fn screen(bytes: &[u8], rows: u16, cols: u16) -> vt100::Parser {
    let mut p = vt100::Parser::new(nz(rows), nz(cols), 0);
    p.process(bytes);
    p
}

fn row(p: &vt100::Parser, r: u16) -> String {
    let cols = p.screen().size().1.get();
    p.screen().rows(0, cols).nth(r as usize).unwrap_or_default()
}

#[test]
fn sem_quebra_o_excesso_sobrescreve_a_ultima_coluna() {
    let p = screen(b"\x1b[?7lABCDEFGHIJKLMNOPQRSTUVWXY\x1b[?7h", 3, 20);
    assert_eq!(row(&p, 0), "ABCDEFGHIJKLMNOPQRSY");
    assert_eq!(row(&p, 1), "");
    assert!(!p.screen().row_wrapped(0));
    // O cursor fica na última coluna, sem quebra pendente.
    assert_eq!(p.screen().cursor_position(), (0, 19));
}

#[test]
fn movimento_relativo_parte_da_ultima_coluna() {
    let p = screen(b"\x1b[?7lABCDEFGHIJKLMNOPQRST\x1b[DZ", 3, 20);
    assert_eq!(row(&p, 0), "ABCDEFGHIJKLMNOPQRZT");
}

#[test]
fn caractere_largo_no_fim_da_linha() {
    let p = screen("\x1b[?7lABCDEFGHIJKLMNOPQRS界".as_bytes(), 3, 20);
    assert_eq!(row(&p, 0), "ABCDEFGHIJKLMNOPQR界");
    assert_eq!(row(&p, 1), "");
}

#[test]
fn com_a_quebra_ligada_o_texto_continua_embaixo() {
    let p = screen(b"\x1b[?7l\x1b[?7hABCDEFGHIJKLMNOPQRSTUV", 3, 20);
    assert_eq!(row(&p, 0), "ABCDEFGHIJKLMNOPQRST");
    assert_eq!(row(&p, 1), "UV");
}

#[test]
fn estado_reenviado_mantem_a_quebra_desligada() {
    let p = screen(b"\x1b[?7lantes", 3, 20);
    let state = p.screen().state_formatted();
    assert!(
        state.ends_with("\x1b[?7l") || state.contains("\x1b[?7l"),
        "{state:?}"
    );
    // Um cliente que recebe o estado e continua o fluxo não quebra a linha.
    let mut c = vt100::Parser::new(nz(3), nz(20), 0);
    c.process(state.as_bytes());
    c.process(b"\r0123456789012345678901234");
    assert_eq!(row(&c, 0), "01234567890123456784");
    assert_eq!(row(&c, 1), "");
    // A diferença de modos também leva a mudança.
    let on = screen(b"", 3, 20);
    assert!(p.screen().state_diff(on.screen()).contains("\x1b[?7l"));
    assert!(on.screen().state_diff(p.screen()).contains("\x1b[?7h"));
}

#[test]
fn aba_da_tui_corta_a_linha_sem_quebra() {
    let e = Env::new();
    e.write_config("[tui]\npanel = false\n");
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 60, 12);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    // 70 caracteres numa linha de 60 colunas, com a quebra desligada.
    t.write(b"clear; printf '\\033[?7l%070dFIM\\033[?7h\\n'; echo depois-$((2+3))\r");
    t.wait_text("depois-5");
    let s = t.screen();
    let lines: Vec<&str> = s.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.contains("depois-5"))
        .expect("linha do depois");
    // A linha cortada termina no último caractere escrito, e a de baixo é
    // a do comando seguinte, sem o excesso.
    assert!(lines[i - 1].trim_end().ends_with("0M"), "{s}");
    assert!(!lines[i - 1].contains("FIM"), "{s}");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
