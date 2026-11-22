//! HTTP/2 no gateway: o protocolo segue o cliente e o destino, a chave do
//! cofre é trocada também num pedido HTTP/2, e `http2 = false` volta ao
//! HTTP/1.1. O destino é um servidor local com a CA de teste, que fala h2 e
//! HTTP/1.1, no `host.sandmux.internal`.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::process::{Output, Stdio};
use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Incoming;

/// O destino de teste: responde o protocolo do pedido e o `Authorization`
/// recebido, em HTTP/2 ou HTTP/1.1, pelo ALPN que o cliente negociar.
fn h2_server() -> (u16, String) {
    let (ca_cert, ca_key) = sandmux_net::tls::generate_ca().unwrap();
    let ca = sandmux_net::tls::Ca::from_pem(&ca_cert, &ca_key).unwrap();
    let cfg = ca
        .server_config_alpn("host.sandmux.internal", true)
        .unwrap();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let l = tokio::net::TcpListener::from_std(l).unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(cfg);
            loop {
                let Ok((c, _)) = l.accept().await else { return };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(c).await else {
                        return;
                    };
                    let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2");
                    let svc =
                        hyper::service::service_fn(|req: hyper::Request<Incoming>| async move {
                            let auth = req
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("-")
                                .to_string();
                            let text = format!("eco={:?} recebido={auth}\n", req.version());
                            Ok::<_, std::convert::Infallible>(hyper::Response::new(
                                http_body_util::Full::new(bytes::Bytes::from(text)),
                            ))
                        });
                    let io = hyper_util::rt::TokioIo::new(tls);
                    if h2 {
                        let _ = hyper::server::conn::http2::Builder::new(
                            hyper_util::rt::TokioExecutor::new(),
                        )
                        .serve_connection(io, svc)
                        .await;
                    } else {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, svc)
                            .await;
                    }
                });
            }
        });
    });
    (port, ca_cert)
}

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

/// O cofre com o daemon já com a CA de teste, e a chave `seg` para o
/// destino; devolve o falso dela.
fn setup(e: &Env, ca: &str) -> String {
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
        e,
        &["add", "seg", "--domain", "host.sandmux.internal"],
        "real-h2\n",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let list = stdout(&vault(e, &["list"], ""));
    list.split("falso: ").nth(1).unwrap().trim().to_string()
}

/// Roda `script` numa aba de shell e devolve a tela.
fn in_tab(e: &Env, script: &str) -> String {
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 40);
    t.write(format!("{script}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    let screen = t.with_screen(|s| s.contents());
    t.write(b"exit\r");
    t.wait_exit();
    screen
}

fn curls(port: u16, fake: &str) -> String {
    let get = |flag: &str, tag: &str| {
        format!(
            "echo {tag}=$(/usr/bin/curl -s {flag} -o /dev/null -w '%{{http_version}}' https://host.sandmux.internal:{port}/) $(/usr/bin/curl -s {flag} -H 'Authorization: Bearer {fake}' https://host.sandmux.internal:{port}/)"
        )
    };
    [get("--http2", "H2"), get("--http1.1", "H1")].join("; ")
}

#[test]
fn http2_decifrado_segue_o_cliente() {
    let e = Env::new();
    let (port, ca) = h2_server();
    let fake = setup(&e, &ca);
    let s = in_tab(&e, &curls(port, &fake));
    // Com o cliente e o destino em h2, as duas pontas falam HTTP/2, e a
    // chave é trocada.
    assert!(
        s.contains("H2=2 eco=HTTP/2.0 recebido=Bearer real-h2"),
        "{s}"
    );
    // Um cliente só com HTTP/1.1 fica no HTTP/1.1 até o destino.
    assert!(
        s.contains("H1=1.1 eco=HTTP/1.1 recebido=Bearer real-h2"),
        "{s}"
    );
    let net: String = std::fs::read_dir(e.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|f| std::fs::read_to_string(f.path()).unwrap())
        .collect();
    assert!(net.matches("\"secret\":\"seg\"").count() >= 2, "{net}");
    assert!(!net.contains("real-h2"), "valor no registro");
}

#[test]
fn http2_desligado_no_projeto() {
    let e = Env::new();
    let (port, ca) = h2_server();
    let fake = setup(&e, &ca);
    let work = e.work.display().to_string();
    let o = e.run(
        &e.work,
        &["config", "set", "--project", &work, "http2", "false"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let s = in_tab(&e, &curls(port, &fake));
    assert!(
        s.contains("H2=1.1 eco=HTTP/1.1 recebido=Bearer real-h2"),
        "{s}"
    );
    // O `config show` mostra a chave e de onde ela vem.
    let o = e.run(&e.work, &["config", "show"]);
    let shown = stdout(&o);
    assert!(
        shown
            .lines()
            .any(|l| l.contains("http2") && l.contains("false")),
        "{shown}"
    );
}

/// O cliente h2 do próprio teste confere que o destino fala h2 sem o
/// gateway (o servidor de teste é o que diz ser).
#[test]
fn servidor_de_teste_fala_h2() {
    let (port, ca) = h2_server();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut roots = rustls::RootCertStore::empty();
        for c in sandmux_net::tls::certs_of(&ca) {
            roots.add(c).unwrap();
        }
        let mut cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        cfg.alpn_protocols = vec![b"h2".to_vec()];
        let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let tls = tokio_rustls::TlsConnector::from(Arc::new(cfg))
            .connect("host.sandmux.internal".try_into().unwrap(), tcp)
            .await
            .unwrap();
        assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
        let (mut s, conn) =
            hyper::client::conn::http2::handshake::<_, _, http_body_util::Empty<bytes::Bytes>>(
                hyper_util::rt::TokioExecutor::new(),
                hyper_util::rt::TokioIo::new(tls),
            )
            .await
            .unwrap();
        tokio::spawn(conn);
        let req = hyper::Request::builder()
            .uri(format!("https://host.sandmux.internal:{port}/"))
            .body(http_body_util::Empty::new())
            .unwrap();
        let r = s.send_request(req).await.unwrap();
        let body = r.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("eco=HTTP/2.0"));
    });
}
