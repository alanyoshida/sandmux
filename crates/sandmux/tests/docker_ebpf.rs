//! Confinamento do Docker privado por BPF LSM, com o auxiliar root real.
//!
//! Só roda pelo `scripts/check-root.sh`, que sobe o `sandmux-helper` como
//! root e passa o socket em `SANDMUX_HELPER_SOCKET`. Sem a variável, os
//! testes pulam.

mod common;
use common::cli::*;
use common::pty::Term;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn helper() -> Option<PathBuf> {
    let p = std::env::var_os("SANDMUX_HELPER_SOCKET").map(PathBuf::from)?;
    if !p.exists() {
        eprintln!("pulado: {} não existe", p.display());
        return None;
    }
    let host = sandmux_policy::Host::detect().unwrap();
    if let Err(e) = sandmux::docker::support(&host.user, host.uid) {
        eprintln!("pulado: sem Docker sem root: {e}");
        return None;
    }
    Some(p)
}

const IMAGEM: &str = "mkdir -p img && cp /usr/bin/busybox img/ && printf 'FROM scratch\\nCOPY busybox /bin/busybox\\nRUN [\"/bin/busybox\",\"--install\",\"-s\",\"/bin\"]\\nCMD [\"/bin/sh\"]\\n' > img/Dockerfile && DOCKER_BUILDKIT=0 docker build -q -t sx img >/dev/null && echo BUILD-OK";

fn term(e: &Env, cwd: &Path) -> Term {
    let mut cmd = e.cmd(cwd, &["run", "shell"]);
    cmd.env("SHELL", "/bin/bash");
    Term::spawn(cmd, 160, 40)
}

fn run_in(t: &mut Term, cwd: &Path, script: &str) -> String {
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

fn stats(e: &Env) -> String {
    stdout(&e.run(&e.work, &["stats", "1"]))
}

/// Segredos, `file-deny` e links: o kernel decide pelo inode, mesmo depois
/// de um contêiner privilegiado desmontar as coberturas.
#[test]
fn privilegiado_nao_le_segredos_nem_pasta_negada() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    let privado = e.work.join("privado");
    std::fs::create_dir_all(&privado).unwrap();
    std::fs::write(privado.join("velho.txt"), "velho\n").unwrap();
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\n",
        e.work.display(),
        privado.display()
    ));
    std::fs::write(e.work.join(".env"), "SEGREDO=do-usuario\n").unwrap();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "echo do-agente > chave.pem
{IMAGEM}
docker ps >/dev/null && echo DOCKER-OK"
        ),
    );
    assert!(s.contains("BUILD-OK") && s.contains("DOCKER-OK"), "{s}");
    let st = stats(&e);
    assert!(st.contains("eBPF: ligado"), "{st}");
    let s = run_in(
        &mut t,
        &e.work,
        "P='docker run --rm --privileged -v '$PWD':/w sx sh -c'
$P 'umount /w/.env; cat /w/.env && echo ENV-LIDO || echo ENV-NEGADO'
$P 'ln /w/.env /w/l 2>/dev/null; cat /w/l 2>/dev/null && echo LINK-LIDO || echo LINK-NEGADO'
$P 'umount /w/privado; echo x > /w/privado/novo.txt && echo NOVO-CRIADO || echo NOVO-NEGADO'
$P 'umount /w/privado; cat /w/privado/velho.txt && echo VELHO-LIDO || echo VELHO-NEGADO'
$P 'cat /w/chave.pem'
echo FIM",
    );
    assert!(s.contains("ENV-NEGADO") && !s.contains("ENV-LIDO"), "{s}");
    assert!(!s.contains("SEGREDO=do-usuario"), "{s}");
    assert!(s.contains("LINK-NEGADO"), "{s}");
    assert!(
        s.contains("NOVO-NEGADO") && s.contains("VELHO-NEGADO"),
        "{s}"
    );
    assert!(!privado.join("novo.txt").exists());
    assert!(s.contains("do-agente"), "{s}");
    assert_eq!(
        std::fs::read_to_string(e.work.join(".env")).unwrap(),
        "SEGREDO=do-usuario\n"
    );
    // O agente, fora do Docker, continua lendo o que é dele.
    let s = run_in(&mut t, &e.work, "cat chave.pem");
    assert!(s.contains("do-agente"), "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O `.git`: hooks, config e commondir protegidos contra um contêiner
/// privilegiado; leitura e o uso normal do Docker continuam.
#[test]
fn privilegiado_nao_grava_no_git() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&e.work)
        .status()
        .unwrap();
    let cfg = std::fs::read_to_string(e.work.join(".git/config")).unwrap();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
