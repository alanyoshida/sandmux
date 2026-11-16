//! Grupo 8: presets de harness. Os harnesses falsos executam o comando que
//! recebem, o que permite inspecionar o sandbox de cada preset.

mod common;
use common::cli::*;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn exe(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Harness falso que pula as opções que o sandmux põe para ele rodar sem
/// perguntas e executa o resto.
const SKIP_NO_ASKS: &str = "case \"$1\" in --dangerously-skip-permissions) shift 3;; --auto-approve) shift;; esac\nexec \"$@\"";

fn fake_opencode(e: &Env) {
    exe(&e.home.join(".opencode/bin/opencode"), "exec \"$@\"");
}

fn fake_hermes(e: &Env) {
    std::fs::create_dir_all(e.home.join(".hermes/hermes-agent")).unwrap();
    std::fs::write(e.home.join(".hermes/hermes-agent/main.py"), "codigo").unwrap();
    exe(&e.home.join(".local/bin/hermes"), "exec \"$@\"");
    exe(
        &e.home.join(".hermes/tools/node/bin/node"),
        "echo node-do-hermes",
    );
    std::fs::write(e.home.join(".hermes/gateway.sock"), "").unwrap();
    std::fs::write(e.home.join(".hermes/.env"), "OPENROUTER_API_KEY=real\n").unwrap();
}

fn run(e: &Env, args: &[&str]) -> std::process::Output {
    e.run(&e.work, args)
}

#[test]
fn preset_desconhecido_lista_os_disponiveis() {
    let e = Env::new();
    let out = run(&e, &["run", "inexistente"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("opencode, hermes, claude, omp, shell"),
        "{err}"
    );
}

#[test]
fn harness_nao_instalado_recusa_com_instrucao() {
    let e = Env::new();
    let out = run(&e, &["run", "opencode", "--no-import"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains(".opencode/bin/opencode") && err.contains("opencode.ai/install"),
        "{err}"
    );
    assert!(
        !e.data_dir().join("homes/opencode").exists(),
        "criou o HOME sintético antes de recusar"
    );
    let out = run(&e, &["run", "hermes", "--no-import"]);
    assert!(
        stderr(&out).contains(".hermes/hermes-agent"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn preset_shell_repassa_argumentos_e_codigo_de_saida() {
    let e = Env::new();
    let out = run(&e, &["run", "shell", "--", "-c", "id -u; exit 4"]);
    assert_eq!(stdout(&out).trim(), unsafe { libc::getuid() }.to_string());
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn preset_shell_usa_configuracao_gerada_e_nao_a_real() {
    let e = Env::new();
    std::fs::write(e.home.join(".bashrc"), "export GITHUB_TOKEN=ghp_segredo\n").unwrap();
    let mut cmd = e.cmd(
        &e.work,
        &[
            "run",
            "shell",
            "--",
            "-ic",
            "echo token=$GITHUB_TOKEN; echo $PS1",
        ],
    );
    cmd.env("SHELL", "/bin/bash");
    let out = cmd.output().unwrap();
    let s = stdout(&out);
    assert!(s.contains("token=\n"), "{s}");
    assert!(s.contains("[sandmux]"), "{s}");
    let rc = std::fs::read_to_string(e.data_dir().join("homes/shell/.bashrc")).unwrap();
    assert!(rc.contains("Gerado pelo sandmux") && !rc.contains("GITHUB_TOKEN"));
    // Ajuste do usuário no HOME sintético vale nas execuções seguintes.
    std::fs::write(
        e.data_dir().join("homes/shell/.bashrc"),
        "export AJUSTE=sim\n",
    )
    .unwrap();
    let out = e
        .cmd(&e.work, &["run", "shell", "--", "-ic", "echo $AJUSTE"])
        .env("SHELL", "/bin/bash")
        .output()
        .unwrap();
    assert_eq!(stdout(&out).trim(), "sim");
}

#[test]
fn shell_fora_do_sistema_cai_para_bash() {
    let e = Env::new();
    exe(&e.home.join(".local/bin/meu-zsh"), "echo nao-deveria-rodar");
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--",
                "-c",
                "echo $SHELL; echo $BASH_VERSION | cut -c1",
            ],
        )
        .env("SHELL", e.home.join(".local/bin/meu-zsh"))
        .output()
        .unwrap();
    let s = stdout(&out);
    assert!(
        s.starts_with("/usr/bin/bash\n") || s.starts_with("/bin/bash\n"),
        "{s}"
    );
}

#[test]
fn opencode_codigo_somente_leitura_e_atualizacao_desligada() {
    let e = Env::new();
    fake_opencode(&e);
    let out = run(
        &e,
        &[
            "run",
            "opencode",
            "--no-import",
            "--",
            "sh",
            "-c",
            "echo x > ~/.opencode/bin/opencode 2>/dev/null && echo alterou || echo negado; echo $OPENCODE_DISABLE_AUTOUPDATE; pwd",
        ],
    );
    assert_eq!(
        stdout(&out),
        format!("negado\n1\n{}\n", e.work.display()),
        "{}",
        stderr(&out)
    );
}

#[test]
fn opencode_com_pass_env_aponta_a_chave_para_a_variavel() {
    let e = Env::new();
    fake_opencode(&e);
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "opencode",
                "--no-import",
                "--pass-env",
                "OPENROUTER_API_KEY",
                "--",
                "sh",
                "-c",
                "echo $OPENCODE_CONFIG_CONTENT",
            ],
        )
        .env("OPENROUTER_API_KEY", "k")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&out).trim(),
        r#"{"provider":{"openrouter":{"options":{"apiKey":"{env:OPENROUTER_API_KEY}"}}}}"#
    );
}

#[test]
fn hermes_estado_sintetico_codigo_protegido_e_sem_gateway_do_host() {
    let e = Env::new();
    fake_hermes(&e);
    let out = run(
        &e,
        &[
            "run",
            "hermes",
            "--no-import",
            "--",
            "sh",
            "-c",
            "cat ~/.hermes/hermes-agent/main.py; echo; echo x > ~/.hermes/hermes-agent/main.py 2>/dev/null && echo alterou || echo negado; \
             ls ~/.hermes/gateway.sock 2>/dev/null || echo sem-gateway-do-host; cat ~/.hermes/.env 2>/dev/null || echo sem-env-real; \
             ~/.hermes/tools/node/bin/node; grep -A1 '^updates:' ~/.hermes/config.yaml",
        ],
    );
    assert_eq!(
        stdout(&out),
        "codigo\nnegado\nsem-gateway-do-host\nsem-env-real\nnode-do-hermes\nupdates:\n  check: false\n",
        "{}",
        stderr(&out)
    );
}

#[test]
fn hermes_com_pass_env_ve_o_valor_real_so_nesta_execucao() {
    let e = Env::new();
    fake_hermes(&e);
    let out = run(&e, &["run", "hermes", "--no-import", "--", "true"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let synthetic = e.data_dir().join("homes/hermes/.hermes/.env");

    // Sem `.env` sintético (sem importação): o arquivo da execução só tem a
    // chave, e o que fica no host é um `.env` vazio.
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "hermes",
                "--no-import",
                "--pass-env",
                "OPENROUTER_API_KEY",
                "--",
                "cat",
                &e.home.join(".hermes/.env").to_string_lossy(),
            ],
        )
        .env("OPENROUTER_API_KEY", "sk-or-real")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&out),
        "OPENROUTER_API_KEY=\"sk-or-real\"\n",
        "{}",
        stderr(&out)
    );
    assert_eq!(std::fs::read_to_string(&synthetic).unwrap(), "");

    let fake = "OPENROUTER_API_KEY=sandmux-fake-0123456789abcdef\nOUTRA=1\n";
    std::fs::write(&synthetic, fake).unwrap();

    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "hermes",
                "--no-import",
                "--pass-env",
                "OPENROUTER_API_KEY",
                "--",
                "sh",
                "-c",
                "cat ~/.hermes/.env; echo x >> ~/.hermes/.env 2>/dev/null && echo alterou || echo negado; \
                 mv ~/.hermes/.env ~/.hermes/env.bak 2>/dev/null && echo moveu || echo negado",
            ],
        )
        .env("OPENROUTER_API_KEY", "sk-or-real")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&out),
        "OUTRA=1\nOPENROUTER_API_KEY=\"sk-or-real\"\nnegado\nnegado\n",
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("somente leitura"), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(&synthetic).unwrap(), fake);

    // Valor que o .env do hermes não representa: recusa sem mostrar o valor.
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "hermes",
                "--no-import",
                "--pass-env",
                "OPENROUTER_API_KEY",
                "--",
                "true",
            ],
        )
        .env("OPENROUTER_API_KEY", "a${HOME}b")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("OPENROUTER_API_KEY") && !err.contains("a${HOME}b"),
        "{err}"
    );
}

