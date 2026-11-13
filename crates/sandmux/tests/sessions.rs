//! Sessões no daemon: criar, anexar, desanexar, listar, encerrar e reiniciar.

mod common;
use common::cli::*;
use common::pty::Term;

use std::process::Command;

fn term(e: &Env, args: &[&str], cols: u16, rows: u16) -> Term {
    let mut cmd: Command = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, cols, rows)
}

fn ls(e: &Env) -> String {
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn criar_desanexar_reanexar_e_sair_com_codigo() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo marca-$((40+2))\r");
    t.wait_text("marca-42");
    // Ctrl+b d desanexa; a sessão continua.
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    t.wait_raw("sandmux attach 1");
    let list = ls(&e);
    assert!(
        list.contains("shell:app") && list.contains("viva"),
        "{list}"
    );

    let mut t = term(&e, &["attach", "1"], 100, 30);
    t.wait_text("marca-42");
    t.write(b"exit 3\r");
    assert_eq!(t.wait_exit(), 3);
    t.wait_raw("terminou com código 3");
    assert!(ls(&e).contains("encerrada (3)"), "{}", ls(&e));
}

/// PIDs de processos do host cuja linha de comando contém `marker`.
fn procs_with(marker: &str) -> Vec<i32> {
    let me = std::process::id() as i32;
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|d| d.file_name().to_str()?.parse::<i32>().ok())
        .filter(|&pid| pid != me)
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .map(|c| {
                    String::from_utf8_lossy(&c)
                        .replace('\0', " ")
                        .contains(marker)
                })
                .unwrap_or(false)
        })
        .collect()
}

fn wait_no_procs(marker: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let left = procs_with(marker);
        if left.is_empty() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "processos com {marker} continuam no host: {left:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn terminal_de_controle_e_ctrl_c() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"[ -t 0 ] && echo entrada-$((0+1))-terminal; sleep 100\r");
    t.wait_text("entrada-1-terminal");
    std::thread::sleep(std::time::Duration::from_millis(300));
    t.write(b"\x03");
    t.write(b"echo depois-$((1+1))\r");
    t.wait_text("depois-2");
    t.write(b"exit\r");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn descritores_do_daemon_nao_vazam_para_o_sandbox() {
    let e = Env::new();
    let mut a = term(&e, &["run", "shell"], 100, 30);
    a.write(b"echo sessao-$((10+1))\r");
    a.wait_text("sessao-11");
    a.write(b"\x02d");
    a.wait_exit();
    let mut b = term(&e, &["run", "shell"], 160, 40);
    b.write(b"ls -l /proc/$$/fd | grep -c . ; ls -l /proc/$$/fd | sed 's/.*-> //' | sort | tr '\\n' ' '; echo FIM-FD-$((2*2))\r");
    b.wait_text("FIM-FD-4");
    let s = b.screen();
    for bad in ["ptmx", "socket:", "memfd:", "sandmuxd"] {
        assert!(!s.contains(bad), "descritor {bad} vazou: {s}");
    }
    b.write(b"exit\r");
    b.wait_exit();
}

#[test]
fn ambiente_e_pasta_vem_do_cliente() {
    let e = Env::new();
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash").env("TERM", "xterm-kitty");
    let mut t = Term::spawn(cmd, 100, 30);
    t.write(b"echo T=$TERM P=$(pwd)\r");
    t.wait_text(&format!("T=xterm-kitty P={}", e.work.display()));
    t.write(b"exit 0\r");
    t.wait_exit();
}

#[test]
fn recusas_iguais_ao_run_direto_e_sem_sessao() {
    let e = Env::new();
    // Pasta perigosa: o HOME.
    let direct = e.run(&e.home, &["run", "shell", "--", "-c", "true"]);
    let mut t = Term::spawn(e.cmd(&e.home, &["run", "shell"]), 100, 30);
    assert_eq!(t.wait_exit(), 1);
    let msg = stderr(&direct);
    let msg = msg.lines().last().unwrap().trim();
    assert!(msg.contains("não pode ser usado"), "{msg}");
    t.wait_raw(msg);
    assert!(stdout(&e.run(&e.work, &["ls"])).contains("Não há sessões"));
    // Preset desconhecido não cria sessão.
    let mut t = Term::spawn(e.cmd(&e.work, &["run", "claude"]), 100, 30);
    assert_eq!(t.wait_exit(), 1);
    t.wait_raw("opencode, hermes, shell");
    assert!(stdout(&e.run(&e.work, &["ls"])).contains("Não há sessões"));
}

#[test]
fn aviso_de_pass_env_no_terminal() {
    let e = Env::new();
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--pass-env", "MINHA_CHAVE"]);
    cmd.env("SHELL", "/bin/bash").env("MINHA_CHAVE", "valor-k");
    let mut t = Term::spawn(cmd, 120, 30);
    t.wait_raw("--pass-env deixa ao alcance do agente: MINHA_CHAVE");
    t.write(b"echo K=$MINHA_CHAVE\r");
    t.wait_text("K=valor-k");
    t.write(b"exit\r");
    t.wait_exit();
    // O valor não foi para a lista de sessões.
    let saved = std::fs::read_to_string(e.data_dir().join("sessions.json")).unwrap();
    assert!(
        saved.contains("MINHA_CHAVE") && !saved.contains("valor-k"),
        "{saved}"
    );
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(e.data_dir().join("sessions.json"))
            .unwrap()
            .permissions(),
    );
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn listagem_viva_e_encerrada() {
    let e = Env::new();
    assert!(ls(&e).contains("Não há sessões"));
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"exit 2\r");
    assert_eq!(t.wait_exit(), 2);
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo viva-$((3*3))\r");
    t.wait_text("viva-9");
    t.write(b"\x02d");
    t.wait_exit();
    let list = ls(&e);
    let lines: Vec<&str> = list.lines().collect();
    assert!(
        lines[0].starts_with("ID") && lines[0].contains("ESTADO"),
        "{list}"
    );
    assert!(
        lines[1].starts_with("1 ") && lines[1].contains("encerrada (2)"),
        "{list}"
    );
    assert!(
        lines[2].starts_with("2 ") && lines[2].contains("viva"),
        "{list}"
    );
}

