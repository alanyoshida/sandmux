//! Identidades de Kubernetes: o real nunca entra no sandbox. Um `kubectl`
//! falso faz o papel do kubeconfig do host, um script o de um plugin `exec`,
//! e um servidor HTTPS local, no `127.0.0.1` do host, o do API server, com a
//! CA própria do cluster e o certificado de cliente.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::Arc;

use base64::Engine;

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

fn script(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// O PKI de teste do cluster: a CA, o certificado do API server (IP
/// `127.0.0.1`) e um certificado de cliente.
struct Pki {
    ca_pem: String,
    server: (String, String),
    client: (String, String),
}

fn pki() -> Pki {
    use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
    let ca_key = KeyPair::generate().unwrap();
    let mut p = CertificateParams::new(Vec::<String>::new()).unwrap();
    p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = p.self_signed(&ca_key).unwrap();
    let leaf = |sans: Vec<String>, eku| {
        let k = KeyPair::generate().unwrap();
        let mut p = CertificateParams::new(sans).unwrap();
        p.extended_key_usages = vec![eku];
        let c = p.signed_by(&k, &ca, &ca_key).unwrap();
        (c.pem(), k.serialize_pem())
    };
    Pki {
        ca_pem: ca.pem(),
        server: leaf(
            vec!["127.0.0.1".into()],
            ExtendedKeyUsagePurpose::ServerAuth,
        ),
        client: leaf(Vec::new(), ExtendedKeyUsagePurpose::ClientAuth),
    }
}

/// O API server de teste: devolve o `Authorization`, o caminho, o `Host` e
/// se recebeu um certificado de cliente válido.
fn api_server(pki: &Pki) -> u16 {
    let certs = sandmux_net::tls::certs_of(&pki.server.0);
    let key = rustls_pemfile_key(&pki.server.1);
    let mut roots = rustls::RootCertStore::empty();
    for c in sandmux_net::tls::certs_of(&pki.ca_pem) {
        roots.add(c).unwrap();
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        provider.clone(),
    )
    .allow_unauthenticated()
    .build()
    .unwrap();
    let cfg = Arc::new(
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)
            .unwrap(),
    );
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
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let header = |name: &str| {
                    req.lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(": ")?;
                            k.eq_ignore_ascii_case(name).then(|| v.to_string())
                        })
                        .unwrap_or_else(|| "-".into())
                };
                let path = req
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("-")
                    .to_string();
                let client = if s.conn.peer_certificates().is_some() {
                    "sim"
                } else {
                    "nao"
                };
                let body = format!(
                    "recebido={} caminho={path} host={} cliente={client}",
                    header("authorization"),
                    header("host")
                );
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
    port
}

fn rustls_pemfile_key(pem: &str) -> rustls::pki_types::PrivateKeyDer<'static> {
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body)
        .unwrap();
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(der))
}

/// Os scripts do host: o `kubectl` (a visão do kubeconfig de teste) e o
/// plugin `exec`, que falha com o arquivo `sso-vencido`.
struct Host {
    kubectl: PathBuf,
    view: PathBuf,
}

