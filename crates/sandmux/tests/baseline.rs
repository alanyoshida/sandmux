//! Grupo 5: baseline e visão de arquivos, pela linha de comando.

mod common;
use common::cli::*;
use common::*;

use std::path::Path;

// ---- 5.1 pasta de trabalho ----

#[test]
fn pasta_de_trabalho_e_a_pasta_atual_e_e_a_real() {
    let e = Env::new();
    std::fs::write(e.work.join("doc.pdf"), "pdf").unwrap();
    let out = e.shell(&[], "pwd; cat doc.pdf; echo agente > criado.txt");
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(s.starts_with(&format!("{}\npdf", e.work.display())), "{s}");
    assert_eq!(
        std::fs::read_to_string(e.work.join("criado.txt")).unwrap(),
        "agente\n"
    );
}

#[test]
fn arquivo_alterado_no_host_aparece_no_sandbox() {
    let e = Env::new();
    std::fs::write(e.work.join("a.txt"), "antes").unwrap();
    let work = e.work.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        std::fs::write(work.join("a.txt"), "depois").unwrap();
    });
    let out = e.shell(&[], "cat a.txt; echo; sleep 1; cat a.txt");
    writer.join().unwrap();
    assert_eq!(stdout(&out), "antes\ndepois");
}

#[test]
fn workdir_indica_outra_pasta() {
    let e = Env::new();
    let other = e.home.join("outro");
    std::fs::create_dir_all(&other).unwrap();
    let out = e.shell_in(
        &e.home.join("projetos"),
        &["--workdir", other.to_str().unwrap()],
        "pwd",
    );
    assert_eq!(stdout(&out).trim(), other.to_str().unwrap());
}

#[test]
fn pastas_perigosas_sao_recusadas() {
    let e = Env::new();
    let parent = e.home.parent().unwrap().to_path_buf();
    for cwd in [e.home.clone(), parent.clone(), "/".into()] {
        let out = e.shell_in(&cwd, &[], "echo executou");
        assert_eq!(out.status.code(), Some(1), "{}", cwd.display());
        assert!(!stdout(&out).contains("executou"));
        assert!(
            stderr(&out).contains("Use uma subpasta"),
            "{}",
            stderr(&out)
        );
    }
    std::fs::create_dir_all(e.data_dir().join("homes/x")).unwrap();
    let out = e.shell_in(&e.data_dir().join("homes/x"), &[], "echo executou");
    assert!(
        stderr(&out).contains("diretório de dados"),
        "{}",
        stderr(&out)
    );
    for (flag, p) in [("--ro", e.home.clone()), ("--rw", "/".into())] {
        let out = e.shell(&[flag, p.to_str().unwrap()], "echo executou");
        assert_eq!(out.status.code(), Some(1));
        assert!(!stdout(&out).contains("executou"));
    }
    let ok = e.shell(&[], "echo executou");
    assert!(stdout(&ok).contains("executou"), "{}", stderr(&ok));
}

// ---- 5.2 HOME sintético ----

#[test]
fn home_real_fica_fora_e_home_sintetico_persiste() {
    let e = Env::new();
    std::fs::create_dir_all(e.home.join(".ssh")).unwrap();
    std::fs::write(e.home.join(".ssh/id_ed25519"), "chave").unwrap();
    std::fs::create_dir_all(e.home.join("outro-projeto")).unwrap();
    for p in [".ssh", "outro-projeto", ".ssh/id_ed25519"] {
        let v = e.probe(&[], &["exists", e.home.join(p).to_str().unwrap()]);
        assert!(!ok(&v), "{p} visível");
    }
    let out = e.shell(&[], "echo guardado > ~/nota.txt");
    assert!(out.status.success(), "{}", stderr(&out));
    let out = e.shell(&[], "cat ~/nota.txt");
    assert_eq!(stdout(&out), "guardado\n");
    assert!(
        !e.home.join("nota.txt").exists(),
        "o HOME real foi alterado"
    );
    assert!(e.data_dir().join("homes/shell/nota.txt").exists());
}

// ---- 5.3 sistema e contas ----

#[test]
fn sistema_e_somente_leitura_e_contas_sao_geradas() {
    let e = Env::new();
    for p in ["/usr/x", "/etc/x", "/x"] {
        assert!(!ok(&e.probe(&[], &["write", p])), "escrita em {p}");
    }
    let out = e.shell(&[], "id -un; cut -d: -f1 /etc/passwd | tr '\\n' ' '");
    let s = stdout(&out);
    let me = std::env::var("USER").unwrap_or_default();
    let mut lines = s.lines();
    let name = lines.next().unwrap_or("");
    assert!(!name.is_empty() && (me.is_empty() || name == me), "{s}");
    assert_eq!(lines.next().unwrap().trim(), format!("root {name} nobody"));
}

