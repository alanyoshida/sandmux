//! Rede do sandbox pelo gateway: namespace próprio, TUN, DNS, bloqueios e
//! registro. Sem internet: os destinos são servidores no `127.0.0.1` do host,
//! alcançados por `host.sandmux.internal`, ou endereços que nunca saem do
//! gateway.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};

use common::cli::{Env, stderr, stdout};

/// Linhas do registro de rede deste ambiente.
fn log_lines(env: &Env) -> Vec<serde_json::Value> {
    let dir = env.state.join("sandmux/net");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "jsonl") {
            let text = std::fs::read_to_string(e.path()).unwrap();
            out.extend(text.lines().map(|l| serde_json::from_str(l).unwrap()));
        }
    }
    out
}

fn shell(env: &Env, extra: &[&str], script: &str) -> String {
    let o = env.shell(extra, script);
    assert!(
        o.status.success(),
        "código {:?}\n{}\n{}",
        o.status.code(),
        stdout(&o),
        stderr(&o)
    );
    stdout(&o)
}

#[test]
fn interfaces_sao_so_lo_e_sandmux0() {
    let env = Env::new();
    let out = shell(
        &env,
        &[],
        "tail -n +3 /proc/net/dev | cut -d: -f1 | tr -d ' '; cat /etc/resolv.conf; echo \"host=$SANDMUX_HOST\"",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(&lines[..2], &["lo", "sandmux0"], "{out}");
    assert!(out.contains("nameserver 10.0.2.3"), "{out}");
    assert!(out.contains("host=host.sandmux.internal"), "{out}");
}

#[test]
fn servidor_do_host_por_host_sandmux_internal() {
    let env = Env::new();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    let srv = std::thread::spawn(move || {
        let (mut c, _) = tcp.accept().unwrap();
        let mut buf = [0u8; 64];
        let n = c.read(&mut buf).unwrap();
        c.write_all(&buf[..n]).unwrap();
        c.write_all(b" eco").unwrap();
    });
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let uport = udp.local_addr().unwrap().port();
    let usrv = std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        let (n, from) = udp.recv_from(&mut buf).unwrap();
        udp.send_to(&buf[..n], from).unwrap();
    });
    let script = format!(
        r#"python3 -c '
import socket
s = socket.create_connection(("host.sandmux.internal", {port}), timeout=5)
s.sendall(b"ola")
data = b""
while True:
    c = s.recv(64)
    if not c:
        break
    data += c
print(data.decode())
s.close()
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); u.settimeout(5)
u.sendto(b"udp", ("10.0.2.254", {uport}))
print(u.recv(64).decode())
'"#
    );
    let out = shell(&env, &[], &script);
    assert_eq!(out, "ola eco\nudp\n");
    srv.join().unwrap();
    usrv.join().unwrap();

    let lines = log_lines(&env);
    let t = lines
        .iter()
        .find(|l| l["proto"] == "tcp" && l["port"] == port)
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!(t["domain"], "host.sandmux.internal");
    assert_eq!(t["ip"], "10.0.2.254");
    assert_eq!(t["up"], 3);
    assert_eq!(t["down"], 7);
    assert!(t["tag"].as_str().unwrap().starts_with("run-"), "{t}");
    assert_eq!(t["workdir"], env.work.to_str().unwrap());
    let u = lines
        .iter()
        .find(|l| l["proto"] == "udp" && l["port"] == uport)
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert_eq!((u["up"].as_u64(), u["down"].as_u64()), (Some(3), Some(3)));
    // O nome foi resolvido pelo DNS do gateway e marcado como novo.
    assert!(
        lines.iter().any(|l| l["proto"] == "dns"
            && l["domain"] == "host.sandmux.internal"
            && l["new"] == true),
        "{lines:?}"
    );
}

#[test]
fn dns_responde_o_nome_especial_e_localhost() {
    let env = Env::new();
    // Consulta A mandada direto ao 10.0.2.3, sem o resolvedor da libc.
    let script = r#"python3 -c '
import socket, struct
def q(name):
    m = struct.pack(">HHHHHH", 0x4242, 0x0100, 1, 0, 0, 0)
    for l in name.split("."):
        m += bytes([len(l)]) + l.encode()
    m += b"\x00" + struct.pack(">HH", 1, 1)
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(5)
    s.sendto(m, ("10.0.2.3", 53))
    r = s.recv(512)
    an = struct.unpack(">H", r[6:8])[0]
    print(name, hex(struct.unpack(">H", r[0:2])[0]), an, ".".join(str(b) for b in r[-4:]) if an else "-")
q("host.sandmux.internal")
q("localhost")
'"#;
    let out = shell(&env, &[], script);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "host.sandmux.internal 0x4242 1 10.0.2.254");
    // localhost vai ao resolvedor do host, que responde (com ou sem registro).
    assert!(lines[1].starts_with("localhost 0x4242 "), "{out}");
    let log = log_lines(&env);
    assert!(
        log.iter()
            .any(|l| l["proto"] == "dns" && l["domain"] == "localhost"),
        "{log:?}"
    );
}

