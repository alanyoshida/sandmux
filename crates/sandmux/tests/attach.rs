//! Cliente anexado e `run` com terminal: modos, reenvio, tamanho, vários
//! clientes, desanexar e clipboard.

mod common;
use common::cli::*;
use common::pty::Term;

use std::time::Duration;

fn term(e: &Env, args: &[&str], cols: u16, rows: u16) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, cols, rows)
}

fn ls(e: &Env) -> String {
    stdout(&e.run(&e.work, &["ls"]))
}

/// Cria uma sessão do shell, espera o prompt e desanexa.
fn detached_shell(e: &Env) {
    let mut t = term(e, &["run", "shell"], 100, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn desanexar_restaura_o_terminal() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"printf '\\033[?1049h\\033[?1000h\\033[?1006h\\033[?1004h\\033[?2004h\\033[>1u\\033[?25l'; echo modos-$((2+2))on\r");
    t.wait_text("modos-4on");
    assert!(t.with_screen(|s| s.alternate_screen()));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    t.with_screen(|s| {
        assert!(!s.alternate_screen(), "continua na tela alternativa");
        assert_eq!(s.mouse_protocol_mode(), vt100::MouseProtocolMode::None);
        assert!(!s.bracketed_paste());
        assert!(!s.hide_cursor());
    });
    let raw = t.raw_text();
    let tail = &raw[raw.rfind("modos-4on").unwrap()..];
    assert!(
        tail.contains("\x1b[<1u") && tail.contains("\x1b[?1004l"),
        "{tail:?}"
    );
}

#[test]
fn reanexar_reenvia_historico_e_mesma_tela() {
    let e = Env::new();
    let mut a = term(&e, &["run", "shell"], 80, 30);
    a.write(b"seq 1 300; echo FIM-$((150*2))\r");
    a.wait_text("FIM-300\n");
    let mut b = term(&e, &["attach", "1"], 80, 30);
    b.wait_text("FIM-300");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(a.screen(), b.screen());
    let hist = b.scrollback_text();
    let lines: Vec<&str> = hist.lines().map(str::trim).collect();
    for n in [1, 150, 300] {
        assert!(lines.contains(&n.to_string().as_str()), "falta a linha {n}");
    }
    b.write(b"\x02d");
    b.wait_exit();
    a.write(b"exit\r");
    a.wait_exit();
}

#[test]
fn harness_em_tela_alternativa_reaparece_ao_anexar() {
    let e = Env::new();
    let mut a = term(&e, &["run", "shell"], 80, 24);
    a.write(b"printf '\\033[?1049h\\033[2J\\033[5;10HTELA-%s' CHEIA; sleep 1000\r");
    a.wait_text("TELA-CHEIA");
    a.write(b"\x02d");
    a.wait_exit();
    let b = term(&e, &["attach", "1"], 80, 24);
    b.wait_text("TELA-CHEIA");
    assert!(b.with_screen(|s| s.alternate_screen()));
    assert_eq!(
        b.with_screen(|s| s.contents_between(4, 9, 4, 19)),
        "TELA-CHEIA"
    );
}

#[test]
fn tamanho_segue_o_menor_terminal() {
    let e = Env::new();
    let mut a = term(&e, &["run", "shell"], 200, 50);
    a.write(b"stty size\r");
    a.wait_text("\n50 200");
    let mut b = term(&e, &["attach", "1"], 100, 30);
    b.wait_text("50 200");
    b.write(b"clear; stty size\r");
    b.wait_text("30 100");
    b.write(b"\x02d");
    b.wait_exit();
    a.write(b"clear; stty size\r");
    a.wait_text("50 200");
    // Redimensionar o terminal anexado também conta.
    a.resize(120, 40);
    std::thread::sleep(Duration::from_millis(300));
    a.write(b"clear; stty size\r");
    a.wait_text("40 120");
    a.write(b"exit\r");
    a.wait_exit();
}