#[test]
fn pastas_fora_da_visao_nao_existem() {
    let e = Env::new();
    for p in ["/mnt", "/media", "/srv", "/root", "/var/log", "/run/user"] {
        assert!(!ok(&e.probe(&[], &["exists", p])), "{p} existe");
    }
    // /tmp só tem o caminho até a pasta de trabalho do teste (que vive em /tmp);
    // /run só tem o resolv.conf montado.
    let out = e.shell(&[], "ls -A /tmp; echo ---; find /run -type f");
    let s = stdout(&out);
    let (tmp, run) = s.split_once("---\n").unwrap();
    let first = e
        .work
        .strip_prefix("/tmp")
        .unwrap()
        .components()
        .next()
        .unwrap();
    assert_eq!(tmp.trim(), first.as_os_str().to_str().unwrap(), "{s}");
    for f in run.lines() {
        assert!(
            f.starts_with("/run/systemd/resolve/"),
            "arquivo inesperado em /run: {f}"
        );
    }
}

// ---- 5.4 /etc ----

#[test]
fn arquivos_sensiveis_de_etc_aparecem_vazios() {
    let e = Env::new();
    let all: Vec<&str> = sandmux_policy::lists::ETC_PRIVILEGE
        .iter()
        .chain(sandmux_policy::lists::ETC_SERVICE_SECRETS)
        .chain(sandmux_policy::lists::ETC_NETWORK)
        .copied()
        .filter(|p| Path::new(p).symlink_metadata().is_ok())
        .collect();
    assert!(all.contains(&"/etc/shadow"));
    let script = all
        .iter()
        .map(|p| format!("if [ -d {p} ]; then echo \"{p} $(ls -A {p} | wc -l)\"; else echo \"{p} $(wc -c < {p})\"; fi"))
        .collect::<Vec<_>>()
        .join("; ");
    let out = e.shell(&[], &script);
    let s = stdout(&out);
    assert_eq!(s.lines().count(), all.len(), "{s}\n{}", stderr(&out));
    for l in s.lines() {
        assert!(l.ends_with(" 0"), "não está vazio: {l}");
    }
}

#[test]
fn certificados_publicos_continuam_legiveis() {
    let e = Env::new();
    let out = e.shell(
        &[],
        "ls /etc/ssl/certs | wc -l; head -c 27 /etc/ssl/certs/ca-certificates.crt",
    );
    let s = stdout(&out);
    assert!(s.lines().next().unwrap().trim().parse::<i32>().unwrap() > 10);
    assert!(s.contains("-----BEGIN CERTIFICATE-----"), "{s}");
}

// ---- 5.5 resolv.conf ----