#[test]
fn docker_do_host_nao_e_alcancado() {
    let e = Env::new();
    let out = e.shell(&[], "ls /var/run/docker.sock /run/docker.sock 2>/dev/null || echo sem-socket; docker ps >/dev/null 2>&1 && echo alcancou || echo negado");
    assert_eq!(stdout(&out), "sem-socket\nnegado\n");
}

fn fake_claude(e: &Env) {
    let real = e.home.join(".local/share/claude/versions/9.9.9");
    exe(&real, SKIP_NO_ASKS);
    std::fs::create_dir_all(e.home.join(".local/bin")).unwrap();
    std::os::unix::fs::symlink(&real, e.home.join(".local/bin/claude")).unwrap();
    std::fs::create_dir_all(e.home.join(".claude/projects/x")).unwrap();
    std::fs::write(e.home.join(".claude/settings.json"), "{\"theme\":\"dark\"}").unwrap();
    std::fs::write(
        e.home.join(".claude/.credentials.json"),
        "{\"claudeAiOauth\":{\"accessToken\":\"sk-ant-oat01-REAL\"}}",
    )
    .unwrap();
    std::fs::write(e.home.join(".claude/projects/x/conversa.jsonl"), "{}").unwrap();
    std::fs::write(e.home.join(".claude.json"), "{\"numStartups\":3}").unwrap();
}

