//! Arquivo de configuração, camadas de política e `sandmux config`.

mod common;
use common::cli::*;

use std::path::Path;

fn devs(e: &Env, extra: &[&str]) -> String {
    let out = e.shell(extra, "ls /dev");
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn has_gpu() -> bool {
    Path::new("/dev/dri/renderD128").exists() || Path::new("/dev/nvidia0").exists()
}

// ---- opções do run ----

#[test]
fn opcao_e_negativa_juntas_recusadas() {
    let e = Env::new();
    for (a, b) in [
        ("--kvm", "--no-kvm"),
        ("--gpu", "--no-gpu"),
        ("--allow-clipboard", "--no-allow-clipboard"),
        ("--repo", "--no-repo"),
    ] {
        let out = e.run(&e.work, &["run", "shell", a, b, "--", "-c", "true"]);
        assert_ne!(out.status.code(), Some(0), "{a} {b}");
        assert!(
            stderr(&out).contains("cannot be used with"),
            "{}",
            stderr(&out)
        );
    }
}

#[test]
fn ajuda_do_run_mostra_as_opcoes_novas() {
    let e = Env::new();
    let out = e.run(&e.work, &["run", "--help"]);
    let s = stdout(&out);
    for o in [
        "--profile",
        "--no-kvm",
        "--gpu",
        "--no-allow-clipboard",
        "--no-repo",
    ] {
        assert!(s.contains(o), "{o}: {s}");
    }
}

// ---- camadas no run sem terminal ----

#[test]
fn kvm_pelo_perfil_do_projeto_e_desligado_pelo_run() {
    if !Path::new("/dev/kvm").exists() {
        eprintln!("pulado: a máquina não tem /dev/kvm");
        return;
    }
    let e = Env::new();
    e.write_config(&format!("[project.\"{}\"]\nkvm = true\n", e.work.display()));
    assert!(devs(&e, &[]).split_whitespace().any(|x| x == "kvm"));
    assert!(
        !devs(&e, &["--no-kvm"])
            .split_whitespace()
            .any(|x| x == "kvm")
    );
    // Fora da pasta do projeto, nada muda.
    let other = e.home.join("outro");
    std::fs::create_dir_all(&other).unwrap();
    let out = e.shell_in(&other, &[], "ls /dev");
    assert!(!stdout(&out).split_whitespace().any(|x| x == "kvm"));
}

#[test]
fn gpu_desligada_no_global_e_religada_pelo_run() {
    let e = Env::new();
    e.write_config("[global]\ngpu = false\n");
    let d = devs(&e, &[]);
    assert!(!d.contains("renderD") && !d.contains("nvidia"), "{d}");
    if has_gpu() {
        let d = devs(&e, &["--gpu"]);
        assert!(d.contains("renderD") || d.contains("nvidia0"), "{d}");
    }
}

#[test]
fn ro_no_perfil_nomeado() {
    let e = Env::new();
    let docs = e.home.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("a.txt"), "ref\n").unwrap();
    e.write_config("[profile.leitura]\nro = [\"~/docs\"]\n");
    let script = format!(
        "cat {d}/a.txt; echo x > {d}/b.txt && echo gravou || echo negado",
        d = docs.display()
    );
    let out = e.shell(&["--profile", "leitura"], &script);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "ref\nnegado\n");
    assert!(!docs.join("b.txt").exists());
    // Sem o perfil, o conteúdo da pasta não aparece (o ponto de montagem
    // criado no HOME sintético pode ficar, vazio).
    let out = e.shell(
        &[],
        &format!("test -e {}/a.txt && echo sim || echo nao", docs.display()),
    );
    assert_eq!(stdout(&out), "nao\n");
}

#[test]
fn arquivo_invalido_recusa_com_a_linha() {
    let e = Env::new();
    e.write_config("[global]\nkvm = true\nrede = \"aberta\"\n");
    let out = e.shell(&[], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("config.toml") && err.contains("linha 3") && err.contains("`rede`"),
        "{err}"
    );
    assert!(!stdout(&out).contains("rodou"));
}

