//! Identidades da AWS: o real nunca entra no sandbox. Um `aws` falso faz o
//! papel do `aws` do host; na aba, um cliente SigV4 mínimo em Python assina
//! com o par falso; um servidor HTTPS local, no `host.sandmux.internal`,
//! confere a assinatura com a chave real e faz o papel do STS.

mod common;
use common::cli::*;
use common::pty::Term;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use sandmux_net::sigv4;

/// As chaves reais que o servidor conhece.
const SECRETS: &[(&str, &str)] = &[
    ("AKIAREALCI", "SEGREDO-REAL-CI"),
    ("ASIAREALHOST", "SEGREDO-REAL-HOST"),
    ("ASIAREALCAPT", "SEGREDO-REAL-CAPT"),
];

fn script(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// O `aws` falso: o `export-credentials` do perfil `Dev_SSO` dá uma
/// credencial temporária, ou falha com o SSO vencido quando o arquivo
/// `sso-vencido` existe.
fn fake_aws(e: &Env) -> PathBuf {
    let p = e.dir.path().join("bin/aws");
    script(
        &p,
        &format!(
            r#"#!/bin/sh
case "$*" in
  *"export-credentials --profile Dev_SSO"*)
    if [ -e {vencido} ]; then
      echo "" >&2
      echo "Error when retrieving token from sso: Token has expired and refresh failed" >&2
      exit 255
    fi
    echo '{{"Version":1,"AccessKeyId":"ASIAREALHOST","SecretAccessKey":"SEGREDO-REAL-HOST","SessionToken":"TOKEN-REAL-HOST","Expiration":"2099-01-01T00:00:00+00:00"}}' ;;
  *"export-credentials --profile default"*)
    echo '{{"Version":1,"AccessKeyId":"AKIAREALCI","SecretAccessKey":"SEGREDO-REAL-CI"}}' ;;
  *) echo "The config profile could not be found" >&2; exit 255 ;;
esac
"#,
            vencido = e.dir.path().join("sso-vencido").display()
        ),
    );
    p
}

/// Os arquivos do host: `default` e `ci` com chave estática, `Dev_SSO` de
/// SSO e `temp` com um token de sessão.
fn host_files(e: &Env) {
    let d = e.home.join(".aws");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("config"),
        "[default]\nregion = us-east-1\n\n[profile Dev_SSO]\nsso_session = corp\nregion = sa-east-1\n\n[profile ci]\nregion = eu-west-1\n\n[sso-session corp]\nsso_start_url = https://x\n",
    )
    .unwrap();
    std::fs::write(
        d.join("credentials"),
        "[default]\naws_access_key_id = AKIAREALCI\naws_secret_access_key = SEGREDO-REAL-CI\n\n[ci]\naws_access_key_id = AKIAREALCI\naws_secret_access_key = SEGREDO-REAL-CI\n\n[temp]\naws_access_key_id = ASIAX\naws_secret_access_key = S\naws_session_token = T\n",
    )
    .unwrap();
}

/// O cofre criado com o daemon já com o `aws` falso, o domínio de teste e a
/// CA dos destinos de teste.
fn setup(e: &Env, ca: Option<&str>) -> PathBuf {
    let aws = fake_aws(e);
    host_files(e);
    let ca_file = e.dir.path().join("ca.pem");
    std::fs::write(&ca_file, ca.unwrap_or("")).unwrap();
    let mut cmd = e.cmd(&e.work, &["vault", "init"]);
    cmd.env("SANDMUX_AWS", &aws)
        .env("SANDMUX_TEST_AWS_DOMAIN", "host.sandmux.internal");
    if ca.is_some() {
        cmd.env("SANDMUX_UPSTREAM_CA", &ca_file);
    }
    let mut c = cmd.stdin(Stdio::piped()).spawn().unwrap();
    c.stdin.take().unwrap().write_all(b"s3nha\n").unwrap();
    assert!(c.wait().unwrap().success());
    aws
}