#[test]
fn dois_clientes_digitando() {
    let e = Env::new();
    let mut a = term(&e, &["run", "shell"], 100, 30);
    a.write(b"echo um-$((0+1))\r");
    a.wait_text("um-1");
    let mut b = term(&e, &["attach"], 100, 30);
    b.wait_text("um-1");
    b.write(b"echo dois-$((1+1))\r");
    a.wait_text("dois-2");
    b.wait_text("dois-2");
    assert!(ls(&e).contains(" 2 "), "{}", ls(&e));
    a.write(b"exit\r");
    assert_eq!(a.wait_exit(), 0);
    assert_eq!(b.wait_exit(), 0);
}

#[test]
fn erros_do_attach() {
    let e = Env::new();
    // Sem terminal.
    let out = e.run(&e.work, &["attach", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("terminal"), "{}", stderr(&out));
    detached_shell(&e);
    detached_shell(&e);
    // Sem identificador, com duas sessões vivas.
    let mut t = term(&e, &["attach"], 100, 30);
    assert_eq!(t.wait_exit(), 1);
    t.wait_raw("há 2 sessões vivas");
    t.wait_raw("shell:app");
    // Identificador inexistente.
    let mut t = term(&e, &["attach", "99"], 100, 30);
    assert_eq!(t.wait_exit(), 1);
    t.wait_raw("não existe a sessão 99");
    t.wait_raw("1  shell:app  viva");
}

#[test]
fn detach_de_outro_terminal() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    let out = e.run(&e.work, &["detach", "1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(t.wait_exit(), 0);
    t.wait_raw("sandmux detach 1");
    assert!(ls(&e).contains("viva"));
}

#[test]
fn fechar_o_terminal_desanexa() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    let mut child = t.close();
    let st = child.wait().unwrap();
    assert_eq!(st.code(), Some(0));
    std::thread::sleep(Duration::from_millis(300));
    let list = ls(&e);
    let line = list.lines().nth(1).unwrap();
    assert!(line.contains("viva") && line.contains("  0  "), "{list}");
}

#[test]
fn sigterm_no_cliente_desanexa() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    // SAFETY: sinal para o cliente deste teste.
    unsafe { libc::kill(t.child.id() as i32, libc::SIGTERM) };
    assert_eq!(t.wait_exit(), 0);
    t.wait_raw("sandmux attach 1");
    assert!(ls(&e).contains("viva"));
}

const OSC_WRITE: &str = "\x1b]52;c;b2xh\x07";

#[test]
fn clipboard_bloqueado_por_padrao_e_liberado_por_opcao() {
    let e = Env::new();
    let cmd = "printf '\\033]52;c;b2xh\\007\\033]52;c;?\\007'; echo osc-$((5*2))\r";
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(cmd.as_bytes());
    t.wait_text("osc-10");
    assert!(!t.raw_text().contains("\x1b]52"), "{:?}", t.raw_text());
    t.write(b"exit\r");
    t.wait_exit();

    let mut t = term(&e, &["run", "shell", "--allow-clipboard"], 100, 30);
    t.write(cmd.as_bytes());
    t.wait_text("osc-10");
    let raw = t.raw_text();
    assert!(raw.contains(OSC_WRITE), "{raw:?}");
    assert!(!raw.contains("\x1b]52;c;?"), "a consulta passou: {raw:?}");
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn pergunta_da_importacao_aparece_no_terminal() {
    let e = Env::new();
    let bin = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, "#!/bin/sh\necho opencode-falso\n").unwrap();
    std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(e.home.join(".config/opencode")).unwrap();
    std::fs::write(e.home.join(".config/opencode/opencode.json"), "{}").unwrap();
    let mut t = term(&e, &["run", "opencode"], 120, 30);
    t.wait_raw("Importar a configuração");
    t.write(b"n\r");
    assert_eq!(t.wait_exit(), 0);
    t.wait_raw("opencode-falso");
    assert!(ls(&e).contains("opencode:app"), "{}", ls(&e));
}
