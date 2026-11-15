//! Importação da configuração do shell para o preset `shell`.

mod common;
use common::cli::*;

const TOKEN: &str = "ghp_0123456789abcdefABCDEF0123456789abcd";

fn bashrc(e: &Env) {
    let w = |rel: &str, s: &str| {
        let p = e.home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    };
    w(
        ".bashrc",
        &format!("export GITHUB_TOKEN={TOKEN}\nalias oi='echo ola-do-alias'\nPS1='meu> '\n"),
    );
    w(".oh-my-bash/oh-my-bash.sh", "# omb\n");
    w(".oh-my-bash/.git/HEAD", "ref: refs/heads/master\n");
    w(".zshrc", "alias zz=1\n");
    w(".config/starship.toml", "format = '$all'\n");
}

fn shell(e: &Env, args: &[&str]) -> std::process::Output {
    e.cmd(&e.work, args)
        .env("SHELL", "/bin/bash")
        .output()
        .unwrap()
}

#[test]
fn primeira_execucao_com_import_traz_o_bashrc_com_falso_e_marca() {
    let e = Env::new();
    bashrc(&e);
    let out = shell(
        &e,
        &[
            "run",
            "shell",
            "--import",
            "--",
            "-ic",
            "echo token=$GITHUB_TOKEN; oi; echo \"ps1=$PS1\"",
        ],
    );
    let s = stdout(&out);
    assert!(out.status.success(), "{s}\n{}", stderr(&out));
    assert!(s.contains("token=sandmux-fake-"), "{s}");
    assert!(s.contains("ola-do-alias"), "{s}");
    assert!(s.contains("ps1=[sandmux] meu> "), "{s}");
    let homes = e.data_dir().join("homes/shell");
    let rc = std::fs::read_to_string(homes.join(".bashrc")).unwrap();
    assert!(
        !rc.contains(TOKEN) && !rc.contains("Gerado pelo sandmux"),
        "{rc}"
    );
    assert!(homes.join(".oh-my-bash/oh-my-bash.sh").is_file());
    assert!(!homes.join(".oh-my-bash/.git").exists());
    assert!(!homes.join(".zshrc").exists());
    assert!(!homes.join(".config/starship.toml").exists());
    // O original não muda.
    let real = std::fs::read_to_string(e.home.join(".bashrc")).unwrap();
    assert!(real.contains(TOKEN));
}

#[test]
fn primeira_execucao_sem_terminal_comeca_sem_importar() {
    let e = Env::new();
    bashrc(&e);
    let out = shell(
        &e,
        &["run", "shell", "--", "-ic", "echo token=$GITHUB_TOKEN"],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("token=\n"), "{}", stdout(&out));
    let rc = std::fs::read_to_string(e.data_dir().join("homes/shell/.bashrc")).unwrap();
    assert!(rc.contains("Gerado pelo sandmux"), "{rc}");

    // Num HOME já criado, o arquivo gerado aparece como diferente.
    let out = shell(&e, &["import", "shell"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("diferentes do host") && err.contains(".bashrc"),
        "{err}"
    );
    assert!(err.contains(".oh-my-bash"), "{err}");
    let rc = std::fs::read_to_string(e.data_dir().join("homes/shell/.bashrc")).unwrap();
    assert!(rc.contains("Gerado pelo sandmux"), "{rc}");

    // Com --overwrite, o real (com o falso) entra.
    let out = shell(&e, &["import", "shell", "--overwrite"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("GITHUB_TOKEN"), "{err}");
    assert!(!err.contains(TOKEN), "{err}");
    let out = shell(
        &e,
        &["run", "shell", "--", "-ic", "echo token=$GITHUB_TOKEN"],
    );
    assert!(
        stdout(&out).contains("token=sandmux-fake-"),
        "{}",
        stdout(&out)
    );
}
