//! Cofre de segredos: criação, desbloqueio, segredos, troca de senha, e os
//! valores de `--pass-env` guardados para a restauração.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::process::{Output, Stdio};

/// `sandmux vault ...` com `input` na entrada padrão.
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

fn status(e: &Env) -> String {
    stdout(&vault(e, &["status"], ""))
}

fn file_text(e: &Env) -> String {
    String::from_utf8_lossy(&std::fs::read(e.data_dir().join("vault.json")).unwrap()).into_owned()
}

#[test]
fn ciclo_do_cofre_pela_linha_de_comando() {
    let e = Env::new();
    assert!(status(&e).contains("não existe"), "{}", status(&e));
    let o = vault(&e, &["add", "x"], "v\n");
    assert!(
        !o.status.success() && stderr(&o).contains("vault init"),
        "{}",
        stderr(&o)
    );

    let o = vault(&e, &["init"], "senha1\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(status(&e).contains("desbloqueado, 0 segredos"));
    let o = vault(&e, &["init"], "outra\n");
    assert!(!o.status.success() && stderr(&o).contains("já existe"));

    let o = vault(
        &e,
        &["add", "openrouter", "--domain", "openrouter.ai"],
        "sk-teste-123\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("openrouter") && list.contains("openrouter.ai"),
        "{list}"
    );
    assert!(!list.contains("sk-teste-123"));
    assert!(list.contains("falso: sandmux-fake-"), "{list}");
    // Nada em claro no arquivo, e modo 600.
    let t = file_text(&e);
    assert!(
        !t.contains("sk-teste-123") && !t.contains("openrouter"),
        "{t}"
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(e.data_dir().join("vault.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    assert!(vault(&e, &["lock"], "").status.success());
    assert!(status(&e).contains("trancado"));
    let o = vault(&e, &["add", "y"], "v\n");
    assert!(
        !o.status.success() && stderr(&o).contains("vault unlock"),
        "{}",
        stderr(&o)
    );
    let o = vault(&e, &["unlock"], "errada\n");
    assert!(
        !o.status.success() && stderr(&o).contains("senha não confere"),
        "{}",
        stderr(&o)
    );
    assert!(vault(&e, &["unlock"], "senha1\n").status.success());
    assert!(status(&e).contains("desbloqueado, 1 segredos"));

    // Troca de senha.
    let o = vault(&e, &["passwd"], "senha1\nsenha2\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(vault(&e, &["lock"], "").status.success());
    assert!(!vault(&e, &["unlock"], "senha1\n").status.success());
    assert!(vault(&e, &["unlock"], "senha2\n").status.success());

    let o = vault(&e, &["remove", "openrouter"], "");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!vault(&e, &["remove", "openrouter"], "").status.success());

    // O daemon sobe de novo com o cofre trancado.
    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());
    assert!(status(&e).contains("trancado"), "{}", status(&e));
}

#[test]
fn import_pede_os_valores_dos_manifestos() {
    let e = Env::new();
    let dir = e.data_dir().join("manifests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        r#"{"harness":"opencode","entries":[{"file":".local/share/opencode/auth.json","key":"openrouter.key","line":3,"fake":"sandmux-fake-abc"},{"file":".config/x","key":"<token>","line":1,"fake":"sandmux-fake-def"}]}"#,
    )
    .unwrap();
    assert!(vault(&e, &["init"], "s\n").status.success());
    // O primeiro recebe valor; o segundo é pulado com uma linha vazia.
    let o = vault(&e, &["import"], "sk-real-1\n\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("1 de 2"), "{}", stderr(&o));
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("opencode:.local/share/opencode/auth.json:openrouter.key  (openrouter.ai)"),
        "{list}"
    );
    // Na segunda vez, só o que faltou. Uma chave sem domínio pelo nome
    // pergunta o domínio depois do valor.
    let o = vault(&e, &["import"], "tok-local\nhost.sandmux.internal\n");
    assert!(stderr(&o).contains("1 de 1"), "{}", stderr(&o));
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("opencode:.config/x:<token>  (host.sandmux.internal)"),
        "{list}"
    );
}

fn ls(e: &Env) -> String {
    stdout(&e.run(&e.work, &["ls"]))
}

#[test]
fn pass_env_no_cofre_volta_depois_do_reinicio_e_do_desbloqueio() {
    let e = Env::new();
    // Sem cofre: aviso de que a sessão fica parada num reboot.
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--pass-env", "CHAVE_TESTE"]);
    cmd.env("SHELL", "/bin/bash").env("CHAVE_TESTE", "valor-x");
    let mut t = Term::spawn(cmd, 100, 30);
    t.wait_raw("não foram guardados");
    t.write(b"\x02d");
    t.wait_exit();
    assert!(e.run(&e.work, &["kill", "1"]).status.success());

    assert!(vault(&e, &["init"], "s3nha\n").status.success());
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--pass-env", "CHAVE_TESTE"]);
    cmd.env("SHELL", "/bin/bash").env("CHAVE_TESTE", "valor-x");
    let mut t = Term::spawn(cmd, 100, 30);
    t.write(b"echo v=$CHAVE_TESTE-$((1+1))\r");
    t.wait_text("v=valor-x-2");
    assert!(!t.raw_text().contains("não foram guardados"));
    t.write(b"\x02d");
    t.wait_exit();

    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());
    let l = ls(&e);
    assert!(l.contains("aguardando cofre"), "{l}");
    // O valor não está em claro em nenhum arquivo do sandmux.
    for f in ["vault.json", "sessions.json"] {
        let t = String::from_utf8_lossy(&std::fs::read(e.data_dir().join(f)).unwrap_or_default())
            .into_owned();
        assert!(!t.contains("valor-x"), "{f}: {t}");
    }
    assert!(vault(&e, &["unlock"], "s3nha\n").status.success());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !ls(&e).contains("viva") && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ls(&e).contains("viva"), "{}", ls(&e));
    let mut cmd = e.cmd(&e.work, &["attach", "2"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 100, 30);
    t.write(b"echo depois=$CHAVE_TESTE-$((2+2))\r");
    t.wait_text("depois=valor-x-4");
    t.write(b"\x02d");
    t.wait_exit();
    // `kill` tira os valores do cofre.
    assert!(e.run(&e.work, &["kill", "2"]).status.success());
}

#[test]
fn tui_pede_a_senha_e_sobe_as_sessoes_do_cofre() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s3nha\n").status.success());
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--pass-env", "CHAVE_TESTE"]);
    cmd.env("SHELL", "/bin/bash").env("CHAVE_TESTE", "valor-y");
    let mut t = Term::spawn(cmd, 100, 30);
    t.write(b"echo v=$CHAVE_TESTE-$((1+1))\r");
    t.wait_text("v=valor-y-2");
    t.write(b"\x02d");
    t.wait_exit();
    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());

    let mut cmd = e.cmd(&e.work, &[]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn_answering(cmd, 100, 30);
    t.wait_text("O cofre de segredos está trancado.");
    t.wait_text(&tab(1, "shell:app (cofre)"));
    t.write(b"errada\r");
    t.wait_text("senha não confere");
    t.write(b"s3nha");
    t.wait_text("Senha: •••••");
    t.with_screen(|s| assert!(!s.contents().contains("s3nha"), "{}", s.contents()));
    t.write(b"\r");
    t.wait_for("modal fecha", |s| !s.contains("O cofre de segredos"));
    t.wait_for("sessão sobe", |s| !s.contains("(cofre)"));
    t.write(b"echo depois=$CHAVE_TESTE-$((2+2))\r");
    t.wait_text("depois=valor-y-4");
    t.write(b"\x02d");
    assert_eq!(t.wait_exit(), 0);
}

