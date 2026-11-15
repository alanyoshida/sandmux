//! Janelas do desktop (`window`), com um compositor do Wayland falso.

mod common;
use common::cli::*;
use common::pty::Term;
use common::wayland::{Fake, LIST_GLOBALS, MANAGER};

const GLOBALS: &[&str] = &[
    "wl_compositor",
    "wl_shm",
    "ext_data_control_manager_v1",
    "zwlr_screencopy_manager_v1",
    MANAGER,
];

fn term(e: &Env, fake: &Fake, args: &[&str]) -> Term {
    let mut cmd = e.cmd(&e.work, args);
    cmd.env("SHELL", "/bin/bash")
        .env("WAYLAND_DISPLAY", &fake.path);
    Term::spawn(cmd, 160, 30)
}

fn tui(e: &Env, fake: &Fake) -> Term {
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash")
        .env("WAYLAND_DISPLAY", &fake.path);
    Term::spawn_answering(cmd, 160, 30)
}

fn python() -> bool {
    std::path::Path::new("/usr/bin/python3").is_file()
}

#[test]
fn janela_restrita_esconde_os_privilegiados_e_marca_a_aba() {
    if !python() {
        eprintln!("sem python3: teste pulado");
        return;
    }
    let e = Env::new();
    e.write_config("[tui]\npanel = false\n");
    std::fs::write(e.work.join("lista.py"), LIST_GLOBALS).unwrap();
    let fake = Fake::start(&e.runtime, GLOBALS);
    let mut t = term(&e, &fake, &["run", "shell", "--window"]);
    t.wait_raw("o agente pode abrir janelas");
    t.write(b"python3 lista.py; echo disp=${DISPLAY:-nenhum}\r");
    t.wait_text("globais:wl_compositor,wl_shm:fim");
    t.wait_text("disp=nenhum");
    t.write(b"test -S $XDG_RUNTIME_DIR/pulse/native || echo sem-pulse\r");
    t.wait_text("sem-pulse");
    {
        let s = fake.seen.lock().unwrap();
        assert_eq!(s.engine.as_deref(), Some("sandmux"));
        assert_eq!(s.app_id.as_deref(), Some("sandmux"));
        assert!(s.instance.as_deref().is_some_and(|i| i.starts_with("s1-")));
        assert!(s.sandboxed >= 1);
    }
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // A marca na barra da TUI.
    let mut t = tui(&e, &fake);
    t.wait_text("shell:app ▣");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // Fechada a aba, os sockets da janela somem.
    assert!(e.run(&e.work, &["kill", "1"]).status.success());
    let dir = e.runtime.join("sandmux/window");
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0) > 0 {
        assert!(std::time::Instant::now() < end, "sockets que ficaram");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[test]
fn modo_direto_usa_o_socket_comum() {
    if !python() {
        return;
    }
    let e = Env::new();
    std::fs::write(e.work.join("lista.py"), LIST_GLOBALS).unwrap();
    let fake = Fake::start(&e.runtime, GLOBALS);
    let mut t = term(&e, &fake, &["run", "shell", "--window-direct"]);
    t.wait_raw("ler o clipboard");
    t.write(b"python3 lista.py\r");
    t.wait_text("ext_data_control_manager_v1,zwlr_screencopy_manager_v1");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    assert!(fake.seen.lock().unwrap().engine.is_none());
}

#[test]
fn sem_o_protocolo_ou_sem_terminal_recusa() {
    let e = Env::new();
    let fake = Fake::start(&e.runtime, &["wl_compositor", "wl_shm"]);
    let mut t = term(&e, &fake, &["run", "shell", "--window"]);
    t.wait_raw("wp_security_context_v1");
    t.wait_raw("--window-direct");
    assert_ne!(t.wait_exit(), 0);

    let out = e
        .cmd(&e.work, &["run", "shell", "--window", "--", "-c", "true"])
        .env("WAYLAND_DISPLAY", &fake.path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("abas do daemon"), "{}", stderr(&out));

    // Sem a opção, nada do desktop entra.
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--",
                "-c",
                "echo wl=${WAYLAND_DISPLAY:-nenhum}",
            ],
        )
        .env("WAYLAND_DISPLAY", &fake.path)
        .output()
        .unwrap();
    assert!(stdout(&out).contains("wl=nenhum"), "{}", stdout(&out));
}

#[test]
fn chave_window_na_configuracao() {
    let e = Env::new();
    let set = |v: &str| e.run(&e.work, &["config", "set", "--global", "window", v]);
    assert!(set("direct").status.success());
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(text.contains("window = \"direct\""), "{text}");
    assert!(set("sim").status.success());
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(text.contains("window = true"), "{text}");
    let out = set("talvez");
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("`window` precisa ser"),
        "{}",
        stderr(&out)
    );
    e.write_config("[global]\nwindow = \"sim\"\n");
    let out = e.run(&e.work, &["config", "show"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("`window` precisa ser"),
        "{}",
        stderr(&out)
    );
}