#[test]
fn kill_de_harness_que_ignora_term() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"trap '' TERM; sleep 4242\r");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while procs_with("sleep 4242").is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = e.run(&e.work, &["kill", "1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(t.wait_exit(), 0);
    t.wait_raw("encerrada por `sandmux kill`");
    wait_no_procs("sleep 4242");
    assert!(ls(&e).contains("Não há sessões"));
}

#[test]
fn sigkill_no_daemon_nao_deixa_sandbox() {
    let e = Env::new();
    for n in ["4343", "4444"] {
        let mut t = term(&e, &["run", "shell"], 100, 30);
        t.write(format!("sleep {n} & echo pronto-$((1+{n}))\r").as_bytes());
        let expected: u32 = n.parse::<u32>().unwrap() + 1;
        t.wait_text(&format!("pronto-{expected}"));
        t.write(b"\x02d");
        t.wait_exit();
    }
    assert_eq!(procs_with("sleep 4343").len(), 1);
    assert_eq!(procs_with("sleep 4444").len(), 1);
    let pid = e.daemon_pid().unwrap();
    // SAFETY: SIGKILL no daemon deste teste.
    unsafe { libc::kill(pid, libc::SIGKILL) };
    wait_no_procs("sleep 4343");
    wait_no_procs("sleep 4444");
}

#[test]
fn codigo_127_passa_igual() {
    let e = Env::new();
    let mut t = term(
        &e,
        &["run", "shell", "--", "-c", "comando-que-nao-existe"],
        100,
        30,
    );
    assert_eq!(t.wait_exit(), 127);
    assert!(ls(&e).contains("encerrada (127)"), "{}", ls(&e));
}

#[test]
fn historico_guarda_as_10000_linhas_mais_recentes() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 80, 30);
    t.write(b"seq 1 12000; echo FIM-SEQ-$((1+1))\r");
    t.wait_text("FIM-SEQ-2");
    t.write(b"\x02d");
    t.wait_exit();
    let mut t = term(&e, &["attach", "1"], 80, 30);
    t.wait_text("FIM-SEQ-2");
    let raw = t.raw_text();
    assert!(
        raw.contains("\r\n2500\r\n") && raw.contains("\r\n11999\r\n"),
        "faltam linhas recentes"
    );
    assert!(
        !raw.contains("\r\n1500\r\n"),
        "linha antiga demais foi guardada"
    );
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn saida_da_sessao_nunca_vai_para_o_disco() {
    let e = Env::new();
    let mut t = term(&e, &["run", "shell"], 100, 30);
    t.write(b"echo MARCA-$((7*6))-SECRETA\r");
    t.wait_text("MARCA-42-SECRETA");
    t.write(b"\x02d");
    t.wait_exit();
    let mut t = term(&e, &["attach", "1"], 100, 30);
    t.wait_text("MARCA-42-SECRETA");
    t.write(b"exit\r");
    t.wait_exit();
    for dir in [e.data_dir(), e.runtime.clone(), e.state.clone()] {
        let out = std::process::Command::new("grep")
            .args(["-r", "-l", "MARCA-42-SECRETA"])
            .arg(&dir)
            .output()
            .unwrap();
        assert!(
            out.stdout.is_empty(),
            "o marcador apareceu em {}: {}",
            dir.display(),
            stdout(&out)
        );
    }
    assert!(
        e.state.join("sandmux/sandmuxd.log").exists(),
        "o log do daemon existe e foi verificado"
    );
}