/// Servidor HTTPS local (CA de teste) que devolve o `Authorization` recebido.
fn echo_https() -> (u16, String) {
    use std::io::Read;
    let (ca_cert, ca_key) = sandmux_net::tls::generate_ca().unwrap();
    let ca = sandmux_net::tls::Ca::from_pem(&ca_cert, &ca_key).unwrap();
    let cfg = ca.server_config("host.sandmux.internal").unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(c) = c else { return };
            let cfg = cfg.clone();
            std::thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(cfg) else {
                    return;
                };
                let mut s = rustls::StreamOwned::new(conn, c);
                let mut buf = vec![0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let auth = req
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("authorization: ")
                            .or_else(|| l.strip_prefix("Authorization: "))
                    })
                    .unwrap_or("-")
                    .to_string();
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("-")
                    .to_string();
                let body = format!("recebido={auth} caminho={path}");
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = s.flush();
                s.conn.send_close_notify();
                let _ = s.flush();
            });
        }
    });
    (port, ca_cert)
}

/// Servidor WebSocket com TLS (CA de teste) no papel do `gateway.discord.gg`:
/// lê uma mensagem de texto do cliente e devolve se o pedido trouxe
/// `Sec-WebSocket-Extensions` e o texto recebido.
fn wss_echo() -> (u16, String) {
    use std::io::Read;
    let (ca_cert, ca_key) = sandmux_net::tls::generate_ca().unwrap();
    let ca = sandmux_net::tls::Ca::from_pem(&ca_cert, &ca_key).unwrap();
    let cfg = ca.server_config("host.sandmux.internal").unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(c) = c else { return };
            let cfg = cfg.clone();
            std::thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(cfg) else {
                    return;
                };
                let mut s = rustls::StreamOwned::new(conn, c);
                let mut buf = Vec::new();
                let mut b = [0u8; 1];
                while !buf.ends_with(b"\r\n\r\n") {
                    if s.read(&mut b).unwrap_or(0) == 0 {
                        return;
                    }
                    buf.push(b[0]);
                }
                let req = String::from_utf8_lossy(&buf).to_ascii_lowercase();
                let ext = if req.contains("sec-websocket-extensions") {
                    "sim"
                } else {
                    "nao"
                };
                let _ = s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: x\r\n\r\n");
                let _ = s.flush();
                let mut h = [0u8; 2];
                if s.read_exact(&mut h).is_err() {
                    return;
                }
                let mut n = (h[1] & 0x7f) as usize;
                if n == 126 {
                    let mut e = [0u8; 2];
                    s.read_exact(&mut e).unwrap();
                    n = u16::from_be_bytes(e) as usize;
                }
                let mut k = [0u8; 4];
                s.read_exact(&mut k).unwrap();
                let mut p = vec![0u8; n];
                s.read_exact(&mut p).unwrap();
                for (i, x) in p.iter_mut().enumerate() {
                    *x ^= k[i % 4];
                }
                let body = format!("ext={ext} msg={}", String::from_utf8_lossy(&p));
                let mut f = vec![0x81u8];
                if body.len() < 126 {
                    f.push(body.len() as u8);
                } else {
                    f.push(126);
                    f.extend_from_slice(&(body.len() as u16).to_be_bytes());
                }
                f.extend_from_slice(body.as_bytes());
                let _ = s.write_all(&f);
                let _ = s.write_all(&[0x88, 0]);
                let _ = s.flush();
                s.conn.send_close_notify();
                let _ = s.flush();
            });
        }
    });
    (port, ca_cert)
}

