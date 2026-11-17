//! Docker privado por pasta: visão de arquivos, socket, ciclo de vida.

mod common;
use common::*;

use std::process::{Command, Stdio};

/// O `unshare` com user namespace funciona nesta máquina.
fn unshare_ok() -> bool {
    let ok = Command::new("unshare")
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--pid",
            "--fork",
            "true",
        ])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("pulado: unshare sem user namespace");
    }
    ok
}

/// Roda a especificação da visão com `sandmux-run --view-file`, dentro de um
/// user namespace com mount e pid namespaces novos, como o filho do
/// `rootlesskit`.
fn run_view(base: &Path, spec: &sandmux_spec::LaunchSpec) -> std::process::Output {
    let file = base.join("view.json");
    std::fs::write(&file, spec.to_json()).unwrap();
    Command::new("unshare")
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
        .unwrap()
}

/// O filtro seccomp da visão: as chamadas que o sandbox nega devolvem erro
/// (o chaveiro, `ENOSYS`), e as de montagem continuam funcionando.
#[test]
fn modo_de_visao_nega_as_chamadas_do_sandbox() {
    if !unshare_ok() {
        return;
    }
    let base = tempfile::tempdir().unwrap();
    let base = base.path().canonicalize().unwrap();
    let work = base.join("proj");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::copy(PROBE, work.join("probe")).unwrap();
    let host = sandmux_policy::Host::detect().unwrap();
    let script = format!(
        "P={w}/probe; for c in bpf perf_event_open userfaultfd io_uring_setup keyctl add_key; do $P syscall $c; done; $P ioctl TIOCSTI; mkdir -p /tmp/m && mount -t tmpfs none /tmp/m && echo montou",
        w = work.display()
    );
    let spec = sandmux_policy::compose_docker_view(
        &host,
        &work,
        &[],
        vec!["/bin/sh".into(), "-c".into(), script],
        vec![("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())],
        None,
        None,
        &sandmux_policy::DockerView::default(),
    )
    .unwrap();
    let out = run_view(&base, &spec);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{s}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Uma linha por tentativa, na ordem do script.
    let got: Vec<Option<i64>> = s
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| errno(&serde_json::from_str(l).unwrap()))
        .collect();
    let (eperm, enosys) = (Some(libc::EPERM as i64), Some(libc::ENOSYS as i64));
    let want = [
        ("bpf", eperm),
        ("perf_event_open", eperm),
        ("userfaultfd", eperm),
        ("io_uring_setup", eperm),
        ("keyctl", enosys),
        ("add_key", enosys),
        ("TIOCSTI", eperm),
    ];
    assert_eq!(got.len(), want.len(), "{s}");
    for ((c, w), g) in want.iter().zip(&got) {
        assert_eq!(g, w, "{c}: {s}");
    }
    assert!(s.contains("montou"), "{s}");
}

/// As coberturas da visão: o `.env` chega vazio e em somente leitura, a
/// pasta negada chega vazia, o arquivo do host não muda, e uma cobertura cujo
/// destino sumiu é pulada.
#[test]
fn modo_de_visao_cobre_segredos() {
    if !unshare_ok() {
        return;
    }
    let base = tempfile::tempdir().unwrap();
    let base = base.path().canonicalize().unwrap();
    let work = base.join("proj");
    std::fs::create_dir_all(work.join("privado")).unwrap();
    std::fs::write(work.join(".env"), "SEGREDO=1\n").unwrap();
    std::fs::write(work.join(".env.example"), "SEGREDO=\n").unwrap();
    std::fs::write(work.join("privado/a.txt"), "x").unwrap();
    let host = sandmux_policy::Host::detect().unwrap();
    let script = format!(
        "cd {w}; echo env=$(wc -c < .env); echo exemplo=$(wc -c < .env.example); echo y > .env 2>/dev/null && echo gravou || echo nao-gravou; echo privado=$(ls privado | wc -l); touch privado/b 2>/dev/null && echo criou || echo nao-criou",
        w = work.display()
    );
    let spec = sandmux_policy::compose_docker_view(
        &host,
        &work,
        &[],
        vec!["/bin/sh".into(), "-c".into(), script],
        vec![("PATH".into(), "/usr/bin:/bin".into())],
        None,
        None,
        &sandmux_policy::DockerView {
            covers: vec![work.join(".env"), work.join("privado"), work.join("sumiu")],
            ..Default::default()
        },
    )
    .unwrap();
    let out = run_view(&base, &spec);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{s}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        s,
        "env=0\nexemplo=9\nnao-gravou\nprivado=0\nnao-criou\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(work.join(".env")).unwrap(),
        "SEGREDO=1\n"
    );
    assert!(work.join("privado/a.txt").exists() && !work.join("privado/b").exists());
}

