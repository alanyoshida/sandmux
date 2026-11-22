//! Identidades de cloud (GCP): o real nunca entra no sandbox. Um `gcloud`
//! falso faz o papel da CLI do host, e um servidor local com a CA de teste,
//! no `host.sandmux.internal`, o das APIs do Google.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// O `gcloud` falso: a configuração `trabalho` existe; o `config-helper` dá
/// o token real, ou pede um login novo quando o arquivo `vencido` existe.
fn fake_gcloud(e: &Env) -> PathBuf {
    let p = e.dir.path().join("bin/gcloud");
    let vencido = e.dir.path().join("vencido");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(
        &p,
        format!(
            r#"#!/bin/sh
case "$*" in
  *"configurations describe trabalho"*)
    echo '{{"name":"trabalho","properties":{{"core":{{"account":"ana@exemplo.com","project":"app-dev"}}}}}}' ;;
  *"configurations describe"*)
    echo "ERROR: (gcloud.config.configurations.describe) Cannot find a configuration" >&2; exit 1 ;;
  *config-helper*)
    if [ -e {vencido} ]; then
      echo "ERROR: (gcloud.config.config-helper) Reauthentication required." >&2; exit 1
    fi
    echo '{{"credential":{{"access_token":"ya29.REAL-DO-HOST","token_expiry":"2099-01-01T00:00:00Z"}}}}' ;;
  *) exit 2 ;;
esac
"#,
            vencido = vencido.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// `sandmux vault ...` com `input` na entrada padrão e as variáveis extras.
fn vault_env(e: &Env, args: &[&str], input: &str, env: &[(&str, &Path)]) -> Output {
    let mut a = vec!["vault"];
    a.extend_from_slice(args);
    let mut cmd = e.cmd(&e.work, &a);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut c = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

fn vault(e: &Env, args: &[&str], input: &str) -> Output {
    vault_env(e, args, input, &[])
}

/// O cofre criado com o daemon já com o `gcloud` falso, o domínio de teste e
/// a CA dos destinos de teste.
fn setup(e: &Env, ca: Option<&str>) -> PathBuf {
    let g = fake_gcloud(e);
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca.unwrap_or("")).unwrap();
    let mut cmd = e.cmd(&e.work, &["vault", "init"]);
    cmd.env("SANDMUX_GCLOUD", &g)
        .env("SANDMUX_TEST_GCP_DOMAIN", "host.sandmux.internal");
    if ca.is_some() {
        cmd.env("SANDMUX_UPSTREAM_CA", &ca_file);
    }
    let mut c = cmd.stdin(Stdio::piped()).spawn().unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    g
}

/// A chave JSON de uma conta de serviço de teste.
fn sa_key(e: &Env) -> PathBuf {
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).unwrap();
    let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    let v = serde_json::json!({
        "type": "service_account",
        "project_id": "app",
        "private_key_id": "k1",
        "private_key": pem,
        "client_email": "deploy@app.iam.gserviceaccount.com",
        "token_uri": "https://oauth2.googleapis.com/token",
    });
    let p = e.dir.path().join("sa.json");
    std::fs::write(&p, v.to_string()).unwrap();
    p
}

