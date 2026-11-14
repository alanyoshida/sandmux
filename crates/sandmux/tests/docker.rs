//! Docker privado por pasta: visão de arquivos, socket, ciclo de vida.

mod common;
use common::*;

use std::process::{Command, Stdio};

/// `sandmux-run --view-file` dentro de um user namespace com mount e pid
/// namespaces novos, como o filho do `rootlesskit`.
#[test]
fn modo_de_visao_monta_so_a_visao() {
    if Command::new("unshare")
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--pid",
            "--fork",
            "true",
        ])
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("pulado: unshare sem user namespace");
        return;
    }
    let base = tempfile::tempdir().unwrap();
    let base = base.path().canonicalize().unwrap();
    let work = base.join("proj");
    let dados = base.join("dados");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&dados).unwrap();
    std::fs::write(work.join("a"), "projeto\n").unwrap();
    let host = sandmux_policy::Host::detect().unwrap();
    let script = format!(
        "cat {w}/a; echo x > {w}/b && echo gravou; echo y > {d}/c && echo dados; test -e {h} && echo home-visivel || echo home-fora; test -e /proc/1/status && echo proc; echo $ROOTLESSKIT_STATE_DIR",
        w = work.display(),
        d = dados.display(),
        h = host.home.join(".ssh").display()
    );
    let spec = sandmux_policy::compose_docker_view(
        &host,
        &work,
        std::slice::from_ref(&dados),
        vec!["/bin/sh".into(), "-c".into(), script],
        vec![("PATH".into(), "/usr/bin:/bin".into())],
        None,
        None,
    )
    .unwrap();
    let file = base.join("view.json");
    std::fs::write(&file, spec.to_json()).unwrap();
    let out = Command::new("unshare")
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--pid",
            "--fork",
            "--",
        ])
        .arg(RUN)
        .arg("--view-file")
        .arg(&file)
        .env("ROOTLESSKIT_STATE_DIR", "/rk")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{s}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(s, "projeto\ngravou\ndados\nhome-fora\nproc\n/rk\n");
    assert!(work.join("b").exists() && dados.join("c").exists());
}

// ---- Docker privado nas sessões ----

use common::cli::*;
use common::pty::Term;
use std::path::Path;
use std::time::{Duration, Instant};

/// Sem Docker sem root ou sem `busybox` estático, os testes pulam.
fn docker_ok() -> bool {
    let host = sandmux_policy::Host::detect().unwrap();
    if let Err(e) = sandmux::docker::support(&host.user, host.uid) {
        eprintln!("pulado: sem Docker sem root: {e}");
        return false;
    }
    let ldd = Command::new("ldd").arg("/usr/bin/busybox").output();
    let static_bb = Path::new("/usr/bin/busybox").exists()
        && ldd.is_ok_and(|o| {
            !o.status.success() || String::from_utf8_lossy(&o.stdout).contains("not a dynamic")
        });
    if !static_bb {
        eprintln!("pulado: sem busybox estático em /usr/bin");
    }
    static_bb
}

const IMAGEM: &str = "mkdir -p img && cp /usr/bin/busybox img/ && printf 'FROM scratch\\nCOPY busybox /bin/busybox\\nRUN [\"/bin/busybox\",\"--install\",\"-s\",\"/bin\"]\\nCMD [\"/bin/sh\"]\\n' > img/Dockerfile && DOCKER_BUILDKIT=0 docker build -q -t sx img >/dev/null && echo BUILD-OK";

fn term(e: &Env, cwd: &Path, args: &[&str]) -> Term {
    let mut cmd = e.cmd(cwd, args);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, 160, 40)
}

