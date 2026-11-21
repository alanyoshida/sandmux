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

/// Todas as linhas do registro de arquivos deste ambiente.
fn all_file_lines(e: &Env) -> Vec<serde_json::Value> {
    let dir = e.state.join("sandmux/files");
    let mut out = Vec::new();
    for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if f.path().extension().is_some_and(|x| x == "jsonl") {
            let text = std::fs::read_to_string(f.path()).unwrap();
            out.extend(
                text.lines()
                    .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()),
            );
        }
    }
    out
}

/// Os arquivos da aba pelo kernel (change 78): o `.env` some, o `file-deny`
/// nega, o segredo do agente fica dele, o criado no host fica negado em até
/// ~1 s, as aberturas entram no registro, e a queda do auxiliar para a aba.
#[test]
fn arquivos_da_aba_pelo_kernel() {
    let Some(sock) = helper() else { return };
    let e = Env::new();
    let privado = e.work.join("privado");
    std::fs::create_dir_all(&privado).unwrap();
    std::fs::write(privado.join("dados.sql"), "segredo\n").unwrap();
    std::fs::write(e.work.join(".env"), "SEGREDO=1\n").unwrap();
    std::fs::write(e.work.join("livre.txt"), "livre\n").unwrap();
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\n",
        e.work.display(),
        privado.display()
    ));
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    let st = stats(&e);
    assert!(st.contains("aberturas de arquivo: pelo kernel"), "{st}");
    let s = run_in(
        &mut t,
        &e.work,
        "cat .env 2>&1; cat privado/dados.sql 2>&1; cat livre.txt; \
         echo novo > criado.txt; echo X=1 > .env.local; cat .env.local; echo FIM",
    );
    assert!(s.contains("FIM"), "{s}");
    assert!(s.contains(".env: No such file"), "{s}");
    assert!(s.contains("dados.sql: Permission denied"), "{s}");
    assert!(s.contains("livre\n") || s.contains("livre\r\n"), "{s}");
    assert!(s.contains("X=1"), "o segredo do agente é dele: {s}");
    // Renomear ou ligar o segredo não o expõe: a marca é do inode.
    let s = run_in(
        &mut t,
        &e.work,
        "mv .env outro.txt 2>/dev/null; ln .env elo.txt 2>/dev/null; \
         cat outro.txt elo.txt 2>/dev/null | grep -c SEGREDO; echo FIM3",
    );
    assert!(s.contains("0\n") || s.contains("0\r\n"), "{s}");
    // Um segredo criado no host durante a sessão: negado pela vigia.
    std::fs::write(e.work.join(".env.prod"), "P=1\n").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let s = run_in(&mut t, &e.work, "cat .env.prod 2>&1; echo FIM2");
    assert!(s.contains(".env.prod: No such file"), "{s}");
    // O registro: leituras e escritas pela fila do kernel, e as negações.
    std::thread::sleep(Duration::from_millis(1000));
    let l = all_file_lines(&e);
    let tem = |nome: &str, access: &str| {
        l.iter()
            .any(|x| x["path"].as_str().is_some_and(|p| p.ends_with(nome)) && x["access"] == access)
    };
    assert!(tem("/livre.txt", "r"), "{l:?}");
    assert!(tem("/criado.txt", "w"), "{l:?}");
    assert!(
        l.iter().any(|x| x["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("/dados.sql"))
            && x["blocked"]
                .as_str()
                .is_some_and(|b| b.starts_with("file-deny"))),
        "{l:?}"
    );
    // O auxiliar cai: a aba fica congelada até o confinamento voltar, com
    // as mesmas marcas, e segue.
    std::fs::write(format!("{}.restart", sock.display()), "").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let s = run_in(
        &mut t,
        &e.work,
        "cat .env.prod 2>&1; cat livre.txt; echo FIM4",
    );
    assert!(s.contains(".env.prod: No such file"), "{s}");
    assert!(s.contains("FIM4"), "{s}");
    let st = stats(&e);
    assert!(st.contains("aberturas de arquivo: pelo kernel"), "{st}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O shell de entrada de uma aba pelo kernel (change 81): as mesmas
/// negações, as aberturas pela fila do kernel e os mesmos filtros seccomp
/// do comando, sem o supervisor de aberturas (vinte mil aberturas custam no
/// shell o mesmo que no comando, e não uma ida e volta ao supervisor cada).
#[test]
fn shell_de_entrada_pela_aba_no_kernel() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    let privado = e.work.join("privado");
    std::fs::create_dir_all(&privado).unwrap();
    std::fs::write(privado.join("dados.sql"), "segredo\n").unwrap();
    std::fs::write(e.work.join(".env"), "SEGREDO=1\n").unwrap();
    std::fs::write(e.work.join("comum.txt"), "comum\n").unwrap();
    std::fs::write(e.work.join("misto.txt"), "misto\n").unwrap();
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\n",
        e.work.display(),
        privado.display()
    ));
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    let st = stats(&e);
    assert!(st.contains("aberturas de arquivo: pelo kernel"), "{st}");
    let filtros = |s: &str| {
        s.lines()
            .find(|l| l.starts_with("Seccomp_filters:"))
            .map(|l| l.to_string())
    };
    let da_aba = run_in(&mut t, &e.work, "grep Seccomp_filters /proc/self/status");
    let da_aba = filtros(&da_aba).unwrap_or_else(|| panic!("{da_aba}"));
    const ABERTURAS: &str = "python3 -c \"import os,time\nfor _ in range(3):\n t=time.perf_counter()\n for _ in range(20000): os.close(os.open('comum.txt', os.O_RDONLY))\n print('T', int((time.perf_counter()-t)*1e6))\"";
    let menor = |s: &str| -> u64 {
        s.lines()
            .filter_map(|l| l.trim().strip_prefix("T "))
            .filter_map(|x| x.parse().ok())
            .min()
            .unwrap_or_else(|| panic!("sem a medida: {s}"))
    };
    let t_aba = menor(&run_in(&mut t, &e.work, ABERTURAS));

    let mut cmd = e.cmd(&e.work, &["enter", "1"]);
    cmd.env("SHELL", "/bin/bash");
    let mut d = Term::spawn(cmd, 160, 40);
    let s = run_in(
        &mut d,
        &e.work,
        "cat .env 2>&1; cat privado/dados.sql 2>&1; cat comum.txt; \
         exec 3<>misto.txt; exec 3>&-; grep Seccomp_filters /proc/self/status; echo FIM",
    );
    assert!(s.contains("FIM"), "{s}");
    assert!(s.contains(".env: No such file"), "{s}");
    assert!(s.contains("dados.sql: Permission denied"), "{s}");
    assert!(s.contains("comum\n"), "{s}");
    assert_eq!(filtros(&s), Some(da_aba), "{s}");
    let t_shell = menor(&run_in(&mut d, &e.work, ABERTURAS));
    assert!(
        t_shell < 3 * t_aba.max(20_000),
        "as aberturas do shell passam pelo supervisor: {t_shell} µs, contra {t_aba} µs na aba"
    );
    std::thread::sleep(Duration::from_millis(1000));
    let l = all_file_lines(&e);
    let de = |nome: &str| {
        l.iter()
            .filter(|x| x["path"].as_str().is_some_and(|p| p.ends_with(nome)))
            .collect::<Vec<_>>()
    };
    assert!(de("/comum.txt").iter().any(|x| x["access"] == "r"), "{l:?}");
    assert!(
        de("/dados.sql").iter().any(|x| x["blocked"]
            .as_str()
            .is_some_and(|b| b.starts_with("file-deny"))),
        "{l:?}"
    );
    // A leitura e escrita, `rw` como no supervisor (change 82).
    let misto = de("/misto.txt");
    assert!(misto.iter().any(|x| x["access"] == "rw"), "{misto:?}");
    d.write(b"exit\r");
    d.wait_exit();
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// O acesso das aberturas numa aba pelo kernel (change 82): `r`, `w` e
/// `rw` como no supervisor, também numa abertura bloqueada, e o mesmo
/// arquivo lido e depois aberto para leitura e escrita com as duas linhas.
#[test]
fn acesso_de_leitura_e_escrita_pelo_kernel() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    let privado = e.work.join("privado");
    std::fs::create_dir_all(&privado).unwrap();
    std::fs::write(privado.join("dados.sql"), "segredo\n").unwrap();
    for n in ["le.txt", "grava.txt", "ambos.txt", "depois.txt"] {
        std::fs::write(e.work.join(n), "x\n").unwrap();
    }
    e.write_config(&format!(
        "[project.\"{}\"]\nfile-deny = [\"{}\"]\n",
        e.work.display(),
        privado.display()
    ));
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    assert!(stats(&e).contains("aberturas de arquivo: pelo kernel"));
    let s = run_in(
        &mut t,
        &e.work,
        "cat le.txt >/dev/null; echo y > grava.txt; exec 3<>ambos.txt; exec 3>&-; \
         cat depois.txt >/dev/null; exec 4<>depois.txt; exec 4>&-; \
         (exec 5<>privado/dados.sql) 2>&1; echo FIM",
    );
    assert!(s.contains("FIM"), "{s}");
    assert!(s.contains("Permission denied"), "{s}");
    std::thread::sleep(Duration::from_millis(1000));
    let l = all_file_lines(&e);
    let acessos = |nome: &str| -> Vec<String> {
        let mut v: Vec<String> = l
            .iter()
            .filter(|x| x["path"].as_str().is_some_and(|p| p.ends_with(nome)))
            .filter(|x| x["blocked"].is_null())
            .filter_map(|x| x["access"].as_str().map(String::from))
            .collect();
        v.sort();
        v
    };
    assert_eq!(acessos("/le.txt"), ["r"], "{l:?}");
    assert_eq!(acessos("/grava.txt"), ["w"], "{l:?}");
    assert_eq!(acessos("/ambos.txt"), ["rw"], "{l:?}");
    assert_eq!(acessos("/depois.txt"), ["r", "rw"], "{l:?}");
    assert!(
        l.iter().any(|x| x["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("/dados.sql"))
            && x["access"] == "rw"
            && x["blocked"]
                .as_str()
                .is_some_and(|b| b.starts_with("file-deny"))),
        "{l:?}"
    );
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// A leitura em massa numa aba pelo kernel (change 80): trinta mil arquivos
/// diferentes, cada um lido três vezes, chegam todos ao registro, sem linha
/// de descarte. As repetições ficam no kernel, e a aba espera a fila esvaziar.
#[test]
fn leitura_em_massa_pela_aba_completa() {
    if helper().is_none() {
        return;
    }
    const DIRS: usize = 300;
    const FILES: usize = 100;
    let e = Env::new();
    for d in 0..DIRS {
        let dir = e.work.join(format!("massa/d{d:03}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..FILES {
            std::fs::write(dir.join(format!("f{f:03}")), "x\n").unwrap();
        }
    }
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    let st = stats(&e);
    assert!(st.contains("aberturas de arquivo: pelo kernel"), "{st}");
    let s = run_in(
        &mut t,
        &e.work,
        "for i in 1 2 3; do cat massa/*/* > /dev/null; done; echo FIM",
    );
    assert!(s.contains("FIM"), "{s}");
    // Os arquivos, sem as pastas que o `sh` abre para expandir o padrão.
    let arquivo = |x: &serde_json::Value| {
        x["path"].as_str().is_some_and(|p| {
            p.contains("/massa/d") && p.rsplit('/').next().is_some_and(|n| n.starts_with('f'))
        })
    };
    let massa = |l: &[serde_json::Value]| {
        l.iter()
            .filter(|x| x["access"] == "r" && arquivo(x))
            .filter_map(|x| x["path"].as_str().map(str::to_string))
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut l = all_file_lines(&e);
    while massa(&l) < DIRS * FILES && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        l = all_file_lines(&e);
    }
    let descartes: Vec<_> = l.iter().filter(|x| x["access"] == "?").collect();
    assert!(descartes.is_empty(), "{descartes:?}");
    assert_eq!(massa(&l), DIRS * FILES);
    // Uma linha por arquivo, e não uma por leitura.
    let linhas = l.iter().filter(|x| arquivo(x)).count();
    assert_eq!(linhas, DIRS * FILES);
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// Um arquivo renomeado pela aba e lido logo depois com o nome novo vai para
/// o registro com o nome novo (change 80): renomear tira o arquivo das
/// repetições do kernel.
#[test]
fn arquivo_renomeado_pela_aba_no_registro() {
    if helper().is_none() {
        return;
    }
    let e = Env::new();
    std::fs::write(e.work.join("antes.txt"), "a\n").unwrap();
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
    assert!(s.contains("PRONTO"), "{s}");
    let st = stats(&e);
    assert!(st.contains("aberturas de arquivo: pelo kernel"), "{st}");
    let s = run_in(
        &mut t,
        &e.work,
        "cat antes.txt; mv antes.txt depois.txt; cat depois.txt; ln depois.txt elo.txt; \
         cat elo.txt; echo FIM",
    );
    assert!(s.contains("FIM"), "{s}");
    let tem = |l: &[serde_json::Value], nome: &str| {
        l.iter()
            .any(|x| x["access"] == "r" && x["path"].as_str().is_some_and(|p| p.ends_with(nome)))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut l = all_file_lines(&e);
    while !(tem(&l, "/depois.txt") && tem(&l, "/elo.txt")) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
        l = all_file_lines(&e);
    }
    assert!(tem(&l, "/antes.txt"), "{l:?}");
    assert!(tem(&l, "/depois.txt"), "{l:?}");
    assert!(tem(&l, "/elo.txt"), "{l:?}");
    t.write(b"exit\r");
    t.wait_exit();
    limpar(&e);
}

/// A ABI do Landlock deste kernel.
fn landlock_abi() -> i64 {
    // SAFETY: a consulta da versão não usa os ponteiros.
    unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0usize,
            1u32,
        )
    }
}

/// O sinal ao processo 1 do sandbox, de dentro: negado pelo escopo.
const SINAL_1: &str = "kill -0 1 2>/dev/null && echo SINAL-OK || echo SINAL-NEGADO";

/// O escopo do sandbox (Landlock abaixo do ABI 6) volta depois de um
/// reinício do auxiliar (change 79): nas abas do daemon, com os arquivos pelo
/// kernel e pelo supervisor, a aba congela e segue com o sinal ao processo 1
/// ainda negado; um lançamento logo depois do reinício sobe; no `run` sem o
/// daemon, o lançador registra de novo.
#[test]
fn escopo_de_volta_depois_do_reinicio() {
    let Some(sock) = helper() else { return };
    if landlock_abi() >= 6 {
        eprintln!("pulado: o Landlock tem o ABI 6, e o escopo não vem do auxiliar");
        return;
    }
    let restart = || {
        std::fs::write(format!("{}.restart", sock.display()), "").unwrap();
    };
    for supervisor in [false, true] {
        let e = Env::new();
        if supervisor {
            e.write_config(&format!(
                "[project.\"{}\"]\nfile-deny = [\"*.nunca\"]\n",
                e.work.display()
            ));
        }
        let mut t = term(&e, &e.work);
        let s = run_in(&mut t, &e.work, SINAL_1);
        assert!(s.contains("SINAL-NEGADO"), "{supervisor}: {s}");
        assert_eq!(
            stats(&e).contains("aberturas de arquivo: pelo kernel"),
            !supervisor
        );
        restart();
        std::thread::sleep(Duration::from_millis(500));
        let s = run_in(&mut t, &e.work, SINAL_1);
        assert!(s.contains("SINAL-NEGADO"), "{supervisor}, depois: {s}");
        t.write(b"exit\r");
        t.wait_exit();
        limpar(&e);
    }
    // Um lançamento logo depois do reinício.
    let e = Env::new();
    restart();
    let mut t = term(&e, &e.work);
    let s = run_in(&mut t, &e.work, SINAL_1);
    assert!(s.contains("SINAL-NEGADO"), "lançamento na troca: {s}");
    t.write(b"exit\r");
    t.wait_exit();
    // O `run` sem o daemon: o próprio lançador registra de novo.
    let e = Env::new();
    let script = format!("{SINAL_1}; sleep 5; {SINAL_1}");
    let bins = common::bin_dir();
    let kid = e
        .cmd(
            &e.work,
            &[
                "run",
                "shell",
                "--ro",
                bins.to_str().unwrap(),
                "--",
                "-c",
                &script,
            ],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    restart();
    let o = kid.wait_with_output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        out.matches("SINAL-NEGADO").count(),
        2,
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// Custo das aberturas de uma aba: ler 13 mil arquivos da pasta e o import
/// do Python, no host, com os arquivos pelo kernel e pelo supervisor (um
/// `file-deny` por nome deixa a aba com ele), e no shell de entrada da aba
/// pelo kernel. Só mede; roda com
/// `scripts/vm-ubuntu-2404.sh check-root --ignored custo_dos_arquivos --nocapture`.
#[test]
#[ignore]
fn custo_dos_arquivos_da_aba() {
    if helper().is_none() {
        return;
    }
    const SCRIPT: &str = "s=$(date +%s%N); cat arv/*/* >/dev/null; m=$(date +%s%N); \
        python3 -c 'import json, email.parser, http.client, asyncio, xml.dom.minidom, decimal, sqlite3'; \
        f=$(date +%s%N); echo T $(( (m-s)/1000000 )) $(( (f-m)/1000000 ))";
    let parse = |s: &str| -> (u64, u64) {
        let l = s
            .lines()
            .find_map(|l| l.trim().strip_prefix("T "))
            .unwrap_or_else(|| panic!("sem a medida: {s}"));
        let mut it = l.split_whitespace().map(|x| x.parse::<u64>().unwrap());
        (it.next().unwrap(), it.next().unwrap())
    };
    let arvore = |w: &Path| {
        for d in 0..130 {
            let dir = w.join(format!("arv/d{d:03}"));
            std::fs::create_dir_all(&dir).unwrap();
            for f in 0..100 {
                std::fs::write(dir.join(format!("f{f:03}")), "x\n").unwrap();
            }
        }
    };
    let mediana = |mut v: Vec<u64>| {
        v.sort();
        v[v.len() / 2]
    };
    // No host, sem sandbox.
    let host = tempfile::tempdir().unwrap();
    arvore(host.path());
    let mut hv = (Vec::new(), Vec::new());
    for _ in 0..3 {
        let o = Command::new("sh")
            .args(["-c", SCRIPT])
            .current_dir(host.path())
            .output()
            .unwrap();
        let (a, b) = parse(&String::from_utf8_lossy(&o.stdout));
        hv.0.push(a);
        hv.1.push(b);
    }
    let mut linhas = vec![format!(
        "host: leitura {} ms, import {} ms",
        mediana(hv.0),
        mediana(hv.1)
    )];
    for (nome, supervisor) in [("kernel", false), ("supervisor", true)] {
        let e = Env::new();
        arvore(&e.work);
        if supervisor {
            e.write_config(&format!(
                "[project.\"{}\"]\nfile-deny = [\"*.nunca\"]\n",
                e.work.display()
            ));
        }
        let mut t = term(&e, &e.work);
        let s = run_in(&mut t, &e.work, "sleep 1; echo PRONTO");
        assert!(s.contains("PRONTO"), "{s}");
        let st = stats(&e);
        assert_eq!(
            st.contains("aberturas de arquivo: pelo kernel"),
            !supervisor,
            "{st}"
        );
        let mut v = (Vec::new(), Vec::new());
        // A primeira rodada aquece o cache de páginas e o registro.
        let _ = run_in(&mut t, &e.work, SCRIPT);
        for _ in 0..3 {
            let (a, b) = parse(&run_in(&mut t, &e.work, SCRIPT));
            v.0.push(a);
            v.1.push(b);
        }
        linhas.push(format!(
            "{nome}: leitura {} ms, import {} ms",
            mediana(v.0),
            mediana(v.1)
        ));
        // O shell de entrada da aba segue o modo dela (change 81).
        if !supervisor {
            let mut cmd = e.cmd(&e.work, &["enter", "1"]);
            cmd.env("SHELL", "/bin/bash");
            let mut d = Term::spawn(cmd, 160, 40);
            let _ = run_in(&mut d, &e.work, SCRIPT);
            let mut v = (Vec::new(), Vec::new());
            for _ in 0..3 {
                let (a, b) = parse(&run_in(&mut d, &e.work, SCRIPT));
                v.0.push(a);
                v.1.push(b);
            }
            linhas.push(format!(
                "shell de entrada: leitura {} ms, import {} ms",
                mediana(v.0),
                mediana(v.1)
            ));
            d.write(b"exit\r");
            d.wait_exit();
        }
        t.write(b"exit\r");
        t.wait_exit();
        limpar(&e);
    }
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    eprintln!("CUSTO {} | {}", kernel.trim(), linhas.join(" | "));
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

/// O teste do escopo dentro de um user namespace: espera uma linha na entrada
/// antes de cada rodada e escreve o resultado de cada tentativa.
const ESCOPO_PY: &str = r#"
import os, socket, sys, fcntl, termios, json
host_pid = int(sys.argv[1]); name = sys.argv[2]
def tenta(f):
    try:
        f(); return "ok"
    except OSError as e:
        return e.errno
def abstrato_host():
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.connect("\0" + name); s.close()
def abstrato_proprio():
    l = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); l.bind("\0" + name + "-dentro"); l.listen(1)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.connect("\0" + name + "-dentro"); s.close(); l.close()
def datagrama_host():
    s = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM); s.sendto(b"x", "\0" + name + "-dg"); s.close()
def ioctl(dev, cmd):
    fd = os.open(dev, os.O_RDONLY)
    try:
        fcntl.ioctl(fd, cmd, b"\0" * 8)
    finally:
        os.close(fd)
for linha in sys.stdin:
    r = {
        "abstrato_host": tenta(abstrato_host),
        "abstrato_proprio": tenta(abstrato_proprio),
        "datagrama_host": tenta(datagrama_host),
        "sinal_host": tenta(lambda: os.kill(host_pid, 0)),
        "sinal_proprio": tenta(lambda: os.kill(os.getpid(), 0)),
        "ioctl_zero": tenta(lambda: ioctl("/dev/zero", termios.FIONREAD)),
        "ioctl_null": tenta(lambda: ioctl("/dev/null", termios.FIONREAD)),
        "fioclex_zero": tenta(lambda: ioctl("/dev/zero", termios.FIOCLEX)),
    }
    print(json.dumps(r), flush=True)
"#;

/// O escopo de sandbox (`scope`), para os kernels sem o Landlock ABI 6: um
/// processo num user namespace registrado não alcança um socket abstrato do
/// host, não sinaliza um processo de fora e não faz `ioctl` num dispositivo
/// fora da lista; o resto da máquina não muda, e tudo volta quando a conexão
/// fecha.
#[test]
fn escopo_de_sandbox_no_kernel() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::fd::OwnedFd;
    use std::os::linux::net::SocketAddrExt;
    let Some(sock) = std::env::var_os("SANDMUX_HELPER_SOCKET").map(PathBuf::from) else {
        return;
    };
    let name = format!("sandmux-escopo-{}", std::process::id());
    let addr = |n: &str| std::os::unix::net::SocketAddr::from_abstract_name(n.as_bytes()).unwrap();
    let l = std::os::unix::net::UnixListener::bind_addr(&addr(&name)).unwrap();
    let dg = std::os::unix::net::UnixDatagram::bind_addr(&addr(&format!("{name}-dg"))).unwrap();
    std::thread::spawn(move || {
        for c in l.incoming() {
            drop(c)
        }
    });
    std::thread::spawn(move || {
        let mut b = [0u8; 8];
        while dg.recv(&mut b).is_ok() {}
    });
    let mut kid = Command::new("unshare")
        .args(["-U", "python3", "-c", ESCOPO_PY])
        .arg(std::process::id().to_string())
        .arg(&name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = kid.stdin.take().unwrap();
    let mut out = BufReader::new(kid.stdout.take().unwrap());
    let mut round = || -> serde_json::Value {
        input.write_all(b"\n").unwrap();
        let mut s = String::new();
        out.read_line(&mut s).unwrap();
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("{e}: {s:?}"))
    };
    // Sem o escopo, só o `ioctl` do driver falha (ENOTTY).
    let antes = round();
    assert_eq!(antes["abstrato_host"], "ok", "{antes}");
    assert_eq!(antes["sinal_host"], "ok", "{antes}");
    assert_eq!(antes["ioctl_zero"], libc::ENOTTY, "{antes}");
    // O user namespace do filho, registrado.
    let own = std::fs::metadata("/proc/self/ns/user").unwrap();
    let ns = loop {
        let f = std::fs::File::open(format!("/proc/{}/ns/user", kid.id())).unwrap();
        if std::os::unix::fs::MetadataExt::ino(&f.metadata().unwrap())
            != std::os::unix::fs::MetadataExt::ino(&own)
        {
            break OwnedFd::from(f);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    let st = c.status().unwrap();
    assert!(st.scope, "{:?}", st.scope_reason);
    let null = (1 << 20) | 3;
    c.scope(&ns, true, &[(null, u32::MAX)]).unwrap();
    let dentro = round();
    eprintln!("ESCOPO {dentro}");
    assert_eq!(dentro["abstrato_host"], libc::EPERM, "{dentro}");
    assert_eq!(dentro["datagrama_host"], libc::EPERM, "{dentro}");
    assert_eq!(dentro["abstrato_proprio"], "ok", "{dentro}");
    assert_eq!(dentro["sinal_host"], libc::EPERM, "{dentro}");
    assert_eq!(dentro["sinal_proprio"], "ok", "{dentro}");
    assert_eq!(dentro["ioctl_zero"], libc::EACCES, "{dentro}");
    assert_eq!(dentro["ioctl_null"], libc::ENOTTY, "{dentro}");
    assert_eq!(dentro["fioclex_zero"], "ok", "{dentro}");
    // Fora do namespace, nada muda.
    let s = std::os::unix::net::UnixStream::connect_addr(&addr(&name));
    assert!(s.is_ok(), "{s:?}");
    // A conexão fecha: o escopo sai.
    drop(c);
    std::thread::sleep(Duration::from_millis(200));
    let depois = round();
    assert_eq!(depois["abstrato_host"], "ok", "{depois}");
    assert_eq!(depois["sinal_host"], "ok", "{depois}");
    drop(input);
    let _ = kid.wait();
}

/// O processo da aba confinada pelo kernel: espera uma linha e tenta as
/// aberturas, escrevendo o resultado de cada uma.
const ABA_PY: &str = r#"
import os, sys, json
d = sys.argv[1]
def tenta(f):
    try:
        f(); return "ok"
    except OSError as e:
        return e.errno
for linha in sys.stdin:
    r = {
        "segredo": tenta(lambda: open(os.path.join(d, ".env")).read()),
        "negado": tenta(lambda: open(os.path.join(d, "dados.sql")).read()),
        "livre": tenta(lambda: open(os.path.join(d, "livre.txt")).read()),
        "grava": tenta(lambda: open(os.path.join(d, "novo.txt"), "w").write("x")),
    }
    print(json.dumps(r), flush=True)
"#;

/// As marcas das abas (`SECRET` com `ENOENT`, `FDENY` com `EACCES`) e os
/// eventos de abertura de uma montagem registrada, num escopo do systemd
/// confinado pelo auxiliar real.
#[test]
fn marcas_e_aberturas_da_aba_no_kernel() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::MetadataExt;
    let Some(sock) = std::env::var_os("SANDMUX_HELPER_SOCKET").map(PathBuf::from) else {
        return;
    };
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().canonicalize().unwrap();
    for (n, c) in [(".env", "S=1"), ("dados.sql", "x"), ("livre.txt", "ok")] {
        std::fs::write(dir.join(n), c).unwrap();
    }
    // A montagem da pasta: o `mnt_id` dela no `mountinfo` deste processo.
    let mi = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    let mnt_of = |p: &Path| -> u32 {
        let mut best = (0usize, 0u32);
        for l in mi.lines() {
            let f: Vec<&str> = l.split_whitespace().collect();
            let mp = Path::new(f[4]);
            if p.starts_with(mp) && mp.as_os_str().len() >= best.0 {
                best = (mp.as_os_str().len(), f[0].parse().unwrap());
            }
        }
        best.1
    };
    let mnt = mnt_of(&dir);
    let unit = format!("sandmux-aba-{}", std::process::id());
    let mut kid = Command::new("systemd-run")
        .args([
            "--user",
            "--scope",
            "-q",
            &format!("--unit={unit}"),
            "python3",
            "-c",
            ABA_PY,
        ])
        .arg(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = kid.stdin.take().unwrap();
    let mut out = BufReader::new(kid.stdout.take().unwrap());
    let mut round = || -> serde_json::Value {
        input.write_all(b"\n").unwrap();
        let mut s = String::new();
        out.read_line(&mut s).unwrap();
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("{e}: {s:?}"))
    };
    let antes = round();
    assert_eq!(antes["segredo"], "ok", "{antes}");
    // O escopo do processo.
    let cg = loop {
        let c = std::fs::read_to_string(format!("/proc/{}/cgroup", kid.id())).unwrap();
        if c.contains(&unit) {
            break PathBuf::from("/sys/fs/cgroup").join(c.trim().trim_start_matches("0::/"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let c = sandmux::helper::Client::connect(&sock).unwrap();
    let dev = std::fs::metadata(&dir).unwrap().dev();
    let kdev = ((libc::major(dev) as u32) << 20) | (libc::minor(dev) as u32 & 0xf_ffff);
    c.confine_with(&cg, kdev, &[mnt]).unwrap();
    c.mark(sandmux::helper::MarkKind::Secret, &[dir.join(".env")])
        .unwrap();
    c.mark(
        sandmux::helper::MarkKind::FileDeny,
        &[dir.join("dados.sql")],
    )
    .unwrap();
    let ev = c.take_events().unwrap();
    let dentro = round();
    eprintln!("ABA {dentro}");
    assert_eq!(dentro["segredo"], libc::ENOENT, "{dentro}");
    assert_eq!(dentro["negado"], libc::EACCES, "{dentro}");
    assert_eq!(dentro["livre"], "ok", "{dentro}");
    assert_eq!(dentro["grava"], "ok", "{dentro}");
    // Os eventos: as aberturas de `livre.txt` (leitura) e de `novo.txt`
    // (escrita), com o caminho, e os bloqueios.
    let fim = Instant::now() + Duration::from_secs(5);
    let (mut leu, mut gravou, mut negou) = (false, false, 0);
    while Instant::now() < fim && !(leu && gravou && negou >= 2) {
        let Ok(m) = ev.recv_timeout(Duration::from_millis(300)) else {
            continue;
        };
        for e in m["ev"].as_array().cloned().unwrap_or_default() {
            match e["kind"].as_str() {
                Some("open") if e["path"] == dir.join("livre.txt").to_str().unwrap() => {
                    leu |= e["write"] == false
                }
                Some("open") if e["path"] == dir.join("novo.txt").to_str().unwrap() => {
                    gravou |= e["write"] == true
                }
                Some("deny") => negou += 1,
                _ => {}
            }
        }
    }
    assert!(
        leu && gravou && negou >= 2,
        "leu {leu} gravou {gravou} negou {negou}"
    );
    // Fora do escopo, nada muda.
    assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), "S=1");
    drop(c);
    std::thread::sleep(Duration::from_millis(300));
    let depois = round();
    assert_eq!(depois["segredo"], "ok", "{depois}");
    drop(input);
    let _ = kid.wait();
}