#[test]
fn identidades_de_gcp_no_cofre() {
    let e = Env::new();
    let g = setup(&e, None);
    let with_g = [("SANDMUX_GCLOUD", g.as_path())];
    // Referência ao host.
    let o = vault_env(
        &e,
        &[
            "add-cloud",
            "gcp",
            "trabalho",
            "--configuration",
            "trabalho",
        ],
        "",
        &with_g,
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("IAM"), "{}", stderr(&o));
    // Uma configuração que não existe no host.
    let o = vault_env(
        &e,
        &["add-cloud", "gcp", "outra", "--configuration", "nao-existe"],
        "",
        &with_g,
    );
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("não existe no gcloud do host"),
        "{}",
        stderr(&o)
    );
    // Cópia do ADC do host.
    let adc = e
        .home
        .join(".config/gcloud/application_default_credentials.json");
    std::fs::create_dir_all(adc.parent().unwrap()).unwrap();
    std::fs::write(
        &adc,
        r#"{"type":"authorized_user","client_id":"c","client_secret":"s","refresh_token":"1//REFRESH-REAL"}"#,
    )
    .unwrap();
    let o = vault(&e, &["add-cloud", "gcp", "pessoal", "--copy-adc"], "");
    assert!(o.status.success(), "{}", stderr(&o));
    // Cópia de uma chave de conta de serviço.
    let key = sa_key(&e);
    let o = vault(
        &e,
        &[
            "add-cloud",
            "gcp",
            "deploy",
            "--copy-key",
            key.to_str().unwrap(),
        ],
        "",
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let before = std::fs::read_to_string(&key).unwrap();
    // Recusas: arquivo errado, duas fontes, nenhuma, nome repetido.
    let o = vault(
        &e,
        &[
            "add-cloud",
            "gcp",
            "errada",
            "--copy-key",
            adc.to_str().unwrap(),
        ],
        "",
    );
    assert!(!o.status.success());
    assert!(stderr(&o).contains("service_account"), "{}", stderr(&o));
    let o = vault(
        &e,
        &["add-cloud", "gcp", "x", "--copy-adc", "--copy-key", "a"],
        "",
    );
    assert!(!o.status.success());
    assert!(!vault(&e, &["add-cloud", "gcp", "y"], "").status.success());
    assert!(
        !vault(&e, &["add-cloud", "gcp", "pessoal", "--copy-adc"], "")
            .status
            .success()
    );
    // A lista: os dados públicos, nunca o refresh nem a chave.
    let list = stdout(&vault(&e, &["list"], ""));
    assert!(
        list.contains("gcp:trabalho  cloud  gcp  host: configuração trabalho  conta: ana@exemplo.com  projeto: app-dev  (padrão)"),
        "{list}"
    );
    assert!(
        list.contains("gcp:pessoal  cloud  gcp  cópia: ADC"),
        "{list}"
    );
    assert!(
        list.contains("gcp:deploy  cloud  gcp  cópia: conta de serviço  conta: deploy@app.iam.gserviceaccount.com"),
        "{list}"
    );
    assert!(list.contains("falso: ya29.sandmux-fake-"), "{list}");
    assert!(
        !list.contains("REFRESH-REAL") && !list.contains("PRIVATE KEY"),
        "{list}"
    );
    assert_eq!(std::fs::read_to_string(&key).unwrap(), before);
    // Nem o arquivo do cofre tem algo em claro.
    let file = std::fs::read_to_string(e.data_dir().join("vault.json")).unwrap();
    assert!(
        !file.contains("REFRESH-REAL") && !file.contains("trabalho"),
        "cofre em claro"
    );
    // `set` recusa; `remove` apaga.
    let o = vault(&e, &["set", "gcp:pessoal"], "novo\n");
    assert!(!o.status.success());
    assert!(stderr(&o).contains("add-cloud"), "{}", stderr(&o));
    assert!(vault(&e, &["remove", "gcp:pessoal"], "").status.success());
    assert!(!stdout(&vault(&e, &["list"], "")).contains("gcp:pessoal"));
    // O doctor avisa da referência sem o gcloud no host.
    let o = e
        .cmd(&e.work, &["doctor"])
        .env("SANDMUX_GCLOUD", "/nonexistent/gcloud")
        .output()
        .unwrap();
    let out = stdout(&o) + &stderr(&o);
    assert!(
        out.contains("a identidade gcp:trabalho precisa do gcloud no host"),
        "{out}"
    );
    let o = e
        .cmd(&e.work, &["doctor"])
        .env("SANDMUX_GCLOUD", &g)
        .output()
        .unwrap();
    assert!(!(stdout(&o) + &stderr(&o)).contains("precisa do gcloud"));
}