fn vault(e: &Env, aws: &Path, args: &[&str]) -> Output {
    let mut a = vec!["vault"];
    a.extend_from_slice(args);
    e.cmd(&e.work, &a)
        .env("SANDMUX_AWS", aws)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn add(e: &Env, aws: &Path, args: &[&str]) -> Output {
    let mut a = vec!["add-cloud", "aws"];
    a.extend_from_slice(args);
    vault(e, aws, &a)
}

#[test]
fn identidades_da_aws_no_cofre() {
    let e = Env::new();
    let aws = setup(&e, None);
    // Recusas: um perfil que não existe, a cópia de um perfil de SSO.
    let o = add(&e, &aws, &["--profile", "nao-existe"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("não existe"), "{}", stderr(&o));
    let o = add(&e, &aws, &["--profile", "Dev_SSO", "--copy"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("chave estática"), "{}", stderr(&o));
    assert!(!stdout(&vault(&e, &aws, &["list"])).contains("aws:"));
    // Cópia de um perfil com chave estática.
    let o = add(&e, &aws, &["--profile", "ci", "--copy"]);
    assert!(o.status.success(), "{}", stderr(&o));
    // Todos os outros, por referência; o repetido é pulado.
    let o = add(&e, &aws, &[]);
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("perfil ci pulado"), "{err}");
    assert!(err.contains("aws:dev-sso  (Dev_SSO)"), "{err}");
    assert!(
        err.contains("IAM") && err.contains("iam:CreateAccessKey"),
        "{err}"
    );
    let list = stdout(&vault(&e, &aws, &["list"]));
    assert!(
        list.contains("aws:default  cloud  aws  host: perfil default  região: us-east-1  (padrão)  falso: AKIASANDMUX"),
        "{list}"
    );
    assert!(
        list.contains("aws:dev-sso  cloud  aws  host: perfil Dev_SSO  região: sa-east-1"),
        "{list}"
    );
    assert!(
        list.contains("aws:ci  cloud  aws  cópia: perfil ci  região: eu-west-1"),
        "{list}"
    );
    assert!(
        list.contains("aws:temp  cloud  aws  host: perfil temp"),
        "{list}"
    );
    assert!(
        !list.contains("SEGREDO") && !list.contains("AKIAREAL"),
        "{list}"
    );
    let file = std::fs::read_to_string(e.data_dir().join("vault.json")).unwrap();
    assert!(
        !file.contains("SEGREDO") && !file.contains("Dev_SSO"),
        "cofre em claro"
    );
    // O doctor avisa das referências sem o aws no host, não da cópia.
    let o = e
        .cmd(&e.work, &["doctor"])
        .env("SANDMUX_AWS", "/nonexistent/aws")
        .output()
        .unwrap();
    let out = stdout(&o) + &stderr(&o);
    assert!(
        out.contains("a identidade aws:dev-sso precisa do aws no host"),
        "{out}"
    );
    assert!(!out.contains("aws:ci precisa"), "{out}");
}

/// Roda `script` numa aba de shell e devolve a tela.
fn in_tab(e: &Env, script: &str) -> String {
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = Term::spawn(cmd, 400, 60);
    t.write(format!("{script}; echo FIM-$((1+1))\r").as_bytes());
    t.wait_text("FIM-2");
    let screen = t.with_screen(|s| s.contents());
    t.write(b"exit\r");
    t.wait_exit();
    screen
}

#[test]
fn pasta_do_aws_da_aba() {
    let e = Env::new();
    let aws = setup(&e, None);
    let s = in_tab(
        &e,
        "test -n \"$AWS_CONFIG_FILE\" && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
    assert!(
        add(&e, &aws, &["--profile", "ci", "--copy"])
            .status
            .success()
    );
    assert!(add(&e, &aws, &["--profile", "Dev_SSO"]).status.success());
    let s = in_tab(
        &e,
        "echo CFG=$AWS_CONFIG_FILE; tr '\\n' '|' < $AWS_CONFIG_FILE; echo; tr '\\n' '|' < $AWS_SHARED_CREDENTIALS_FILE; echo; echo CA=$AWS_CA_BUNDLE; mkdir -p ~/.aws/cli/cache && echo GRAVA-$((3*3))",
    );
    assert!(s.contains("CFG=") && s.contains(".aws/config"), "{s}");
    // A padrão é a primeira guardada (sem o perfil default no cofre).
    assert!(s.contains("[default]|region = eu-west-1|"), "{s}");
    assert!(s.contains("[profile ci]|region = eu-west-1|"), "{s}");
    assert!(s.contains("[profile Dev_SSO]|region = sa-east-1|"), "{s}");
    assert!(s.contains("[ci]|aws_access_key_id = AKIASANDMUX"), "{s}");
    assert!(!s.contains("SEGREDO") && !s.contains("AKIAREAL"), "{s}");
    assert!(s.contains("CA=/"), "{s}");
    assert!(s.contains("GRAVA-9"), "{s}");
    assert!(vault(&e, &aws, &["lock"]).status.success());
    let s = in_tab(
        &e,
        "test -n \"$AWS_CONFIG_FILE\" && echo TEM-$((2*2)) || echo NAO-TEM-$((2*2))",
    );
    assert!(s.contains("NAO-TEM-4"), "{s}");
}

/// O cliente SigV4 mínimo da aba (só a biblioteca padrão do Python).
const CLIENT: &str = r#"
import sys, hmac, hashlib, datetime, urllib.request, urllib.parse, urllib.error, configparser, os

def h(b): return hashlib.sha256(b).hexdigest()
def hm(k, m): return hmac.new(k, m.encode(), hashlib.sha256).digest()
def enc(s): return urllib.parse.quote(s, safe='-_.~')

def creds(profile):
    c = configparser.ConfigParser(); c.read(os.environ['AWS_SHARED_CREDENTIALS_FILE'])
    return c[profile]['aws_access_key_id'], c[profile]['aws_secret_access_key'], None

def skey(secret, date, region, service):
    k = hm(('AWS4' + secret).encode(), date); k = hm(k, region); k = hm(k, service); return hm(k, 'aws4_request')

def sign(method, host, path, query, headers, payload, cid, secret, token, service):
    amz = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ'); date = amz[:8]; region = 'us-east-1'
    headers = dict(headers); headers['host'] = host; headers['x-amz-date'] = amz
    if token: headers['x-amz-security-token'] = token
    signed = sorted(headers)
    cq = '&'.join(sorted(enc(k) + '=' + enc(v) for k, v in query))
    cpath = path if service == 's3' else '/'.join(enc(p) for p in path.split('/'))
    ch = ''.join(k + ':' + headers[k].strip() + '\n' for k in signed)
    creq = '\n'.join([method, cpath, cq, ch, ';'.join(signed), payload])
    scope = date + '/' + region + '/' + service + '/aws4_request'
    sts = '\n'.join(['AWS4-HMAC-SHA256', amz, scope, h(creq.encode())])
    k = skey(secret, date, region, service)
    sig = hmac.new(k, sts.encode(), hashlib.sha256).hexdigest()
    headers['authorization'] = 'AWS4-HMAC-SHA256 Credential=' + cid + '/' + scope + ', SignedHeaders=' + ';'.join(signed) + ', Signature=' + sig
    return headers, k, amz, scope, sig

def send(url, method, headers, data=None):
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req) as r: return str(r.status) + ' ' + r.read().decode()
    except urllib.error.HTTPError as e:
        return str(e.code) + ' ' + e.read().decode()

cmd, port = sys.argv[1], sys.argv[2]
host = 'host.sandmux.internal:' + port
base = 'https://' + host
if cmd in ('s3', 'use'):
    cid, secret, token = creds(sys.argv[3]) if cmd == 's3' else sys.argv[3:6]
    q = [('list-type', '2'), ('prefix', 'a b')]
    hd, *_ = sign('GET', host, '/bucket', q, {'x-amz-content-sha256': h(b'')}, h(b''), cid, secret, token, 's3')
    print('R=' + send(base + '/bucket?list-type=2&prefix=a%20b', 'GET', hd))
elif cmd == 'sts':
    cid, secret, _ = creds(sys.argv[3])
    body = b'Action=GetSessionToken&Version=2011-06-15'
    hd, *_ = sign('POST', host, '/', [], {'content-type': 'application/x-www-form-urlencoded'}, h(body), cid, secret, None, 'sts')
    print('R=' + send(base + '/', 'POST', hd, body))
elif cmd == 'presign':
    cid, secret, _ = creds(sys.argv[3])
    amz = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ'); date = amz[:8]
    scope = date + '/us-east-1/s3/aws4_request'
    q = [('X-Amz-Algorithm', 'AWS4-HMAC-SHA256'), ('X-Amz-Credential', cid + '/' + scope), ('X-Amz-Date', amz), ('X-Amz-Expires', '300'), ('X-Amz-SignedHeaders', 'host')]
    cq = '&'.join(sorted(enc(k) + '=' + enc(v) for k, v in q))
    creq = '\n'.join(['GET', '/bucket/obj.txt', cq, 'host:' + host + '\n', 'host', 'UNSIGNED-PAYLOAD'])
    sts = '\n'.join(['AWS4-HMAC-SHA256', amz, scope, h(creq.encode())])
    sig = hmac.new(skey(secret, date, 'us-east-1', 's3'), sts.encode(), hashlib.sha256).hexdigest()
    print('R=' + send(base + '/bucket/obj.txt?' + cq + '&X-Amz-Signature=' + sig, 'GET', {'host': host}))
elif cmd == 'chunked':
    cid, secret, _ = creds(sys.argv[3])
    data = [b'a' * 70000, b'b' * 100]
    hd, k, amz, scope, sig = sign('PUT', host, '/bucket/obj', [], {
        'content-encoding': 'aws-chunked', 'x-amz-content-sha256': 'STREAMING-AWS4-HMAC-SHA256-PAYLOAD',
        'x-amz-decoded-content-length': str(sum(map(len, data)))}, 'STREAMING-AWS4-HMAC-SHA256-PAYLOAD', cid, secret, None, 's3')
    body, prev = b'', sig
    for d in data + [b'']:
        s = '\n'.join(['AWS4-HMAC-SHA256-PAYLOAD', amz, scope, prev, h(b''), h(d)])
        prev = hmac.new(k, s.encode(), hashlib.sha256).hexdigest()
        body += ('%x;chunk-signature=%s\r\n' % (len(d), prev)).encode() + d + b'\r\n'
    print('R=' + send(base + '/bucket/obj', 'PUT', hd, body))
"#;

/// O servidor da AWS de teste: confere a assinatura (e a dos pedaços) com a
/// chave real e responde o `GetSessionToken` com uma credencial real.
fn aws_server() -> (u16, String) {
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
                let mut raw = Vec::new();
                let mut buf = vec![0u8; 65536];
                let head_end = loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i;
                    }
                };
                let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
                let mut lines = head.split("\r\n");
                let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
                let (method, target) = (first[0].to_string(), first[1].to_string());
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let get = |n: &str| headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
                let len: usize = get("content-length")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let mut body = raw[head_end + 4..].to_vec();
                while body.len() < len {
                    let n = s.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..n]);
                }
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let answer = verify(&method, path, query, &headers, &body);
                let token = get("x-amz-security-token").unwrap_or_else(|| "-".into());
                let text = if answer.2
                    && String::from_utf8_lossy(&body).contains("Action=GetSessionToken")
                {
                    "<GetSessionTokenResponse><GetSessionTokenResult><Credentials><SessionToken>TOKEN-REAL-CAPT</SessionToken><SecretAccessKey>SEGREDO-REAL-CAPT</SecretAccessKey><Expiration>2099-01-01T00:00:00Z</Expiration><AccessKeyId>ASIAREALCAPT</AccessKeyId></Credentials></GetSessionTokenResult></GetSessionTokenResponse>".to_string()
                } else {
                    format!(
                        "id={} assinatura={} token={token} pedacos={}",
                        answer.0,
                        if answer.2 { "ok" } else { "ruim" },
                        answer.1
                    )
                };
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
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