/// Cliente WebSocket mínimo, rodado dentro do sandbox.
const WS_CLIENT: &str = r#"import os, socket, ssl, sys
host = "host.sandmux.internal"
s = ssl.create_default_context().wrap_socket(socket.create_connection((host, int(sys.argv[1]))), server_hostname=host)
s.sendall(("GET /?v=10&encoding=json HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n" % host).encode())
buf = b""
while b"\r\n\r\n" not in buf:
    buf += s.recv(4096)
head, data = buf.split(b"\r\n\r\n", 1)
print("status=" + head.split(b"\r\n")[0].decode())
msg = ('{"op":2,"d":{"token":"%s"}}' % os.environ["DISCORD_BOT_TOKEN"]).encode()
k = b"\x01\x02\x03\x04"
s.sendall(bytes([0x81, 0x80 | len(msg)]) + k + bytes(b ^ k[i % 4] for i, b in enumerate(msg)))
while len(data) < 4:
    data += s.recv(4096)
n, off = data[1] & 0x7f, 2
if n == 126:
    n, off = int.from_bytes(data[2:4], "big"), 4
while len(data) < off + n:
    data += s.recv(4096)
print("resposta=" + data[off:off + n].decode())
"#;

#[test]
fn token_do_discord_trocado_no_websocket() {
    let e = Env::new();
    let (port, ca) = wss_echo();
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca).unwrap();
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
        &[
            "add",
            "discord",
            "--domain",
            "host.sandmux.internal",
            "--tool",
            "discord",
        ],
        "REAL-discord-token\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    std::fs::write(e.work.join("ws.py"), WS_CLIENT).unwrap();
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 220, 30);
    t.write(format!("python3 ws.py {port}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(c.contains("status=HTTP/1.1 101"), "{c}");
        // O destino recebeu o real, sem o pedido de compressão.
        assert!(
            c.contains(r#"resposta=ext=nao msg={"op":2,"d":{"token":"REAL-discord-token"}}"#),
            "{c}"
        );
    });
    // A linha do WebSocket sai quando o túnel termina.
    let read = || {
        std::fs::read_dir(e.state.join("sandmux/net"))
            .unwrap()
            .flatten()
            .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
            .map(|f| std::fs::read_to_string(f.path()).unwrap())
            .collect::<String>()
    };
    let has = |n: &str| {
        n.lines()
            .any(|l| l.contains("\"status\":101") && l.contains("\"secret\":\"discord\""))
    };
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut net = read();
    while !has(&net) && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(200));
        net = read();
    }
    assert!(has(&net), "{net}");
    assert!(!net.contains("REAL-discord"), "valor real no registro");
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn token_do_telegram_trocado_no_caminho() {
    let e = Env::new();
    let (port, ca) = echo_https();
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca).unwrap();
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .env("SANDMUX_UPSTREAM_CA", &ca_file)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    // O servidor local faz o papel do api.telegram.org.
    let o = vault(
        &e,
        &[
            "add",
            "telegram",
            "--domain",
            "host.sandmux.internal",
            "--tool",
            "telegram",
        ],
        "123456:REAL-telegram\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 30);
    // O agente só vê o falso, na variável que o hermes lê.
    t.write(
        format!(
            "echo \"tok=$TELEGRAM_BOT_TOKEN\"; curl -s \"https://host.sandmux.internal:{port}/bot$TELEGRAM_BOT_TOKEN/getMe\"; echo; echo FIM-$((1+1))\r"
        )
        .as_bytes(),
    );
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(c.contains("tok=0000000000:sandmux-fake-"), "{c}");
        assert!(c.contains("caminho=/bot123456:REAL-telegram/getMe"), "{c}");
    });
    let net = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect::<String>();
    // O registro guarda o caminho com o falso, nunca o real.
    assert!(
        net.contains("\"path\":\"/bot0000000000:sandmux-fake-"),
        "{net}"
    );
    assert!(net.contains("\"secret\":\"telegram\""), "{net}");
    assert!(!net.contains("REAL-telegram"), "valor real no registro");
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn chave_falsa_trocada_pela_real_so_com_o_cofre_aberto() {
    let e = Env::new();
    let (port, ca) = echo_https();
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca).unwrap();
    // O daemon sobe com a CA de teste dos destinos.
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
        &["add", "seg", "--domain", "host.sandmux.internal"],
        "real-xyz\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let list = stdout(&vault(&e, &["list"], ""));
    let fake = list.split("falso: ").nth(1).unwrap().trim().to_string();
    assert!(fake.starts_with("sandmux-fake-"), "{list}");

    // `--decrypt` mantém o domínio decifrado depois de trancar o cofre (sem
    // ele, a conexão passa cifrada, e o curl não confia na CA de teste).
    let mut cmd = e.cmd(
        &e.work,
        &["run", "shell", "--decrypt", "host.sandmux.internal"],
    );
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 160, 30);
    let curl = format!(
        "curl -s -H 'Authorization: Bearer {fake}' https://host.sandmux.internal:{port}/v1/x; echo"
    );
    t.write(format!("{curl}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        assert!(
            s.contents().contains("recebido=Bearer real-xyz"),
            "{}",
            s.contents()
        )
    });
    // O registro tem o nome do segredo, sem o valor.
    let net = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect::<String>();
    assert!(net.contains("\"secret\":\"seg\""), "{net}");
    assert!(
        !net.contains("real-xyz") && !net.contains(&fake),
        "valor no registro"
    );
    let shown = stdout(&e.run(&e.work, &["net", "1"]));
    assert!(shown.contains("chave: seg"), "{shown}");

    // Cofre trancado: sai o falso.
    assert!(vault(&e, &["lock"], "").status.success());
    t.write(b"clear\r");
    t.write(format!("{curl}; echo FIM-$((2+2))\r").as_bytes());
    t.wait_text("FIM-4");
    t.with_screen(|s| {
        assert!(
            s.contents().contains(&format!("recebido=Bearer {fake}")),
            "{}",
            s.contents()
        )
    });
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn ferramentas_do_segredo() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let o = vault(&e, &["add", "gh", "--domain", "github.com"], "ghp_x\n");
    assert!(o.status.success(), "{}", stderr(&o));
    let o = vault(
        &e,
        &[
            "add",
            "npm-emp",
            "--domain",
            "npm.empresa.com",
            "--tool",
            "npm",
        ],
        "t\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let o = vault(
        &e,
        &["add", "x", "--domain", "a.test", "--tool", "svn"],
        "t\n",
    );
    assert!(
        !o.status.success() && stderr(&o).contains("ferramenta desconhecida"),
        "{}",
        stderr(&o)
    );
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("gh  (github.com)  ferramentas: gh, git"),
        "{list}"
    );
    assert!(
        list.contains("npm-emp  (npm.empresa.com)  ferramentas: npm"),
        "{list}"
    );
}