#[test]
fn modo_de_visao_monta_so_a_visao() {
    if !unshare_ok() {
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
        "cat {w}/a; echo x > {w}/b && echo gravou; echo y > {d}/c && echo dados; test -e {h} && echo home-visivel || echo home-fora; test -e /proc/1/status && echo proc; echo $ROOTLESSKIT_STATE_DIR; echo pts=$(ls /dev/pts); test -e /dev/fuse -o -e /dev/net/tun -o -e /dev/snd && echo dispositivos-do-host || echo dev-minimo",
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
        &sandmux_policy::DockerView::default(),
    )
    .unwrap();
    let out = run_view(&base, &spec);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{s}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Um devpts novo só tem o `ptmx`: nenhum terminal do host.
    assert_eq!(
        s,
        "projeto\ngravou\ndados\nhome-fora\nproc\n/rk\npts=ptmx\ndev-minimo\n"
    );
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
    // Um nome por chamada: um contador do processo. (O nome vinha de
    // `Instant::now().elapsed()`, que quase sempre dá poucas dezenas de
    // nanossegundos; duas chamadas com o mesmo nome achavam o `.feito` da
    // anterior e devolviam a saída antiga.)
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = format!(
        "{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let (sh, out, done) = (
        cwd.join(format!(".t-{n}.sh")),
        cwd.join(format!(".t-{n}.out")),
        cwd.join(format!(".t-{n}.feito")),
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&done);
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

/// Imagem `sp` com o busybox e a sonda, setuid de root. A sonda é dinâmica:
/// os contêineres montam as bibliotecas do sistema da visão (`LIBS`).
fn imagem_com_sonda(e: &Env) {
    let d = e.work.join("sp");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy("/usr/bin/busybox", d.join("busybox")).unwrap();
    std::fs::copy(PROBE, d.join("probe")).unwrap();
    // O `COPY` do construtor antigo perde o bit setuid: o `chmod` vai no
    // `RUN`.
    std::fs::write(
        d.join("Dockerfile"),
        "FROM scratch\nCOPY busybox /bin/busybox\nRUN [\"/bin/busybox\",\"--install\",\"-s\",\"/bin\"]\nCOPY probe /probe\nRUN chmod 4755 /probe\n",
    )
    .unwrap();
}

const LIBS: &str = "-v /usr/lib:/lib:ro -v /usr/lib64:/lib64:ro";

/// O filtro seccomp da visão vale num contêiner privilegiado e sem o seccomp
/// do Docker, e o setuid continua funcionando nos contêineres.
#[test]
fn conteiner_privilegiado_herda_o_filtro() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    imagem_com_sonda(&e);
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "DOCKER_BUILDKIT=0 docker build -q -t sp sp >/dev/null && echo BUILD-OK
docker run --rm --privileged --security-opt seccomp=unconfined {LIBS} sp sh -c 'for c in bpf perf_event_open userfaultfd io_uring_setup keyctl add_key; do /probe syscall $c; done; /probe ioctl TIOCSTI'
echo SETUID
docker run --rm --user 1000:1000 {LIBS} sp /probe identity"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    let (antes, depois) = s.split_once("SETUID").unwrap();
    let got: Vec<Option<i64>> = antes
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| errno(&serde_json::from_str(l).unwrap()))
        .collect();
    let (eperm, enosys) = (Some(libc::EPERM as i64), Some(libc::ENOSYS as i64));
    assert_eq!(
        got,
        [eperm, eperm, eperm, eperm, enosys, enosys, eperm],
        "{s}"
    );
    let id = depois
        .lines()
        .find(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .unwrap_or_else(|| panic!("{s}"));
    let id: serde_json::Value = serde_json::from_str(id["detail"].as_str().unwrap()).unwrap();
    assert_eq!(id["uid"], 1000, "{s}");
    assert_eq!(id["euid"], 0, "o setuid não funcionou no contêiner: {s}");
    assert_eq!(id["no_new_privs"], "0", "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Os contêineres só alcançam o `/dev` mínimo da visão, e o Docker continua
/// com o overlay do kernel e com terminal nos contêineres.
#[test]
fn conteiner_ve_so_o_dev_minimo() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
echo PTS=$(docker run --rm -v /dev:/d sx ls /d/pts)
echo DEV=$(docker run --rm -v /dev:/d sx ls /d)
echo TTY=$(docker run --rm -t sx tty)
echo DRIVER=$(docker info --format '{{{{.Driver}}}}')"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    assert!(s.contains("PTS=ptmx\n"), "{s}");
    let dev = s.lines().find_map(|l| l.strip_prefix("DEV=")).unwrap_or("");
    for d in dev.split_whitespace() {
        assert!(
            [
                "null", "zero", "full", "random", "urandom", "tty", "pts", "ptmx", "shm", "fd",
                "stdin", "stdout", "stderr", "dri", "kvm"
            ]
            .contains(&d)
                || d.starts_with("nvidia"),
            "dispositivo do host no contêiner: {d}\n{s}"
        );
    }
    assert!(s.contains("TTY=/dev/pts/"), "{s}");
    assert!(s.contains("DRIVER=overlay"), "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Os segredos do usuário na pasta chegam vazios aos contêineres; o exemplo e
/// o segredo que o agente criou, não.
#[test]
fn conteiner_nao_le_os_segredos_da_pasta() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    std::fs::write(e.work.join(".env"), "SEGREDO=do-usuario\n").unwrap();
    std::fs::write(e.work.join(".env.example"), "SEGREDO=\n").unwrap();
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "echo do-agente > chave.pem
{IMAGEM}
echo ENV=$(docker run --rm -v $PWD:/w sx cat /w/.env)
echo EXEMPLO=$(docker run --rm -v $PWD:/w sx cat /w/.env.example)
echo AGENTE=$(docker run --rm -v $PWD:/w sx cat /w/chave.pem)"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    assert!(s.contains("ENV=\n"), "{s}");
    assert!(s.contains("EXEMPLO=SEGREDO=\n"), "{s}");
    assert!(s.contains("AGENTE=do-agente\n"), "{s}");
    assert_eq!(
        std::fs::read_to_string(e.work.join(".env")).unwrap(),
        "SEGREDO=do-usuario\n"
    );
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Com mais coberturas que o teto, o Docker privado não sobe, e o `docker`
/// mostra a contagem e como reduzir.
#[test]
fn segredos_demais_o_docker_nao_sobe() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    let d = e.work.join("certs");
    std::fs::create_dir_all(&d).unwrap();
    for i in 0..=sandmux_policy::MAX_DOCKER_COVERS {
        std::fs::write(d.join(format!("{i}.pem")), "").unwrap();
    }
    let mut t = term(&e, &e.work, &["run", "shell"]);
    let s = run_in(&mut t, &e.work, "docker ps; echo PS=$?");
    assert!(
        s.contains("4097") && s.contains("4096") && s.contains("secret-files-off"),
        "{s}"
    );
    assert!(!s.contains("PS=0"), "{s}");
    assert_eq!(docker_procs(&e), 0);
    t.write(b"exit\r");
    t.wait_exit();
}

/// Sem os tetos (aqui, sem o `systemd-run` no `PATH` do daemon), o Docker
/// continua funcionando, e os contêineres só enxergam o cgroup em que ele
/// roda, nunca a árvore do usuário.
#[test]
fn sem_tetos_o_docker_so_ve_o_proprio_cgroup() {
    if !docker_ok() {
        return;
    }
    let e = Env::new();
    // Um PATH com tudo de /usr/bin menos o systemd-run.
    let bin = e.home.join("bin-sem-systemd-run");
    std::fs::create_dir_all(&bin).unwrap();
    for f in std::fs::read_dir("/usr/bin").unwrap().flatten() {
        if f.file_name() != "systemd-run" {
            let _ = std::os::unix::fs::symlink(f.path(), bin.join(f.file_name()));
        }
    }
    let mut cmd = e.cmd(&e.work, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash")
        .env("PATH", format!("{}:/usr/local/bin", bin.display()));
    let mut t = Term::spawn(cmd, 160, 40);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
echo CG=$(docker run --rm -v /sys/fs/cgroup:/c sx ls /c)
docker run --rm sx echo RODOU"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    let cg = s.lines().find_map(|l| l.strip_prefix("CG=")).unwrap_or("");
    for n in ["user.slice", "init.scope", "system.slice"] {
        assert!(!cg.split_whitespace().any(|x| x == n), "{n} visível: {s}");
    }
    assert!(s.contains("RODOU"), "{s}");
    let st = stdout(&e.run(&e.work, &["stats", "1"]));
    assert!(st.contains("Docker privado da pasta: rodando"), "{st}");
    t.write(b"exit\r");
    t.wait_exit();
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

    // O contêiner só vê os cgroups do Docker da pasta: nem o teto (na slice,
    // acima do escopo) nem os cgroups do usuário.
    let s = run_in(
        &mut t,
        &e.work,
        "docker run --rm -v /sys/fs/cgroup:/c sx sh -c 'echo CG=$(ls /c); echo TETO=$(grep -rlx 536870912 /c 2>/dev/null)'",
    );
    let cg = s.lines().find_map(|l| l.strip_prefix("CG=")).unwrap_or("");
    for n in ["user.slice", "init.scope", "app.slice", "session.slice"] {
        assert!(!cg.split_whitespace().any(|x| x == n), "{n} visível: {s}");
    }
    assert!(cg.split_whitespace().any(|x| x == "motor"), "{s}");
    assert!(s.contains("TETO=\n"), "o teto da pasta está visível: {s}");

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
