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
    let out = run(&e, &["run", "claude"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("opencode, hermes, shell"), "{err}");
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