#[test]
fn segredos_dos_canais_do_hermes() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    for (name, value) in [
        ("telegram", "123456:AAA-telegram-real"),
        ("discord", "discord-real"),
        ("slack-bot", "xoxb-real"),
        ("slack-app", "xapp-real"),
    ] {
        // Sem `--domain` nos dois primeiros: o nome traz os domínios.
        let args: Vec<&str> = if name.starts_with("slack") {
            vec!["add", name, "--domain", "slack.com"]
        } else {
            vec!["add", name]
        };
        let o = vault(&e, &args, &format!("{value}\n"));
        assert!(o.status.success(), "{name}: {}", stderr(&o));
    }
    let list = stdout(&vault(&e, &["list"], ""));
    for l in [
        "telegram  (api.telegram.org)  ferramentas: telegram",
        "discord  (discord.com, gateway.discord.gg)  ferramentas: discord",
        "slack-bot  (slack.com)  ferramentas: slack",
        "slack-app  (slack.com)  ferramentas: slack-app",
        "0000000000:sandmux-fake-",
        "xoxb-sandmux-fake-",
        "xapp-sandmux-fake-",
    ] {
        assert!(list.contains(l), "falta {l}: {list}");
    }
    assert!(!list.contains("real"), "{list}");
}

#[test]
fn ajudante_do_git_responde_o_falso_e_o_basic_chega_com_o_real() {
    let e = Env::new();
    let (port, ca) = echo_https();
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca).unwrap();
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
        &[
            "add",
            "git-interno",
            "--domain",
            "host.sandmux.internal",
            "--tool",
            "git",
        ],
        "real-git\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));

    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 30);
    // O ajudante de credenciais do git responde com o falso; o curl manda o
    // `Basic` com ele, e o gateway troca pelo real dentro do base64.
    let script = format!(
        "P=$(printf 'protocol=https\\nhost=host.sandmux.internal\\n\\n' | git credential fill \
         | sed -n 's/^password=//p'); echo \"falso=${{P:0:13}}\"; \
         curl -s -u \"sandmux:$P\" https://host.sandmux.internal:{port}/repo.git/info/refs; \
         echo; echo FIM-$((1+1))\r"
    );
    t.write(script.as_bytes());
    t.wait_text("FIM-2");
    let expected = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode("sandmux:real-git")
    };
    t.with_screen(|s| {
        let text = s.contents();
        assert!(text.contains("falso=sandmux-fake-"), "{text}");
        assert!(
            text.contains(&format!("recebido=Basic {expected}")),
            "{text}"
        );
        assert!(!text.contains("real-git"), "o real apareceu no sandbox");
    });
    t.write(b"\x02d");
    t.wait_exit();
}

/// Gera uma chave SSH de teste com `ssh-keygen`.
fn ssh_keygen(dir: &std::path::Path, name: &str, pass: &str) -> std::path::PathBuf {
    let f = dir.join(name);
    let ok = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", pass, "-C", "teste", "-f"])
        .arg(&f)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    f
}

