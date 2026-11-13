//! Grupo 7: proteção do `.git` e identidade do git.

mod common;
use common::cli::*;

use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "host")
        .env("GIT_AUTHOR_EMAIL", "host@x")
        .env("GIT_COMMITTER_NAME", "host")
        .env("GIT_COMMITTER_EMAIL", "host@x")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// HOME falso com identidade no git e um repositório na pasta de trabalho.
fn with_repo() -> Env {
    let e = Env::new();
    std::fs::write(e.home.join(".gitconfig"), "[user]\n\tname = Ana Teste\n\temail = ana@exemplo.com\n[alias]\n\tst = status\n[credential]\n\thelper = store\n").unwrap();
    git(&e.work, &["init", "-q", "-b", "main"]);
    std::fs::write(e.work.join("a.txt"), "a").unwrap();
    git(&e.work, &["add", "."]);
    git(&e.work, &["commit", "-q", "-m", "inicial"]);
    e
}

fn sh(e: &Env, extra: &[&str], script: &str) -> (String, String, Option<i32>) {
    let out = e.shell(extra, script);
    (stdout(&out), stderr(&out), out.status.code())
}

#[test]
fn commit_branch_merge_e_stash_funcionam() {
    let e = with_repo();
    let (out, err, code) = sh(
        &e,
        &[],
        "set -e; git checkout -q -b feat; echo b > b.txt; git add b.txt; git commit -q -m feat; git checkout -q main; git merge -q --no-edit feat; echo c > a.txt; git stash -q; git stash pop -q; git log --format='%an <%ae> %s' -1; echo fim",
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("Ana Teste <ana@exemplo.com> feat"), "{out}");
    assert!(git(&e.work, &["log", "--format=%s", "main"]).contains("feat"));
}

#[test]
fn hooks_e_config_nao_podem_ser_alterados() {
    let e = with_repo();
    let config_before = std::fs::read_to_string(e.work.join(".git/config")).unwrap();
    let (out, _, _) = sh(
        &e,
        &[],
        "echo 'echo pwned' > .git/hooks/pre-commit 2>/dev/null && echo hook-criado || echo hook-negado; \
         git config core.fsmonitor 'echo pwned' 2>/dev/null && echo config-alterada || echo config-negada; \
         git remote add evil https://exemplo.com/x.git 2>/dev/null && echo remote-criado || echo remote-negado; \
         mv .git .git2 2>/dev/null && echo git-renomeado || echo rename-negado; \
         rm .git/config 2>/dev/null && echo config-apagada || echo apagar-negado",
    );
    assert_eq!(
        out,
        "hook-negado\nconfig-negada\nremote-negado\nrename-negado\napagar-negado\n"
    );
    assert!(!e.work.join(".git/hooks/pre-commit").exists());
    assert_eq!(
        std::fs::read_to_string(e.work.join(".git/config")).unwrap(),
        config_before
    );
}

#[test]
fn hooks_de_submodulo_sao_protegidos() {
    let e = with_repo();
    let sub = e.dir.path().join("subrepo");
    std::fs::create_dir_all(&sub).unwrap();
    git(&sub, &["init", "-q", "-b", "main"]);
    git(&sub, &["commit", "-q", "--allow-empty", "-m", "s"]);
    git(
        &e.work,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            sub.to_str().unwrap(),
            "lib",
        ],
    );
    git(&e.work, &["commit", "-q", "-m", "sub"]);
    let (out, _, _) = sh(
        &e,
        &[],
        "echo x > .git/modules/lib/hooks/post-checkout 2>/dev/null && echo criado || echo negado; \
         echo x >> .git/modules/lib/config 2>/dev/null && echo alterado || echo negado",
    );
    assert_eq!(out, "negado\nnegado\n");
}

#[test]
fn worktree_funciona_e_protege_o_repositorio_principal() {
    let e = with_repo();
    let wt = e.home.join("projetos/wt");
    git(&e.work, &["worktree", "add", "-q", wt.to_str().unwrap()]);
    let (out, err, code) = sh_in(
        &e,
        &wt,
        "echo w > w.txt; git add w.txt; git commit -q -m no-worktree && echo commit-ok; \
         echo x > ../app/.git/hooks/pre-commit 2>/dev/null && echo hook-criado || echo hook-negado; \
         echo 'gitdir: /tmp/evil' > .git 2>/dev/null && echo gitfile-alterado || echo gitfile-negado",
    );
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "commit-ok\nhook-negado\ngitfile-negado\n");
    assert!(git(&e.work, &["log", "--all", "--format=%s"]).contains("no-worktree"));
}

fn sh_in(e: &Env, cwd: &Path, script: &str) -> (String, String, Option<i32>) {
    let out = e.shell_in(cwd, &[], script);
    (stdout(&out), stderr(&out), out.status.code())
}

#[test]
fn subpasta_sem_repo_por_padrao_e_com_repo_por_opcao() {
    let e = with_repo();
    let sub = e.work.join("pacote");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(e.work.join("fora.txt"), "fora").unwrap();
    let out = e.shell_in(&sub, &[], "git status >/dev/null 2>&1 && echo git-ok || echo sem-git; ls ../fora.txt 2>/dev/null || echo sem-resto");
    assert_eq!(stdout(&out), "sem-git\nsem-resto\n");
    assert!(stderr(&out).contains("--repo"), "{}", stderr(&out));

    let out = e.shell_in(
        &sub,
        &["--repo"],
        "echo p > p.txt; git add p.txt && git commit -q -m pacote && echo commit-ok; echo x > ../fora.txt 2>/dev/null && echo escreveu-fora || echo fora-negado; cat ../fora.txt",
    );
    assert_eq!(
        stdout(&out),
        "commit-ok\nfora-negado\nfora",
        "{}",
        stderr(&out)
    );
    assert!(git(&e.work, &["log", "--format=%s", "-1"]).contains("pacote"));
}

#[test]
fn identidade_minima_no_home_sintetico() {
    let e = with_repo();
    let (out, _, _) = sh(
        &e,
        &[],
        "cat ~/.gitconfig; git config --global user.extra x; cat ~/.gitconfig | wc -l",
    );
    assert!(
        out.contains("name = Ana Teste") && out.contains("email = ana@exemplo.com"),
        "{out}"
    );
    assert!(!out.contains("alias") && !out.contains("helper"), "{out}");
    // O ajuste feito no HOME sintético permanece.
    let (out, _, _) = sh(&e, &[], "git config --global user.extra");
    assert_eq!(out, "x\n");
}

#[test]
fn sem_identidade_avisa() {
    let e = Env::new();
    git(&e.work, &["init", "-q"]);
    let out = e.shell(&[], "true");
    assert!(
        stderr(&out).contains("não define user.name e user.email"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn pasta_sem_repositorio_nao_avisa() {
    let e = Env::new();
    std::fs::write(
        e.home.join(".gitconfig"),
        "[user]\n\tname = A\n\temail = a@a\n",
    )
    .unwrap();
    let out = e.shell(&[], "true");
    assert!(out.status.success());
    assert!(!stderr(&out).contains("git"), "{}", stderr(&out));
}
