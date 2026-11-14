//! O repassador `sandmux-db-mcp`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

#[test]
fn repassa_o_stdio_pelo_socket() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("db.sock");
    let l = UnixListener::bind(&path).unwrap();
    std::thread::spawn(move || {
        let (c, _) = l.accept().unwrap();
        let mut w = c.try_clone().unwrap();
        for line in BufReader::new(c).lines() {
            let line = line.unwrap();
            writeln!(w, "eco:{line}").unwrap();
        }
    });
    let mut p = Command::new(env!("CARGO_BIN_EXE_sandmux-db-mcp"))
        .env("SANDMUX_DB_SOCKET", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    p.stdin.take().unwrap().write_all(b"um\ndois\n").unwrap();
    let out = p.wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "eco:um\neco:dois\n");
    let o = Command::new(env!("CARGO_BIN_EXE_sandmux-db-mcp"))
        .output()
        .unwrap();
    assert!(!o.status.success());
}

mod common;
use common::cli::{Env, stderr, stdout};
use common::pty::Term;
use std::time::{Duration, Instant};

/// Um banco no Docker do sistema, removido no fim.
struct Banco {
    id: String,
    port: u16,
}

impl Drop for Banco {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.id]).output();
    }
}

fn banco(image: &str, env: &[&str], inner: u16, ready: &[&str]) -> Option<Banco> {
    let ok = Command::new("docker")
        .args(["image", "inspect", image])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("sem a imagem {image} no Docker do sistema: teste pulado");
        return None;
    }
    let mut a = vec!["run", "-d", "--rm", "-m", "512m"];
    for e in env {
        a.extend(["-e", e]);
    }
    let p = format!("127.0.0.1::{inner}");
    a.extend(["-p", &p, image]);
    let o = Command::new("docker").args(&a).output().ok()?;
    if !o.status.success() {
        eprintln!("docker run: {}", String::from_utf8_lossy(&o.stderr));
        return None;
    }
    let id = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let port = String::from_utf8_lossy(
        &Command::new("docker")
            .args(["port", &id, &format!("{inner}/tcp")])
            .output()
            .ok()?
            .stdout,
    )
    .lines()
    .next()?
    .rsplit(':')
    .next()?
    .parse()
    .ok()?;
    let b = Banco { id, port };
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let mut cmd = vec!["exec", b.id.as_str()];
        cmd.extend_from_slice(ready);
        if Command::new("docker")
            .args(&cmd)
            .output()
            .is_ok_and(|o| o.status.success())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "o banco {image} não ficou pronto"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    Some(b)
}

fn vault(e: &Env, args: &[&str], input: &str) -> std::process::Output {
    let mut c = e
        .cmd(&e.work, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

/// Uma sessão com `--db app` manda as linhas JSON-RPC pelo repassador e
/// devolve a saída dele.
fn sessao(e: &Env, kind: &str, port: u16, user: &str, senha: &str, chamadas: &[&str]) -> String {
    assert!(vault(e, &["vault", "init"], "s\n").status.success());
    let p = port.to_string();
    let o = vault(
        e,
        &[
            "db",
            "add",
            "app",
            "--kind",
            kind,
            "--host",
            "127.0.0.1",
            "--port",
            &p,
            "--database",
            "app",
            "--user",
            user,
        ],
        &format!("{senha}\n"),
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let mut linhas = vec![
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
    ];
    for (i, sql) in chamadas.iter().enumerate() {
        linhas.push(format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"tools/call","params":{{"name":"query","arguments":{{"conexao":"app","sql":"{}"}}}}}}"#,
            i + 1,
            sql
        ));
    }
    linhas.push(r#"{"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"tabelas","arguments":{"conexao":"app"}}}"#.to_string());
    std::fs::write(e.work.join("pedidos.jsonl"), linhas.join("\n") + "\n").unwrap();
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--db", "app"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 160, 30);
    t.write(
        format!(
            "/run/sandmux/bin/sandmux-db-mcp < pedidos.jsonl > respostas.jsonl; \
             env | grep -c {senha} > env.txt; echo FIM-$((1+1))\r"
        )
        .as_bytes(),
    );
    t.wait_text("FIM-2");
    t.write(b"\x02d");
    t.wait_exit();
    assert_eq!(
        std::fs::read_to_string(e.work.join("env.txt"))
            .unwrap()
            .trim(),
        "0",
        "a senha está no ambiente"
    );
    std::fs::read_to_string(e.work.join("respostas.jsonl")).unwrap()
}

fn confere(e: &Env, out: &str) {
    assert!(out.contains(r#"\"x\":"#) && out.contains("42"), "{out}");
    assert!(out.contains("itens"), "{out}");
    let log = stdout(&e.run(&e.work, &["db", "log", "1"]));
    assert!(
        log.contains("app") && log.contains("insert into itens"),
        "{log}"
    );
}

#[test]
fn postgres_pelo_mcp() {
    let Some(b) = banco(
        "postgres:17-alpine",
        &["POSTGRES_PASSWORD=senha-pg-123", "POSTGRES_DB=app"],
        5432,
        &["pg_isready", "-U", "postgres", "-h", "127.0.0.1"],
    ) else {
        return;
    };
    let e = Env::new();
    let out = sessao(
        &e,
        "postgres",
        b.port,
        "postgres",
        "senha-pg-123",
        &[
            "create table itens (x int)",
            "insert into itens values (42)",
            "select x from itens",
        ],
    );
    confere(&e, &out);
}

#[test]
fn mysql_pelo_mcp() {
    let Some(b) = banco(
        "mysql:8.4",
        &["MYSQL_ROOT_PASSWORD=senha-my-123", "MYSQL_DATABASE=app"],
        3306,
        &[
            "mysql",
            "-uroot",
            "-psenha-my-123",
            "-h127.0.0.1",
            "-e",
            "select 1",
        ],
    ) else {
        return;
    };
    let e = Env::new();
    let out = sessao(
        &e,
        "mysql",
        b.port,
        "root",
        "senha-my-123",
        &[
            "create table itens (x int)",
            "insert into itens values (42)",
            "select x from itens",
        ],
    );
    confere(&e, &out);
}