/// O ID que assinou, o resultado dos pedaços e se a assinatura confere com
/// a chave real dele.
fn verify(
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> (String, String, bool) {
    let get = |n: &str| headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    let secret_of = |id: &str| SECRETS.iter().find(|(i, _)| *i == id).map(|(_, s)| *s);
    if let Some(a) = get("authorization").and_then(|v| sigv4::Auth::parse(&v)) {
        let Some(secret) = secret_of(&a.access_key) else {
            return (a.access_key, "-".into(), false);
        };
        let payload = get("x-amz-content-sha256").unwrap_or_else(|| sigv4::sha256_hex(body));
        let r = sigv4::Request {
            method,
            path,
            query,
            headers,
            payload_hash: &payload,
        };
        let s3 = a.scope.service == "s3";
        let c = sigv4::canonical_request(&r, &a.signed, s3, None);
        let key = a.scope.key(secret);
        let amz = get("x-amz-date").unwrap_or_default();
        let ok = sigv4::sign(&key, &amz, &a.scope, &c) == a.signature;
        let mut chunks = "-".to_string();
        if payload == sigv4::STREAMING {
            let mut signer = sigv4::ChunkSigner::new(key, &amz, a.scope.clone(), &a.signature);
            let mut rest = body;
            let mut good = true;
            loop {
                let Some(i) = rest.windows(2).position(|w| w == b"\r\n") else {
                    good = false;
                    break;
                };
                let line = String::from_utf8_lossy(&rest[..i]).into_owned();
                let (size, sig) = line.split_once(";chunk-signature=").unwrap_or(("x", ""));
                let n = usize::from_str_radix(size, 16).unwrap_or(usize::MAX);
                if n == usize::MAX || rest.len() < i + 2 + n {
                    good = false;
                    break;
                }
                let data = &rest[i + 2..i + 2 + n];
                good &= signer.chunk(data) == sig;
                rest = &rest[i + 2 + n + 2..];
                if n == 0 {
                    break;
                }
            }
            chunks = if good { "ok".into() } else { "ruim".into() };
        }
        return (a.access_key, chunks, ok);
    }
    // URL pré-assinada.
    let pairs = sigv4::query_pairs(query);
    let val = |n: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| String::from_utf8_lossy(&sigv4::percent_decode(v)).into_owned())
    };
    let Some((id, scope)) = val("X-Amz-Credential").and_then(|c| sigv4::parse_credential(&c))
    else {
        return ("-".into(), "-".into(), false);
    };
    let Some(secret) = secret_of(&id) else {
        return (id, "-".into(), false);
    };
    let signed: Vec<String> = val("X-Amz-SignedHeaders")
        .unwrap_or_default()
        .split(';')
        .map(str::to_string)
        .collect();
    let r = sigv4::Request {
        method,
        path,
        query,
        headers,
        payload_hash: "UNSIGNED-PAYLOAD",
    };
    let c = sigv4::canonical_request(&r, &signed, true, Some("X-Amz-Signature"));
    let ok = sigv4::sign(
        &scope.key(secret),
        &val("X-Amz-Date").unwrap_or_default(),
        &scope,
        &c,
    ) == val("X-Amz-Signature").unwrap_or_default();
    (id, "-".into(), ok)
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