#[test]
fn toml_malformado_recusa_com_a_linha() {
    let e = Env::new();
    e.write_config("[global]\nro = []\n\n[profile.x\n");
    let out = e.shell(&[], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("linha 4"), "{}", stderr(&out));
}

#[test]
fn perfil_inexistente_lista_os_perfis() {
    let e = Env::new();
    e.write_config("[profile.vm]\nkvm = true\n[profile.web]\nclipboard = true\n");
    let out = e.shell(&["--profile", "nada"], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("\"nada\"") && err.contains("vm, web"), "{err}");
}

#[test]
fn pass_env_pela_configuracao() {
    let e = Env::new();
    e.write_config("[global]\npass-env = [\"SANDMUX_TESTE_VAR\"]\n");
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--ro",
                common::bin_dir().to_str().unwrap(),
                "--",
                "-c",
                "echo $SANDMUX_TESTE_VAR",
            ],
        )
        .env("SANDMUX_TESTE_VAR", "valor-de-teste")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "valor-de-teste\n");
    assert!(
        stderr(&out).contains("SANDMUX_TESTE_VAR"),
        "aviso de pass-env"
    );
    // Sem a variável no ambiente, o run recusa.
    let out = e.shell(&[], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("SANDMUX_TESTE_VAR"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn pasta_de_configuracao_com_escrita_recusada() {
    let e = Env::new();
    std::fs::create_dir_all(e.config.join("sandmux")).unwrap();
    let out = e.shell(&["--rw", e.config.to_str().unwrap()], "echo rodou");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("configuração do sandmux"),
        "{}",
        stderr(&out)
    );
    let out = e.shell(&["--ro", e.config.to_str().unwrap()], "echo rodou");
    assert!(out.status.success(), "{}", stderr(&out));
}

// ---- sandmux config ----

fn config(e: &Env, args: &[&str]) -> std::process::Output {
    let mut a = vec!["config"];
    a.extend_from_slice(args);
    e.run(&e.work, &a)
}

fn file(e: &Env) -> String {
    std::fs::read_to_string(e.config_file()).unwrap_or_default()
}

fn row<'a>(out: &'a str, key: &str) -> Vec<&'a str> {
    out.lines()
        .filter(|l| l.split_whitespace().next() == Some(key))
        .collect()
}