P='docker run --rm --privileged -v '$PWD':/w sx sh -c'
$P 'umount /w/.git/hooks; echo x > /w/.git/hooks/pre-commit && echo HOOK-GRAVOU || echo HOOK-NEGADO'
$P 'umount /w/.git/config; echo x >> /w/.git/config && echo CFG-GRAVOU || echo CFG-NEGADO'
$P 'echo /tmp > /w/.git/commondir && echo CD-GRAVOU || echo CD-NEGADO'
$P 'mv /w/.git/hooks /w/.git/h2 && echo MV-GRAVOU || echo MV-NEGADO'
$P 'mv /w/.git /w/g2 && echo MVGIT-GRAVOU || echo MVGIT-NEGADO'
$P 'cat /w/.git/HEAD'
$P 'echo ok > /w/.git/description && echo DESC-GRAVOU'
printf 'services:\\n  web:\\n    image: sx\\n    command: [\"sleep\", \"300\"]\\n' > compose.yaml
docker compose up -d >/dev/null 2>&1 && docker compose ps --format '{{{{.Service}}}} {{{{.State}}}}'
docker compose down >/dev/null 2>&1; echo FIM"
        ),
    );
    assert!(s.contains("BUILD-OK"), "{s}");
    for n in ["HOOK", "CFG", "CD", "MV", "MVGIT"] {
        assert!(
            s.contains(&format!("{n}-NEGADO")) && !s.contains(&format!("{n}-GRAVOU")),
            "{n}: {s}"
        );
    }
    assert!(s.contains("ref: refs/heads/"), "{s}");
    assert!(s.contains("DESC-GRAVOU"), "{s}");
    assert!(s.contains("web running"), "{s}");
    assert!(!e.work.join(".git/hooks/pre-commit").exists());
    assert!(!e.work.join(".git/commondir").exists());
    assert_eq!(
        std::fs::read_to_string(e.work.join(".git/config")).unwrap(),
        cfg
    );
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O auxiliar recusa o cgroup de outro dono, e as regras somem com a
/// conexão.
#[test]
fn auxiliar_recusa_cgroup_alheio_e_solta_no_fim() {
    let Some(sock) = helper() else { return };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    let s = c.status().unwrap();
    assert!(s.lsm, "{s:?}");
    let e = c.confine(Path::new("/sys/fs/cgroup"), 0).unwrap_err();
    assert!(e.contains("pertence ao uid 0"), "{e}");
    // Um cgroup do próprio usuário: o do teste.
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let rel = cg.trim().strip_prefix("0::").unwrap();
    let dir = Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
    let c2 = sandmux::helper::Client::connect(&sock).unwrap();
    match c2.confine(&dir, 0) {
        Ok(attach) => {
            let progs = || {
                Command::new("bpftool")
                    .args(["cgroup", "show"])
                    .arg(&dir)
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_default()
            };
            if attach == "cgroup" {
                // O bpftool sem root não lista: só confere quando consegue.
                let p = progs();
                if !p.is_empty() {
                    assert!(p.contains("sm_file_open"), "{p}");
                }
            }
            drop(c2);
            std::thread::sleep(Duration::from_millis(300));
            assert!(!progs().contains("sm_file_open"));
        }
        // O cgroup do processo de teste pode ser do root (uma sessão de
        // login): a recusa é a mesma regra.
        Err(e) => assert!(e.contains("pertence ao uid"), "{e}"),
    }
}