#[test]
fn assinatura_refeita_no_gateway() {
    let e = Env::new();
    let (port, ca) = aws_server();
    let aws = setup(&e, Some(&ca));
    std::fs::write(e.work.join("cli.py"), CLIENT).unwrap();
    assert!(
        add(&e, &aws, &["--profile", "ci", "--copy"])
            .status
            .success()
    );
    assert!(add(&e, &aws, &["--profile", "Dev_SSO"]).status.success());
    let run = |args: &str| format!("python3 cli.py {args}");
    let s = in_tab(
        &e,
        &[
            run(&format!("s3 {port} ci")),
            run(&format!("s3 {port} Dev_SSO")),
            run(&format!("presign {port} ci")),
            run(&format!("chunked {port} ci")),
            format!(
                "OUT=$(python3 cli.py sts {port} ci); echo \"$OUT\" | tr -d '\\n'; echo; \
                 ID=$(echo \"$OUT\" | sed -n 's/.*<AccessKeyId>\\(.*\\)<\\/AccessKeyId>.*/\\1/p'); \
                 SK=$(echo \"$OUT\" | sed -n 's/.*<SecretAccessKey>\\(.*\\)<\\/SecretAccessKey>.*/\\1/p'); \
                 TK=$(echo \"$OUT\" | sed -n 's/.*<SessionToken>\\(.*\\)<\\/SessionToken>.*/\\1/p'); \
                 echo CAPT=$ID; python3 cli.py use {port} $ID $SK $TK"
            ),
        ]
        .join("; "),
    );
    // A cópia: a chave real assina, sem token.
    assert!(
        s.contains("R=200 id=AKIAREALCI assinatura=ok token=- pedacos=-"),
        "{s}"
    );
    // A referência: a credencial temporária do host, com o token real.
    assert!(
        s.contains("R=200 id=ASIAREALHOST assinatura=ok token=TOKEN-REAL-HOST"),
        "{s}"
    );
    // A URL pré-assinada e os pedaços assinados.
    assert_eq!(
        s.matches("R=200 id=AKIAREALCI assinatura=ok").count(),
        3,
        "{s}"
    );
    assert!(s.contains("pedacos=ok"), "{s}");
    // O GetSessionToken: a aba só vê falsos, e eles valem pelo gateway.
    assert!(s.contains("CAPT=ASIASANDMUX"), "{s}");
    // (O servidor de teste ecoa o token que recebeu; a resposta do STS, não.)
    let sts = s
        .lines()
        .find(|l| l.contains("<GetSessionTokenResponse>"))
        .unwrap_or_else(|| panic!("sem a resposta do STS: {s}"));
    assert!(
        !sts.contains("REAL") && sts.contains("<AccessKeyId>ASIASANDMUX"),
        "{sts}"
    );
    assert!(!s.contains("SEGREDO-REAL"), "{s}");
    assert!(
        s.contains("R=200 id=ASIAREALCAPT assinatura=ok token=TOKEN-REAL-CAPT"),
        "{s}"
    );
    let net = net_log(&e);
    assert!(net.contains("\"secret\":\"aws:ci\""), "{net}");
    assert!(net.contains("\"secret\":\"aws:dev-sso\""), "{net}");
    assert!(net.contains("\"secret\":\"aws:ci+sts\""), "{net}");
    assert!(
        !net.contains("SEGREDO") && !net.contains("TOKEN-REAL"),
        "valor no registro"
    );
}

#[test]
fn sem_o_real_da_aws_403() {
    let e = Env::new();
    let (port, ca) = aws_server();
    let aws = setup(&e, Some(&ca));
    std::fs::write(e.work.join("cli.py"), CLIENT).unwrap();
    assert!(add(&e, &aws, &["--profile", "Dev_SSO"]).status.success());
    std::fs::write(e.dir.path().join("sso-vencido"), "").unwrap();
    let s = in_tab(
        &e,
        &format!("python3 cli.py s3 {port} Dev_SSO; python3 cli.py s3 {port} Dev_SSO"),
    );
    assert!(s.contains("R=403 <ErrorResponse>"), "{s}");
    assert!(s.contains("aws sso login --profile Dev_SSO"), "{s}");
    assert!(!s.contains("assinatura="), "{s}");
    let log = std::fs::read_to_string(e.state.join("sandmux/sandmuxd.log")).unwrap_or_default();
    assert_eq!(log.matches("aws:dev-sso:").count(), 1, "{log}");
}
