//! Aberturas de arquivo pelo supervisor e o registro de arquivos.

mod common;

use std::os::unix::fs::PermissionsExt;

use common::cli::{Env, stderr, stdout};

fn shell(env: &Env, script: &str) -> String {
    let o = env.shell(&[], script);
    assert!(
        o.status.success(),
        "código {:?}\n{}\n{}",
        o.status.code(),
        stdout(&o),
        stderr(&o)
    );
    stdout(&o)
}

/// Linhas do registro de arquivos deste ambiente.
fn file_lines(env: &Env) -> Vec<serde_json::Value> {
    let dir = env.state.join("sandmux/files");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "jsonl") {
            let text = std::fs::read_to_string(e.path()).unwrap();
            out.extend(text.lines().map(|l| serde_json::from_str(l).unwrap()));
        }
    }
    out
}

#[test]
fn aberturas_comuns_pelo_supervisor() {
    let env = Env::new();
    // Um link absoluto na pasta de trabalho resolve dentro da raiz do sandbox:
    // o /etc/passwd do sandbox é gerado, diferente do host.
    std::os::unix::fs::symlink("/etc/passwd", env.work.join("lnk")).unwrap();
    let out = shell(
        &env,
        "cmp -s lnk /etc/passwd && echo link-ok; \
         echo oi > a.txt && cat a.txt; \
         cat <(echo subst); \
         head -c 5 /proc/self/status; echo; \
         head -c 3 /proc/net/dev; echo; \
         cat /dev/stdin <<< entrada; \
         (umask 077; echo s > secreto.txt); \
         sleep 30 & p=$!; \
         ls /proc/$p/cwd/ >/dev/null 2>&1 && echo cwd-alheio || echo cwd-negado; \
         cat /proc/$p/root/etc/hostname >/dev/null 2>&1 && echo root-alheio || echo root-negado; \
         head -c 4 /proc/$p/stat >/dev/null && echo stat-ok; \
         kill $p",
    );
    for want in [
        "link-ok",
        "oi",
        "subst",
        "Name:",
        "Int",
        "entrada",
        "cwd-negado",
        "root-negado",
        "stat-ok",
    ] {
        assert!(out.contains(want), "falta {want}: {out}");
    }
    let mode = std::fs::metadata(env.work.join("secreto.txt"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn registro_das_pastas_do_agente() {
    let env = Env::new();
    std::fs::write(env.work.join("a.txt"), "x").unwrap();
    shell(
        &env,
        "cat a.txt a.txt >/dev/null; echo b > b.txt; ls /usr/bin >/dev/null; \
         cat /etc/hostname >/dev/null; echo h > ~/no-home.txt",
    );
    let lines = file_lines(&env);
    let has = |name: &str, access: &str| {
        lines
            .iter()
            .filter(|l| {
                l["path"].as_str().is_some_and(|p| p.ends_with(name)) && l["access"] == access
            })
            .count()
    };
    assert_eq!(has("/a.txt", "r"), 1, "{lines:?}");
    assert_eq!(has("/b.txt", "create"), 1, "{lines:?}");
    assert_eq!(has("/no-home.txt", "create"), 1, "{lines:?}");
    // As aberturas fora das pastas do agente não entram; as execuções
    // (`exec`) entram de qualquer lugar.
    assert!(
        !lines.iter().any(|l| l["access"] != "exec"
            && l["path"]
                .as_str()
                .is_some_and(|p| p.starts_with("/usr") || p.starts_with("/etc"))),
        "{lines:?}"
    );
    let tag = lines[0]["tag"].as_str().unwrap();
    assert!(tag.starts_with("run-"), "{tag}");
    let dir = env.state.join("sandmux/files");
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn sandmux_files_mostra_a_sessao() {
    let env = Env::new();
    let mut cmd = env.cmd(&env.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = common::pty::Term::spawn(cmd, 160, 30);
    t.write(b"echo b > criado.txt; echo FIM-$((1+1))\r");
    t.wait_text("FIM-2");
    let o = env.run(&env.work, &["files", "1"]);
    let out = stdout(&o);
    assert!(
        out.lines()
            .any(|l| l.contains("create") && l.contains("criado.txt") && l.contains(" 1 ")),
        "{out}\n{}",
        stderr(&o)
    );
    let other = stdout(&env.run(&env.work, &["files", "2"]));
    assert!(other.contains("Nenhum acesso"), "{other}");
    // Sem o auxiliar root (ou sem o BPF LSM), as aberturas ficam com o
    // supervisor, e o resumo diz por quê.
    if std::env::var_os("SANDMUX_HELPER_SOCKET").is_none() {
        let st = stdout(&env.run(&env.work, &["stats", "1"]));
        assert!(st.contains("aberturas de arquivo: supervisor: "), "{st}");
    }
    t.write(b"\x02d");
    t.wait_exit();
}

fn shell_with(env: &Env, extra: &[&str], script: &str) -> String {
    let o = env.shell(extra, script);
    assert!(o.status.success(), "{}\n{}", stdout(&o), stderr(&o));
    stdout(&o)
}

#[test]
fn segredos_escondidos_e_criados_pelo_agente() {
    let env = Env::new();
    std::fs::write(env.work.join(".env"), "TOKEN=real").unwrap();
    std::fs::write(env.work.join(".env.example"), "TOKEN=").unwrap();
    std::fs::create_dir_all(env.work.join("k")).unwrap();
    std::fs::write(env.work.join("k/id_ed25519"), "privada").unwrap();
    std::fs::write(env.work.join("k/id_ed25519.pub"), "publica").unwrap();
    let out = shell_with(
        &env,
        &[],
        "cat .env 2>&1; cat .env.example; cat k/id_ed25519 2>&1; cat k/id_ed25519.pub; echo; \
         echo novo > .env 2>&1 || echo escrita-negada; \
         echo minha > chave.pem && cat chave.pem; ls -a | grep -c '^.env$'",
    );
    assert!(
        out.contains("No such file") || out.contains("Arquivo ou diretório inexistente"),
        "{out}"
    );
    assert!(!out.contains("TOKEN=real"), "{out}");
    assert!(out.contains("TOKEN="), "{out}");
    assert!(!out.contains("privada"), "{out}");
    assert!(out.contains("publica"), "{out}");
    assert!(out.contains("escrita-negada"), "{out}");
    assert!(out.contains("minha"), "{out}");
    // O .env do usuário não mudou, e o nome ainda aparece no ls.
    assert_eq!(
        std::fs::read_to_string(env.work.join(".env")).unwrap(),
        "TOKEN=real"
    );
    assert!(out.trim_end().ends_with('1'), "{out}");
    // No lançamento seguinte, o segredo criado pelo agente continua dele.
    let out = shell_with(
        &env,
        &[],
        "cat chave.pem; cat .env 2>/dev/null || echo env-escondido",
    );
    assert!(
        out.contains("minha") && out.contains("env-escondido"),
        "{out}"
    );
    let lines = file_lines(&env);
    assert!(
        lines.iter().any(|l| l["blocked"] == "segredo"
            && l["path"].as_str().is_some_and(|p| p.ends_with("/.env"))),
        "{lines:?}"
    );
}

#[test]
fn negar_e_liberar_pela_aba() {
    let env = Env::new();
    std::fs::write(env.work.join("dados.sql"), "segredo-sql").unwrap();
    std::fs::write(env.work.join("ok.sql"), "liberado").unwrap();
    let out = shell_with(
        &env,
        &["--file-deny", "*.sql", "--file-allow", "ok.sql"],
        "cat dados.sql 2>&1; cat ok.sql",
    );
    assert!(
        out.contains("Permission denied") || out.contains("Permissão negada"),
        "{out}"
    );
    assert!(
        !out.contains("segredo-sql") && out.contains("liberado"),
        "{out}"
    );
    assert!(
        file_lines(&env)
            .iter()
            .any(|l| l["blocked"] == "file-deny: *.sql"),
        "{:?}",
        file_lines(&env)
    );
    let o = env.shell(&["--file-deny", "a/b"], "true");
    assert!(
        !o.status.success() && stderr(&o).contains("--file-deny"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn rename_link_e_symlink_pelo_supervisor() {
    let env = Env::new();
    std::fs::write(env.work.join(".env"), "TOKEN=real").unwrap();
    std::fs::write(env.work.join("dados.sql"), "x").unwrap();
    let out = shell_with(
        &env,
        &["--file-deny", "dados.sql"],
        "echo a > a.txt && mv a.txt b.txt && cat b.txt; \
         mkdir -p sub && mv b.txt sub/ && ln -s sub/b.txt l && cat l && ln sub/b.txt h && cat h; \
         git init -q repo; echo /tmp > x; \
         mv x repo/.git/commondir 2>/dev/null || echo mv-negado; \
         ln -s /tmp repo/.git/commondir 2>/dev/null || echo symlink-negado; \
         ln x repo/.git/commondir 2>/dev/null || echo link-negado; \
         mv .env exposto.txt 2>/dev/null || echo segredo-preso; \
         ln .env exposto2.txt 2>/dev/null || echo segredo-preso-link; \
         ln -s .env l2 && (cat l2 2>/dev/null || echo l2-escondido); \
         mv dados.sql y 2>/dev/null || echo deny-preso; \
         echo k > tmp.txt && mv tmp.txt nova.pem && cat nova.pem",
    );
    for want in [
        "mv-negado",
        "symlink-negado",
        "link-negado",
        "segredo-preso",
        "segredo-preso-link",
        "l2-escondido",
        "deny-preso",
    ] {
        assert!(out.contains(want), "falta {want}: {out}");
    }
    assert!(out.starts_with("a\na\na\n"), "{out}");
    assert!(out.trim_end().ends_with('k'), "{out}");
    assert!(!env.work.join("repo/.git/commondir").exists());
    assert!(env.work.join(".env").exists() && env.work.join("dados.sql").exists());
    assert!(env.work.join("sub/b.txt").exists());
}

#[test]
fn negar_na_hora_pela_linha_de_comando() {
    let env = Env::new();
    std::fs::write(env.work.join("dados.sql"), "conteudo-sql").unwrap();
    let mut cmd = env.cmd(&env.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = common::pty::Term::spawn(cmd, 160, 30);
    t.write(b"cat dados.sql; echo; echo FIM-$((1+1))\r");
    t.wait_text("FIM-2");
    t.with_screen(|s| assert!(s.contents().contains("conteudo-sql"), "{}", s.contents()));

    let o = env.run(&env.work, &["files", "deny", "dados.sql", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    t.write(b"clear; sleep 0.3; cat dados.sql 2>&1 | head -c 60; echo; echo FIM-$((2+2))\r");
    t.wait_text("FIM-4");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(
            !c.contains("conteudo-sql")
                && (c.contains("Permission denied") || c.contains("Permissão negada")),
            "{c}"
        );
    });

    let o = env.run(&env.work, &["files", "forget", "dados.sql", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    t.write(b"clear; sleep 0.3; cat dados.sql; echo; echo FIM-$((3+3))\r");
    t.wait_text("FIM-6");
    t.with_screen(|s| assert!(s.contents().contains("conteudo-sql"), "{}", s.contents()));

    let o = env.run(&env.work, &["files", "deny", "a/b", "1"]);
    assert!(
        !o.status.success() && stderr(&o).contains("padrão de arquivo inválido"),
        "{}",
        stderr(&o)
    );
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn pastas_ao_vivo_em_mnt_sandmux() {
    let env = Env::new();
    let dados = env.dir.path().join("dados");
    let docs = env.dir.path().join("docs");
    std::fs::create_dir_all(&dados).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("leia.txt"), "conteudo-docs").unwrap();
    let mut cmd = env.cmd(&env.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = common::pty::Term::spawn(cmd, 200, 40);
    t.write(b"ls /mnt/sandmux; echo $SANDMUX_FOLDERS; echo FIM-$((1+1))\r");
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        assert!(
            s.contents().contains("PASTAS.txt") && s.contents().contains("/mnt/sandmux"),
            "{}",
            s.contents()
        )
    });

    let o = env.run(
        &env.work,
        &["folder", "add", "1", dados.to_str().unwrap(), "--rw"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let o = env.run(&env.work, &["folder", "add", "1", docs.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = env.run(
        &env.work,
        &["folder", "add", "1", env.home.to_str().unwrap()],
    );
    assert!(!o.status.success(), "o HOME deveria ser recusado");

    t.write(
        b"clear; cd /mnt/sandmux/dados && echo x > a.txt && mkdir sub && mv a.txt sub/b.txt && ls sub; \
          echo y > apagar.txt && rm apagar.txt; cd /; cat /mnt/sandmux/docs/leia.txt; echo; \
          echo z > /mnt/sandmux/docs/novo.txt 2>/dev/null || echo docs-ro; \
          grep -c sandmux /mnt/sandmux/PASTAS.txt; echo FIM-$((2+2))\r",
    );
    t.wait_text("FIM-4");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(
            c.contains("b.txt") && c.contains("conteudo-docs") && c.contains("docs-ro"),
            "{c}"
        );
    });
    assert_eq!(
        std::fs::read_to_string(dados.join("sub/b.txt")).unwrap(),
        "x\n"
    );
    assert!(!dados.join("apagar.txt").exists() && !docs.join("novo.txt").exists());
    let list = stdout(&env.run(&env.work, &["folder", "list", "1"]));
    assert!(
        list.contains("/mnt/sandmux/dados  rw") && list.contains("/mnt/sandmux/docs  ro"),
        "{list}"
    );

    let o = env.run(&env.work, &["folder", "remove", "1", "docs"]);
    assert!(o.status.success(), "{}", stderr(&o));
    t.write(b"clear; ls /mnt/sandmux; echo FIM-$((3+3))\r");
    t.wait_text("FIM-6");
    t.with_screen(|s| {
        assert!(
            !s.contents().contains("docs") && s.contents().contains("dados"),
            "{}",
            s.contents()
        )
    });
    t.write(b"exit\r");
    t.wait_exit();

    // Relançada, a sessão volta com a pasta.
    let o = env.run(&env.work, &["restart", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let mut a = common::pty::Term::spawn(env.cmd(&env.work, &["attach", "1"]), 200, 40);
    a.write(b"ls /mnt/sandmux/dados/sub; echo FIM-$((4+4))\r");
    a.wait_text("FIM-8");
    a.with_screen(|s| assert!(s.contents().contains("b.txt"), "{}", s.contents()));
    a.write(b"\x02d");
    a.wait_exit();
}

/// Sem o auxiliar root, o supervisor registra as execuções, com os
/// argumentos e a fonte; um programa que não existe não trava nada.
#[test]
fn execucoes_pelo_supervisor() {
    let env = Env::new();
    let out = shell(
        &env,
        "ls -la /tmp >/dev/null && echo LS-OK; ./nao-existe 2>/dev/null; sh -c 'true' && echo FIM",
    );
    assert!(out.contains("LS-OK") && out.contains("FIM"), "{out}");
    let execs: Vec<serde_json::Value> = file_lines(&env)
        .into_iter()
        .filter(|l| l["access"] == "exec")
        .collect();
    let ls = execs
        .iter()
        .find(|l| l["path"].as_str().is_some_and(|p| p.ends_with("/ls")))
        .unwrap_or_else(|| panic!("sem a linha do ls: {execs:?}"));
    assert_eq!(ls["via"], "supervisor");
    assert_eq!(ls["args"], serde_json::json!(["ls", "-la", "/tmp"]));
    assert!(
        execs.iter().any(|l| l["path"] == "./nao-existe"),
        "{execs:?}"
    );
}

/// Um `openat` de diretório com `mode` e sem `O_CREAT` (o Bun, e com ele o
/// Claude Code, passa 0666 em toda abertura): o supervisor tratava o
/// `O_DIRECTORY` como `O_TMPFILE`, mantinha o `mode`, e o `openat2` dava
/// `EINVAL`. O kernel ignora o `mode` nesse caso.
#[test]
fn abrir_diretorio_com_mode_sem_creat() {
    let e = Env::new();
    let script = r#"mkdir -p /tmp/d && python3 -c '
import ctypes, os
libc = ctypes.CDLL(None, use_errno=True)
flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC | 0o100000
fd = libc.syscall(257, -100, b"/tmp/d", flags, 0o666)
print("ok" if fd >= 0 else "erro %d" % ctypes.get_errno())
'"#;
    let out = e.shell(&[], script);
    assert_eq!(stdout(&out), "ok\n", "{}", stderr(&out));
}
