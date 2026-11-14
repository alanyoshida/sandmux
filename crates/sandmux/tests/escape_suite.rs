//! Tarefa 11.1: a suíte completa de fuga, dentro de um sandbox lançado por
//! `sandmux run`, num HOME falso com segredos, socket do host e repositório.

mod common;
use common::cli::*;
use common::*;

use std::os::unix::net::UnixListener;
use std::process::Command;

fn abstract_listener(name: &str) -> std::os::fd::OwnedFd {
    use std::os::fd::FromRawFd;
    // SAFETY: estruturas zeradas e preenchidas com tamanhos conferidos.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (i, b) in name.as_bytes().iter().enumerate() {
            addr.sun_path[i + 1] = *b as libc::c_char;
        }
        let len = std::mem::size_of::<libc::sa_family_t>() + 1 + name.len();
        assert_eq!(
            libc::bind(fd, &addr as *const _ as *const libc::sockaddr, len as u32),
            0
        );
        assert_eq!(libc::listen(fd, 8), 0);
        std::os::fd::OwnedFd::from_raw_fd(fd)
    }
}

#[test]
fn suite_completa_de_fuga() {
    let e = Env::new();
    let h = &e.home;
    std::fs::create_dir_all(h.join(".ssh")).unwrap();
    std::fs::write(h.join(".ssh/id_ed25519"), "chave privada").unwrap();
    std::fs::create_dir_all(h.join("outro-projeto")).unwrap();
    std::fs::write(h.join("outro-projeto/segredo.txt"), "x").unwrap();
    std::fs::create_dir_all(h.join(".cargo/bin")).unwrap();
    std::fs::write(h.join(".cargo/credentials.toml"), "token = \"real\"").unwrap();
    std::fs::create_dir_all(h.join(".cache/huggingface")).unwrap();
    std::fs::write(h.join(".cache/huggingface/token"), "hf_real").unwrap();
    let ok_git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&e.work)
        .status()
        .unwrap()
        .success();
    assert!(ok_git);
    let sock_dir = tempfile::tempdir().unwrap();
    let sock = sock_dir.path().join("host.sock");
    let _l = UnixListener::bind(&sock).unwrap();
    let abs = format!("sandmux-suite-{}", std::process::id());
    let _a = abstract_listener(&abs);
    // O daemon do sandmux rodando, com o socket dele e um abstrato homônimo.
    let started = e.run(&e.work, &["ls"]);
    assert!(started.status.success(), "{}", stderr(&started));
    assert!(e.socket().exists());
    let daemon_abs = e.socket().display().to_string();
    let _d = abstract_listener(&daemon_abs);
    // A configuração do sandmux visível em somente leitura.
    let config_text = "# do usuário\n[global]\nkvm = false\n";
    e.write_config(config_text);

    let args = [
        format!("ssh={}", h.join(".ssh").display()),
        format!("other={}", h.join("outro-projeto").display()),
        format!("hostsock={}", sock.display()),
        format!("abstract={abs}"),
        format!("covered={}", h.join(".cargo/credentials.toml").display()),
        format!("covered={}", h.join(".cache/huggingface/token").display()),
        "covered=/etc/shadow".into(),
        format!("git={}", e.work.join(".git").display()),
        format!("inbox={}", h.join("inbox").display()),
        format!("daemonsock={}", e.socket().display()),
        format!("daemonabstract={daemon_abs}"),
        format!("config={}", e.config_file().display()),
    ];
    let script = std::iter::once(format!("{PROBE} suite"))
        .chain(args.iter().map(|a| shell_quote(a)))
        .collect::<Vec<_>>()
        .join(" ");
    let out = e.shell(&["--ro", e.config.to_str().unwrap()], &script);
    let report = stdout(&out);
    let escaped: Vec<&str> = report.lines().filter(|l| l.contains("CONSEGUIU")).collect();
    assert!(
        escaped.is_empty(),
        "tentativas que conseguiram:\n{}\n{}",
        escaped.join("\n"),
        stderr(&out)
    );
    let summary: serde_json::Value = serde_json::from_str(report.lines().last().unwrap()).unwrap();
    assert_eq!(summary["resumo"]["conseguiram"], 0);
    assert!(
        summary["resumo"]["tentativas"].as_u64().unwrap() >= 56,
        "{report}"
    );
    assert_eq!(out.status.code(), Some(0));
    // Nada mudou no host.
    assert_eq!(
        std::fs::read_to_string(h.join(".cargo/credentials.toml")).unwrap(),
        "token = \"real\""
    );
    assert!(!e.work.join(".git/hooks/pre-commit").exists());
    assert_eq!(
        std::fs::read_to_string(e.config_file()).unwrap(),
        config_text,
        "a configuração do sandmux mudou"
    );
    std::fs::write(
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sandmux-suite-relatorio.jsonl"),
        &report,
    )
    .unwrap();
}

/// Arquivos de configuração plantados na pasta de trabalho não mudam a
/// política do lançamento seguinte.
#[test]
fn configuracao_plantada_na_pasta_de_trabalho_e_ignorada() {
    let e = Env::new();
    let plant = "mkdir -p .config/sandmux && \
        printf '[global]\\nrw = [\"~\"]\\nkvm = true\\n' > .sandmux.toml && \
        cp .sandmux.toml .config/sandmux/config.toml && \
        cp .sandmux.toml sandmux.toml && echo plantado";
    let out = e.shell(&[], plant);
    assert_eq!(stdout(&out), "plantado\n", "{}", stderr(&out));
    assert!(e.work.join(".config/sandmux/config.toml").exists());
    // O HOME real (falso do teste) tem um arquivo que só apareceria com `rw = ["~"]`.
    std::fs::write(e.home.join("marcador-home"), "x").unwrap();
    let out = e.shell(
        &[],
        &format!(
            "test -e {}/marcador-home && echo home-montado || echo home-fora; ls /dev | grep -x kvm || echo sem-kvm",
            e.home.display()
        ),
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "home-fora\nsem-kvm\n");
}
