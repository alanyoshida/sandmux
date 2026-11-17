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
    let e = c.confine(Path::new("/sys/fs/cgroup")).unwrap_err();
    assert!(e.contains("pertence ao uid 0"), "{e}");
    // Um cgroup do próprio usuário: o do teste.
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let rel = cg.trim().strip_prefix("0::").unwrap();
    let dir = Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
    let c2 = sandmux::helper::Client::connect(&sock).unwrap();
    match c2.confine(&dir) {
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
