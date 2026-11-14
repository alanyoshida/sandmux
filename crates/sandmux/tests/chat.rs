//! Aba de chat (change 21): o `sandmux-chat` confinado conversa com um
//! provedor falso por HTTPS, com a chave do cofre trocada no gateway; a
//! conversa volta depois de `daemon restart`; a pasta some ao fechar; e
//! `sandmux chat` sem terminal imprime a resposta.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::{Read, Write};
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const REAL: &str = "REAL-chave-do-provedor";

fn vault(e: &Env, args: &[&str], input: &str) -> Output {
    let mut a = vec!["vault"];
    a.extend_from_slice(args);
    let mut c = e
        .cmd(&e.work, &a)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

/// Provedor falso compatível com a OpenAI, por HTTPS em 127.0.0.1. Responde
/// "eco: <última mensagem>" (em dois pedaços no streaming) quando a chave é a
/// real, e 401 com o que recebeu quando não é. Devolve a porta, a CA e o
/// número de pedidos.
fn fake_llm() -> (u16, String, Arc<AtomicUsize>, Bodies) {
    let (ca_cert, ca_key) = sandmux_net::tls::generate_ca().unwrap();
    let ca = sandmux_net::tls::Ca::from_pem(&ca_cert, &ca_key).unwrap();
    let cfg = ca.server_config("host.sandmux.internal").unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let n = count.clone();
    let bodies: Bodies = Arc::default();
    let seen = bodies.clone();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(c) = c else { return };
            let cfg = cfg.clone();
            let n = n.clone();
            let seen = seen.clone();
            std::thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(cfg) else {
                    return;
                };
                let mut s = rustls::StreamOwned::new(conn, c);
                let mut req = Vec::new();
                let mut buf = [0u8; 8192];
                let body = loop {
                    let Ok(k) = s.read(&mut buf) else { return };
                    if k == 0 {
                        return;
                    }
                    req.extend_from_slice(&buf[..k]);
                    let t = String::from_utf8_lossy(&req).to_string();
                    let Some(i) = t.find("\r\n\r\n") else {
                        continue;
                    };
                    let len = t[..i]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break req[i + 4..i + 4 + len].to_vec();
                    }
                };
                n.fetch_add(1, Ordering::SeqCst);
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
                let head = String::from_utf8_lossy(&req).to_string();
                let auth = head
                    .lines()
                    .find_map(|l| {
                        let low = l.to_ascii_lowercase();
                        low.starts_with("authorization:")
                            .then(|| l["authorization:".len()..].trim().to_string())
                    })
                    .unwrap_or_default();
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                let last = v["messages"]
                    .as_array()
                    .and_then(|m| m.last())
                    .and_then(|m| m["content"].as_str())
                    .unwrap_or("")
                    .to_string();
                // Com texto de sistema (o modal de comando), um comando em
                // bloco de código; "duas linhas" pede um de duas linhas.
                let system = v["messages"][0]["role"] == "system";
                let last = if !system {
                    last
                } else if last.contains("duas linhas") {
                    "```sh\necho um\necho dois\n```".to_string()
                } else {
                    "```bash\n$ echo gerado-$((40+2))\n```".to_string()
                };
                let reply = |t: &str| {
                    if system {
                        t.to_string()
                    } else {
                        format!("eco: {t}")
                    }
                };
                let resp = if auth != format!("Bearer {REAL}") {
                    let b =
                        serde_json::json!({"error": {"message": format!("chave errada: {auth}")}})
                            .to_string();
                    format!(
                        "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}",
                        b.len()
                    )
                } else if v["stream"] == serde_json::Value::Bool(true) {
                    let chunk = |t: &str| {
                        format!(
                            "data: {}\n\n",
                            serde_json::json!({"choices": [{"delta": {"content": t}}]})
                        )
                    };
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{}{}data: [DONE]\n\n",
                        chunk(if system { "" } else { "eco: " }),
                        chunk(&last)
                    )
                } else {
                    let b =
                        serde_json::json!({"choices": [{"message": {"content": reply(&last)}}]})
                            .to_string();
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{b}",
                        b.len()
                    )
                };
                let _ = s.write_all(resp.as_bytes());
                let _ = s.flush();
                s.conn.send_close_notify();
                let _ = s.flush();
            });
        }
    });
    (port, ca_cert, count, bodies)
}

type Bodies = Arc<std::sync::Mutex<Vec<String>>>;

/// Cofre com a chave do provedor falso e a configuração do provedor próprio.
fn setup() -> (Env, Arc<AtomicUsize>) {
    let (e, count, _) = setup_with_bodies();
    (e, count)
}

fn setup_with_bodies() -> (Env, Arc<AtomicUsize>, Bodies) {
    let e = Env::new();
    let (port, ca, count, bodies) = fake_llm();
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca).unwrap();
    e.write_config(&format!(
        "[assistant]\nchat-model = \"local/eco\"\n[assistant.provider.local]\nurl = \"https://host.sandmux.internal:{port}/v1\"\n"
    ));
    // O daemon sobe aqui, com a CA do servidor local.
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .env("SANDMUX_UPSTREAM_CA", &ca_file)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    let o = vault(
        &e,
        &["add", "llm", "--domain", "host.sandmux.internal"],
        &format!("{REAL}\n"),
    );
    assert!(o.status.success(), "{}", stderr(&o));
    (e, count, bodies)
}