fn host(e: &Env, port: u16, pki: &Pki) -> Host {
    let d = e.dir.path();
    let plugin = d.join("bin/aws-falso");
    script(
        &plugin,
        &format!(
            r#"#!/bin/sh
echo "$KUBERNETES_EXEC_INFO" > {info}
if [ -e {vencido} ]; then
  echo "trace da AWS" >&2
  echo "Error: the SSO session has expired; run aws sso login" >&2
  exit 255
fi
echo '{{"kind":"ExecCredential","apiVersion":"client.authentication.k8s.io/v1beta1","status":{{"token":"EXEC-REAL","expirationTimestamp":"2099-01-01T00:00:00Z"}}}}'
"#,
            info = d.join("exec-info").display(),
            vencido = d.join("sso-vencido").display()
        ),
    );
    let server = format!("https://127.0.0.1:{port}");
    let cluster =
        serde_json::json!({ "server": server, "certificate-authority-data": b64(&pki.ca_pem) });
    let view = serde_json::json!({
        "apiVersion": "v1",
        "kind": "Config",
        "current-context": "kind-dev",
        "clusters": [
            { "name": "dev", "cluster": cluster },
            { "name": "pre", "cluster": { "server": format!("{server}/pre"), "certificate-authority-data": b64(&pki.ca_pem) } }
        ],
        "users": [
            { "name": "token", "user": { "token": "k8s-REAL-TOKEN" } },
            { "name": "cert", "user": {
                "client-certificate-data": b64(&pki.client.0),
                "client-key-data": b64(&pki.client.1)
            } },
            { "name": "exec", "user": { "exec": {
                "command": plugin.display().to_string(),
                "args": ["eks", "get-token"],
                "apiVersion": "client.authentication.k8s.io/v1beta1",
                "env": [{ "name": "AWS_PROFILE", "value": "prod" }]
            } } },
            { "name": "velho", "user": { "auth-provider": { "name": "gcp" } } }
        ],
        "contexts": [
            { "name": "kind-dev", "context": { "cluster": "dev", "user": "token", "namespace": "app" } },
            { "name": "kind-cert", "context": { "cluster": "dev", "user": "cert" } },
            { "name": "arn:aws:eks:us-east-1:1:cluster/prod", "context": { "cluster": "pre", "user": "exec" } },
            { "name": "velho", "context": { "cluster": "dev", "user": "velho" } }
        ]
    });
    let view_file = d.join("view.json");
    std::fs::write(&view_file, view.to_string()).unwrap();
    let kubectl = d.join("bin/kubectl");
    script(
        &kubectl,
        &format!(
            r#"#!/bin/sh
echo "$KUBECONFIG" > {seen}
[ -e {quebrado} ] && {{ echo "error: o kubectl quebrou" >&2; exit 1; }}
cat {view}
"#,
            seen = d.join("kubeconfig-seen").display(),
            quebrado = d.join("quebrado").display(),
            view = view_file.display()
        ),
    );
    Host {
        kubectl,
        view: view_file,
    }
}

/// O cofre criado com o daemon já com o `kubectl` falso.
fn setup(e: &Env, h: &Host) {
    let mut c = e
        .cmd(&e.work, &["vault", "init"])
        .env("SANDMUX_KUBECTL", &h.kubectl)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
}