#[test]
fn chave_ssh_no_cofre() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let key = ssh_keygen(e.dir.path(), "id_teste", "frase");
    let k = key.to_str().unwrap();
    let o = vault(
        &e,
        &["add-ssh", "github", "--file", k, "--server", "github.com"],
        "errada\n",
    );
    assert!(
        !o.status.success() && stderr(&o).contains("passphrase errada"),
        "{}",
        stderr(&o)
    );
    let o = vault(
        &e,
        &["add-ssh", "github", "--file", k, "--server", "*.x.com"],
        "frase\n",
    );
    assert!(
        !o.status.success() && stderr(&o).contains("servidor inválido"),
        "{}",
        stderr(&o)
    );
    let o = vault(
        &e,
        &["add-ssh", "github", "--file", k, "--server", "github.com"],
        "frase\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let fp = std::process::Command::new("ssh-keygen")
        .args(["-l", "-E", "sha256", "-f"])
        .arg(key.with_extension("pub"))
        .output()
        .unwrap();
    let fp = String::from_utf8_lossy(&fp.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains(&format!("github  ssh  (github.com)  {fp}")),
        "{list}"
    );
    assert!(!list.contains("falso"), "{list}");
    // A chave fica só cifrada no arquivo.
    let private = std::fs::read_to_string(&key).unwrap();
    let body = private.lines().nth(1).unwrap();
    assert!(!file_text(&e).contains(body));
    assert!(vault(&e, &["remove", "github"], "").status.success());
}

#[test]
fn agente_ssh_lista_e_assina_so_com_as_chaves_escolhidas() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let keys = e.dir.path().join("chaves");
    std::fs::create_dir_all(&keys).unwrap();
    let gh = ssh_keygen(&keys, "gh", "");
    let tr = ssh_keygen(&keys, "tr", "");
    for (name, k) in [("github", &gh), ("trabalho", &tr)] {
        let o = vault(
            &e,
            &[
                "add-ssh",
                name,
                "--file",
                k.to_str().unwrap(),
                "--server",
                "github.com",
            ],
            "",
        );
        assert!(o.status.success(), "{}", stderr(&o));
    }
    // A chave pública vai para a pasta de trabalho; a privada fica fora.
    std::fs::copy(gh.with_extension("pub"), e.work.join("gh.pub")).unwrap();
    let gh_pub = std::fs::read_to_string(gh.with_extension("pub")).unwrap();
    let gh_blob = gh_pub.split_whitespace().nth(1).unwrap().to_string();
    let tr_pub = std::fs::read_to_string(tr.with_extension("pub")).unwrap();
    let tr_blob = tr_pub.split_whitespace().nth(1).unwrap().to_string();

    let mut cmd = e.cmd(&e.work, &["run", "shell", "--ssh-key", "github"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 220, 40);
    t.write(
        b"ssh-add -L | cut -c1-60 > /tmp/l; wc -l < /tmp/l; echo SOCK=$SSH_AUTH_SOCK; \
          echo dados > /tmp/m; ssh-keygen -Y sign -f gh.pub -n teste /tmp/m 2>/dev/null; \
          ssh-keygen -Y check-novalidate -n teste -s /tmp/m.sig < /tmp/m; echo FIM-$((1+1))\r",
    );
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        let text = s.contents();
        assert!(text.contains("SOCK=/run/sandmux/ssh-agent.sock"), "{text}");
        assert!(text.contains("Good \"teste\" signature"), "{text}");
    });
    t.write(b"clear; cat /tmp/l; echo FIM-$((2+2))\r");
    t.wait_text("FIM-4");
    t.with_screen(|s| {
        let text = s.contents();
        assert!(text.contains(&gh_blob[..40]), "{text}");
        assert!(!text.contains(&tr_blob[..40]), "{text}");
    });
    // A linha da assinatura no registro, com o nome e sem a chave.
    let net = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect::<String>();
    assert!(
        net.contains("\"proto\":\"ssh-sign\"") && net.contains("\"secret\":\"github\""),
        "{net}"
    );
    let shown = stdout(&e.run(&e.work, &["net", "1"]));
    assert!(shown.contains("assinatura ssh: github"), "{shown}");

    // Cofre trancado: nenhuma chave.
    assert!(vault(&e, &["lock"], "").status.success());
    t.write(b"clear; ssh-add -L; echo FIM-$((3+3))\r");
    t.wait_text("FIM-6");
    t.with_screen(|s| {
        let text = s.contents();
        assert!(text.contains("no identities"), "{text}");
    });
    t.write(b"\x02d");
    t.wait_exit();
}