#[test]
fn show_sem_arquivo_mostra_padroes() {
    let e = Env::new();
    let out = config(&e, &["show"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("(não existe)"), "{s}");
    for k in ["repo", "kvm", "gpu", "clipboard", "ro", "rw", "pass-env"] {
        let r = row(&s, k);
        assert_eq!(r.len(), 1, "{k}: {s}");
        assert!(r[0].ends_with("padrão"), "{k}: {s}");
    }
    assert!(row(&s, "gpu")[0].contains("true"));
    assert!(!e.config_file().exists(), "o show criou o arquivo");
}

#[test]
fn show_marca_a_origem() {
    let e = Env::new();
    e.write_config(&format!(
        "[global]\nro = [\"~/docs\"]\n[project.\"{}\"]\ngpu = false\n[profile.vm]\nkvm = true\n",
        e.work.display()
    ));
    let s = stdout(&config(&e, &["show"]));
    let gpu = row(&s, "gpu")[0];
    assert!(
        gpu.contains("false")
            && gpu.contains(&format!("modificada (projeto {})", e.work.display())),
        "{s}"
    );
    let ro = row(&s, "ro")[0];
    assert!(
        ro.contains(&e.home.join("docs").display().to_string())
            && ro.contains("modificada (global)"),
        "{s}"
    );
    assert!(row(&s, "kvm")[0].ends_with("padrão"), "{s}");
    let s = stdout(&config(
        &e,
        &[
            "show",
            "--profile",
            "vm",
            "--workdir",
            e.home.to_str().unwrap(),
        ],
    ));
    assert!(row(&s, "kvm")[0].contains("modificada (perfil vm)"), "{s}");
    assert!(
        row(&s, "gpu")[0].ends_with("padrão"),
        "fora do projeto: {s}"
    );
}

#[test]
fn show_com_arquivo_invalido() {
    let e = Env::new();
    e.write_config("[global]\n\nkvm = \"sim\"\n");
    let out = config(&e, &["show"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("linha 3"), "{}", stderr(&out));
}

#[test]
fn set_cria_o_perfil_e_exige_alvo() {
    let e = Env::new();
    let out = config(&e, &["set", "kvm", "true"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("--global"), "{}", stderr(&out));
    assert!(!e.config_file().exists());

    let out = config(&e, &["set", "kvm", "true", "--profile", "vm"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(file(&e), "[profile.vm]\nkvm = true\n");
}

#[test]
fn add_no_projeto_da_pasta_atual() {
    let e = Env::new();
    let out = config(&e, &["add", "ro", "~/docs", "--project"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        file(&e),
        format!("[project.\"{}\"]\nro = [\"~/docs\"]\n", e.work.display())
    );
}

#[test]
fn escrita_na_configuracao_recusada_pelo_comando() {
    let e = Env::new();
    // Com o XDG_CONFIG_HOME do teste, a pasta de configuração fica em
    // `e.config` (fora do HOME): é ela, ou uma pasta que a contém, que
    // precisa ser recusada com escrita.
    let out = config(&e, &["add", "rw", e.config.to_str().unwrap(), "--global"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("configuração do sandmux"),
        "{}",
        stderr(&out)
    );
    assert!(!e.config_file().exists());
}

#[test]
fn so_a_diferenca_e_comentarios_preservados() {
    let e = Env::new();
    let original =
        "# escrito à mão\n[global]\n# a GPU\ngpu = true # ao lado\n\n[profile.vm]\nkvm = true\n";
    e.write_config(original);
    let out = config(&e, &["set", "gpu", "false", "--global"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(file(&e), original.replace("gpu = true", "gpu = false"));
    let out = config(&e, &["unset", "kvm", "--profile", "vm"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        file(&e),
        "# escrito à mão\n[global]\n# a GPU\ngpu = false # ao lado\n"
    );
}

#[test]
fn list_mostra_perfis_e_projetos() {
    let e = Env::new();
    let velho = e.home.join("velho");
    e.write_config(&format!(
        "[profile.vm]\nkvm = true\n[project.\"{}\"]\nro = [\"~/docs\"]\n[project.\"{}\"]\nrepo = true\n",
        e.work.display(),
        velho.display()
    ));
    let out = config(&e, &["list"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(s.contains("  vm\n    kvm = true"), "{s}");
    assert!(
        s.contains(&format!("  {}\n    ro = [\"~/docs\"]", e.work.display())),
        "{s}"
    );
    assert!(
        s.contains(&format!("{} (pasta não existe)", velho.display())),
        "{s}"
    );
}

#[test]
fn doctor_mostra_a_configuracao_sem_alterar() {
    let e = Env::new();
    let s = stdout(&e.run(&e.work, &["doctor"]));
    assert!(
        s.contains("Configuração:") && s.contains("estado: não existe"),
        "{s}"
    );
    assert!(!e.config_file().exists());
    let text = "[global]\nkvm = true\nrede = 1\n";
    e.write_config(text);
    let s = stdout(&e.run(&e.work, &["doctor"]));
    assert!(
        s.contains(&e.config_file().display().to_string())
            && s.contains("estado: inválido, linha 3")
            && s.contains("`rede`"),
        "{s}"
    );
    assert_eq!(file(&e), text);
    e.write_config("[profile.vm]\nkvm = true\n");
    let s = stdout(&e.run(&e.work, &["doctor"]));
    assert!(s.contains("estado: válido (1 perfil, 0 projetos)"), "{s}");
}