/// Parar o auxiliar com o Docker rodando para o Docker da pasta.
#[test]
fn queda_do_auxiliar_para_o_docker() {
    let Some(sock) = helper() else { return };
    let e = Env::new();
    // Um auxiliar só deste teste, como o usuário, que derrubamos no meio. Sem
    // root ele não carrega programas: o confinamento falha e o Docker sobe
    // sem ele. Por isso o teste usa o auxiliar root do script e pede ao
    // script, pelo arquivo de controle, que o reinicie.
    let ctl = PathBuf::from(format!("{}.restart", sock.display()));
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!("{IMAGEM}\ndocker ps >/dev/null && echo UP"),
    );
    assert!(s.contains("UP"), "{s}");
    assert!(stats(&e).contains("eBPF: ligado"));
    std::fs::write(&ctl, "").unwrap();
    // O script reinicia o auxiliar; o daemon vê a conexão cair e para o
    // Docker.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let st = stats(&e);
        if !st.contains("Docker privado da pasta: rodando") {
            break;
        }
        assert!(Instant::now() < deadline, "o Docker não parou: {st}");
        std::thread::sleep(Duration::from_millis(300));
    }
    let s = run_in(&mut t, &e.work, "docker ps >/dev/null && echo UP2");
    assert!(s.contains("UP2"), "{s}");
    assert!(stats(&e).contains("eBPF: ligado"));
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Custo do confinamento: a mesma leitura em massa (os arquivos de
/// `/usr/share`) e o mesmo `docker build`, com e sem o auxiliar. Só mede;
/// roda com `sudo scripts/check-root.sh --ignored custo`.
#[test]
#[ignore]
fn custo_do_confinamento() {
    if helper().is_none() {
        return;
    }
    let script = format!(
        "{IMAGEM}
docker run --rm -v /usr/share:/u:ro sx sh -c 'find /u -type f | head -40000 | xargs cat >/dev/null 2>&1'
for i in 1 2 3; do
  s=$(date +%s%N)
  docker run --rm -v /usr/share:/u:ro sx sh -c 'find /u -type f | head -40000 | xargs cat >/dev/null 2>&1'
  e=$(date +%s%N)
  echo LER=$(( (e-s)/1000000 ))
  s=$(date +%s%N)
  DOCKER_BUILDKIT=0 docker build -q --no-cache -t sx2 img >/dev/null
  e=$(date +%s%N)
  echo BUILD=$(( (e-s)/1000000 ))
done"
    );
    let mut out = Vec::new();
    for com in [false, true, false, true] {
        let e = Env::new();
        let mut cmd = e.cmd(&e.work, &["run", "shell"]);
        cmd.env("SHELL", "/bin/bash");
        if !com {
            cmd.env("SANDMUX_HELPER_SOCKET", "/nonexistent/sem-auxiliar");
        }
        let mut t = Term::spawn(cmd, 160, 40);
        let s = run_in(&mut t, &e.work, &script);
        assert!(s.contains("BUILD-OK"), "{s}");
        assert_eq!(stats(&e).contains("eBPF: ligado"), com);
        let med = |k: &str| {
            let mut v: Vec<u64> = s
                .lines()
                .filter_map(|l| l.strip_prefix(k)?.parse().ok())
                .collect();
            v.sort();
            v[v.len() / 2]
        };
        out.push((com, med("LER="), med("BUILD=")));
        t.write(b"exit\r");
        t.wait_exit();
        limpar(&e);
    }
    for (com, ler, build) in out {
        eprintln!(
            "MEDIDA {}: leitura de 40 mil arquivos {ler} ms, docker build {build} ms",
            if com { "com eBPF" } else { "sem eBPF" }
        );
    }
}

/// Arquivos criados com o Docker rodando: o `.env` novo e o que aparece no
/// host dentro de uma pasta negada são negados; o segredo do agente, não.
#[test]
fn arquivos_novos_negados_ao_vivo() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    let privado = e.work.join("privado");
    std::fs::create_dir_all(&privado).unwrap();
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\n",
        e.work.display(),
        privado.display()
    ));
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!("{IMAGEM}\ndocker ps >/dev/null && echo UP"),
    );
    assert!(s.contains("UP"), "{s}");
    assert!(stats(&e).contains("eBPF: ligado"));
    std::fs::write(e.work.join(".env"), "SEGREDO=novo\n").unwrap();
    std::fs::write(privado.join("outro.txt"), "outro\n").unwrap();
    let s = run_in(&mut t, &e.work, "echo do-agente > chave.pem; echo CRIADO");
    assert!(s.contains("CRIADO"), "{s}");
    std::thread::sleep(Duration::from_millis(1500));
    let s = run_in(
        &mut t,
        &e.work,
        "P='docker run --rm --privileged -v '$PWD':/w sx sh -c'
$P 'cat /w/.env && echo ENV-LIDO || echo ENV-NEGADO'
$P 'umount /w/privado; cat /w/privado/outro.txt && echo OUTRO-LIDO || echo OUTRO-NEGADO'
$P 'cat /w/chave.pem'
echo FIM",
    );
    assert!(
        s.contains("ENV-NEGADO") && !s.contains("SEGREDO=novo"),
        "{s}"
    );
    assert!(s.contains("OUTRO-NEGADO") && !s.contains("outro\n"), "{s}");
    assert!(s.contains("do-agente"), "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// A política de arquivos ao vivo: negar vale na leitura seguinte; liberar
/// tira a negação no kernel, e o arquivo coberto chega vazio.
#[test]
fn politica_de_arquivos_ao_vivo() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    std::fs::write(e.work.join("dados.csv"), "linha\n").unwrap();
    std::fs::write(e.work.join(".env"), "SEGREDO=velho\n").unwrap();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
P='docker run --rm --privileged -v '$PWD':/w sx sh -c'
$P 'cat /w/dados.csv'
$P 'umount /w/.env; cat /w/.env && echo ENV-ABRIU || echo ENV-NEGADO'"
        ),
    );
    assert!(s.contains("linha") && s.contains("ENV-NEGADO"), "{s}");
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\nfile-allow = [\"{}\"]\n",
        e.work.display(),
        e.work.join("dados.csv").display(),
        e.work.join(".env").display()
    ));
    std::thread::sleep(Duration::from_millis(3000));
    let s = run_in(
        &mut t,
        &e.work,
        "P='docker run --rm -v '$PWD':/w sx sh -c'
$P 'cat /w/dados.csv && echo CSV-LIDO || echo CSV-NEGADO'
$P 'cat /w/.env && echo ENV-ABRIU || echo ENV-NEGADO'
echo PRIV
docker run --rm --privileged -v $PWD:/w sx sh -c 'umount /w/.env; cat /w/.env'",
    );
    assert!(s.contains("CSV-NEGADO") && !s.contains("linha"), "{s}");
    let (comum, priv_) = s.split_once("PRIV").unwrap();
    // Liberado no kernel, mas ainda coberto pela montagem: um contêiner comum
    // abre o arquivo vazio; sem a cobertura, o conteúdo.
    assert!(
        comum.contains("ENV-ABRIU") && !comum.contains("SEGREDO=velho"),
        "{s}"
    );
    assert!(priv_.contains("SEGREDO=velho"), "{s}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O registro tem os bloqueios e as criações dos contêineres, com o caminho
/// do host, e nada do que é criado fora da pasta.
#[test]
fn registro_dos_conteineres() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    std::fs::write(e.work.join(".env"), "SEGREDO=do-usuario\n").unwrap();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "{IMAGEM}
P='docker run --rm --privileged -v '$PWD':/w sx sh -c'
$P 'umount /w/.env; cat /w/.env; echo oi > /w/saida.txt; echo x > /tmp/x-do-conteiner; echo FEITO'"
        ),
    );
    assert!(s.contains("FEITO"), "{s}");
    std::thread::sleep(Duration::from_millis(1500));
    let out = stdout(&e.run(&e.work, &["files", "1"]));
    let env = e.work.join(".env").display().to_string();
    let saida = e.work.join("saida.txt").display().to_string();
    assert!(
        out.lines()
            .any(|l| l.contains("docker") && l.contains(&env) && l.contains(" r ")),
        "{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.contains("docker") && l.contains(&saida) && l.contains("create")),
        "{out}"
    );
    assert!(!out.contains("x-do-conteiner"), "{out}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O programa de execução no kernel: o `observe` de um escopo do systemd de
/// usuário recebe o `ls -la` com o caminho e os argumentos.
#[test]
fn observacao_de_execucoes_no_kernel() {
    let Some(sock) = helper() else { return };
    let mut child = Command::new("systemd-run")
        .args([
            "--user",
            "--scope",
            "--quiet",
            "--",
            "sh",
            "-c",
            "sleep 2; ls -la / >/dev/null",
        ])
        .spawn()
        .unwrap();
    // O escopo: o cgroup do processo (o systemd-run --scope vira o comando).
    let deadline = Instant::now() + Duration::from_secs(5);
    let own = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let dir = loop {
        let cg =
            std::fs::read_to_string(format!("/proc/{}/cgroup", child.id())).unwrap_or_default();
        if let Some(rel) = cg.trim().strip_prefix("0::")
            && rel.ends_with(".scope")
            && cg != own
        {
            break Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
        }
        assert!(Instant::now() < deadline, "sem escopo: {cg}");
        std::thread::sleep(Duration::from_millis(20));
    };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    let ev = c.take_events().unwrap();
    c.observe(&dir).unwrap_or_else(|e| panic!("observe: {e}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut achou = false;
    while Instant::now() < deadline && !achou {
        if let Ok(v) = ev.recv_timeout(Duration::from_millis(200)) {
            for e in v["ev"].as_array().into_iter().flatten() {
                if e["kind"] == "exec" && e["path"].as_str().is_some_and(|p| p.ends_with("ls")) {
                    assert_eq!(e["args"][0], "ls", "{e}");
                    assert_eq!(e["args"][1], "-la", "{e}");
                    assert_eq!(e["args"][2], "/", "{e}");
                    achou = true;
                }
            }
        }
    }
    let _ = child.wait();
    assert!(achou, "o evento do ls não chegou");
}

/// Linhas `exec` do registro de arquivos deste ambiente.
fn exec_lines(e: &Env) -> Vec<serde_json::Value> {
    let dir = e.state.join("sandmux/files");
    let mut out = Vec::new();
    for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if f.path().extension().is_some_and(|x| x == "jsonl") {
            let text = std::fs::read_to_string(f.path()).unwrap();
            out.extend(
                text.lines()
                    .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                    .filter(|l| l["access"] == "exec"),
            );
        }
    }
    out
}

/// Com o auxiliar, as execuções da aba vêm do kernel, com os argumentos, e
/// as do Docker vêm com a etiqueta dele; parar o auxiliar devolve o registro
/// ao supervisor.
#[test]
fn execucoes_pelo_kernel_e_recuo() {
    let Some(sock) = helper() else { return };
    let e = Env::new();
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1.5; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    let st = stats(&e);
    assert!(st.contains("execuções: pelo kernel"), "{st}");
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "ls -la /tmp >/dev/null; echo LS
{IMAGEM}
docker run --rm sx sh -c 'ls / >/dev/null'; echo FIM"
        ),
    );
    assert!(s.contains("FIM"), "{s}");
    std::thread::sleep(Duration::from_millis(1500));
    let l = exec_lines(&e);
    let ls = l
        .iter()
        .find(|x| {
            x["path"].as_str().is_some_and(|p| p.ends_with("/ls"))
                && x["args"] == serde_json::json!(["ls", "-la", "/tmp"])
        })
        .unwrap_or_else(|| panic!("sem a linha do ls -la: {l:?}"));
    assert_eq!(ls["via"], "kernel", "{ls}");
    assert!(ls["tag"].as_str().unwrap().starts_with('s'), "{ls}");
    // O `sh` do contêiner (o busybox roda o `ls` como função dele, sem
    // `exec`).
    assert!(
        l.iter().any(
            |x| x["tag"].as_str().is_some_and(|t| t.starts_with("docker-"))
                && x["path"] == "/bin/sh"
                && x["args"][2] == "ls / >/dev/null"
        ),
        "sem a execução do contêiner: {l:?}"
    );
    // O auxiliar cai: o supervisor volta para a execução seguinte.
    std::fs::write(format!("{}.restart", sock.display()), "").unwrap();
    std::thread::sleep(Duration::from_millis(3000));
    let s = run_in(&mut t, &e.work, "uname -s >/dev/null; echo FIM2");
    assert!(s.contains("FIM2"), "{s}");
    std::thread::sleep(Duration::from_millis(500));
    let l = exec_lines(&e);
    let un = l
        .iter()
        .filter(|x| x["path"].as_str().is_some_and(|p| p.ends_with("uname")))
        .collect::<Vec<_>>();
    assert!(un.iter().any(|x| x["via"] == "supervisor"), "{un:?}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Custo do programa de execução: 2000 `/bin/true` no host, com e sem um
/// escopo observado, e numa aba. Só mede; roda com
/// `sudo scripts/check-root.sh --ignored custo_das_exec --nocapture`.
#[test]
#[ignore]
fn custo_das_execucoes() {
    let Some(sock) = helper() else { return };
    let laco = || {
        let t = Instant::now();
        let ok = Command::new("sh")
            .args([
                "-c",
                "i=0; while [ $i -lt 2000 ]; do /bin/true; i=$((i+1)); done",
            ])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        t.elapsed().as_millis()
    };
    laco();
    let sem = [laco(), laco(), laco()];
    let mut child = Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--", "sleep", "60"])
        .spawn()
        .unwrap();
    let own = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let dir = loop {
        let cg =
            std::fs::read_to_string(format!("/proc/{}/cgroup", child.id())).unwrap_or_default();
        if let Some(rel) = cg.trim().strip_prefix("0::")
            && rel.ends_with(".scope")
            && cg != own
        {
            break Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    c.observe(&dir).unwrap();
    let com = [laco(), laco(), laco()];
    drop(c);
    let _ = child.kill();
    let _ = child.wait();
    let e = Env::new();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        "sleep 1.5; s=$(date +%s%N); i=0; while [ $i -lt 2000 ]; do /bin/true; i=$((i+1)); done; e=$(date +%s%N); echo ABA=$(( (e-s)/1000000 ))",
    );
    eprintln!(
        "MEDIDA 2000 /bin/true no host: sem observação {sem:?} ms, com uma aba observada {com:?} ms"
    );
    eprintln!(
        "MEDIDA 2000 /bin/true numa aba (kernel + supervisor que só deixa seguir): {}",
        s.trim()
    );
    t.write(b"exit\r");
    t.wait_exit();
}

/// O programa de conexões no kernel: o `observe` de um escopo recebe a
/// conexão TCP de um processo dele, com a porta de origem certa.
#[test]
fn observacao_de_conexoes_no_kernel() {
    let Some(sock) = helper() else { return };
    let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = srv.local_addr().unwrap().port();
    let mut child = Command::new("systemd-run")
        .args(["--user", "--scope", "--quiet", "--", "bash", "-c"])
        .arg(format!("sleep 2; exec 3<>/dev/tcp/127.0.0.1/{port}"))
        .spawn()
        .unwrap();
    let own = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let dir = loop {
        let cg =
            std::fs::read_to_string(format!("/proc/{}/cgroup", child.id())).unwrap_or_default();
        if let Some(rel) = cg.trim().strip_prefix("0::")
            && rel.ends_with(".scope")
            && cg != own
        {
            break Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
        }
        assert!(Instant::now() < deadline, "sem escopo: {cg}");
        std::thread::sleep(Duration::from_millis(20));
    };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    let ev = c.take_events().unwrap();
    c.observe(&dir).unwrap();
    // O `accept` com tempo limite: sem a conexão, o teste falha em vez de
    // ficar parado.
    srv.set_nonblocking(true).unwrap();
    let fim = Instant::now() + Duration::from_secs(10);
    let peer = loop {
        match srv.accept() {
            Ok((conn, peer)) => {
                drop(conn);
                break peer;
            }
            Err(_) if Instant::now() < fim => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("a conexão não chegou: {e}"),
        }
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut achou = false;
    while Instant::now() < deadline && !achou {
        if let Ok(v) = ev.recv_timeout(Duration::from_millis(200)) {
            for e in v["ev"].as_array().into_iter().flatten() {
                if e["kind"] == "conn" && e["dport"] == port {
                    assert_eq!(e["sport"], peer.port(), "{e}");
                    assert_eq!(e["ip"], "127.0.0.1", "{e}");
                    assert!(e["pid"].as_u64().unwrap() > 0, "{e}");
                    achou = true;
                }
            }
        }
    }
    let _ = child.wait();
    assert!(achou, "o evento da conexão não chegou");
}

/// Linhas do registro de rede deste ambiente.
fn net_lines(e: &Env) -> Vec<serde_json::Value> {
    let dir = e.state.join("sandmux/net");
    let mut out = Vec::new();
    for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if f.path().extension().is_some_and(|x| x == "jsonl") {
            let text = std::fs::read_to_string(f.path()).unwrap();
            out.extend(text.lines().filter_map(|l| serde_json::from_str(l).ok()));
        }
    }
    out
}

/// Com o auxiliar, o processo das conexões de uma aba vem do kernel; as
/// conexões do Docker ficam sem processo.
#[test]
fn processo_das_conexoes_pelo_kernel() {
    if helper().is_none() {
        return;
    }
    let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = srv.local_addr().unwrap().port();
    let t_srv = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut c, _) = srv.accept().unwrap();
        let mut buf = [0u8; 8];
        let _ = c.read(&mut buf);
        let _ = c.write_all(b"y");
    });
    let e = Env::new();
    let mut t = term(&e, &e.work);
    let s = run_in(
        &mut t,
        &e.work,
        &format!(
            "sleep 1.5
python3 -c 'import socket;s=socket.create_connection((\"host.sandmux.internal\",{port}),timeout=5);s.sendall(b\"x\");s.recv(8);s.close();print(\"CONECTOU\")'
{IMAGEM}
docker run --rm sx wget -q -T 2 -O /dev/null http://169.254.169.254/ ; echo FIM"
        ),
    );
    assert!(s.contains("CONECTOU") && s.contains("FIM"), "{s}");
    t_srv.join().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let l = net_lines(&e);
    let c = l
        .iter()
        .find(|x| x["proto"] == "tcp" && x["port"] == port)
        .unwrap_or_else(|| panic!("sem a conexão: {l:?}"));
    assert_eq!(c["proc_via"], "kernel", "{c}");
    assert!(
        c["exe"].as_str().is_some_and(|x| x.contains("python")),
        "{c}"
    );
    assert!(c["pid"].as_u64().is_some_and(|p| p > 0), "{c}");
    let docker: Vec<_> = l
        .iter()
        .filter(|x| x["tag"].as_str().is_some_and(|t| t.starts_with("docker-")))
        .collect();
    assert!(!docker.is_empty(), "sem linha do Docker: {l:?}");
    assert!(docker.iter().all(|x| x.get("pid").is_none()), "{docker:?}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Custo do processo das conexões: 200 conexões a um servidor local numa
/// aba, com o auxiliar (kernel) e sem ele (sock_diag e `/proc`). Mede o
/// tempo e confere que as linhas têm o processo; roda com
/// `sudo scripts/check-root.sh --ignored custo_das_con --nocapture`.
#[test]
#[ignore]
fn custo_das_conexoes() {
    if helper().is_none() {
        return;
    }
    let srv = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = srv.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for c in srv.incoming().flatten() {
            let mut c = c;
            let mut b = [0u8; 4];
            let _ = c.read(&mut b);
            let _ = c.write_all(b"y");
        }
    });
    let script = format!(
        "sleep 1.5; s=$(date +%s%N); python3 -c 'import socket
for i in range(200):
    s=socket.create_connection((\"host.sandmux.internal\",{port}),timeout=5); s.sendall(b\"x\"); s.recv(4); s.close()'; e=$(date +%s%N); echo T=$(( (e-s)/1000000 ))"
    );
    for com in [false, true, false, true] {
        let e = Env::new();
        let mut cmd = e.cmd(&e.work, &["run", "shell"]);
        cmd.env("SHELL", "/bin/bash");
        if !com {
            cmd.env("SANDMUX_HELPER_SOCKET", "/nonexistent/sem-auxiliar");
        }
        let mut t = Term::spawn(cmd, 160, 40);
        let s = run_in(&mut t, &e.work, &script);
        std::thread::sleep(Duration::from_millis(1500));
        let l = net_lines(&e);
        let com_proc = l
            .iter()
            .filter(|x| x["port"] == port && x.get("pid").is_some())
            .count();
        eprintln!(
            "MEDIDA {}: 200 conexões em {} ms; {com_proc} de {} linhas com processo",
            if com {
                "com o auxiliar"
            } else {
                "sem o auxiliar"
            },
            s.lines().find_map(|x| x.strip_prefix("T=")).unwrap_or("?"),
            l.iter().filter(|x| x["port"] == port).count()
        );
        // As conexões duram cerca de 2 ms: o gateway espera a busca antes
        // de abrir a conexão de saída, e quase todas ganham o processo.
        assert!(com_proc >= 190, "{com_proc} de 200 com processo");
        t.write(b"exit\r");
        t.wait_exit();
    }
}
