//! A TUI: abas, atalhos, modais, mouse, colagem; e o renomear de sessões.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::Path;

fn term_in(e: &Env, cwd: &Path, args: &[&str]) -> Term {
    let mut cmd = e.cmd(cwd, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn_answering(cmd, 100, 30)
}

fn ls(e: &Env) -> String {
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

/// Cria uma sessão do shell e desanexa.
fn sessao(e: &Env, cwd: &Path) {
    let mut t = term_in(e, cwd, &["run", "shell"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

// ---- renomear ----

#[test]
fn renomear_sessao_sobrevive_ao_reinicio_do_daemon() {
    let e = Env::new();
    sessao(&e, &e.work);
    let out = e.run(&e.work, &["rename", "1", "api"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        ls(&e)
            .lines()
            .any(|l| l.starts_with("1 ") && l.contains(" api ")),
        "{}",
        ls(&e)
    );
    let out = e.run(&e.work, &["daemon", "restart"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        ls(&e)
            .lines()
            .any(|l| l.starts_with("1 ") && l.contains(" api ")),
        "{}",
        ls(&e)
    );
    let out = e.run(&e.work, &["rename", "1", ""]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("vazio"), "{}", stderr(&out));
    let longo = "x".repeat(41);
    assert_eq!(
        e.run(&e.work, &["rename", "1", &longo]).status.code(),
        Some(1)
    );
    assert!(ls(&e).contains(" api "));
}

// ---- TUI ----

fn tui(e: &Env) -> Term {
    term_in(e, &e.work, &[])
}

#[test]
fn tui_sem_terminal_recusa() {
    let e = Env::new();
    let out = e.run(&e.work, &["tui"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("terminal interativo"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn tui_abre_com_as_abas_e_repassa_a_entrada() {
    let e = Env::new();
    sessao(&e, &e.work);
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.wait_text("2:shell:outra");
    t.write(b"echo ola-$((2+3))\r");
    t.wait_text("ola-5");
    // Ctrl+b n vai para a aba 2; o texto da aba 1 some da tela.
    t.write(b"\x02n");
    t.write(b"echo segunda-$((1+1))\r");
    t.wait_text("segunda-2");
    t.with_screen(|s| assert!(!s.contents().contains("ola-5"), "{}", s.contents()));
    // E volta, sem esperar redesenho do shell.
    t.write(b"\x021");
    t.wait_text("ola-5");
    // Cor verdadeira chega à tela.
    t.write(b"printf '\\033[38;2;10;200;30mVERDE\\033[0m\\n'\r");
    t.wait_text("VERDE");
    t.with_screen(|s| {
        let found = (0..30).any(|r| {
            (0..100).any(|c| {
                s.cell(r, c).is_some_and(|x| {
                    x.contents() == "V" && x.fgcolor() == vt100::Color::Rgb(10, 200, 30)
                })
            })
        });
        assert!(found, "{}", s.contents());
    });
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    assert!(
        t.raw_text().contains("\x1b[?1049l"),
        "a tela alternativa não foi desfeita"
    );
    assert!(
        ls(&e).lines().filter(|l| l.contains("viva")).count() == 2,
        "{}",
        ls(&e)
    );
}

#[test]
fn modos_da_aba_vao_e_voltam_do_terminal() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.write(b"printf '\\033[>1u\\033[?2004h'; echo modos-$((3+4))\r");
    t.wait_text("modos-7");
    t.wait_raw("\x1b[>1u");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    let raw = t.raw_text();
    let fim = &raw[raw.rfind("\x1b[>1u").unwrap()..];
    for s in [
        "\x1b[<1u",
        "\x1b[?2004l",
        "\x1b[?1002l",
        "\x1b[?1006l",
        "\x1b[?1049l",
    ] {
        assert!(fim.contains(s), "{s:?} não desfeito: {fim:?}");
    }
}

#[test]
fn atalhos_prefixo_e_tecla_desconhecida() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    // Ctrl+b Ctrl+b manda um Ctrl+b; `cat -v` mostra ^B. Ctrl+b z some.
    t.write(b"cat -v\r");
    t.write(b"\x02\x02\x02zX\r");
    t.wait_text("^BX");
    t.write(b"\x03");
    t.write(b"\x02?");
    t.wait_text("Tecla de prefixo: Ctrl+b");
    t.write(b"\x1b");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn nova_aba_por_modal_e_pasta_perigosa() {
    let e = Env::new();
    let outra = e.home.join("projetos/nova");
    std::fs::create_dir_all(&outra).unwrap();
    let mut t = tui(&e);
    // Sem sessões, o modal abre sozinho.
    t.wait_text("Nova aba");
    // Pasta perigosa: o HOME.
    t.write(b"\t");
    t.write(&[0x7f; 200]);
    t.write(e.home.to_str().unwrap().as_bytes());
    t.write(b"\r");
    t.wait_text("HOME do usu");
    assert!(ls(&e).contains("Não há sessões"));
    t.write(&[0x7f; 200]);
    t.write(outra.to_str().unwrap().as_bytes());
    t.write(b"\r");
    t.wait_text("1:shell:nova");
    t.write(b"pwd\r");
    t.wait_text(&format!("{}", outra.display()));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn lista_renomear_fechar() {
    let e = Env::new();
    sessao(&e, &e.work);
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("2:shell:app");
    t.write(b"\x02w");
    t.wait_text("Abas");
    t.write(b"\x1b[A\x1b[A\r");
    t.write(b"\x02,");
    t.wait_text("Renomear");
    t.write(&[0x7f; 30]);
    t.write(b"api\r");
    t.wait_text("1:api");
    assert!(
        ls(&e)
            .lines()
            .any(|l| l.starts_with("1 ") && l.contains(" api ")),
        "{}",
        ls(&e)
    );
    t.write(b"\x02x");
    t.wait_text("Encerrar a sess");
    t.write(b"s");
    t.wait_for("aba 1 fechada", |s| !s.contains("1:api"));
    assert!(!ls(&e).lines().any(|l| l.starts_with("1 ")), "{}", ls(&e));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn aba_encerrada_continua_e_reinicia() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.write(b"echo antes-$((1+1)); exit 3\r");
    t.wait_text("encerrada (3)");
    t.wait_text("antes-2");
    t.write(b"\x02r");
    t.wait_text("aba 1 reiniciada");
    t.write(b"echo depois-$((2+2))\r");
    t.wait_text("depois-4");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn abas_seguem_o_daemon() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    sessao(&e, &e.work);
    t.wait_text("2:shell:app");
    let out = e.run(&e.work, &["kill", "1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    // Sobra uma aba só, que passa a ser a posição 1.
    t.wait_for("aba 1 some", |s| {
        !s.contains("2:shell:app") && s.contains("1:shell:app")
    });
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn mouse_clique_repasse_e_selecao() {
    let e = Env::new();
    sessao(&e, &e.work);
    let outra = e.home.join("projetos/b");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);
    let mut t = tui(&e);
    t.wait_text("2:shell:b");
    // Clique na aba 2 (coluna depois de " 1:shell:app ").
    t.write(b"\x1b[<0;17;1M\x1b[<0;17;1m");
    t.write(b"echo aqui-b-$((1+1))\r");
    t.wait_text("aqui-b-2");
    // Seleção sem mouse do harness: arrastar sobre "aqui-b-2" copia por OSC 52.
    let pos = t.with_screen(|s| {
        (0..30u16).find_map(|r| {
            let line = s.contents_between(r, 0, r, 100);
            line.find("aqui-b-2")
                .filter(|_| !line.contains("echo"))
                .map(|c| (r, c as u16))
        })
    });
    let (r, c) = pos.expect("texto na tela");
    let (y, x) = (r + 1, c + 1);
    t.write(
        format!(
            "\x1b[<0;{x};{y}M\x1b[<32;{};{y}M\x1b[<0;{};{y}m",
            x + 7,
            x + 7
        )
        .as_bytes(),
    );
    t.wait_raw("\x1b]52;c;YXF1aS1iLTI=\x07");
    // Com o mouse ligado pelo harness, o clique vai para ele, com a linha da área.
    t.write(b"printf '\\033[?1000h\\033[?1006h'; cat -v\r");
    std::thread::sleep(std::time::Duration::from_millis(500));
    t.write(b"\x1b[<0;5;4M");
    t.wait_text("^[[<0;5;3M");
    t.write(b"\x03");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn colar_imagem_sem_clipboard_avisa() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.write(b"\x02v");
    t.wait_text("clipboard");
    assert!(
        std::fs::read_dir(e.data_dir().join("inbox"))
            .map(|d| d.count())
            .unwrap_or(0)
            == 0
    );
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn tema_muda_as_cores_da_barra() {
    let e = Env::new();
    sessao(&e, &e.work);
    // Sem tema: a aba ativa em vídeo inverso.
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.with_screen(|s| assert!(s.cell(0, 1).unwrap().inverse(), "{}", s.contents()));
    t.write(b"\x02d");
    t.wait_exit();
    e.write_config("[tui]\ntheme = \"escuro\"\n[tui.colors]\ntab-active-bg = \"#aa0000\"\n");
    let mut t = tui(&e);
    t.wait_text("1:shell:app");
    t.with_screen(|s| {
        let c = s.cell(0, 1).unwrap();
        assert_eq!(
            c.bgcolor(),
            vt100::Color::Rgb(0xaa, 0, 0),
            "{}",
            s.contents()
        );
        assert!(!c.inverse());
    });
    t.write(b"\x02d");
    t.wait_exit();
}

// ---- painel lateral ----

fn tui_cols(e: &Env, cols: u16) -> Term {
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn_answering(cmd, cols, 30)
}

#[test]
fn painel_lateral_mostra_protecao_e_some_com_atalho() {
    let e = Env::new();
    let docs = e.home.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    let mut cmd = e.cmd(
        &e.work,
        &[
            "run",
            "shell",
            "--ro",
            docs.to_str().unwrap(),
            "--pass-env",
            "CHAVE_PAINEL",
        ],
    );
    cmd.env("SHELL", "/bin/bash").env("CHAVE_PAINEL", "v");
    let mut s = Term::spawn(cmd, 100, 30);
    s.write(b"echo pronto-$((1+1))\r");
    s.wait_text("pronto-2");
    s.write(b"\x02d");
    s.wait_exit();

    let mut t = tui_cols(&e, 120);
    t.wait_text("Proteção");
    t.wait_text("docs (leitura)");
    t.wait_text("CHAVE_PAINEL");
    t.wait_text("Recursos");
    t.write(b"stty size\r");
    t.wait_text("29 86");
    t.write(b"\x02s");
    t.wait_for("painel some", |s| !s.contains("Proteção"));
    t.write(b"clear; stty size\r");
    t.wait_text("29 120");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // Terminal estreito: sem painel.
    let mut t = tui_cols(&e, 80);
    t.wait_text("1:shell:app");
    std::thread::sleep(std::time::Duration::from_millis(500));
    t.with_screen(|s| assert!(!s.contents().contains("Proteção"), "{}", s.contents()));
    t.write(b"\x02d");
    t.wait_exit();

    // O mesmo resumo no `sandmux stats`.
    let out = e.run(&e.work, &["stats", "1"]);
    let st = stdout(&out);
    assert!(
        st.contains("Proteção do último lançamento")
            && st.contains(&format!("extra: {} (leitura)", docs.display()))
            && st.contains("CHAVE_PAINEL (ao alcance do agente)")
            && st.contains("rede: aberta, registrada"),
        "{st}"
    );
}

#[test]
fn rede_no_painel_e_marca_de_dominio_novo_na_barra() {
    use std::io::{Read, Write};
    let e = Env::new();
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in tcp.incoming() {
            let Ok(mut c) = c else { return };
            let mut buf = [0u8; 256];
            let _ = c.read(&mut buf);
            let _ = c.write_all(b"HTTP/1.0 200 OK\r\n\r\nok");
        }
    });
    sessao(&e, &e.work);
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);

    let mut t = tui_cols(&e, 120);
    t.wait_text("2:shell:outra");
    t.wait_text("Rede");
    t.wait_text("sem conexões registradas");
    // A aba 1 acessa um domínio novo depois que a aba 2 fica ativa.
    t.write(
        format!("(sleep 2; curl -s http://host.sandmux.internal:{port}/ >/dev/null) &\r")
            .as_bytes(),
    );
    t.write(b"\x022");
    t.wait_text("1:shell:app •");
    t.with_screen(|s| {
        assert!(
            !s.contents().contains("2:shell:outra •"),
            "{}",
            s.contents()
        )
    });
    // Ao voltar, a marca some e o painel mostra o domínio, marcado como novo.
    t.write(b"\x021");
    t.wait_for("marca some", |s| !s.contains("1:shell:app •"));
    t.wait_text("host.sandmux.internal");
    t.wait_text("novo");
    t.wait_text("modo: aberta, registrada");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

fn ok_server() -> u16 {
    use std::io::{Read, Write};
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in tcp.incoming() {
            let Ok(mut c) = c else { return };
            let mut buf = [0u8; 256];
            let _ = c.read(&mut buf);
            let _ = c.write_all(b"HTTP/1.0 200 OK\r\n\r\nok");
        }
    });
    port
}

/// Roda o curl na aba ativa e espera o código de saída.
fn curl_tab(t: &mut Term, port: u16, n: u32) -> String {
    t.write(
        format!(
            "curl -s -m 5 http://host.sandmux.internal:{port}/ >/dev/null; echo R$(({n}+0))=$?\r"
        )
        .as_bytes(),
    );
    let marker = format!("R{n}=");
    t.wait_text(&marker);
    t.with_screen(|s| {
        let c = s.contents();
        let i = c.rfind(&marker).unwrap() + marker.len();
        c[i..].chars().take_while(|c| c.is_ascii_digit()).collect()
    })
}

#[test]
fn marca_de_bloqueio_na_barra() {
    let e = Env::new();
    let port = ok_server();
    let mut t = term_in(
        &e,
        &e.work,
        &["run", "shell", "--deny", "host.sandmux.internal"],
    );
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    t.wait_exit();
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);

    let mut t = tui_cols(&e, 120);
    t.wait_text("2:shell:outra");
    t.write(
        format!("(sleep 2; curl -s http://host.sandmux.internal:{port}/ >/dev/null) &\r")
            .as_bytes(),
    );
    t.write(b"\x022");
    t.wait_text("1:shell:app !");
    t.write(b"\x021");
    t.wait_for("marca some", |s| !s.contains("1:shell:app !"));
    t.wait_text("bloqueado:");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn tela_de_rede_troca_modo_libera_no_projeto_e_nega_na_aba() {
    let e = Env::new();
    let port = ok_server();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 120);
    t.wait_text("1:shell:app");
    assert_eq!(curl_tab(&mut t, port, 11), "0");

    // Trocar o modo: aberta -> inferencia -> custom.
    t.write(b"\x02a");
    t.wait_text("Rede da aba");
    t.write(b"m");
    t.wait_text("modo da aba: inferencia");
    t.write(b"m");
    t.wait_text("modo da aba: custom");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Rede da aba"));
    t.wait_text("modo: custom (allowlist)");
    assert_eq!(curl_tab(&mut t, port, 12), "6");

    // Liberar no projeto o bloqueio mais recente (a consulta de DNS).
    t.write(b"\x02a");
    t.wait_text("bloqueado: fora da allowlist");
    t.write(b"l");
    t.wait_text("liberar host.sandmux.internal em:");
    t.write(b"p");
    t.wait_text("host.sandmux.internal liberado em");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Rede da aba"));
    let cfg = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(cfg.contains("allow = [\"host.sandmux.internal\"]"), "{cfg}");
    assert!(
        cfg.contains(&format!("[project.\"{}\"]", e.work.display())),
        "{cfg}"
    );
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(curl_tab(&mut t, port, 13), "0");

    // Negar na aba o acesso mais recente.
    t.write(b"\x02a");
    t.wait_text("Rede da aba");
    t.write(b"n");
    t.wait_text("negar host.sandmux.internal em:");
    t.write(b"a");
    t.wait_text("host.sandmux.internal negado na aba");
    // As regras mostram a origem.
    t.write(b"\t");
    t.wait_text("catálogo de exfiltração");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Rede da aba"));
    assert_eq!(curl_tab(&mut t, port, 14), "6");
    let rules = stdout(&e.run(&e.work, &["net", "rules", "1"]));
    assert!(
        rules
            .lines()
            .any(|l| l.contains("host.sandmux.internal") && l.ends_with("aba")),
        "{rules}"
    );
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn portas_no_stats_e_no_painel() {
    let e = Env::new();
    let p = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 120);
    t.wait_text("1:shell:app");
    t.wait_text("Portas");
    t.write(format!("python3 -m http.server {p} --bind 0.0.0.0 >/dev/null 2>&1 &\r").as_bytes());
    t.wait_text(&format!("{p} → 127.0.0.1:{p}"));
    let st = stdout(&e.run(&e.work, &["stats", "1"]));
    assert!(
        st.contains(&format!("{p} → 127.0.0.1:{p} (sandbox)")),
        "{st}"
    );
    t.write(b"kill %1\r");
    t.wait_for("porta some", |s| !s.contains(&format!("{p} → 127.0.0.1")));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn allowlist_de_portas_muda_ao_vivo() {
    let e = Env::new();
    let free = || {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };
    let (p, other) = (free(), free());
    let o = e.run(
        &e.work,
        &["config", "add", "ports", &other.to_string(), "--global"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let mut t = term_in(&e, &e.work, &["run", "shell"]);
    t.write(
        format!(
            "python3 -m http.server {p} --bind 0.0.0.0 >/dev/null 2>&1 & echo subiu-$((1+1))\r"
        )
        .as_bytes(),
    );
    t.wait_text("subiu-2");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let st = stdout(&e.run(&e.work, &["stats", "1"]));
    assert!(
        !st.contains(&format!("{p} →")),
        "publicada fora da allowlist: {st}"
    );
    // Liberada com o agente rodando: publicada em até 2 segundos.
    let o = e.run(
        &e.work,
        &[
            "config",
            "add",
            "ports",
            &format!("{p}@0.0.0.0"),
            "--global",
        ],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(4);
    let mut st = String::new();
    while std::time::Instant::now() < end {
        st = stdout(&e.run(&e.work, &["stats", "1"]));
        if st.contains(&format!("{p} → 0.0.0.0:{p} (sandbox)")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    // Com `@0.0.0.0`, o endereço aparece no `stats`.
    assert!(st.contains(&format!("{p} → 0.0.0.0:{p} (sandbox)")), "{st}");
    t.write(b"kill %1; exit\r");
    let _ = t.wait_exit();
}

#[test]
fn tela_de_arquivos_bloqueia_o_caminho() {
    let e = Env::new();
    std::fs::write(e.work.join("dados.sql"), "conteudo-sql").unwrap();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 120);
    t.wait_text("1:shell:app");
    t.write(b"cat dados.sql; echo; echo L$((1+1))=ok\r");
    t.wait_text("L2=ok");

    t.write(b"\x02f");
    t.wait_text("Acessos a arquivos da sess");
    t.wait_text("dados.sql");
    t.with_screen(|s| assert!(s.contents().contains("•"), "{}", s.contents()));
    // O mais recente vem primeiro: escolhe a linha do dados.sql.
    let idx = t.with_screen(|s| {
        let c = s.contents();
        let rows: Vec<&str> = c
            .lines()
            .skip_while(|l| !l.contains("Acessos a arquivos"))
            .skip(1)
            .take_while(|l| !l.trim().is_empty() && !l.contains("b bloqueia"))
            .collect();
        rows.iter().position(|l| l.contains("dados.sql")).unwrap()
    });
    for _ in 0..idx {
        t.write(b"j");
    }
    t.write(b"b");
    t.wait_text("negado na aba");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Acessos a arquivos"));
    t.write(b"clear; sleep 0.3; cat dados.sql 2>&1 | head -c 50; echo; echo L$((2+2))=ok\r");
    t.wait_text("L4=ok");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(
            !c.contains("conteudo-sql")
                && (c.contains("Permission denied") || c.contains("Permissão negada")),
            "{c}"
        );
    });
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn tela_de_pastas_monta_ao_vivo() {
    let e = Env::new();
    let docs = e.dir.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("leia.txt"), "x").unwrap();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 160);
    t.wait_text("1:shell:app");
    t.write(b"\x02o");
    t.wait_text("Pastas da sess");
    t.write(b"a");
    t.wait_text("pasta:");
    t.write(docs.to_str().unwrap().as_bytes());
    t.write(b"\r");
    t.wait_text("r só leitura");
    t.write(b"r");
    t.wait_text("montada");
    t.wait_text("/mnt/sandmux/docs");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Pastas da sess"));
    t.write(b"ls /mnt/sandmux/docs; echo L$((1+1))=ok\r");
    t.wait_text("L2=ok");
    t.with_screen(|s| assert!(s.contents().contains("leia.txt"), "{}", s.contents()));
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn tela_de_mcp_desabilita_na_aba() {
    use std::os::unix::fs::PermissionsExt;
    let e = Env::new();
    // Um "opencode" que é só um shell, e os MCPs no HOME sintético dele.
    let exe = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "#!/bin/sh\nexec /bin/bash\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let oc = e.data_dir().join("homes/opencode/.config/opencode");
    std::fs::create_dir_all(&oc).unwrap();
    std::fs::write(
        oc.join("opencode.json"),
        r#"{"mcp": {"github": {"type": "remote", "url": "https://gh.test/mcp"}, "fs": {"type": "local", "command": ["fs-mcp"]}}}"#,
    )
    .unwrap();
    let mut t = term_in(&e, &e.work, &["run", "opencode", "--no-import"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // O painel tem a seção MCP no fim: um terminal mais alto.
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn_answering(cmd, 140, 60);
    t.wait_text("1:opencode:app");
    t.wait_text("✓ github");
    t.write(b"\x02m");
    t.wait_text("Servidores MCP da sess");
    t.write(b" ");
    t.wait_text("github desabilitado");
    t.write(b"\x1b");
    t.wait_for("tela fecha", |s| !s.contains("Servidores MCP da sess"));
    t.wait_text("✗ github");
    let out = stdout(&e.run(&e.work, &["mcp", "list", "1"]));
    assert!(
        out.lines()
            .any(|l| l.starts_with("desabilitado") && l.contains("github")),
        "{out}"
    );
    t.write(b"\x02d");
    t.wait_exit();
}