#[test]
fn metadados_e_quic_bloqueados_e_registrados() {
    let env = Env::new();
    let script = r#"python3 -c '
import socket, time
t = time.time()
try:
    s = socket.create_connection(("169.254.169.254", 80), timeout=5)
    s.sendall(b"GET / HTTP/1.0\r\n\r\n")
    print("meta", repr(s.recv(64)))
except Exception as e:
    print("meta", type(e).__name__)
print("rapido", time.time() - t < 3)
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); u.settimeout(1)
u.sendto(b"quic", ("192.0.2.1", 443))
try:
    print("quic", u.recv(64))
except Exception as e:
    print("quic", type(e).__name__)
'"#;
    let out = shell(&env, &[], script);
    assert!(
        out.contains("meta b''") || out.contains("meta ConnectionResetError"),
        "{out}"
    );
    assert!(out.contains("rapido True"), "{out}");
    assert!(out.contains("quic TimeoutError"), "{out}");
    let log = log_lines(&env);
    let meta = log
        .iter()
        .find(|l| l["ip"] == "169.254.169.254")
        .unwrap_or_else(|| panic!("{log:?}"));
    assert_eq!(meta["blocked"], "metadados de cloud");
    let quic = log
        .iter()
        .find(|l| l["ip"] == "192.0.2.1")
        .unwrap_or_else(|| panic!("{log:?}"));
    assert_eq!(
        (quic["proto"].as_str(), quic["blocked"].as_str()),
        (Some("udp"), Some("QUIC"))
    );
}

#[test]
fn sem_rede_falha_na_hora() {
    let env = Env::new();
    let script = r#"tail -n +3 /proc/net/dev | cut -d: -f1 | tr -d ' '; python3 -c '
import socket, time
t = time.time()
try:
    socket.create_connection(("192.0.2.1", 80), timeout=5)
    print("conectou")
except OSError as e:
    print("erro", e.errno, time.time() - t < 1)
'"#;
    let out = shell(&env, &["--network", "sem-rede"], script);
    assert_eq!(out, "lo\nerro 101 True\n");
    assert!(log_lines(&env).is_empty());
}