fn chat_dirs(e: &Env) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(e.state.join("sandmux/chat"))
        .map(|d| d.flatten().map(|f| f.path()).collect())
        .unwrap_or_default()
}

fn net_log(e: &Env) -> String {
    std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect()
}

fn wait(secs: u64, f: impl Fn() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn chat_sem_terminal_imprime_a_resposta() {
    let (e, count) = setup();
    let o = e.run(&e.work, &["chat", "quanto", "é", "40+2?"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "eco: quanto é 40+2?\n");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    // Nada guardado.
    assert!(chat_dirs(&e).is_empty(), "{:?}", chat_dirs(&e));
    let net = net_log(&e);
    assert!(net.contains("\"secret\":\"llm\""), "{net}");
    assert!(!net.contains(REAL), "valor real no registro");
    // Sem texto e sem terminal, recusa.
    let o = e.run(&e.work, &["chat"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("precisa do texto"), "{}", stderr(&o));
    // Provedor desconhecido.
    let o = e.run(&e.work, &["chat", "--model", "nada/x", "oi"]);
    assert!(
        stderr(&o).contains("provedor desconhecido"),
        "{}",
        stderr(&o)
    );
    // Provedor conhecido sem chave no cofre.
    let o = e.run(&e.work, &["chat", "--model", "openai/gpt-5", "oi"]);
    assert!(
        stderr(&o).contains("sandmux vault add openai --domain api.openai.com"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn erro_do_provedor_sem_terminal() {
    let (e, _) = setup();
    // A chave trocada por outra: o provedor responde 401.
    let o = vault(
        &e,
        &["add", "llm", "--domain", "host.sandmux.internal"],
        "OUTRA\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let o = e.run(&e.work, &["chat", "oi"]);
    assert!(!o.status.success());
    let err = stderr(&o);
    assert!(
        err.contains("401") && err.contains("chave errada: Bearer OUTRA"),
        "{err}"
    );
}

#[test]
fn aba_de_chat_conversa_volta_e_some_ao_fechar() {
    let (e, count) = setup();
    let mut t = Term::spawn(e.cmd(&e.work, &["chat", "primeira"]), 120, 30);
    t.wait_text("eco: primeira");
    t.write(b"segunda pergunta");
    t.wait_text("segunda pergunta");
    // O botão "Enviar", clicado (SGR): procura a coluna na tela.
    let (row, col) = t.with_screen(|s| {
        let (rows, cols) = (s.size().0.get(), s.size().1.get());
        for r in 0..rows {
            let line = s.contents_between(r, 0, r, cols);
            if let Some(c) = line.find("[ Enviar ]") {
                return (r, line[..c].chars().count() as u16);
            }
        }
        panic!("sem botão: {}", s.contents());
    });
    t.write(
        format!(
            "\x1b[<0;{};{}M\x1b[<0;{};{}m",
            col + 3,
            row + 1,
            col + 3,
            row + 1
        )
        .as_bytes(),
    );
    t.wait_text("eco: segunda pergunta");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let dirs = chat_dirs(&e);
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    let conv = std::fs::read_to_string(dirs[0].join("conversa.json")).unwrap();
    assert!(
        conv.contains("eco: segunda pergunta") && conv.contains("local/eco"),
        "{conv}"
    );
    // O programa vê só o falso.
    assert!(!conv.contains(REAL));
    t.write(b"\x02d");
    t.wait_exit();

    let ls = stdout(&e.run(&e.work, &["ls"]));
    assert!(ls.contains("chat:local/eco"), "{ls}");
    let id: String = ls
        .lines()
        .find(|l| l.contains("chat:local/eco"))
        .and_then(|l| l.split_whitespace().next())
        .unwrap()
        .to_string();
    let stats = stdout(&e.run(&e.work, &["stats", &id]));
    assert!(stats.contains("só inferência"), "{stats}");

    // Reinício do daemon: o cofre tranca, a aba espera e volta com a conversa.
    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());
    let ls = stdout(&e.run(&e.work, &["ls"]));
    assert!(ls.contains("aguardando cofre"), "{ls}");
    let o = vault(&e, &["unlock"], "s3nha\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        wait(10, || stdout(&e.run(&e.work, &["ls"])).contains("viva")),
        "{}",
        stdout(&e.run(&e.work, &["ls"]))
    );
    let mut t = Term::spawn(e.cmd(&e.work, &["attach", &id]), 120, 30);
    t.wait_text("eco: segunda pergunta");
    std::thread::sleep(Duration::from_millis(500));
    // Nada foi reenviado.
    assert_eq!(count.load(Ordering::SeqCst), 2);
    t.write(b"\x02d");
    t.wait_exit();

    // Fechar a aba apaga a pasta.
    assert!(e.run(&e.work, &["kill", &id]).status.success());
    assert!(wait(5, || chat_dirs(&e).is_empty()), "{:?}", chat_dirs(&e));
}

#[test]
fn modal_de_chat_na_tui_com_o_cofre_trancado() {
    let (e, count) = setup();
    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn_answering(cmd, 120, 30);
    // Sem abas, a TUI abre o modal de aba nova.
    t.wait_text("Nova aba");
    t.write(b"\x1b");
    std::thread::sleep(Duration::from_millis(300));
    t.write(b"\x02i");
    t.wait_text("Chat");
    t.wait_text("local/eco");
    t.write(b"pelo modal\r");
    t.wait_text("eco: pelo modal");
    t.wait_text("chat:local/eco");
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Cofre trancado: o desbloqueio vem antes, e a aba nasce depois.
    assert!(vault(&e, &["lock"], "").status.success());
    t.write(b"\x02i");
    t.wait_text("Tab troca de campo");
    // O modelo trocado no campo, por um provedor desconhecido, é recusado.
    t.write(b"oi\t\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7f\x7fnada/x\r");
    t.wait_text("provedor desconhecido");
    t.write(b"\x7f\x7f\x7f\x7f\x7f\x7flocal/eco\r");
    t.wait_text("O cofre de segredos está trancado");
    t.write(b"s3nha\r");
    t.wait_text("eco: oi");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(chat_dirs(&e).len(), 2);
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn modal_de_comando_escreve_sem_executar() {
    let (e, _, bodies) = setup_with_bodies();
    // Uma sessão do shell, desanexada.
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn_answering(cmd, 120, 30);
    t.write(b"echo segredo-da-tela-$((1+1))\r");
    t.wait_text("segredo-da-tela-2");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);

    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn_answering(cmd, 120, 30);
    t.wait_text("1:shell:app");
    t.write(b"\x02g");
    t.wait_text("Comando");
    t.write(b"mostrar um eco\r");
    t.wait_text("[ Inserir ]");
    t.wait_text("echo gerado-$((40+2))");
    // O que o provedor recebeu: a pergunta, a pasta e o shell; nada da tela.
    let body = bodies.lock().unwrap().last().cloned().unwrap();
    assert!(body.contains("mostrar um eco"), "{body}");
    assert!(body.contains(&e.work.display().to_string()), "{body}");
    assert!(body.contains("bash"), "{body}");
    assert!(!body.contains("segredo-da-tela"), "{body}");
    // Enter escreve na linha do shell, sem executar.
    t.write(b"\r");
    t.wait_for("o comando na linha", |s| {
        s.contains("$ echo gerado-$((40+2))")
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(!t.screen().contains("gerado-42"), "{}", t.screen());
    // O usuário executa.
    t.write(b"\r");
    t.wait_text("gerado-42");

    // Duas linhas: o bash liga a colagem, e as duas linhas vão coladas,
    // sem rodar. O botão é clicado.
    t.write(b"\x02g");
    t.wait_text("Comando");
    t.write(b"duas linhas\r");
    t.wait_text("[ Inserir ]");
    let (row, col) = t.with_screen(|s| {
        let (rows, cols) = (s.size().0.get(), s.size().1.get());
        for r in 0..rows {
            let line = s.contents_between(r, 0, r, cols);
            if let Some(c) = line.find("[ Inserir ]") {
                return (r, line[..c].chars().count() as u16);
            }
        }
        panic!("sem botão: {}", s.contents());
    });
    t.write(
        format!(
            "\x1b[<0;{};{}M\x1b[<0;{};{}m",
            col + 3,
            row + 1,
            col + 3,
            row + 1
        )
        .as_bytes(),
    );
    t.wait_text("echo dois");
    std::thread::sleep(Duration::from_millis(500));
    let ran = |t: &Term| t.with_screen(|s| s.contents().lines().any(|l| l.trim() == "um"));
    assert!(!ran(&t), "{}", t.screen());
    // Apaga a linha colada (Ctrl+u apaga só a última linha; Ctrl+c descarta).
    t.write(b"\x03");
    std::thread::sleep(Duration::from_millis(300));

    // Sem o modo de colagem, duas linhas são recusadas.
    t.write(b"bind 'set enable-bracketed-paste off'; echo sem-colagem-$((1+1))\r");
    t.wait_text("sem-colagem-2");
    t.write(b"\x02g");
    t.wait_text("Comando");
    t.write(b"duas linhas\r");
    t.wait_text("[ Inserir ]");
    t.write(b"\r");
    t.wait_text("aceita colagem");
    t.write(b"\x1b");
    std::thread::sleep(Duration::from_millis(500));
    assert!(!ran(&t), "{}", t.screen());

    // Erro do provedor: aparece no modal, que continua aberto.
    let o = vault(
        &e,
        &["add", "llm", "--domain", "host.sandmux.internal"],
        "OUTRA\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    t.write(b"\x02g");
    t.wait_text("Comando");
    t.write(b"qualquer\r");
    t.wait_text("chave errada: Bearer OUTRA");
    t.wait_text("Enter pergunta");
    t.write(b"\x1b");
    t.write(b"\x02d");
    t.wait_exit();
}