/// Servidor TCP local que fala `SSH-2.0` primeiro e depois devolve o que
/// recebe.
fn ssh_like_server() -> u16 {
    use std::io::Read;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { return };
            std::thread::spawn(move || {
                let _ = c.write_all(b"SSH-2.0-teste\r\n");
                let mut buf = [0u8; 256];
                while let Ok(n) = c.read(&mut buf) {
                    if n == 0 || c.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    port
}

#[test]
fn ssh_so_para_os_servidores_das_chaves() {
    let e = Env::new();
    let port = ssh_like_server();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let keys = e.dir.path().join("chaves");
    std::fs::create_dir_all(&keys).unwrap();
    let k = ssh_keygen(&keys, "k", "");
    let o = vault(
        &e,
        &[
            "add-ssh",
            "local",
            "--file",
            k.to_str().unwrap(),
            "--server",
            "host.sandmux.internal",
        ],
        "",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    // O cliente fala primeiro (`SSH-` dele) e o cliente calado (o servidor
    // fala primeiro).
    let probe = format!(
        "exec 3<>/dev/tcp/host.sandmux.internal/{port} && printf 'SSH-2.0-x\\r\\n' >&3 && \
         timeout 3 head -c 13 <&3; echo; exec 3<&-; \
         exec 4<>/dev/tcp/host.sandmux.internal/{port} && timeout 4 head -c 13 <&4 | sed 's/^/calado:/'; \
         echo; exec 4<&-; echo FIM-$((1+1))\r"
    );
    for (args, passes) in [
        (vec!["run", "shell"], false),
        (vec!["run", "shell", "--ssh-key", "local"], true),
    ] {
        let mut cmd = e.cmd(&e.work, &args);
        cmd.env("SHELL", "/bin/bash");
        let mut t = Term::spawn(cmd, 200, 30);
        t.write(probe.as_bytes());
        t.wait_text("FIM-2");
        t.with_screen(|s| {
            let text = s.contents();
            assert_eq!(text.contains("\nSSH-2.0-teste"), passes, "{args:?}: {text}");
            assert_eq!(
                text.contains("calado:SSH-2.0-teste"),
                passes,
                "{args:?}: {text}"
            );
        });
        t.write(b"\x02d");
        t.wait_exit();
    }
    let net = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect::<String>();
    assert!(
        net.contains("\"proto\":\"ssh\"") && net.contains("ssh fora dos servidores das chaves"),
        "{net}"
    );
}

#[test]
fn conexoes_de_banco_no_cofre() {
    let e = Env::new();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let db = |args: &[&str], input: &str| {
        let mut a = vec!["db"];
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
    };
    let o = db(
        &[
            "add",
            "app",
            "--kind",
            "postgres",
            "--host",
            "localhost",
            "--database",
            "app",
            "--user",
            "dev",
        ],
        "senha-do-banco\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let o = db(
        &[
            "add",
            "x",
            "--kind",
            "oracle",
            "--host",
            "h",
            "--database",
            "d",
            "--user",
            "u",
        ],
        "s\n",
    );
    assert!(
        !o.status.success() && stderr(&o).contains("postgres ou mysql"),
        "{}",
        stderr(&o)
    );
    let list = stdout(&db(&["list"], ""));
    assert!(
        list.contains("app  postgres  dev@localhost:5432/app"),
        "{list}"
    );
    assert!(!list.contains("senha-do-banco"));
    assert!(
        !file_text(&e).contains("senha-do-banco"),
        "a senha ficou em claro"
    );
    // Fora da troca de tokens.
    let vl = stdout(&vault(&e, &["list"], ""));
    assert!(vl.contains("app  db"), "{vl}");
    assert!(db(&["remove", "app"], "").status.success());
    assert!(stdout(&db(&["list"], "")).contains("Nenhuma conexão"));
}

/// Servidor HTTP sem TLS no 127.0.0.1 do host que devolve o `Authorization`.
fn echo_http() -> u16 {
    use std::io::Read;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { return };
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 4096];
                let n = c.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let auth = req
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("authorization: ")
                            .or_else(|| l.strip_prefix("Authorization: "))
                    })
                    .unwrap_or("-")
                    .to_string();
                let body = format!("auth={auth}\n");
                let _ = c.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    port
}

#[test]
fn chave_trocada_na_porta_do_host_em_http() {
    let e = Env::new();
    let port = echo_http();
    let fake = "sandmux-fake-porta0do0host0000000000000";
    let dir = e.data_dir().join("manifests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        format!(
            r#"{{"harness":"opencode","entries":[{{"file":".config/opencode/opencode.json","key":"Authorization","line":10,"fake":"{fake}"}}]}}"#
        ),
    )
    .unwrap();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let o = vault(&e, &["import"], "REAL-local-key\nhost.sandmux.internal\n");
    assert!(stderr(&o).contains("1 de 1"), "{}", stderr(&o));
    let p = port.to_string();
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--host-port", &p]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 30);
    t.write(
        format!(
            "sleep 1.5; curl -s -H 'Authorization: Bearer {fake}' http://127.0.0.1:{p}/v1/models; echo FIM-$((1+1))\r"
        )
        .as_bytes(),
    );
    t.wait_text("FIM-2");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(c.contains("auth=Bearer REAL-local-key"), "{c}");
    });
    let net = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect::<String>();
    assert!(net.contains("\"proto\":\"http\""), "{net}");
    assert!(
        net.contains("\"secret\":\"opencode:.config/opencode/opencode.json:Authorization\""),
        "{net}"
    );
    assert!(!net.contains("REAL-local-key"), "valor real no registro");
    // Sem o segredo para o host, a cópia é direta: o falso chega como está.
    assert!(
        vault(
            &e,
            &[
                "remove",
                "opencode:.config/opencode/opencode.json:Authorization"
            ],
            ""
        )
        .status
        .success()
    );
    t.write(
        format!(
            "curl -s -H 'Authorization: Bearer {fake}' http://127.0.0.1:{p}/; echo FIM-$((2+2))\r"
        )
        .as_bytes(),
    );
    t.wait_text("FIM-4");
    t.with_screen(|s| {
        let c = s.contents();
        assert!(c.contains(&format!("auth=Bearer {fake}")), "{c}");
    });
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn vault_set_troca_o_valor_e_mantem_o_falso() {
    let e = Env::new();
    let port = echo_http();
    let fake = "sandmux-fake-chave0vencida000000000000000";
    let name = "opencode:.config/opencode/opencode.json:Authorization";
    let dir = e.data_dir().join("manifests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        format!(
            r#"{{"harness":"opencode","entries":[{{"file":".config/opencode/opencode.json","key":"Authorization","line":10,"fake":"{fake}"}}]}}"#
        ),
    )
    .unwrap();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let o = vault(&e, &["import"], "REAL-velha\nhost.sandmux.internal\n");
    assert!(stderr(&o).contains("1 de 1"), "{}", stderr(&o));
    let p = port.to_string();
    let mut cmd = e.cmd(&e.work, &["run", "shell", "--host-port", &p]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 30);
    let curl = format!("curl -s -H 'Authorization: Bearer {fake}' http://127.0.0.1:{p}/v1/x");
    t.write(format!("sleep 1.5; {curl}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    t.wait_text("auth=Bearer REAL-velha");
    // A chave venceu: só o valor muda, sem reiniciar a aba.
    let o = vault(&e, &["set", name], "REAL-nova\n");
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("falso e os domínios continuam"),
        "{}",
        stderr(&o)
    );
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains(fake) && list.contains("host.sandmux.internal"),
        "{list}"
    );
    t.write(format!("clear; {curl}; echo FIM-$((2+2))\r").as_bytes());
    t.wait_text("FIM-4");
    t.wait_text("auth=Bearer REAL-nova");
    // Nome que não existe: aponta o `vault add`.
    let o = vault(&e, &["set", "naoexiste"], "x\n");
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("sandmux vault add naoexiste"),
        "{}",
        stderr(&o)
    );
    t.write(b"\x02d");
    t.wait_exit();
}

