//! Grupo 6: toolchains e caches com camada privada de escrita.

mod common;
use common::cli::*;
use common::*;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn exe(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// HOME falso com uma toolchain de cada tipo da lista.
fn with_toolchains() -> Env {
    let e = Env::new();
    let h = &e.home;
    exe(&h.join(".cargo/bin/cargo"), "echo cargo-falso");
    exe(
        &h.join(".nvm/versions/node/v20.1.0/bin/node"),
        "echo node-falso",
    );
    std::fs::create_dir_all(h.join(".nvm/alias")).unwrap();
    std::fs::write(h.join(".nvm/alias/default"), "20").unwrap();
    exe(&h.join(".bun/bin/bun"), "echo bun-falso");
    exe(&h.join(".pyenv/shims/python"), "echo python-falso");
    exe(&h.join("go/bin/gotool"), "echo go-falso");
    exe(&h.join(".local/bin/uv"), "echo uv-falso");
    exe(&h.join(".local/bin/kubectl"), "echo nao-devia-aparecer");
    std::fs::create_dir_all(h.join(".rustup/toolchains")).unwrap();
    std::fs::create_dir_all(h.join(".npm/_cacache")).unwrap();
    std::fs::write(h.join(".npm/_cacache/pacote"), "cache do host").unwrap();
    e
}

#[test]
fn toolchains_da_lista_entram_e_o_resto_nao() {
    let e = with_toolchains();
    let out = e.shell(
        &[],
        "cargo; node; bun; python; gotool; uv; command -v kubectl || echo sem-kubectl; echo $PATH",
    );
    let s = stdout(&out);
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(
        &lines[..7],
        &[
            "cargo-falso",
            "node-falso",
            "bun-falso",
            "python-falso",
            "go-falso",
            "uv-falso",
            "sem-kubectl"
        ],
        "{s}\n{}",
        stderr(&out)
    );
    let path = lines[7];
    let h = e.home.display().to_string();
    let idx = |p: &str| {
        path.split(':')
            .position(|x| x == p)
            .unwrap_or_else(|| panic!("{p} fora do PATH: {path}"))
    };
    assert!(idx(&format!("{h}/.cargo/bin")) < idx("/usr/bin"));
    idx(&format!("{h}/.nvm/versions/node/v20.1.0/bin"));
    idx(&format!("{h}/.pyenv/shims"));
}

#[test]
fn toolchain_ausente_e_ignorada() {
    let e = Env::new();
    let out = e.shell(&[], "ls -A ~ | tr '\\n' ' '");
    assert!(out.status.success(), "{}", stderr(&out));
    for d in [".cargo", ".nvm", ".pyenv", "go"] {
        assert!(
            !stdout(&out).split_whitespace().any(|x| x == d),
            "{d}: {}",
            stdout(&out)
        );
    }
}

#[test]
fn escrita_vai_para_a_camada_persiste_e_nao_toca_o_host() {
    let e = with_toolchains();
    let out = e.shell(&[], "echo instalado > ~/.cargo/bin/nova; rm ~/.cargo/bin/cargo; echo novo > ~/.npm/_cacache/outro; echo ok");
    assert_eq!(stdout(&out), "ok\n", "{}", stderr(&out));
    assert!(!e.home.join(".cargo/bin/nova").exists());
    assert!(e.home.join(".cargo/bin/cargo").exists());
    assert!(!e.home.join(".npm/_cacache/outro").exists());
    let out = e.shell(&[], "cat ~/.cargo/bin/nova; ls ~/.cargo/bin | tr '\\n' ' '");
    assert_eq!(stdout(&out), "instalado\nnova ");
    // Uma ferramenta nova instalada no host aparece na execução seguinte.
    exe(&e.home.join(".cargo/bin/do-host"), "echo do-host");
    let out = e.shell(&[], "do-host");
    assert_eq!(stdout(&out), "do-host\n");
}

#[test]
fn credenciais_das_toolchains_e_caches_aparecem_vazias() {
    let e = with_toolchains();
    std::fs::write(
        e.home.join(".cargo/credentials.toml"),
        "token = \"segredo\"",
    )
    .unwrap();
    std::fs::create_dir_all(e.home.join(".cache/huggingface")).unwrap();
    std::fs::write(e.home.join(".cache/huggingface/token"), "hf_segredo").unwrap();
    std::fs::create_dir_all(e.home.join(".local/share/uv/credentials")).unwrap();
    std::fs::write(e.home.join(".local/share/uv/credentials/x"), "segredo").unwrap();
    for (p, expect) in [
        (".cargo/credentials.toml", "0 bytes"),
        (".cache/huggingface/token", "0 bytes"),
        (".local/share/uv/credentials", "pasta com 0 entradas"),
    ] {
        let v = e.probe(&[], &["read", e.home.join(p).to_str().unwrap()]);
        assert_eq!(v["detail"], expect, "{p}: {v}");
    }
    let f = e.home.join(".cargo/credentials.toml");
    let f = f.to_str().unwrap();
    assert!(!ok(&e.probe(&[], &["unlink", f])));
    assert!(!ok(&e.probe(&[], &["rename", f, &format!("{f}.x")])));
    assert!(!ok(&e.probe(&[], &["write", f])));
    assert!(!ok(&e.probe(
        &[],
        &[
            "rename",
            e.home.join(".cargo").to_str().unwrap(),
            e.home.join(".c2").to_str().unwrap()
        ]
    )));
}

#[test]
fn segunda_execucao_simultanea_usa_camada_temporaria() {
    let e = with_toolchains();
    let mut first = e
        .cmd(
            &e.work,
            &["run", "shell", "--", "-c", "echo a > ~/.cargo/a; sleep 3"],
        )
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(800));
    let second = e.shell(&[], "echo b > ~/.cargo/b; ls ~/.cargo | tr '\\n' ' '");
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(
        stderr(&second).contains("outra execução"),
        "{}",
        stderr(&second)
    );
    assert!(first.wait().unwrap().success());
    let third = e.shell(&[], "ls ~/.cargo | tr '\\n' ' '");
    let s = stdout(&third);
    assert!(s.split_whitespace().any(|x| x == "a"), "{s}");
    assert!(
        !s.split_whitespace().any(|x| x == "b"),
        "a camada temporária foi guardada: {s}"
    );
    assert!(!stderr(&third).contains("outra execução"));
}

#[test]
fn modo_de_recuo_sem_overlay() {
    let e = with_toolchains();
    let out = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--",
                "-c",
                "cargo; touch ~/.cargo/x 2>/dev/null || echo toolchain-somente-leitura; ls -A ~/.npm | wc -l; echo meu > ~/.npm/proprio; echo fim",
            ],
        )
        .env("SANDMUX_DISABLE_OVERLAY", "1")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&out),
        "cargo-falso\ntoolchain-somente-leitura\n0\nfim\n",
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("camada de escrita não está disponível"));
    assert!(!e.home.join(".npm/proprio").exists());
    assert!(!e.home.join(".cargo/x").exists());
}
