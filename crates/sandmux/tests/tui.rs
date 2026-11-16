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
    t.wait_text(&tab(1, "shell:app"));
    t.wait_text(&tab(2, "shell:outra"));
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "shell:nova"));
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
    t.wait_text(&tab(2, "shell:app"));
    t.write(b"\x02w");
    t.wait_text("Abas");
    t.write(b"\x1b[A\x1b[A\r");
    t.write(b"\x02,");
    t.wait_text("Renomear");
    t.write(&[0x7f; 30]);
    t.write(b"api\r");
    t.wait_text(&tab(1, "api"));
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
    t.wait_for("aba 1 fechada", |s| !s.contains(&tab(1, "api")));
    assert!(!ls(&e).lines().any(|l| l.starts_with("1 ")), "{}", ls(&e));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn aba_encerrada_continua_e_reinicia() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "shell:app"));
    sessao(&e, &e.work);
    t.wait_text(&tab(2, "shell:app"));
    let out = e.run(&e.work, &["kill", "1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    // Sobra uma aba só, que passa a ser a posição 1.
    t.wait_for("aba 1 some", |s| {
        !s.contains(&tab(2, "shell:app")) && s.contains(&tab(1, "shell:app"))
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
    t.wait_text(&tab(2, "shell:b"));
    // Clique na aba 2, na barra da última linha (a aba 1 ocupa 16 colunas:
    // separador, " 1 ", separador e " shell:app ").
    t.write(b"\x1b[<0;18;30M\x1b[<0;18;30m");
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
    // Com a barra embaixo, a área começa na linha 1: a linha não muda.
    t.write(b"\x1b[<0;5;4M");
    t.wait_text("^[[<0;5;4M");
    t.write(b"\x03");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn colar_imagem_sem_clipboard_avisa() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
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
    // Padrão: barra em blocos na última linha, sem símbolos de Nerd Font, o
    // `≡` do menu e o número da aba ativa no bloco lilás.
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    t.with_screen(|s| {
        let last = s.size().0.get() - 1;
        assert_eq!(s.cell(last, 1).unwrap().contents(), "≡");
        let bar: String = (0..s.size().1.get())
            .map(|c| s.cell(last, c).unwrap().contents().to_string())
            .collect();
        assert!(
            !bar.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)),
            "{bar}"
        );
        let c = s.cell(last, 5).unwrap();
        assert_eq!(c.contents(), "1", "{}", s.contents());
        assert_eq!(c.bgcolor(), vt100::Color::Rgb(0xcb, 0xa6, 0xf7));
    });
    t.write(b"\x02d");
    t.wait_exit();
    // O tema e a cor trocada pela configuração valem no bloco.
    e.write_config("[tui]\ntheme = \"escuro\"\n[tui.colors]\ntab-active-index-bg = \"#aa0000\"\n");
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    t.with_screen(|s| {
        let last = s.size().0.get() - 1;
        let c = s.cell(last, 5).unwrap();
        assert_eq!(
            c.bgcolor(),
            vt100::Color::Rgb(0xaa, 0, 0),
            "{}",
            s.contents()
        );
    });
    t.write(b"\x02d");
    t.wait_exit();
    // Estilo simples no topo: a barra de antes, a aba ativa em vídeo inverso.
    e.write_config("[tui]\nstyle = \"simples\"\nbar = \"top\"\n");
    let mut t = tui(&e);
    t.wait_text(" 1:shell:app ");
    t.with_screen(|s| {
        assert!(s.cell(0, 1).unwrap().inverse(), "{}", s.contents());
        assert!(!s.contents().contains('\u{e0bc}'));
        assert!(!s.contents().contains('≡'));
    });
    t.write(b"\x02d");
    t.wait_exit();
    // Powerline, para quem tem a fonte.
    e.write_config("[tui]\nstyle = \"powerline\"\n");
    let mut t = tui(&e);
    t.wait_text(" 1 \u{e0bc} shell:app");
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn secoes_do_painel_recolhem_e_mudam_de_ordem() {
    let e = Env::new();
    sessao(&e, &e.work);
    // 120 colunas: o painel começa na coluna 87 (1-based), o texto na 89 e
    // as setas da faixa nas colunas 117 (↑) e 119 (↓).
    let mut t = tui_cols(&e, 120);
    t.wait_text("▾ Proteção");
    t.wait_text(" pico ");
    // ↓ na faixa de Proteção (linha 1): Recursos sobe para o topo.
    t.write(b"\x1b[<0;119;1M\x1b[<0;119;1m");
    t.wait_for("Recursos no topo", |s| {
        s.lines().next().is_some_and(|l| l.contains("▾ Recursos"))
    });
    t.wait_for("ordem gravada", |_| {
        std::fs::read_to_string(e.config_file())
            .unwrap_or_default()
            .contains(r#"panel-order = ["recursos", "protecao", "portas", "rede", "mcp"]"#)
    });
    // Clique no título: Recursos recolhe, e a linha do pico some.
    t.write(b"\x1b[<0;92;1M\x1b[<0;92;1m");
    t.wait_text("▸ Recursos");
    t.wait_for("pico some", |s| !s.contains(" pico "));
    t.wait_for("recolhida gravada", |_| {
        std::fs::read_to_string(e.config_file())
            .unwrap_or_default()
            .contains(r#"panel-collapsed = ["recursos"]"#)
    });
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    // Ao abrir de novo, a ordem e a seção recolhida continuam.
    let mut t = tui_cols(&e, 120);
    t.wait_text("▸ Recursos");
    t.with_screen(|s| {
        assert!(s.contents().lines().next().unwrap().contains("▸ Recursos"));
        assert!(!s.contents().contains(" pico "));
    });
    // Expandir e voltar à ordem padrão apaga as chaves.
    t.write(b"\x1b[<0;92;1M\x1b[<0;92;1m");
    t.wait_text(" pico ");
    t.write(b"\x1b[<0;119;1M\x1b[<0;119;1m");
    t.wait_for("Proteção no topo", |s| {
        s.lines().next().is_some_and(|l| l.contains("▾ Proteção"))
    });
    t.wait_for("chaves apagadas", |_| {
        !std::fs::read_to_string(e.config_file())
            .unwrap_or_default()
            .contains("panel-")
    });
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

// ---- menu e configurações ----

const UP: &[u8] = b"\x1b[A";
const DOWN: &[u8] = b"\x1b[B";
const RIGHT: &[u8] = b"\x1b[C";
const LEFT: &[u8] = b"\x1b[D";

#[test]
fn menu_abre_as_configuracoes_e_a_pagina_grava_o_toml() {
    let e = Env::new();
    e.write_config("# do usuário\n[global]\nrepo = false\n");
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    // O menu lista as ações; a última é configurações.
    t.write(b"\x02:");
    t.wait_text("Menu");
    t.wait_text("configurações");
    // "configurações" é a antepenúltima (depois vêm o diagnóstico e o
    // shell de dentro).
    t.write(UP);
    t.write(UP);
    t.write(UP);
    t.write(b"\r");
    t.wait_text("sandmux · configurações");
    t.wait_text("valendo em: global");
    // Sandbox: liga o KVM.
    t.write(DOWN);
    t.write(DOWN);
    t.write(DOWN);
    t.write(b"\t");
    t.write(DOWN);
    t.wait_text("Espaço liga e desliga");
    t.write(b" ");
    t.wait_text("kvm gravado em [global]");
    // Com o valor gravado no alvo, o rodapé oferece voltar ao padrão.
    t.wait_text("x volta ao padrão");
    // A sessão criada antes continua com o valor antigo.
    t.wait_text("1 aba(s) viva(s) com o valor antigo");
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(text.starts_with("# do usuário"), "{text}");
    assert!(text.contains("kvm = true"), "{text}");
    t.wait_text("● ligado");
    // Memória recusada: nada muda (depois de GPU, janela e clipboard).
    for _ in 0..4 {
        t.write(DOWN);
    }
    t.write(b"\r");
    t.write(b"abc\r");
    t.wait_text("\"abc\"");
    assert_eq!(std::fs::read_to_string(e.config_file()).unwrap(), text);
    // Rede: acrescenta uma porta do host.
    t.write(b"\t");
    t.write(DOWN);
    t.write(b"\t");
    for _ in 0..8 {
        t.write(DOWN);
    }
    t.write(b"\r");
    t.wait_text("a acrescenta");
    t.write(b"a8888\r");
    t.wait_text("host-ports gravado");
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(text.contains("host-ports = [8888]"), "{text}");
    t.write(LEFT);
    // Aparência: o tema muda na hora, na página e na barra.
    t.write(b"\t");
    for _ in 0..3 {
        t.write(UP);
    }
    t.write(b"\t");
    for _ in 0..5 {
        t.write(RIGHT);
    }
    t.wait_text("‹ tokyo-night ›");
    t.wait_for("tema gravado", |_| {
        std::fs::read_to_string(e.config_file())
            .unwrap()
            .contains("theme = \"tokyo-night\"")
    });
    t.with_screen(|s| {
        let c = s.cell(0, 0).unwrap();
        assert_eq!(c.contents(), "╭");
        assert_eq!(c.fgcolor(), vt100::Color::Rgb(0x7a, 0xa2, 0xf7));
    });
    t.write(b"q");
    t.wait_for("página fecha", |s| !s.contains("sandmux · configurações"));
    t.with_screen(|s| {
        let last = s.size().0.get() - 1;
        let c = s.cell(last, 5).unwrap();
        assert_eq!(
            c.bgcolor(),
            vt100::Color::Rgb(0x7a, 0xa2, 0xf7),
            "{}",
            s.contents()
        );
    });
    // O arquivo continua válido.
    let out = e.run(&e.work, &["config", "list"]);
    assert!(out.status.success(), "{}", stderr(&out));
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn clique_no_menu_da_barra_e_projeto_na_pagina() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    // Clique no `≡` (coluna 2, última linha).
    t.write(b"\x1b[<0;2;30M\x1b[<0;2;30m");
    t.wait_text("Menu");
    t.write(b"e");
    t.wait_text("sandmux · configurações");
    // `g` passa o alvo para o projeto da aba; o modo de rede vai para ele.
    t.write(b"g");
    t.wait_text("valendo em: projeto");
    t.write(DOWN);
    t.write(DOWN);
    t.write(DOWN);
    t.write(DOWN);
    t.write(b"\t");
    t.write(RIGHT);
    t.wait_text("network gravado em [project.");
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    let c = sandmux_policy::config::Config::parse(e.config_file().as_path(), &text).unwrap();
    assert!(c.global.network.is_none(), "{text}");
    assert_eq!(c.projects.len(), 1, "{text}");
    t.write(b"q");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(2, "shell:outra"));
    t.wait_text("Rede");
    t.wait_text("sem conexões registradas");
    // A aba 1 acessa um domínio novo depois que a aba 2 fica ativa.
    t.write(
        format!("(sleep 2; curl -s http://host.sandmux.internal:{port}/ >/dev/null) &\r")
            .as_bytes(),
    );
    t.write(b"\x022");
    t.wait_text(&tab(1, "shell:app •"));
    t.with_screen(|s| {
        assert!(
            !s.contents().contains(&tab(2, "shell:outra •")),
            "{}",
            s.contents()
        )
    });
    // Ao voltar, a marca some e o painel mostra o domínio, marcado como novo.
    t.write(b"\x021");
    t.wait_for("marca some", |s| !s.contains(&tab(1, "shell:app •")));
    t.wait_text("host.sandmux.internal");
    t.wait_text("novo");
    t.wait_text("aberta, registrada");
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
    t.wait_text(&tab(2, "shell:outra"));
    t.write(
        format!("(sleep 2; curl -s http://host.sandmux.internal:{port}/ >/dev/null) &\r")
            .as_bytes(),
    );
    t.write(b"\x022");
    t.wait_text(&tab(1, "shell:app !"));
    t.write(b"\x021");
    t.wait_for("marca some", |s| !s.contains(&tab(1, "shell:app !")));
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text("custom (allowlist)");
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "shell:app"));
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
    t.wait_text(&tab(1, "opencode:app"));
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

#[test]
fn copia_do_harness_liberada_chega_pela_tui() {
    let e = Env::new();
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    // Duas sessões: uma com o clipboard liberado, outra com o padrão.
    let mut t = term_in(&e, &e.work, &["run", "shell", "--allow-clipboard"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
    sessao(&e, &outra);

    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    // "liberado" e "bloqueado" em base64.
    t.write(b"printf '\\033]52;c;bGliZXJhZG8=\\007'; echo enviou-$((1+1))\r");
    t.wait_text("enviou-2");
    t.wait_raw("\x1b]52;c;bGliZXJhZG8=");
    t.write(b"\x02n");
    t.wait_for("aba 2 ativa", |s| s.contains("outra"));
    t.write(b"printf '\\033]52;c;YmxvcXVlYWRv\\007'; echo enviou-$((2+2))\r");
    t.wait_text("enviou-4");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        !t.raw_text().contains("\x1b]52;c;YmxvcXVlYWRv"),
        "a cópia bloqueada passou"
    );
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn clique_no_x_fecha_a_aba_encerrada() {
    let e = Env::new();
    sessao(&e, &e.work);
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);
    let mut t = tui(&e);
    t.wait_text(&tab(2, "shell:outra"));
    // A aba 2 termina: ganha o ✕.
    t.write(b"\x02n");
    std::thread::sleep(std::time::Duration::from_millis(300));
    t.write(b"exit\r");
    t.wait_text(&tab(2, "shell:outra ✕"));
    // Um clique no nome só ativa; no ✕, fecha.
    // A barra é a última linha.
    let (row, col) = t.with_screen(|s| {
        let last = s.size().0.get() - 1;
        let line = s.contents_between(last, 0, last, s.size().1.get());
        let i = line.find('✕').expect("sem ✕");
        (last + 1, line[..i].chars().count() as u16)
    });
    t.write(format!("\x1b[<0;{};{row}M\x1b[<0;{};{row}m", col + 1, col + 1).as_bytes());
    t.wait_for("aba fechada", |s| !s.contains(&tab(2, "shell:outra")));
    assert!(!ls(&e).contains("outra"), "{}", ls(&e));
    assert!(ls(&e).contains("shell:app"));
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn aba_encerrada_pergunta_se_reinicia() {
    let e = Env::new();
    sessao(&e, &e.work);
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    sessao(&e, &outra);
    let mut t = tui(&e);
    t.wait_text(&tab(2, "shell:outra"));
    // A aba ativa termina: o modal aparece, e Enter reinicia.
    t.write(b"exit 3\r");
    t.wait_text("terminou com o código 3");
    t.write(b"\r");
    t.wait_text("aba 1 reiniciada");
    t.write(b"echo voltou-$((1+1))\r");
    t.wait_text("voltou-2");
    // A aba 2 termina em segundo plano: a aba 1 não pergunta nada.
    t.write(b"\x02n");
    t.wait_for("aba 2 ativa", |s| s.contains("projetos/outra"));
    t.write(b"sleep 1; exit 5\r");
    std::thread::sleep(std::time::Duration::from_millis(200));
    t.write(b"\x02p");
    t.wait_text(&tab(2, "shell:outra ✕"));
    assert!(!t.screen().contains("terminou com o código 5"));
    // Ao ativar a aba 2, pergunta; Esc deixa encerrada e não pergunta de novo.
    t.write(b"\x02n");
    t.wait_text("terminou com o código 5");
    t.write(b"\x1b");
    t.wait_for("modal fechado", |s| !s.contains("terminou com o código"));
    t.write(b"\x02p");
    std::thread::sleep(std::time::Duration::from_millis(300));
    t.write(b"\x02n");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        !t.screen().contains("terminou com o código"),
        "{}",
        t.screen()
    );
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn blocos_da_direita_na_barra() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui(&e);
    t.wait_text(&tab(1, "shell:app"));
    // Harness e pasta da aba ativa, em blocos de cores diferentes, estado do
    // cofre e tempo da aba.
    t.wait_for("blocos da direita", |s| {
        let last = s.lines().last().unwrap_or("");
        last.contains(" shell   ~/projetos/app ")
            && last.contains("sem cofre")
            && last.contains("0m")
    });
    t.with_screen(|s| {
        let row = s.size().0.get() - 1;
        let cols = s.size().1.get();
        let text: Vec<String> = (0..cols)
            .map(|c| s.cell(row, c).unwrap().contents().to_string())
            .collect();
        let line: String = text.concat();
        let at = |needle: &str| line.find(needle).map(|b| line[..b].chars().count() as u16);
        let h = at(" shell   ~/projetos").unwrap() + 1;
        let f = at("~/projetos/app").unwrap();
        let bg = |c: u16| s.cell(row, c).unwrap().bgcolor();
        assert_ne!(bg(h), bg(f), "{line}");
    });
    t.write(b"\x02d");
    t.wait_exit();
}

// ---- idioma ----

#[test]
fn tui_em_ingles_e_troca_pela_pagina() {
    let e = Env::new();
    sessao(&e, &e.work);
    // SANDMUX_LANG=en: a TUI fala inglês.
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash").env("SANDMUX_LANG", "en");
    let mut t = Term::spawn_answering(cmd, 120, 30);
    t.wait_text("Protection");
    t.wait_text("Resources");
    // O resumo gravado no lançamento (em português) aparece em inglês.
    t.wait_text("open, logged");
    t.write(b"\x02e");
    t.wait_text("sandmux · settings");
    for _ in 0..4 {
        t.write(DOWN);
    }
    t.write(b"\t");
    t.wait_text("‹ open ›");
    t.write(b"\x1b");
    t.wait_text("applies to: global");
    t.write(b"q");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    // Em português, a página troca o idioma para inglês na hora e grava.
    let mut t = tui_cols(&e, 120);
    t.wait_text("Proteção");
    t.write(b"\x02e");
    t.wait_text("sandmux · configurações");
    t.wait_text("‹ auto ›");
    t.write(b"\t");
    t.write(RIGHT);
    t.wait_text("‹ pt ›");
    t.write(RIGHT);
    t.wait_text("‹ en ›");
    t.wait_text("sandmux · settings");
    t.wait_text("language saved in [global]");
    let text = std::fs::read_to_string(e.config_file()).unwrap();
    assert!(text.contains("language = \"en\""), "{text}");
    t.write(b"q");
    t.wait_text("Protection");
    // `auto` apaga a chave.
    t.write(b"\x02e");
    t.wait_text("‹ en ›");
    t.write(b"\t");
    t.write(RIGHT);
    t.wait_text("‹ auto ›");
    t.wait_for("chave apagada", |_| {
        !std::fs::read_to_string(e.config_file())
            .unwrap_or_default()
            .contains("language")
    });
    t.wait_text("sandmux · configurações");
    t.write(b"q");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

// ---- cofre na página ----

#[test]
fn cofre_gerenciado_pela_pagina() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 120);
    t.wait_text(&tab(1, "shell:app"));
    t.write(b"\x02e");
    t.wait_text("sandmux · configurações");
    for _ in 0..9 {
        t.write(DOWN);
    }
    t.wait_text("sem cofre");
    t.wait_text("↑ ↓ seção · Tab ou → entra");
    t.write(b"\t");
    // Sem cofre, o rodapé só oferece criar.
    t.wait_text("n cria o cofre · r relê · Esc volta às seções");
    t.with_screen(|s| assert!(!s.contents().contains("x apaga"), "{}", s.contents()));
    // Cria o cofre.
    t.write(b"n");
    t.wait_text("Criar o cofre");
    t.write(b"senha-um\r");
    t.write(b"senha-um\r");
    t.wait_text("cofre criado e aberto");
    t.wait_text("aberto · 0 segredos");
    // Acrescenta um segredo: o valor nunca aparece, localhost vira o host.
    t.write(b"a");
    t.wait_text("Novo segredo");
    t.write(b"lmstudio\r");
    t.write(b"valor-secreto-123");
    t.wait_text("•••••••••••••••••");
    t.with_screen(|s| assert!(!s.contents().contains("valor-secreto"), "{}", s.contents()));
    t.write(b"\r");
    t.write(b"localhost\r");
    t.wait_text("lmstudio guardado no cofre");
    t.wait_text("host.sandmux.internal");
    let list = stdout(&e.run(&e.work, &["vault", "list"]));
    assert!(
        list.contains("lmstudio") && list.contains("host.sandmux.internal"),
        "{list}"
    );
    // Edita os domínios.
    t.write(b"d");
    t.wait_text("Domínios de lmstudio");
    t.write(b", api.exemplo.com\r");
    t.wait_text("domínios de lmstudio trocados");
    let list = stdout(&e.run(&e.work, &["vault", "list"]));
    assert!(list.contains("api.exemplo.com"), "{list}");
    // Domínio inválido é recusado, e nada muda.
    t.write(b"d");
    t.wait_text("Domínios de lmstudio");
    t.write(b", nao valido!\r");
    t.wait_text("domínio inválido");
    let list = stdout(&e.run(&e.work, &["vault", "list"]));
    assert!(!list.contains("valido"), "{list}");
    // Tranca e desbloqueia.
    t.write(b"l");
    t.wait_text("● trancado");
    t.write(b"u");
    t.wait_text("Desbloquear");
    t.write(b"senha-um\r");
    t.wait_text("cofre desbloqueado");
    t.wait_text("aberto · 1 segredo");
    // Apaga com confirmação.
    t.write(b"x");
    t.wait_text("Apagar lmstudio do cofre?");
    t.write(b"s");
    t.wait_text("lmstudio apagado do cofre");
    t.wait_text("aberto · 0 segredos");
    let list = stdout(&e.run(&e.work, &["vault", "list"]));
    assert!(!list.contains("lmstudio"), "{list}");
    t.write(b"\x1b");
    t.write(b"q");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn diagnostico_na_pagina_e_chaves_ao_cofre() {
    use std::io::Write;
    let e = Env::new();
    sessao(&e, &e.work);
    // O cofre existe e fica trancado.
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"senha-um\n").unwrap();
    assert!(c.wait_with_output().unwrap().status.success());
    assert!(e.run(&e.work, &["vault", "lock"]).status.success());
    // Chaves reais sem domínio conhecido (o vigia do daemon não as move):
    // duas no opencode.json e uma no auth.json, com uma URL logo abaixo.
    let home = e.data_dir().join("homes/opencode");
    let cfg = home.join(".config/opencode/opencode.json");
    let auth = home.join(".local/share/opencode/auth.json");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    let cfg_text = "{\n  \"provider\": {\n    \"a\": {\n      \"options\": {\n        \"headers\": {\n          \"Authorization\": \"Bearer sk-a-REALAAA11\"\n        }\n      }\n    },\n    \"b\": {\n      \"options\": {\n        \"headers\": {\n          \"Authorization\": \"Bearer sk-b-REALBBB22\"\n        }\n      }\n    }\n  }\n}\n";
    std::fs::write(&cfg, cfg_text).unwrap();
    std::fs::write(
        &auth,
        "{\n  \"corp\": {\n    \"type\": \"api\",\n    \"key\": \"sk-corp-REALCORP33\",\n    \"baseURL\": \"https://api.exemplo.com/v1\"\n  }\n}\n",
    )
    .unwrap();

    let mut t = tui_cols(&e, 120);
    t.wait_text("O cofre de segredos está trancado.");
    t.write(b"\x1b");
    t.wait_text(&tab(1, "shell:app"));
    // Ctrl+b D abre a página no diagnóstico, com as falhas na lateral.
    t.write(b"\x02D");
    t.wait_text("sandmux · configurações");
    t.wait_text("Diagnóstico ✗");
    t.wait_text("✓ Landlock com escopo");
    // As chaves reais ficam no fim da lista.
    for _ in 0..40 {
        t.write(DOWN);
    }
    t.wait_text("Chaves reais");
    t.wait_text("opencode: .local/share/opencode/auth.json");
    t.wait_text("O agente pode ler esta chave");
    // f pede a senha do cofre trancado e segue arquivo a arquivo.
    t.write(b"f");
    t.wait_text("Desbloquear o cofre para mover as chaves");
    t.write(b"senha-um");
    t.wait_text("••••••••");
    t.write(b"\r");
    // opencode.json: confirma a primeira chave e pula o arquivo na segunda.
    t.wait_text("Domínios de opencode:.config/opencode/opencode.json:a.Authorization");
    t.write(b"a.exemplo.com\r");
    t.wait_text("Domínios de opencode:.config/opencode/opencode.json:b.Authorization");
    t.write(b"\x1b");
    // auth.json: o campo vem com a URL próxima, e o usuário o troca.
    t.wait_text("Domínios de opencode:.local/share/opencode/auth.json:corp.key");
    t.wait_text("api.exemplo.com▏");
    t.write(&[0x7f; 15]);
    t.write(b"llm.exemplo.com\r");
    t.wait_text("1 chave(s) levadas ao cofre; reinicie as abas de opencode");
    t.with_screen(|s| assert!(!s.contents().contains("REAL"), "{}", s.contents()));
    // O relatório foi relido: o auth.json saiu da lista.
    t.wait_for("auth.json fora da lista", |s| {
        !s.contains("opencode: .local/share/opencode/auth.json")
            && s.contains("opencode: .config/opencode/opencode.json")
    });
    let list = stdout(&e.run(&e.work, &["vault", "list"]));
    assert!(
        list.contains("opencode:.local/share/opencode/auth.json:corp.key  (llm.exemplo.com)"),
        "{list}"
    );
    assert!(!list.contains("Authorization"), "{list}");
    let a = std::fs::read_to_string(&auth).unwrap();
    assert!(
        a.contains("sandmux-fake-") && !a.contains("REALCORP"),
        "{a}"
    );
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), cfg_text);
    // r roda de novo: sem o arquivo, as linhas dele somem.
    std::fs::remove_file(&cfg).unwrap();
    t.wait_text("r verifica de novo · f leva as chaves ao cofre");
    t.write(b"r");
    t.wait_text("verificado de novo às");
    t.wait_text("verificado às");
    t.wait_for("opencode.json fora da lista", |s| {
        !s.contains("opencode: .config/opencode/opencode.json")
    });
    // Sem chaves reais, o rodapé não oferece o f.
    t.wait_for("rodapé sem o f", |s| {
        s.contains("r verifica de novo · Esc volta às seções") && !s.contains("f leva as chaves")
    });
    // O aviso some ao mudar de linha.
    t.write(UP);
    t.wait_for("aviso apagado", |s| !s.contains("verificado de novo às"));
    t.write(b"\x1b");
    t.write(b"q");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

#[test]
fn ctrl_b_t_abre_shell_dentro_da_aba() {
    let e = Env::new();
    sessao(&e, &e.work);
    let mut t = tui_cols(&e, 120);
    t.wait_text(&tab(1, "shell:app"));
    t.write(b"\x02t");
    t.wait_text(&tab(2, "dentro:app"));
    t.write(b"echo dentro-$((2+3))\r");
    t.wait_text("dentro-5");
    // Um shell de dentro não abre outro.
    t.write(b"\x02t");
    t.wait_text("esta aba já é um shell de dentro");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}