/// Servidor `/v1/models` no 127.0.0.1 do host: a lista só com a chave real.
fn models_server(real: &'static str) -> u16 {
    use std::io::Read;
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { return };
            let mut buf = vec![0u8; 4096];
            let n = c.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let ok = req.contains(&format!("Authorization: Bearer {real}"));
            let (status, body) = if ok {
                (
                    "200 OK",
                    r#"{"data":[{"id":"Bonsai-27B-GGUF"},{"id":"gemma-x"}]}"#,
                )
            } else {
                (
                    "401 Unauthorized",
                    r#"{"error":{"message":"Invalid token payload"}}"#,
                )
            };
            let _ = c.write_all(
                format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    port
}

#[test]
fn modelos_do_servidor_local_cadastrados_no_opencode() {
    let e = Env::new();
    let port = models_server("REAL-unsloth");
    let fake = "sandmux-fake-unsloth0000000000000000000";
    // opencode falso: roda o comando dado.
    let exe = e.home.join(".opencode/bin/opencode");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "#!/bin/sh\nexec \"$@\"\n").unwrap();
    std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let cfg = e.data_dir().join("homes/opencode/.config/opencode");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("opencode.json"),
        format!(
            r#"{{"provider":{{"unsloth-local":{{"npm":"@ai-sdk/openai-compatible","options":{{"baseURL":"http://127.0.0.1:{port}/v1","headers":{{"Authorization":"Bearer {fake}"}}}},"models":{{"manual":{{}}}}}}}}}}"#
        ),
    )
    .unwrap();
    let dir = e.data_dir().join("manifests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        format!(
            r#"{{"harness":"opencode","entries":[{{"file":".config/opencode/opencode.json","key":"Authorization","line":1,"fake":"{fake}"}}]}}"#
        ),
    )
    .unwrap();
    assert!(vault(&e, &["init"], "s\n").status.success());
    let o = vault(&e, &["import"], "REAL-unsloth\nhost.sandmux.internal\n");
    assert!(stderr(&o).contains("1 de 1"), "{}", stderr(&o));
    let p = port.to_string();
    let cmd = e.cmd(
        &e.work,
        &[
            "run",
            "opencode",
            "--no-import",
            "--host-port",
            &p,
            "--",
            "sh",
            "-c",
            "echo \"CFG=$OPENCODE_CONFIG_CONTENT\"; env | grep -c REAL-unsloth; sleep 30",
        ],
    );
    let mut t = Term::spawn(cmd, 250, 30);
    t.wait_text("CFG=");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let screen = t.screen().replace('\n', "");
    assert!(screen.contains("\"Bonsai-27B-GGUF\""), "{screen}");
    assert!(screen.contains("\"gemma-x\""), "{screen}");
    assert!(screen.contains("unsloth-local"), "{screen}");
    // A chave real não entrou no sandbox.
    assert!(!screen.contains("REAL-unsloth"), "{screen}");
    t.write(b"\x02d");
    t.wait_exit();
    // Depois de um reinício do daemon, o cofre tranca: a aba espera o
    // cofre, em vez de subir sem a lista, e volta com ela no desbloqueio.
    assert!(e.run(&e.work, &["daemon", "restart"]).status.success());
    let ls = stdout(&e.run(&e.work, &["ls"]));
    assert!(ls.contains("aguardando cofre"), "{ls}");
    let o = vault(&e, &["unlock"], "s\n");
    assert!(o.status.success(), "{}", stderr(&o));
    let mut viva = false;
    for _ in 0..100 {
        if stdout(&e.run(&e.work, &["ls"])).contains("viva") {
            viva = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(viva, "{}", stdout(&e.run(&e.work, &["ls"])));
    let mut t = Term::spawn(e.cmd(&e.work, &["attach", "1"]), 250, 30);
    t.wait_text("CFG=");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let screen = t.screen().replace('\n', "");
    assert!(screen.contains("\"Bonsai-27B-GGUF\""), "{screen}");
    t.write(b"\x02d");
    t.wait_exit();
}

#[test]
fn import_deduz_o_dominio_pelo_provedor_em_volta() {
    let e = Env::new();
    let auth = e
        .data_dir()
        .join("homes/opencode/.local/share/opencode/auth.json");
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    std::fs::write(
        &auth,
        "{\n  \"openrouter\": {\n    \"type\": \"api\",\n    \"key\": \"sandmux-fake-or00000000000000000000000000\"\n  }\n}\n",
    )
    .unwrap();
    let dir = e.data_dir().join("manifests");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("opencode.json"),
        r#"{"harness":"opencode","entries":[{"file":".local/share/opencode/auth.json","key":"key","line":4,"fake":"sandmux-fake-or00000000000000000000000000"}]}"#,
    )
    .unwrap();
    assert!(vault(&e, &["init"], "s\n").status.success());
    // Só o valor: o domínio vem do provedor, sem pergunta.
    let o = vault(&e, &["import"], "sk-or-real\n");
    let err = stderr(&o);
    // (A pergunta, com o provedor e a linha, só aparece num terminal.)
    assert!(
        err.contains("auth.json:openrouter.key guardado no cofre (openrouter.ai)"),
        "{err}"
    );
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("auth.json:openrouter.key  (openrouter.ai)"),
        "{list}"
    );
}