fn vault(e: &Env, h: &Host, args: &[&str]) -> Output {
    let mut a = vec!["vault"];
    a.extend_from_slice(args);
    e.cmd(&e.work, &a)
        .env("SANDMUX_KUBECTL", &h.kubectl)
        .env("KUBECONFIG", e.dir.path().join("kube-do-host"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn add(e: &Env, h: &Host, args: &[&str]) -> Output {
    let mut a = vec!["add-cloud", "kubernetes"];
    a.extend_from_slice(args);
    vault(e, h, &a)
}

/// O falso de uma identidade, pela lista do cofre.
fn fake_of(e: &Env, h: &Host, name: &str) -> String {
    let list = stdout(&vault(e, h, &["list"]));
    list.lines()
        .find(|l| l.starts_with(&format!("{name}  ")))
        .and_then(|l| l.split("falso: ").nth(1))
        .unwrap_or_else(|| panic!("sem {name}: {list}"))
        .trim()
        .to_string()
}

/// Roda `script` numa aba de shell e devolve a tela.
fn in_tab(e: &Env, script: &str) -> String {
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 400, 50);
    t.write(format!("{script}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    let screen = t.with_screen(|s| s.contents());
    t.write(b"exit\r");
    t.wait_exit();
    screen
}

#[test]
fn identidades_de_kubernetes_no_cofre() {
    let e = Env::new();
    let p = pki();
    let h = host(&e, 6443, &p);
    setup(&e, &h);
    // Recusas: um contexto que não existe, a cópia de um plugin exec, um
    // auth-provider pedido pelo nome.
    let o = add(&e, &h, &["--context", "nao-existe"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("não existe"), "{}", stderr(&o));
    let o = add(
        &e,
        &h,
        &[
            "--context",
            "arn:aws:eks:us-east-1:1:cluster/prod",
            "--copy",
        ],
    );
    assert!(!o.status.success());
    assert!(stderr(&o).contains("plugin exec"), "{}", stderr(&o));
    let o = add(&e, &h, &["--context", "velho"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("auth-provider"), "{}", stderr(&o));
    assert!(!stdout(&vault(&e, &h, &["list"])).contains("kube:"));
    // Cópia de um contexto: a chave fica cifrada no cofre.
    let o = add(&e, &h, &["--context", "kind-cert", "--copy"]);
    assert!(o.status.success(), "{}", stderr(&o));
    // Todos os outros: o auth-provider é pulado, e o repetido também.
    let o = add(&e, &h, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("contexto velho pulado"), "{err}");
    assert!(err.contains("contexto kind-cert pulado"), "{err}");
    assert!(
        err.contains("kube:arn-aws-eks-us-east-1-1-cluster-prod"),
        "{err}"
    );
    assert!(err.contains("RBAC"), "{err}");
    // O `kubectl` do host leu o KUBECONFIG do usuário.
    let seen = std::fs::read_to_string(e.dir.path().join("kubeconfig-seen")).unwrap();
    assert!(seen.contains("kube-do-host"), "{seen}");
    let list = stdout(&vault(&e, &h, &["list"]));
    assert!(
        list.contains("kube:kind-dev  cloud  kubernetes  host: contexto kind-dev  servidor: https://127.0.0.1:6443  (padrão)  falso: sandmux-fake-kube-"),
        "{list}"
    );
    assert!(
        list.contains("kube:kind-cert  cloud  kubernetes  cópia: contexto kind-cert"),
        "{list}"
    );
    assert!(!list.contains("REAL") && !list.contains("BEGIN"), "{list}");
    let file = std::fs::read_to_string(e.data_dir().join("vault.json")).unwrap();
    assert!(
        !file.contains("PRIVATE KEY") && !file.contains("kind-dev"),
        "cofre em claro"
    );
    // Tudo já guardado: nada novo.
    let o = add(&e, &h, &[]);
    assert!(o.status.success());
    assert!(
        stderr(&o).contains("nenhuma identidade nova"),
        "{}",
        stderr(&o)
    );
    // `set` recusa; `remove` apaga.
    assert!(!vault(&e, &h, &["set", "kube:kind-dev"]).status.success());
    // O doctor avisa da referência sem o kubectl no host.
    let o = e
        .cmd(&e.work, &["doctor"])
        .env("SANDMUX_KUBECTL", "/nonexistent/kubectl")
        .output()
        .unwrap();
    let out = stdout(&o) + &stderr(&o);
    assert!(
        out.contains("a identidade kube:kind-dev precisa do kubectl no host"),
        "{out}"
    );
    assert!(!out.contains("kube:kind-cert precisa"), "{out}");
    let _ = h.view;
}

#[test]
fn kubeconfig_da_aba() {
    let e = Env::new();
    let p = pki();
    let h = host(&e, 6443, &p);
    setup(&e, &h);
    let s = in_tab(
        &e,
        "test -e ~/.kube/config && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
    assert!(add(&e, &h, &[]).status.success());
    let s = in_tab(
        &e,
        "echo KC=$KUBECONFIG; python3 -c \"import json,os;c=json.load(open(os.environ['KUBECONFIG']));print('ATUAL='+c['current-context']);[print('SRV='+x['cluster']['server']) for x in c['clusters']];[print('CTX='+x['name']+'/'+x['context'].get('namespace','-')) for x in c['contexts']]\"; \
         grep -c 'BEGIN CERTIFICATE' ~/.kube/sandmux-ca.crt; grep -c sandmux-fake-kube- ~/.kube/config; \
         sed -i 's/\"current-context\": \"kind-dev\"/\"current-context\": \"kind-cert\"/' ~/.kube/config && grep -o 'current-context.*kind-cert' ~/.kube/config",
    );
    assert!(s.contains("KC=") && s.contains(".kube/config"), "{s}");
    assert!(s.contains("ATUAL=kind-dev"), "{s}");
    assert!(
        s.contains("SRV=https://kind-dev.kube.sandmux.internal"),
        "{s}"
    );
    assert!(
        s.contains("SRV=https://arn-aws-eks-us-east-1-1-cluster-prod.kube.sandmux.internal"),
        "{s}"
    );
    assert!(s.contains("CTX=kind-dev/app"), "{s}");
    assert!(
        s.contains("CTX=arn:aws:eks:us-east-1:1:cluster/prod/-"),
        "{s}"
    );
    assert!(!s.contains("velho"), "{s}");
    // A troca de contexto grava na pasta da aba.
    assert!(s.contains("current-context\": \"kind-cert"), "{s}");
    assert!(!s.contains("k8s-REAL") && !s.contains("PRIVATE"), "{s}");
    // Nada fica no HOME sintético depois da aba.
    let mut found = Vec::new();
    for d in [&e.data, &e.home] {
        let o = std::process::Command::new("grep")
            .args(["-rl", "--exclude=.bash_history", "sandmux-fake-kube-"])
            .arg(d)
            .output()
            .unwrap();
        found.push(stdout(&o));
    }
    assert!(found.iter().all(|f| f.trim().is_empty()), "{found:?}");
    // Com o cofre trancado, nada é acrescentado.
    assert!(vault(&e, &h, &["lock"]).status.success());
    let s = in_tab(
        &e,
        "test -e ~/.kube/config && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
}

fn net_log(e: &Env) -> String {
    std::fs::read_dir(e.state.join("sandmux/net"))
        .map(|d| {
            d.flatten()
                .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
                .filter_map(|f| std::fs::read_to_string(f.path()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn get(name: &str, fake: &str, path: &str) -> String {
    format!(
        "echo {name}=$(curl -s --cacert ~/.kube/sandmux-ca.crt -H 'Authorization: Bearer {fake}' https://{name}.kube.sandmux.internal{path})"
    )
}

#[test]
fn real_do_kubernetes_so_no_gateway() {
    let e = Env::new();
    let p = pki();
    let port = api_server(&p);
    let h = host(&e, port, &p);
    setup(&e, &h);
    assert!(
        add(&e, &h, &["--context", "kind-cert", "--copy"])
            .status
            .success()
    );
    assert!(add(&e, &h, &[]).status.success());
    let eks = "arn-aws-eks-us-east-1-1-cluster-prod";
    let script = [
        get(
            "kind-dev",
            &fake_of(&e, &h, "kube:kind-dev"),
            "/api/v1/pods?watch=1",
        ),
        get("kind-cert", &fake_of(&e, &h, "kube:kind-cert"), "/api"),
        get(eks, &fake_of(&e, &h, &format!("kube:{eks}")), "/apis"),
        get("nenhum", "x", "/api"),
    ]
    .join("; ");
    let s = in_tab(&e, &script);
    // Token: o real no API server, com o Host real; o falso nunca chega.
    assert!(
        s.contains(&format!(
            "kind-dev=recebido=Bearer k8s-REAL-TOKEN caminho=/api/v1/pods?watch=1 host=127.0.0.1:{port} cliente=nao"
        )),
        "{s}"
    );
    // Certificado de cliente: sem Authorization, com o certificado.
    assert!(s.contains("kind-cert=recebido=- caminho=/api host="), "{s}");
    assert!(s.contains("cliente=sim"), "{s}");
    // Plugin exec rodado no host, e o prefixo do servidor no caminho.
    assert!(
        s.contains(&format!(
            "{eks}=recebido=Bearer EXEC-REAL caminho=/pre/apis"
        )),
        "{s}"
    );
    let info = std::fs::read_to_string(e.dir.path().join("exec-info")).unwrap();
    assert!(info.contains("\"interactive\":false"), "{info}");
    // Um nome sem identidade: 404 do próprio gateway.
    assert!(s.contains("nenhum={") && s.contains("\"code\":404"), "{s}");
    let net = net_log(&e);
    assert!(net.contains("\"secret\":\"kube:kind-dev\""), "{net}");
    assert!(net.contains("\"secret\":\"kube:kind-cert\""), "{net}");
    assert!(
        !net.contains("k8s-REAL") && !net.contains("EXEC-REAL"),
        "valor no registro"
    );
}

#[test]
fn sem_o_real_do_kubernetes_401() {
    let e = Env::new();
    let p = pki();
    let port = api_server(&p);
    let h = host(&e, port, &p);
    setup(&e, &h);
    assert!(
        add(
            &e,
            &h,
            &["--context", "arn:aws:eks:us-east-1:1:cluster/prod"]
        )
        .status
        .success()
    );
    std::fs::write(e.dir.path().join("sso-vencido"), "").unwrap();
    let eks = "arn-aws-eks-us-east-1-1-cluster-prod";
    let fake = fake_of(&e, &h, &format!("kube:{eks}"));
    let s = in_tab(
        &e,
        &format!("{}; {}", get(eks, &fake, "/api"), get(eks, &fake, "/apis")),
    );
    assert!(s.contains("\"code\":401"), "{s}");
    assert!(s.contains("aws sso login"), "{s}");
    assert!(!s.contains("recebido="), "{s}");
    let log = std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default();
    assert_eq!(log.matches(&format!("kube:{eks}:")).count(), 1, "{log}");
}