#[test]
fn claude_sem_o_token_do_host_e_atualizacao_desligada() {
    let e = Env::new();
    // Sem a instalação: a dica de instalar.
    let out = run(&e, &["run", "claude"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("claude.ai/install.sh"),
        "{}",
        stderr(&out)
    );
    fake_claude(&e);
    let out = run(
        &e,
        &[
            "run",
            "claude",
            "--import",
            "--",
            "sh",
            "-c",
            "test -e ~/.claude/.credentials.json && echo TEM || echo SEM; test -e ~/.claude/settings.json && echo SET; test -e ~/.claude.json && echo JSON; test -e ~/.claude/projects && echo CONVERSAS || echo SEM-CONVERSAS; echo $DISABLE_AUTOUPDATER",
        ],
    );
    assert_eq!(
        stdout(&out),
        "SEM\nSET\nJSON\nSEM-CONVERSAS\n1\n",
        "{}",
        stderr(&out)
    );
}

#[test]
fn omp_sem_os_bancos_e_com_a_chave_falsa() {
    let e = Env::new();
    let out = run(&e, &["run", "omp"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("oh-my-pi"), "{}", stderr(&out));
    exe(&e.home.join(".local/bin/omp"), SKIP_NO_ASKS);
    std::fs::create_dir_all(e.home.join(".omp/agent/extensions")).unwrap();
    std::fs::write(e.home.join(".omp/agent/config.yml"), "theme:\n  dark: x\n").unwrap();
    std::fs::write(e.home.join(".omp/agent/extensions/e.ts"), "//").unwrap();
    std::fs::write(e.home.join(".omp/agent/agent.db"), "CREDENCIAIS").unwrap();
    std::fs::write(e.home.join(".omp/agent/history.db"), "H").unwrap();
    let out = run(
        &e,
        &[
            "run",
            "omp",
            "--import",
            "--",
            "sh",
            "-c",
            "cd ~/.omp/agent; ls; echo x > ~/.local/bin/omp 2>/dev/null && echo ALTEROU || echo NEGADO",
        ],
    );
    assert_eq!(
        stdout(&out),
        "config.yml\nextensions\nNEGADO\n",
        "{}",
        stderr(&out)
    );
}

/// Atualizador falso: registra no HOME real como foi chamado.
fn fake_updater(p: &Path, code: i32) {
    exe(
        p,
        &format!(
            "if [ \"$1\" = update ] || [ \"$1\" = upgrade ]; then echo \"$0 $*\" > \"$HOME/atualizou\"; exit {code}; fi; exec \"$@\""
        ),
    );
}

#[test]
fn update_roda_o_atualizador_oficial_fora_do_sandbox() {
    let e = Env::new();
    let omp = e.home.join(".local/bin/omp");
    fake_updater(&omp, 7);
    let out = run(&e, &["update", "omp", "--", "--check"]);
    assert_eq!(out.status.code(), Some(7), "{}", stderr(&out));
    assert!(stderr(&out).contains("fora do sandbox"), "{}", stderr(&out));
    let got = std::fs::read_to_string(e.home.join("atualizou")).unwrap();
    assert_eq!(got.trim(), format!("{} update --check", omp.display()));

    fake_updater(&e.home.join(".opencode/bin/opencode"), 0);
    let out = run(&e, &["update", "opencode"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let got = std::fs::read_to_string(e.home.join("atualizou")).unwrap();
    assert!(got.trim().ends_with("opencode upgrade"), "{got}");
}

#[test]
fn update_recusa_sem_atualizador_e_sem_instalacao() {
    let e = Env::new();
    let out = run(&e, &["update", "shell"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("não tem atualizador"),
        "{}",
        stderr(&out)
    );
    let out = run(&e, &["update", "claude"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains(".local/bin/claude") && err.contains("claude.ai/install.sh"),
        "{err}"
    );
    let out = run(&e, &["update", "inexistente"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!e.home.join("atualizou").exists());
}

/// Harness falso que mostra os argumentos e as variáveis de permissão.
fn show(e: &Env, path: &Path) {
    exe(
        &e.home.join(path),
        "echo \"args=$*\"; echo \"oc=${OPENCODE_PERMISSION:-}\"; echo \"yolo=${HERMES_YOLO_MODE:-}\"",
    );
}

#[test]
fn harnesses_sem_perguntas_por_padrao_e_harness_asks_devolve() {
    let e = Env::new();
    show(&e, Path::new(".opencode/bin/opencode"));
    fake_hermes(&e);
    show(&e, Path::new(".local/bin/hermes"));
    show(&e, Path::new(".local/bin/omp"));
    let real = e.home.join(".local/share/claude/versions/9.9.9");
    show(&e, Path::new(".local/share/claude/versions/9.9.9"));
    std::os::unix::fs::symlink(&real, e.home.join(".local/bin/claude")).unwrap();
    let go = |h: &str| {
        let out = run(&e, &["run", h, "--no-import", "--", "x"]);
        assert!(out.status.success(), "{h}: {}", stderr(&out));
        stdout(&out)
    };
    let oc = go("opencode");
    assert!(oc.contains("args=x\n"), "{oc}");
    assert!(
        oc.contains(r#"oc={"*":"allow""#) && oc.contains(r#""external_directory":"allow""#),
        "{oc}"
    );
    let cl = go("claude");
    assert!(
        cl.contains(r#"args=--dangerously-skip-permissions --settings {"skipDangerousModePermissionPrompt":true} x"#),
        "{cl}"
    );
    assert!(go("hermes").contains("yolo=1\n"));
    assert!(go("omp").contains("args=--auto-approve x\n"));
    let sh = stdout(&run(
        &e,
        &[
            "run",
            "shell",
            "--",
            "-c",
            "echo \"oc=${OPENCODE_PERMISSION:-} yolo=${HERMES_YOLO_MODE:-}\"",
        ],
    ));
    assert_eq!(sh, "oc= yolo=\n");

    // Com `harness-asks = true`, o harness fica como é.
    let out = run(&e, &["config", "set", "--global", "harness-asks", "true"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let oc = go("opencode");
    assert!(oc.contains("args=x\noc=\n"), "{oc}");
    assert!(go("claude").contains("args=x\n"));
    assert!(go("hermes").contains("yolo=\n"));
    assert!(go("omp").contains("args=x\n"));
}