/// Roda `script` na sessão (em `cwd`) e devolve a saída, esperando até 3 min.
fn run_in(t: &mut Term, cwd: &Path, script: &str) -> String {
    let n = format!(
        "{:x}",
        Instant::now().elapsed().as_nanos() ^ std::process::id() as u128
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
    let deadline = Instant::now() + Duration::from_secs(180);
    while !done.exists() {
        assert!(
            Instant::now() < deadline,
            "o script não terminou: {}",
            t.raw_text()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    std::fs::read_to_string(&out).unwrap()
}

/// Apaga os dados do Docker privado, que têm donos da faixa subordinada.
fn limpar(e: &Env) {
    let d = e.data_dir().join("docker");
    if d.exists() {
        let _ = Command::new("rootlesskit")
            .args(["--net=host", "--", "rm", "-rf"])
            .arg(&d)
            .stderr(Stdio::null())
            .status();
    }
}

fn docker_procs(e: &Env) -> usize {
    let tag = e.runtime.display().to_string();
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|p| std::fs::read(p.path().join("cmdline")).ok())
        .filter(|c| {
            let s = String::from_utf8_lossy(c);
            (s.contains("rootlesskit") || s.contains("dockerd")) && s.contains(&tag)
        })
        .count()
}

#[test]
fn docker_e_compose_na_sessao_com_a_visao_restrita() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&e.work)
        .status()
        .unwrap();
    std::fs::create_dir_all(e.home.join(".ssh")).unwrap();
    std::fs::write(e.home.join(".ssh/id_ed25519"), "chave").unwrap();
    let mut t = term(&e, &e.work, &["run", "shell"]);
    t.write(b"echo pronto-$((1+1))\r");
    t.wait_text("pronto-2");
    // Sem nenhum `docker`, nenhum Docker sobe.
    assert_eq!(docker_procs(&e), 0);
    let ssh = e.home.join(".ssh").display().to_string();
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "echo H=$DOCKER_HOST
{IMAGEM}
echo projeto > a.txt
docker run --rm -v $PWD:/app sx sh -c 'cat /app/a.txt; echo do-conteiner > /app/b.txt'
echo SSH=$(docker run --rm -v {ssh}:/x sx ls /x 2>/dev/null | wc -l)
ln -sfn {ssh} link
echo LINK=$(docker run --rm -v $PWD/link:/x sx ls /x 2>/dev/null | wc -l)
docker run --rm -v $PWD:/app sx sh -c 'echo x > /app/.git/hooks/pre-commit' 2>/dev/null && echo HOOK-GRAVOU || echo HOOK-NEGADO
printf 'services:\\n  web:\\n    image: sx\\n    command: [\"sleep\", \"300\"]\\n' > compose.yaml
docker compose up -d >/dev/null 2>&1 && docker compose ps --format '{{{{.Service}}}} {{{{.State}}}}'
docker compose down >/dev/null 2>&1; echo COMPOSE-FIM"
        ),
    );
    assert!(s.contains("H=unix:///run/sandmux/docker.sock"), "{s}");
    assert!(s.contains("BUILD-OK"), "{s}");
    assert!(s.contains("projeto\n"), "{s}");
    assert_eq!(
        std::fs::read_to_string(e.work.join("b.txt")).unwrap(),
        "do-conteiner\n"
    );
    assert!(s.contains("SSH=0") && s.contains("LINK=0"), "{s}");
    assert!(s.contains("HOOK-NEGADO"), "{s}");
    assert!(!e.work.join(".git/hooks/pre-commit").exists());
    assert!(s.contains("web running"), "{s}");
    assert!(docker_procs(&e) > 0, "o Docker privado não subiu");

    // A imagem persiste para a próxima sessão da pasta, e não aparece noutra.
    t.write(b"exit\r");
    t.wait_exit();
    let deadline = Instant::now() + Duration::from_secs(30);
    while docker_procs(&e) > 0 {
        assert!(
            Instant::now() < deadline,
            "o Docker não parou com a última sessão"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(&mut t, &e.work, "docker images --format '{{.Repository}}'");
    assert!(s.lines().any(|l| l == "sx"), "{s}");
    let outra = e.home.join("projetos/outra");
    std::fs::create_dir_all(&outra).unwrap();
    let mut t2 = term(&e, &outra, &["run", "shell"]);
    let s = run_in(
        &mut t2,
        &outra,
        "docker images --format '{{.Repository}}'; echo IMG-FIM",
    );
    assert!(!s.lines().any(|l| l == "sx"), "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    t2.write(b"exit\r");
    t2.wait_exit();
    limpar(&e);
}

#[test]
fn porta_so_no_localhost_teto_e_stats() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    e.write_config(&format!(
        "[project.\"{}\"]\nmemory = \"512M\"\n",
        e.work.display()
    ));
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let port = 20000 + (std::process::id() % 20000);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
docker run -d --name web -p {port}:80 sx httpd -f -p 80 >/dev/null
docker run -d --name dorme sx sleep 300 >/dev/null
sleep 1; wget -q -O /dev/null http://host.sandmux.internal:{port}/ ; echo WGET=$?"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    // O `httpd` do busybox responde 404 sem página: o `wget` sai com 8.
    assert!(s.contains("WGET=0") || s.contains("WGET=8"), "{s}");
    let ss = Command::new("ss").args(["-ltnH"]).output().unwrap();
    let l: Vec<String> = String::from_utf8_lossy(&ss.stdout)
        .lines()
        .filter(|l| l.contains(&format!(":{port} ")))
        .map(str::to_string)
        .collect();
    assert!(
        !l.is_empty() && l.iter().all(|x| x.contains("127.0.0.1:")),
        "{l:?}"
    );
    let out = e.run(&e.work, &["stats", "1"]);
    let st = stdout(&out);
    assert!(
        st.contains("Docker privado da pasta: rodando, 2 contêineres")
            && st.contains("teto de 512M"),
        "{st}"
    );
    assert!(st.contains(&format!("80 → 127.0.0.1:{port} (web)")), "{st}");

    // 700M num teto de 512M: o OOM da slice mata o `dd` (137). A alocação é
    // limitada por `ulimit -v`, para nunca passar de ~1 GB se o teto falhar, e
    // sem core. Um segundo OOM pode levar outro contêiner da pasta (os
    // contêineres têm `oom_score_adj` 900), nunca o `dockerd`.
    let s = run_in(
        &mut t,
        &e.work,
        "docker run --rm sx sh -c 'ulimit -c 0; ulimit -v 1000000; dd if=/dev/zero of=/dev/null bs=700M count=1 2>/dev/null'; echo ALOCOU=$?
docker ps >/dev/null; echo PS=$?",
    );
    assert!(s.contains("ALOCOU=137"), "o contêiner passou do teto: {s}");
    assert!(s.contains("PS=0"), "o Docker parou de responder: {s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

#[test]
fn sem_suporte_o_docker_mostra_o_erro() {
    let e = Env::new();
    // PATH do daemon sem rootlesskit: só o systemd-run, para os tetos.
    let bin = e.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink("/usr/bin/systemd-run", bin.join("systemd-run")).unwrap();
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash").env("PATH", &bin);
    let mut t = Term::spawn(cmd, 160, 40);
    let s = run_in(&mut t, &e.work, "/usr/bin/docker ps 2>&1; echo FIM");
    assert!(
        s.contains("Docker privado não está disponível") && s.contains("sandmux doctor"),
        "{s}"
    );
    t.write(b"exit\r");
    t.wait_exit();
}

#[test]
fn run_sem_terminal_nao_tem_docker() {
    let e = Env::new();
    let out = e.shell(
        &[],
        "echo ${DOCKER_HOST:-sem}; test -e /run/sandmux/docker.sock && echo sock || echo sem-sock",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "sem\nsem-sock\n");
}

#[test]
fn daemon_stop_para_o_docker() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(&mut t, &e.work, "docker info --format '{{.ServerVersion}}'");
    assert!(!s.trim().is_empty(), "{s}");
    assert!(docker_procs(&e) > 0);
    t.write(b"\x02d");
    t.wait_exit();
    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let deadline = Instant::now() + Duration::from_secs(30);
    while docker_procs(&e) > 0 {
        assert!(Instant::now() < deadline, "sobrou processo do Docker");
        std::thread::sleep(Duration::from_millis(200));
    }
    limpar(&e);
}

#[test]
fn doctor_mostra_o_docker_privado() {
    let e = Env::new();
    let s = stdout(&e.run(&e.work, &["doctor"]));
    let host = sandmux_policy::Host::detect().unwrap();
    if sandmux::docker::support(&host.user, host.uid).is_ok() {
        assert!(s.contains("[ok  ] Docker privado por pasta"), "{s}");
        assert!(s.contains("rede pelo gateway do sandmux"), "{s}");
    }
    if sandmux::docker::registry_binary().is_none() {
        assert!(
            s.contains("[FALHA] cache de imagens do Docker entre pastas")
                && s.contains("pacote docker-registry"),
            "{s}"
        );
    } else {
        assert!(
            s.contains("[ok  ] cache de imagens do Docker entre pastas"),
            "{s}"
        );
    }
    let bin = e.dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let out = e
        .cmd(&e.work, &["doctor"])
        .env("PATH", &bin)
        .output()
        .unwrap();
    let s2 = stdout(&out);
    assert!(
        s2.contains("Docker privado por pasta") && s2.contains("faltam no PATH: rootlesskit"),
        "{s2}"
    );
}

fn net_log(e: &Env) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for f in std::fs::read_dir(e.state.join("sandmux/net"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if f.path().extension().is_some_and(|x| x == "jsonl") {
            let t = std::fs::read_to_string(f.path()).unwrap_or_default();
            out.extend(t.lines().filter_map(|l| serde_json::from_str(l).ok()));
        }
    }
    out
}

#[test]
fn conteiner_pelo_gateway_com_a_politica_mais_restrita() {
    if !docker_ok() {
        return;
    }
    use std::io::{Read, Write};
    let e = Env::new();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            let Ok(mut c) = c else { return };
            let mut b = [0u8; 256];
            let _ = c.read(&mut b);
            let _ = c.write_all(b"HTTP/1.0 200 OK\r\n\r\nola-do-host");
        }
    });
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}\ndocker run --rm sx wget -q -O - http://host.sandmux.internal:{port}/; echo; echo RC=$?"
        ),
    );
    assert!(s.contains("ola-do-host") && s.contains("RC=0"), "{s}");
    // A linha da conexão é gravada quando ela termina no gateway.
    let tcp = |log: &[serde_json::Value]| {
        log.iter().any(|l| {
            l["tag"].as_str().is_some_and(|t| t.starts_with("docker-"))
                && l["domain"] == "host.sandmux.internal"
                && l["proto"] == "tcp"
        })
    };
    let end = Instant::now() + Duration::from_secs(10);
    while !tcp(&net_log(&e)) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(tcp(&net_log(&e)), "{:?}", net_log(&e));
    // Outra aba da pasta no modo só inferência: o Docker segue a mais restrita.
    let mut t2 = term(&e, &e.work, &["run", "shell", "--network", "inferencia"]);
    t2.write(b"echo pronto-$((1+1))\r");
    t2.wait_text("pronto-2");
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "docker run --rm sx wget -q -T 5 -O - http://host.sandmux.internal:{port}/; echo RC=$?"
        ),
    );
    assert!(!s.contains("ola-do-host"), "{s}");
    let log = net_log(&e);
    assert!(
        log.iter().any(
            |l| l["tag"].as_str().is_some_and(|t| t.starts_with("docker-"))
                && l["domain"] == "host.sandmux.internal"
                && l["blocked"] == "fora do modo só inferência"
        ),
        "{log:?}"
    );
    t2.write(b"exit\r");
    t2.wait_exit();
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}