/// Roda `script` na sessão e espera ele terminar (até 1 min).
fn run_in(t: &mut common::pty::Term, cwd: &std::path::Path, script: &str) -> String {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = format!(
        "{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    let (sh, out, done) = (
        cwd.join(format!(".t-{n}.sh")),
        cwd.join(format!(".t-{n}.out")),
        cwd.join(format!(".t-{n}.feito")),
    );
    std::fs::write(&sh, script).unwrap();
    t.write(
        format!(
            "sh {} > {} 2>&1; touch {}\r",
            sh.display(),
            out.display(),
            done.display()
        )
        .as_bytes(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !done.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "o script não terminou: {}",
            t.raw_text()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::fs::read_to_string(&out).unwrap()
}

#[test]
fn sessao_acessa_servidor_local_e_sandmux_net_mostra() {
    let env = Env::new();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    let srv = std::thread::spawn(move || {
        let (mut c, _) = tcp.accept().unwrap();
        let mut buf = [0u8; 64];
        let _ = c.read(&mut buf);
        c.write_all(b"HTTP/1.0 200 OK\r\n\r\nok").unwrap();
    });
    let mut cmd = env.cmd(&env.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = common::pty::Term::spawn(cmd, 160, 40);
    let out = run_in(
        &mut t,
        &env.work,
        &format!("curl -s http://host.sandmux.internal:{port}/; echo; echo FIM"),
    );
    assert!(out.contains("ok\nFIM"), "{out}");
    srv.join().unwrap();

    // A linha leva a etiqueta da sessão 1, primeiro lançamento. Ela é gravada
    // quando a conexão termina no gateway, logo depois do curl.
    let found = |lines: &[serde_json::Value]| {
        lines
            .iter()
            .any(|l| l["tag"] == "s1-1" && l["port"] == port && l["proto"] == "tcp")
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !found(&log_lines(&env)) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let lines = log_lines(&env);
    assert!(found(&lines), "{lines:?}");
    let o = env.run(&env.work, &["net", "1"]);
    let s = stdout(&o);
    assert!(o.status.success(), "{s}{}", stderr(&o));
    let row = s
        .lines()
        .find(|l| l.contains(&format!("10.0.2.254:{port}")))
        .unwrap_or_else(|| panic!("{s}"));
    assert!(
        row.contains("host.sandmux.internal") && row.contains(" 1 "),
        "{row}"
    );
    assert!(s.starts_with("HORA"), "{s}");
    // Sessão que não existe.
    let o = env.run(&env.work, &["net", "9"]);
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("não existe a sessão 9"),
        "{}",
        stderr(&o)
    );
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn daemon_apaga_registro_velho_ao_subir() {
    let env = Env::new();
    let dir = env.state.join("sandmux/net");
    std::fs::create_dir_all(&dir).unwrap();
    let day = |n: u32| {
        let o = std::process::Command::new("date")
            .args(["-d", &format!("{n} days ago"), "+%F"])
            .output()
            .unwrap();
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    };
    let old = dir.join(format!("{}.jsonl", day(40)));
    let recent = dir.join(format!("{}.jsonl", day(2)));
    std::fs::write(&old, "{}\n").unwrap();
    std::fs::write(&recent, "{}\n").unwrap();
    let mut d = env
        .daemon_cmd()
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    env.wait_socket();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while old.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(!old.exists(), "o arquivo de 40 dias ficou");
    assert!(recent.exists());
    let _ = env.run(&env.work, &["daemon", "stop"]);
    let _ = d.wait();
}

// ---- política de rede (change 9) ----

/// Servidor HTTP mínimo no 127.0.0.1 do host: responde "ok" a cada conexão,
/// por até 20 s. Devolve a porta.
fn http_server() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::time::Instant::now() < end {
            match l.accept() {
                Ok((mut c, _)) => {
                    c.set_nonblocking(false).unwrap();
                    let mut buf = [0u8; 512];
                    let _ = c.read(&mut buf);
                    let _ = c.write_all(b"HTTP/1.0 200 OK\r\n\r\nok");
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
    });
    port
}

/// Script python com `get(host, porta, cabecalho_host)`, que imprime o corpo
/// ou "BLOQ", e `dns(nome)`, que imprime o código de resposta do 10.0.2.3.
const PY: &str = r#"
import socket, struct, ssl
def get(addr, port, host):
    try:
        s = socket.create_connection((addr, port), timeout=5)
        s.sendall(("GET / HTTP/1.0\r\nHost: %s\r\n\r\n" % host).encode())
        data = b""
        while True:
            c = s.recv(256)
            if not c:
                break
            data += c
        print(host, "BLOQ" if not data else data.split(b"\r\n\r\n")[-1].decode())
    except OSError as e:
        print(host, "ERRO", type(e).__name__)
def dns(name):
    m = struct.pack(">HHHHHH", 7, 0x0100, 1, 0, 0, 0)
    for l in name.split("."):
        m += bytes([len(l)]) + l.encode()
    m += b"\x00" + struct.pack(">HH", 1, 1)
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(5)
    s.sendto(m, ("10.0.2.3", 53))
    print(name, "rcode", s.recv(512)[3] & 15)
def tls(addr, sni):
    ctx = ssl.create_default_context()
    try:
        with socket.create_connection((addr, 443), timeout=5) as s:
            ctx.wrap_socket(s, server_hostname=sni)
        print("tls", "passou")
    except Exception as e:
        print("tls", "falhou")
"#;

fn py(env: &Env, extra: &[&str], body: &str) -> String {
    let script = format!("python3 -c '{}\n{}'", PY.replace('\'', "\\'"), body);
    shell(env, extra, &script)
}

fn blocked_line<'a>(log: &'a [serde_json::Value], domain: &str) -> &'a serde_json::Value {
    log.iter()
        .find(|l| l["domain"] == domain && !l["blocked"].is_null())
        .unwrap_or_else(|| panic!("sem bloqueio de {domain}: {log:?}"))
}

#[test]
fn modo_aberta_nega_host_e_catalogo() {
    let env = Env::new();
    let port = http_server();
    let body = format!(
        "get(\"host.sandmux.internal\", {port}, \"proibido.test\")\nget(\"host.sandmux.internal\", {port}, \"pastebin.com\")\nget(\"host.sandmux.internal\", {port}, \"livre.test\")\ndns(\"pastebin.com\")"
    );
    let out = py(&env, &["--deny", "proibido.test"], &body);
    assert_eq!(
        out,
        "proibido.test BLOQ\npastebin.com BLOQ\nlivre.test ok\npastebin.com rcode 3\n"
    );
    let log = log_lines(&env);
    assert_eq!(
        blocked_line(&log, "proibido.test")["blocked"],
        "denylist: proibido.test"
    );
    let p = log
        .iter()
        .filter(|l| l["domain"] == "pastebin.com")
        .map(|l| l["blocked"].as_str().unwrap_or("").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        p,
        vec!["catálogo de exfiltração: pastebin.com"; 2],
        "{log:?}"
    );

    // Desligado no global: passa.
    env.write_config("[global]\ncatalog-off = [\"pastebin.com\"]\n");
    let out = py(
        &env,
        &[],
        &format!(
            "get(\"host.sandmux.internal\", {port}, \"pastebin.com\")\nget(\"host.sandmux.internal\", {port}, \"paste.ee\")"
        ),
    );
    assert_eq!(out, "pastebin.com ok\npaste.ee BLOQ\n");
}

#[test]
fn modo_custom_libera_so_a_allowlist() {
    let env = Env::new();
    let port = http_server();
    let body = format!(
        "get(\"host.sandmux.internal\", {port}, \"host.sandmux.internal\")\nget(\"192.0.2.1\", 80, \"x\")\ndns(\"localhost\")\ntls(\"192.0.2.1\", \"host.sandmux.internal\")"
    );
    let out = py(
        &env,
        &["--network", "custom", "--allow", "host.sandmux.internal"],
        &body,
    );
    assert_eq!(
        out,
        "host.sandmux.internal ok\nx BLOQ\nlocalhost rcode 3\ntls falhou\n"
    );
    let log = log_lines(&env);
    assert_eq!(
        blocked_line(&log, "localhost")["blocked"],
        "fora da allowlist"
    );
    let x = log
        .iter()
        .find(|l| l["ip"] == "192.0.2.1" && l["port"] == 80)
        .unwrap();
    assert_eq!(x["blocked"], "sem domínio");
    // SNI forjado num IP que não veio de consulta: "sem domínio".
    let t = log
        .iter()
        .find(|l| l["ip"] == "192.0.2.1" && l["port"] == 443)
        .unwrap_or_else(|| panic!("{log:?}"));
    assert_eq!(t["domain"], "host.sandmux.internal");
    assert_eq!(t["blocked"], "sem domínio");

    // Faixa CIDR: o IP da máquina passa sem o nome liberado.
    let out = py(
        &env,
        &["--network", "custom", "--allow", "10.0.2.0/24"],
        &format!("get(\"10.0.2.254\", {port}, \"y\")"),
    );
    assert_eq!(out, "y BLOQ\n", "o Host y não está liberado");
    let out = py(
        &env,
        &[
            "--network",
            "custom",
            "--allow",
            "10.0.2.0/24",
            "--allow",
            "y.test",
        ],
        &format!("get(\"10.0.2.254\", {port}, \"y.test\")"),
    );
    assert_eq!(out, "y.test ok\n");
}

#[test]
fn modo_inferencia_nega_o_resto_no_dns() {
    let env = Env::new();
    let out = py(
        &env,
        &["--network", "inferencia"],
        "dns(\"host.sandmux.internal\")",
    );
    assert_eq!(out, "host.sandmux.internal rcode 3\n");
    let log = log_lines(&env);
    assert_eq!(
        blocked_line(&log, "host.sandmux.internal")["blocked"],
        "fora do modo só inferência"
    );
}

#[test]
fn padrao_invalido_recusa_o_run() {
    let env = Env::new();
    let o = env.shell(&["--deny", "http://x.test/a"], "true");
    assert!(!o.status.success());
    assert!(
        stderr(&o).contains("padrão de destino inválido"),
        "{}",
        stderr(&o)
    );
    env.write_config("[global]\ncatalog = [\"nada\"]\n");
    let o = env.shell(&[], "true");
    assert!(!o.status.success());
    assert!(stderr(&o).contains("grupo desconhecido"), "{}", stderr(&o));
}

fn session(env: &Env, extra: &[&str]) -> common::pty::Term {
    let mut args = vec!["run", "shell"];
    args.extend_from_slice(extra);
    let mut cmd = env.cmd(&env.work, &args);
    cmd.env("SHELL", "/bin/bash");
    common::pty::Term::spawn(cmd, 160, 40)
}

fn curl(t: &mut common::pty::Term, env: &Env, port: u16) -> String {
    run_in(
        t,
        &env.work,
        &format!("curl -s -m 5 http://host.sandmux.internal:{port}/; echo \" rc=$?\""),
    )
}

#[test]
fn regra_global_vale_na_sessao_viva_em_ate_2s() {
    let env = Env::new();
    let port = http_server();
    let mut t = session(&env, &[]);
    assert_eq!(curl(&mut t, &env, port), "ok rc=0\n");
    let o = env.run(
        &env.work,
        &["config", "add", "deny", "host.sandmux.internal", "--global"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    std::thread::sleep(std::time::Duration::from_millis(2000));
    // O arquivo da política do lançamento já tem a regra.
    let f = env.runtime.join("sandmux/net/s1-1.json");
    assert!(
        std::fs::read_to_string(&f)
            .unwrap()
            .contains("host.sandmux.internal")
    );
    // Nome negado: NXDOMAIN, e o curl não resolve (código 6).
    assert_eq!(curl(&mut t, &env, port), " rc=6\n");
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn regras_da_aba_liberam_e_sobrevivem_ao_reinicio() {
    let env = Env::new();
    env.write_config("[global]\ndeny = [\"a.test\"]\n");
    let port = http_server();
    let mut t = session(&env, &["--network", "custom", "--deny", "b.test"]);
    assert_eq!(curl(&mut t, &env, port), " rc=6\n");
    let o = env.run(&env.work, &["net", "allow", "host.sandmux.internal", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(curl(&mut t, &env, port), "ok rc=0\n");
    // Padrão inválido e sessão inexistente.
    assert!(
        !env.run(&env.work, &["net", "deny", "x/y", "1"])
            .status
            .success()
    );
    let o = env.run(&env.work, &["net", "deny", "c.test", "9"]);
    assert!(
        stderr(&o).contains("não existe a sessão 9"),
        "{}",
        stderr(&o)
    );
    // Sair de custom para sem-rede só no próximo lançamento.
    let o = env.run(&env.work, &["net", "mode", "sem-rede", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("próximo lançamento"), "{}", stderr(&o));
    assert_eq!(curl(&mut t, &env, port), "ok rc=0\n");
    let o = env.run(&env.work, &["net", "mode", "custom", "1"]);
    assert!(o.status.success(), "{}", stderr(&o));

    let rules = stdout(&env.run(&env.work, &["net", "rules", "1"]));
    let line = |p: &str| {
        rules
            .lines()
            .find(|l| l.split_whitespace().nth(1) == Some(p))
            .unwrap_or_else(|| panic!("{p}:\n{rules}"))
            .to_string()
    };
    assert!(rules.contains("Modo: custom"), "{rules}");
    assert!(line("a.test").contains("global") && line("a.test").contains("ligado"));
    assert!(line("b.test").ends_with("aba"), "{rules}");
    assert!(line("host.sandmux.internal").ends_with("aba"), "{rules}");
    assert!(line("pastebin.com").contains("catálogo de exfiltração"));
    assert!(line("pypi.org").starts_with("  desligado"), "{rules}");

    t.write(b"\x02d");
    t.wait_exit();
    let o = env.run(&env.work, &["daemon", "restart"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let rules = stdout(&env.run(&env.work, &["net", "rules", "1"]));
    assert!(
        rules
            .lines()
            .any(|l| l.contains("host.sandmux.internal") && l.ends_with("aba")),
        "{rules}"
    );
    let _ = env.run(&env.work, &["kill", "1"]);
}

#[test]
fn config_show_e_add_das_regras_de_rede() {
    let env = Env::new();
    for args in [
        &["config", "add", "deny", "Proibido.TEST", "--global"][..],
        &["config", "add", "catalog", "npm", "--global"],
        &["config", "add", "catalog-off", "pastebin.com", "--global"],
    ] {
        let o = env.run(&env.work, args);
        assert!(o.status.success(), "{args:?}: {}", stderr(&o));
    }
    let o = env.run(&env.work, &["config", "add", "catalog", "nada", "--global"]);
    assert!(!o.status.success() && stderr(&o).contains("grupo desconhecido"));
    let o = env.run(&env.work, &["config", "add", "deny", "a b", "--global"]);
    assert!(!o.status.success() && stderr(&o).contains("padrão de destino inválido"));
    let text = std::fs::read_to_string(env.config_file()).unwrap();
    assert!(text.contains("deny = [\"proibido.test\"]"), "{text}");
    let show = stdout(&env.run(&env.work, &["config", "show"]));
    assert!(
        show.lines()
            .any(|l| l.starts_with("deny") && l.contains("proibido.test") && l.contains("global")),
        "{show}"
    );
    assert!(
        show.contains("exfiltração 50/51, inferência 8/8, comuns: npm"),
        "{show}"
    );
}

// ---- portas publicadas (change 10) ----

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// GET simples no 127.0.0.1 do host; `None` se a conexão falha.
fn host_get(port: u16) -> Option<String> {
    let mut s = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_secs(2),
    )
    .ok()?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .ok()?;
    s.write_all(b"GET / HTTP/1.0\r\n\r\n").ok()?;
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    (!out.is_empty()).then_some(out)
}

fn wait_until(secs: u64, f: impl Fn() -> bool) -> bool {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

#[test]
fn porta_do_sandbox_publicada_no_host() {
    let env = Env::new();
    let p = free_port();
    let other = free_port();
    let script = format!(
        "python3 -m http.server {p} --bind 0.0.0.0 >/dev/null 2>&1 & S=$!; python3 -m http.server {other} --bind 10.0.2.100 >/dev/null 2>&1 & sleep 4; kill $S; sleep 3"
    );
    let mut child = env
        .cmd(&env.work, &["run", "shell", "--", "-c", &script])
        .spawn()
        .unwrap();
    assert!(
        wait_until(4, || host_get(p).is_some_and(|r| r.contains("200 OK"))),
        "a porta {p} não foi publicada"
    );
    // Só no 10.0.2.100 do sandbox: não publicada.
    assert!(host_get(other).is_none());
    // A lista do lançamento.
    let file = env
        .runtime
        .join(format!("sandmux/net/run-{}.ports.json", child.id()));
    let list = std::fs::read_to_string(&file).unwrap();
    assert!(
        list.contains(&format!("\"guest\":{p},\"host\":{p}")),
        "{list}"
    );
    // Servidor encerrado: a porta do host fecha.
    assert!(
        wait_until(4, || host_get(p).is_none()),
        "a porta {p} continuou aberta"
    );
    assert!(child.wait().unwrap().success());
    assert!(!file.exists(), "a lista ficou depois do fim");
}

#[test]
fn porta_ocupada_no_host_vai_para_a_proxima() {
    let env = Env::new();
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let p = busy.local_addr().unwrap().port();
    let script = format!("python3 -m http.server {p} --bind 127.0.0.1 >/dev/null 2>&1 & sleep 4");
    let mut child = env
        .cmd(&env.work, &["run", "shell", "--", "-c", &script])
        .spawn()
        .unwrap();
    let file = env
        .runtime
        .join(format!("sandmux/net/run-{}.ports.json", child.id()));
    assert!(wait_until(4, || std::fs::read_to_string(&file)
        .is_ok_and(|t| t.contains(&format!("\"guest\":{p}")))));
    let list = std::fs::read_to_string(&file).unwrap();
    let host: u16 = list
        .split("\"host\":")
        .nth(1)
        .unwrap()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(host > p, "{list}");
    assert!(host_get(host).is_some_and(|r| r.contains("200 OK")));
    drop(busy);
    let _ = child.wait();
}

// ---- allowlist de portas (change 19) ----

/// Um endereço da máquina fora do loopback (o da rota padrão), se houver.
fn lan_ip() -> Option<std::net::Ipv4Addr> {
    let u = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Sem pacote: o `connect` de UDP só escolhe a rota.
    u.connect("192.0.2.1:9").ok()?;
    match u.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

fn get_at(ip: std::net::Ipv4Addr, port: u16) -> Option<String> {
    let mut s = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from((ip, port)),
        std::time::Duration::from_secs(2),
    )
    .ok()?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .ok()?;
    s.write_all(b"GET / HTTP/1.0\r\n\r\n").ok()?;
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    (!out.is_empty()).then_some(out)
}

#[test]
fn allowlist_de_portas_endereco_e_registro() {
    let env = Env::new();
    let (wide, local, out) = (free_port(), free_port(), free_port());
    let script = format!(
        "for p in {wide} {local} {out}; do python3 -m http.server $p --bind 0.0.0.0 >/dev/null 2>&1 & done; sleep 8"
    );
    let wide_rule = format!("{wide}@0.0.0.0");
    let local_rule = local.to_string();
    let mut child = env
        .cmd(
            &env.work,
            &[
                "run",
                "--port",
                &wide_rule,
                "--port",
                &local_rule,
                "shell",
                "--",
                "-c",
                &script,
            ],
        )
        .spawn()
        .unwrap();
    assert!(
        wait_until(5, || host_get(wide).is_some_and(|r| r.contains("200 OK"))
            && host_get(local).is_some_and(|r| r.contains("200 OK"))),
        "as portas da allowlist não foram publicadas"
    );
    // Fora da allowlist: não publicada.
    assert!(host_get(out).is_none(), "a porta {out} foi publicada");
    if let Some(ip) = lan_ip() {
        // Com `@0.0.0.0`, aceita por outro endereço da máquina; sem endereço, não.
        assert!(get_at(ip, wide).is_some_and(|r| r.contains("200 OK")));
        assert!(get_at(ip, local).is_none(), "a {local} aceitou por {ip}");
    }
    let file = env
        .runtime
        .join(format!("sandmux/net/run-{}.ports.json", child.id()));
    let list = std::fs::read_to_string(&file).unwrap();
    assert!(
        list.contains(&format!(
            "\"guest\":{wide},\"host\":{wide},\"addr\":\"0.0.0.0\""
        )),
        "{list}"
    );
    assert!(child.wait().unwrap().success());
    // Cada conexão aceita vira uma linha `inbound`.
    let inbound: Vec<_> = log_lines(&env)
        .into_iter()
        .filter(|l| l["proto"] == "inbound")
        .collect();
    assert!(
        inbound
            .iter()
            .any(|l| l["port"] == wide && l["ip"] == "127.0.0.1" && l["blocked"].is_null()),
        "{inbound:?}"
    );
    assert!(inbound.iter().any(|l| l["port"] == local), "{inbound:?}");
}

// ---- decifração do TLS (change 12) ----

/// Servidor HTTPS no 127.0.0.1 do host com um certificado de
/// `host.sandmux.internal` assinado por uma CA de teste. Devolve a porta e o
/// PEM da CA.
fn https_server() -> (u16, String) {
    let (ca_cert, ca_key) = sandmux_net::tls::generate_ca().unwrap();
    let ca = sandmux_net::tls::Ca::from_pem(&ca_cert, &ca_key).unwrap();
    let cfg = ca.server_config("host.sandmux.internal").unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
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
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
                let _ = s.flush();
                s.conn.send_close_notify();
                let _ = s.flush();
            });
        }
    });
    (port, ca_cert)
}

fn run_with_ca(
    env: &Env,
    ca: Option<&std::path::Path>,
    extra: &[&str],
    script: &str,
) -> (bool, String) {
    let mut args = vec!["run", "shell"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--", "-c", script]);
    let mut c = env.cmd(&env.work, &args);
    if let Some(p) = ca {
        c.env("SANDMUX_UPSTREAM_CA", p);
    }
    let o = c.output().unwrap();
    (o.status.success(), stdout(&o) + &stderr(&o))
}

#[test]
fn decifra_so_a_lista_e_registra_metodo_e_caminho() {
    let env = Env::new();
    let (port, ca_pem) = https_server();
    let ca_file = env.dir.path().join("ca-teste.pem");
    std::fs::write(&ca_file, &ca_pem).unwrap();
    let url = format!("https://host.sandmux.internal:{port}/v1/models?key=abc");

    // Na lista: o curl do sandbox confia no gateway, e o pedido é registrado.
    let (ok, out) = run_with_ca(
        &env,
        Some(&ca_file),
        &["--decrypt", "host.sandmux.internal"],
        &format!("curl -sS '{url}'; echo; echo SSL=$SSL_CERT_FILE"),
    );
    assert!(ok && out.contains("ok\n"), "{out}");
    assert!(
        out.contains("SSL=/etc/ssl/certs/ca-certificates.crt"),
        "{out}"
    );
    let log = log_lines(&env);
    let h = log
        .iter()
        .find(|l| l["proto"] == "https")
        .unwrap_or_else(|| panic!("{log:?}"));
    assert_eq!(
        (h["method"].as_str(), h["path"].as_str()),
        (Some("GET"), Some("/v1/models"))
    );
    assert_eq!(h["status"], 200);
    assert_eq!(h["domain"], "host.sandmux.internal");
    let raw = std::fs::read_dir(env.state.join("sandmux/net"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect::<String>();
    assert!(!raw.contains("key=abc"), "a query foi gravada");
    // O `sandmux net` mostra o método, o caminho e o status.
    let net = stdout(&env.run(&env.work, &["net"]));
    assert!(
        net.lines()
            .any(|l| l.contains("https") && l.contains("GET /v1/models 200")),
        "{net}"
    );

    // Fora da lista: o certificado é o do servidor (o curl só confia com a
    // CA de teste).
    std::fs::copy(&ca_file, env.work.join("ca.crt")).unwrap();
    let (ok, out) = run_with_ca(
        &env,
        Some(&ca_file),
        &[],
        &format!("curl -sS --cacert ca.crt '{url}'; echo; curl -s '{url}'; echo RC=$?"),
    );
    assert!(ok && out.contains("ok\n"), "{out}");
    assert!(
        out.contains("RC=60"),
        "sem a CA de teste o curl deveria recusar: {out}"
    );

    // Destino com certificado inválido (o gateway sem a CA de teste): bloqueado.
    let (_, out) = run_with_ca(
        &env,
        None,
        &["--decrypt", "host.sandmux.internal"],
        &format!("curl -s '{url}'; echo RC=$?"),
    );
    assert!(!out.contains("ok\n"), "{out}");
    let log = log_lines(&env);
    assert!(
        log.iter().any(|l| l["blocked"]
            .as_str()
            .is_some_and(|b| b.starts_with("certificado inválido"))),
        "{log:?}"
    );
}

/// Tenta `get` por até 4 segundos (o encaminhamento abre em até 1 s).
const RETRY: &str = r#"
import time
def getr(addr, port, host):
    import io, contextlib
    for _ in range(40):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            get(addr, port, host)
        if "ERRO" not in out.getvalue():
            break
        time.sleep(0.1)
    print(out.getvalue().strip())
"#;

#[test]
fn porta_do_host_no_localhost_do_sandbox() {
    let env = Env::new();
    let port = http_server();
    let p = port.to_string();
    // Sem a chave, o 127.0.0.1 do sandbox é só dele.
    let out = py(&env, &[], &format!("get(\"127.0.0.1\", {port}, \"local\")"));
    assert!(out.contains("local ERRO ConnectionRefusedError"), "{out}");
    // Com `--host-port`, a porta do host aparece no 127.0.0.1 de dentro.
    let out = py(
        &env,
        &["--host-port", &p],
        &format!("{RETRY}\ngetr(\"127.0.0.1\", {port}, \"local\")"),
    );
    assert!(out.contains("local ok"), "{out}");
    let log = log_lines(&env);
    assert!(
        log.iter().any(|l| l["proto"] == "tcp"
            && l["domain"] == "host.sandmux.internal"
            && l["ip"] == "127.0.0.1"
            && l["port"] == port
            && l["blocked"].is_null()),
        "{log:?}"
    );
    // A denylist vence.
    let out = py(
        &env,
        &["--host-port", &p, "--deny", "host.sandmux.internal"],
        &format!("import time\ntime.sleep(1.5)\nget(\"127.0.0.1\", {port}, \"negado\")"),
    );
    // Recusada: fechada sem resposta ou reiniciada.
    assert!(
        out.contains("negado BLOQ") || out.contains("negado ERRO ConnectionResetError"),
        "{out}"
    );
    let log = log_lines(&env);
    assert!(
        log.iter().any(|l| l["domain"] == "host.sandmux.internal"
            && l["port"] == port
            && l["blocked"]
                .as_str()
                .is_some_and(|b| b.contains("denylist"))),
        "{log:?}"
    );
}

#[test]
fn porta_do_host_acrescentada_ao_vivo_sem_eco() {
    let env = Env::new();
    let port = http_server();
    let p = port.to_string();
    let mut cmd = env.cmd(&env.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    let mut t = common::pty::Term::spawn_answering(cmd, 120, 30);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    let o = env.run(&env.work, &["config", "add", "host-ports", &p, "--global"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let script =
        format!("{PY}\n{RETRY}\ngetr(\"127.0.0.1\", {port}, \"vivo\")").replace('\'', "\\'");
    t.write(format!("python3 -c '{script}'\r").as_bytes());
    t.wait_text("vivo ok");
    // O ouvinte da porta do host no sandbox não é publicado de volta.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let st = stdout(&env.run(&env.work, &["stats", "1"]));
    assert!(
        !st.contains(&format!("{port} →")),
        "publicada de volta: {st}"
    );
    // Tirada da configuração, a porta fecha no sandbox.
    let o = env.run(
        &env.work,
        &["config", "remove", "host-ports", &p, "--global"],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    std::thread::sleep(std::time::Duration::from_secs(3));
    t.write(
        format!(
            "python3 -c '{}'\r",
            format!("{PY}\nget(\"127.0.0.1\", {port}, \"fechada\")").replace('\'', "\\'")
        )
        .as_bytes(),
    );
    t.wait_text("fechada ERRO");
    t.write(b"\x02d");
    t.wait_exit();
}