#[test]
fn doctor_acha_chaves_reais_e_fix_leva_ao_cofre() {
    let e = Env::new();
    let home = e.data_dir().join("homes/opencode");
    let auth = home.join(".local/share/opencode/auth.json");
    let cfg = home.join(".config/opencode/opencode.json");
    std::fs::create_dir_all(auth.parent().unwrap()).unwrap();
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(
        &auth,
        "{\n  \"openrouter\": {\n    \"type\": \"api\",\n    \"key\": \"sk-or-v1-REALOPENROUTER99\"\n  },\n  \"unsloth-local\": {\n    \"type\": \"api\",\n    \"key\": \"sk-unsloth-REALPASTED55\"\n  }\n}\n",
    )
    .unwrap();
    std::fs::write(
        &cfg,
        "{\n  \"provider\": {\n    \"unsloth-local\": {\n      \"options\": {\n        \"baseURL\": \"http://127.0.0.1:8888/v1\",\n        \"headers\": {\n          \"Authorization\": \"Bearer sk-unsloth-REALUNSLOTH77\"\n        }\n      }\n    }\n  }\n}\n",
    )
    .unwrap();
    let o = e.run(&e.work, &["doctor"]);
    let out = stdout(&o);
    assert!(out.contains("Chaves reais dentro do sandbox (3)"), "{out}");
    assert!(
        out.contains("opencode: .local/share/opencode/auth.json, provedor `openrouter`, chave `key` (linha 4)"),
        "{out}"
    );
    assert!(
        out.contains("provedor `unsloth-local`, chave `Authorization`"),
        "{out}"
    );
    assert!(
        !out.contains("REALOPENROUTER") && !out.contains("REALUNSLOTH"),
        "{out}"
    );
    // Sem cofre, o --fix pede para criar.
    let o = e.run(&e.work, &["doctor", "--fix"]);
    assert!(stderr(&o).contains("sandmux vault init"), "{}", stderr(&o));
    assert!(vault(&e, &["init"], "s\n").status.success());
    let o = e.run(&e.work, &["doctor", "--fix"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("opencode:.local/share/opencode/auth.json:openrouter.key  (openrouter.ai)"),
        "{list}"
    );
    assert!(
        list.contains(
            "opencode:.config/opencode/opencode.json:unsloth-local.Authorization  (host.sandmux.internal)"
        ),
        "{list}"
    );
    // A chave do `auth.json` acha o domínio pelo `baseURL` do `opencode.json`.
    assert!(
        list.contains(
            "opencode:.local/share/opencode/auth.json:unsloth-local.key  (host.sandmux.internal)"
        ),
        "{list}"
    );
    let a = std::fs::read_to_string(&auth).unwrap();
    let c = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        a.contains("sandmux-fake-") && !a.contains("REALOPENROUTER") && !a.contains("REALPASTED"),
        "{a}"
    );
    assert!(
        c.contains("Bearer sandmux-fake-") && !c.contains("REALUNSLOTH"),
        "{c}"
    );
    let out = stdout(&e.run(&e.work, &["doctor"]));
    assert!(
        out.contains("Chaves reais dentro do sandbox: nenhuma."),
        "{out}"
    );
}
