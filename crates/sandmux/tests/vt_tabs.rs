//! Tabulação para trás (CBT, `CSI Z`) e para a frente (CHT, `CSI I`) no
//! emulador das abas. O renderizador do Antigravity CLI move o cursor com
//! o CBT; os resultados do CBT foram conferidos no tmux, e os do CHT (que o
//! tmux não implementa) seguem o ECMA-48.

mod common;
use common::cli::*;
use common::pty::Term;

fn nz(v: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(v).unwrap()
}

fn line(bytes: &[u8]) -> String {
    let mut p = vt100::Parser::new(nz(4), nz(40), 0);
    p.process(bytes);
    p.screen().rows(0, 40).next().unwrap_or_default()
}

#[test]
fn cbt_volta_paradas_de_tabulacao() {
    assert_eq!(line(b"\x1b[1;20HX\x1b[ZA\x1b[2ZB"), "        B       A  X");
    // Mais paradas que as que existem: para na coluna 0.
    assert_eq!(line(b"\x1b[1;5HX\x1b[9ZC"), "C   X");
}

#[test]
fn cbt_com_a_quebra_pendente_parte_da_ultima_coluna() {
    assert_eq!(
        line(b"\x1b[1;1HABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd\x1b[ZW"),
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345W789abcd"
    );
    // O recuo comum fica como no tmux.
    assert_eq!(
        line(b"\x1b[1;1HABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd\x1b[2DW"),
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abWd"
    );
}

#[test]
fn cht_avanca_paradas_de_tabulacao() {
    assert_eq!(
        line(b"\x1b[1;1H\x1b[IA\x1b[2IB"),
        "        A               B"
    );
    // Mais paradas que a largura: para na última coluna.
    assert_eq!(line(b"\x1b[1;1H\x1b[99IZ"), format!("{}Z", " ".repeat(39)));
}

#[test]
fn aba_da_tui_volta_pela_tabulacao() {
    let e = Env::new();
    e.write_config("[tui]\npanel = false\n");
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 60, 12);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    // Na coluna 20, duas paradas para trás levam à coluna 9 (8, contando de 0).
    t.write(b"clear; printf 'abcdefghijklmnopqrs\\033[2Z>\\n'; echo depois-$((2+3))\r");
    t.wait_text("depois-5");
    let s = t.screen();
    assert!(s.contains("abcdefgh>jklmnopqrs"), "{s}");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