/// Roda `script` numa aba de shell e devolve a tela.
fn in_tab(e: &Env, extra: &[&str], script: &str) -> String {
    let mut a = vec!["run", "shell"];
    a.extend_from_slice(extra);
    let mut cmd = e.cmd(&e.work, &a);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 200, 50);
    t.write(format!("{script}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    let screen = t.with_screen(|s| s.contents());
    t.write(b"exit\r");
    t.wait_exit();
    screen
}

#[test]
fn gcloud_da_aba_com_as_identidades() {
    let e = Env::new();
    let g = setup(&e, None);
    // Sem identidade, a pasta do gcloud da aba é a guardada.
    let s = in_tab(
        &e,
        &[],
        "test -e ~/.config/gcloud/active_config && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
    let with_g = [("SANDMUX_GCLOUD", g.as_path())];
    let ok = |args: &[&str]| {
        let o = vault_env(&e, args, "", &with_g);
        assert!(o.status.success(), "{}", stderr(&o));
    };
    ok(&[
        "add-cloud",
        "gcp",
        "trabalho",
        "--configuration",
        "trabalho",
    ]);
    let key = sa_key(&e);
    ok(&[
        "add-cloud",
        "gcp",
        "deploy",
        "--copy-key",
        key.to_str().unwrap(),
        "--default",
    ]);
    let s = in_tab(
        &e,
        &[],
        "cd ~/.config/gcloud && echo ATIVA=$(cat active_config) && grep -h = configurations/config_trabalho && \
         echo TOKEN=$(cat sandmux/deploy.token) && grep -o '\"refresh_token\": \"[^\"]*' application_default_credentials.json && \
         echo trabalho > active_config && echo GRAVOU=$(cat active_config)",
    );
    assert!(s.contains("ATIVA=deploy"), "{s}");
    assert!(s.contains("account = ana@exemplo.com"), "{s}");
    assert!(s.contains("project = app-dev"), "{s}");
    assert!(s.contains("access_token_file = "), "{s}");
    assert!(s.contains("TOKEN=ya29.sandmux-fake-"), "{s}");
    assert!(s.contains("\"refresh_token\": \"1//sandmux-fake-"), "{s}");
    // O `gcloud config configurations activate` grava a ativa.
    assert!(s.contains("GRAVOU=trabalho"), "{s}");
    // Nada disso fica no HOME sintético nem no disco depois da aba.
    let leftovers = Command::new("grep")
        .args([
            "-rl",
            "sandmux-fake-",
            "--include=config_*",
            "--include=active_config",
        ])
        .arg(&e.data)
        .arg(&e.home)
        .output()
        .unwrap();
    assert!(
        stdout(&leftovers).trim().is_empty(),
        "{}",
        stdout(&leftovers)
    );
    // Com o cofre trancado, nada é acrescentado.
    assert!(vault(&e, &["lock"], "").status.success());
    let s = in_tab(
        &e,
        &[],
        "test -e ~/.config/gcloud/active_config && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
}

/// Servidor HTTPS local (CA de teste) que devolve o `Authorization` e o
/// caminho recebidos.
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

/// Os pedidos da aba: à API, com o token falso, e de token, com o refresh
/// falso.
fn requests(port: u16) -> String {
    format!(
        "T=$(cat ~/.config/gcloud/sandmux/trabalho.token); \
         R=$(grep -o '\"refresh_token\": \"[^\"]*' ~/.config/gcloud/application_default_credentials.json | cut -d'\"' -f4); \
         echo API=$(curl -s -H \"Authorization: Bearer $T\" https://host.sandmux.internal:{port}/storage/v1/b); \
         echo TOKEN=$(curl -s --data-urlencode grant_type=refresh_token --data-urlencode \"refresh_token=$R\" https://host.sandmux.internal:{port}/token)"
    )
}

#[test]
fn real_de_gcp_so_no_gateway() {
    let e = Env::new();
    let (port, ca) = echo_https();
    let g = setup(&e, Some(&ca));
    let o = vault_env(
        &e,
        &[
            "add-cloud",
            "gcp",
            "trabalho",
            "--configuration",
            "trabalho",
        ],
        "",
        &[("SANDMUX_GCLOUD", g.as_path())],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let s = in_tab(&e, &[], &requests(port));
    // O destino recebe o real do gcloud do host; o sandbox só vê o falso.
    assert!(
        s.contains("API=recebido=Bearer ya29.REAL-DO-HOST caminho=/storage/v1/b"),
        "{s}"
    );
    // O pedido de token é respondido pelo gateway, sem ir ao destino.
    assert!(
        s.contains("TOKEN={\"access_token\":\"ya29.sandmux-fake-"),
        "{s}"
    );
    assert!(!s.contains("TOKEN=recebido"), "{s}");
    assert!(!s.contains("TOKEN={\"access_token\":\"ya29.REAL"), "{s}");
    // O registro tem o nome da identidade, sem o real.
    let net = net_log(&e);
    assert!(net.contains("\"secret\":\"gcp:trabalho\""), "{net}");
    assert!(!net.contains("REAL-DO-HOST"), "valor no registro");
    // Cofre trancado: sai o falso (o `--decrypt` mantém a decifração).
    assert!(vault(&e, &["lock"], "").status.success());
    let s = in_tab(
        &e,
        &["--decrypt", "host.sandmux.internal"],
        &format!(
            "echo API=$(curl -s -H 'Authorization: Bearer ya29.sandmux-fake-x' https://host.sandmux.internal:{port}/v1)"
        ),
    );
    assert!(s.contains("API=recebido=Bearer ya29.sandmux-fake-x"), "{s}");
}

#[test]
fn sem_o_real_nada_e_trocado() {
    let e = Env::new();
    let (port, ca) = echo_https();
    let g = setup(&e, Some(&ca));
    let o = vault_env(
        &e,
        &[
            "add-cloud",
            "gcp",
            "trabalho",
            "--configuration",
            "trabalho",
        ],
        "",
        &[("SANDMUX_GCLOUD", g.as_path())],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    // O login do host venceu.
    std::fs::write(e.dir.path().join("vencido"), "").unwrap();
    let s = in_tab(&e, &[], &requests(port));
    assert!(s.contains("API=recebido=Bearer ya29.sandmux-fake-"), "{s}");
    assert!(
        s.contains("invalid_grant") && s.contains("gcloud auth login"),
        "{s}"
    );
    let log = std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default();
    assert_eq!(log.matches("gcp:trabalho:").count(), 1, "{log}");
}