#[test]
fn resolv_conf_resolve_o_link_para_run() {
    let e = Env::new();
    let out = e.shell(&[], "grep -c nameserver /etc/resolv.conf");
    assert!(
        stdout(&out).trim().parse::<i32>().unwrap_or(0) >= 1,
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
}

// ---- 5.6 /dev ----

#[test]
fn dev_minimo_com_gpu_e_opcoes() {
    let e = Env::new();
    let list = |extra: &[&str]| {
        let out = e.shell(
            extra,
            "ls /dev /dev/dri 2>/dev/null | sort -u | tr '\\n' ' '",
        );
        stdout(&out)
    };
    let default = list(&[]);
    for d in ["null", "zero", "urandom", "tty", "pts", "shm", "ptmx"] {
        assert!(
            default.split_whitespace().any(|x| x == d),
            "{d} ausente: {default}"
        );
    }
    for d in [
        "kvm",
        "sda",
        "nvme0n1",
        "input",
        "video0",
        "card0",
        "card1",
        "nvidia-modeset",
    ] {
        assert!(
            !default.split_whitespace().any(|x| x == d),
            "{d} presente: {default}"
        );
    }
    let has_gpu = Path::new("/dev/dri/renderD128").exists() || Path::new("/dev/nvidia0").exists();
    if has_gpu {
        assert!(
            default.contains("renderD") || default.contains("nvidia0"),
            "{default}"
        );
        let no_gpu = list(&["--no-gpu"]);
        assert!(
            !no_gpu.contains("renderD") && !no_gpu.contains("nvidia"),
            "{no_gpu}"
        );
    }
    if Path::new("/dev/kvm").exists() {
        assert!(list(&["--kvm"]).split_whitespace().any(|x| x == "kvm"));
    }
}

// ---- 5.7 pasta de entrada ----

#[test]
fn pasta_de_entrada_somente_leitura() {
    let e = Env::new();
    let first = e.shell(&[], "true");
    assert!(first.status.success(), "{}", stderr(&first));
    std::fs::write(e.data_dir().join("inbox/relatorio.pdf"), "conteudo").unwrap();
    let out = e.shell(
        &[],
        "cat ~/inbox/relatorio.pdf; echo; cat \"$SANDMUX_INBOX/relatorio.pdf\"",
    );
    assert_eq!(stdout(&out), "conteudo\nconteudo");
    for args in [
        &["write", "~/inbox/novo"][..],
        &["unlink", "~/inbox/relatorio.pdf"][..],
    ] {
        let p = args[1].replace('~', e.home.to_str().unwrap());
        assert!(!ok(&e.probe(&[], &[args[0], &p])), "{args:?}");
    }
    assert!(e.data_dir().join("inbox/relatorio.pdf").exists());
}

// ---- 5.8 --ro, --rw e aviso de credencial ----

#[test]
fn ro_e_rw_acrescentam_pastas() {
    let e = Env::new();
    let docs = e.home.join("docs");
    let out_dir = e.home.join("saida");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(docs.join("a"), "lido").unwrap();
    let out = e.shell(
        &[
            "--ro",
            docs.to_str().unwrap(),
            "--rw",
            out_dir.to_str().unwrap(),
        ],
        &format!(
            "cat {d}/a; touch {d}/b 2>/dev/null || echo ' negado'; echo ok > {o}/c",
            d = docs.display(),
            o = out_dir.display()
        ),
    );
    assert_eq!(stdout(&out), "lido negado\n");
    assert!(!stderr(&out).contains("credenciais"), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(out_dir.join("c")).unwrap(), "ok\n");
}

#[test]
fn aviso_ao_montar_credencial_conhecida() {
    let e = Env::new();
    std::fs::create_dir_all(e.home.join(".kube")).unwrap();
    std::fs::create_dir_all(e.home.join(".config/gh")).unwrap();
    let out = e.shell(&["--ro", e.home.join(".kube").to_str().unwrap()], "true");
    assert!(
        stderr(&out).contains(".kube") && stderr(&out).contains("credenciais reais"),
        "{}",
        stderr(&out)
    );
    let out = e.shell(&["--ro", e.home.join(".config").to_str().unwrap()], "true");
    assert!(stderr(&out).contains(".config/gh"), "{}", stderr(&out));
    let out = e.shell(
        &["--ro", e.home.join("nao-existe").to_str().unwrap()],
        "echo executou",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("não existe"));
}

// ---- 5.9 ambiente ----

#[test]
fn ambiente_construido_do_zero_e_pass_env() {
    let e = Env::new();
    let mut cmd = e.cmd(
        &e.work,
        &[
            "run",
            "shell",
            "--pass-env",
            "OPENROUTER_API_KEY",
            "--",
            "-c",
            "env | cut -d= -f1 | sort | tr '\\n' ' '; echo; echo $OPENROUTER_API_KEY",
        ],
    );
    cmd.env("OPENROUTER_API_KEY", "valor-de-teste")
        .env("AWS_SECRET_ACCESS_KEY", "nao-pode-entrar")
        .env("SSH_AUTH_SOCK", "/run/x")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/y")
        .env("DISPLAY", ":0")
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("LC_ALL", "C.UTF-8");
    let out = cmd.output().unwrap();
    let s = stdout(&out);
    let names: Vec<&str> = s.lines().next().unwrap().split_whitespace().collect();
    for bad in [
        "AWS_SECRET_ACCESS_KEY",
        "SSH_AUTH_SOCK",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "WAYLAND_DISPLAY",
    ] {
        assert!(!names.contains(&bad), "{bad} entrou");
    }
    for good in [
        "HOME",
        "PATH",
        "TERM",
        "LC_ALL",
        "XDG_CONFIG_HOME",
        "SANDMUX_INBOX",
        "OPENROUTER_API_KEY",
    ] {
        assert!(names.contains(&good), "{good} faltou: {names:?}");
    }
    assert_eq!(s.lines().nth(1), Some("valor-de-teste"));
    let err = stderr(&out);
    assert!(err.contains("--pass-env") && err.contains("OPENROUTER_API_KEY"));
    assert!(!err.contains("valor-de-teste"), "o aviso mostrou o valor");

    let out = e.run(
        &e.work,
        &[
            "run",
            "shell",
            "--pass-env",
            "NAO_EXISTE",
            "--",
            "-c",
            "echo executou",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(!stdout(&out).contains("executou"));
}

#[test]
fn ambiente_do_processo_pai_inacessivel() {
    let e = Env::new();
    let v = e.probe(&[], &["read", "/proc/1/environ"]);
    assert!(!ok(&v));
}
