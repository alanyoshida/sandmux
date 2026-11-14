//! Docker privado por pasta: detecção de suporte, identificador da pasta,
//! pastas e o comando que sobe o `dockerd` no `rootlesskit`, dentro da visão
//! de arquivos da pasta.

use std::path::{Path, PathBuf};

use sandmux_policy::limits::{self, Limits};

/// O que a máquina oferece para o Docker sem root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Support;

fn in_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// O binário do registry (projeto distribution) para o espelho do Docker Hub:
/// `registry`, ou `docker-registry`, nome do pacote no Debian e no Ubuntu.
pub fn registry_binary() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    ["registry", "docker-registry"].iter().find_map(|b| {
        std::env::split_paths(&path)
            .map(|d| d.join(b))
            .find(|p| p.is_file())
    })
}

/// Configuração do espelho pull-through do Docker Hub.
pub fn registry_config(storage: &Path, port: u16) -> String {
    format!(
        "version: 0.1\nlog:\n  level: warn\nstorage:\n  filesystem:\n    rootdirectory: {}\n  delete:\n    enabled: true\nhttp:\n  addr: 127.0.0.1:{port}\nproxy:\n  remoteurl: https://registry-1.docker.io\n",
        storage.display()
    )
}

/// A faixa subordinada do usuário existe em `/etc/subuid` ou `/etc/subgid`
/// (pelo nome ou pelo UID)?
pub fn has_subid(text: &str, user: &str, uid: u32) -> bool {
    let uid = uid.to_string();
    text.lines().any(|l| {
        let mut f = l.split(':');
        let who = f.next().unwrap_or("");
        let count = f.nth(1).and_then(|c| c.trim().parse::<u64>().ok());
        (who == user || who == uid) && count.is_some_and(|c| c >= 65536)
    })
}

/// A máquina tem o necessário? `Err` diz o que falta.
pub fn support(user: &str, uid: u32) -> Result<Support, String> {
    let missing: Vec<&str> = [
        "rootlesskit",
        "dockerd",
        "containerd",
        "newuidmap",
        "newgidmap",
    ]
    .into_iter()
    .filter(|b| !in_path(b))
    .collect();
    if !missing.is_empty() {
        return Err(format!("faltam no PATH: {}", missing.join(", ")));
    }
    for f in ["/etc/subuid", "/etc/subgid"] {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        if !has_subid(&text, user, uid) {
            return Err(format!(
                "o usuário {user} não tem uma faixa de 65536 IDs em {f}"
            ));
        }
    }
    Ok(Support)
}

/// Identificador da pasta: FNV-1a de 64 bits do caminho, em hexadecimal.
pub fn engine_id(workdir: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in workdir.as_os_str().as_encoded_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Pastas do Docker de uma pasta de trabalho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// Dados persistentes (imagens, volumes).
    pub data: PathBuf,
    /// Estado de execução (sockets, pid, log), curto para os sockets.
    pub state: PathBuf,
}

impl Dirs {
    pub fn new(data_dir: &Path, runtime_dir: &Path, id: &str) -> Dirs {
        Dirs {
            data: data_dir.join("docker").join(id),
            state: runtime_dir.join("docker").join(id),
        }
    }

    pub fn socket(&self) -> PathBuf {
        self.state.join("docker.sock")
    }

    pub fn view_file(&self) -> PathBuf {
        self.state.join("view.json")
    }

    pub fn log(&self) -> PathBuf {
        self.state.join("dockerd.log")
    }

    pub fn rootlesskit(&self) -> PathBuf {
        self.state.join("rk")
    }
}

/// Comando do `dockerd`, dentro da visão.
pub fn dockerd_command(dirs: &Dirs, mirror: Option<u16>) -> Vec<String> {
    let s = |p: PathBuf| p.display().to_string();
    let mut v = vec![
        "dockerd".into(),
        "--data-root".into(),
        s(dirs.data.clone()),
        "--host".into(),
        format!("unix://{}", dirs.socket().display()),
        "--exec-root".into(),
        s(dirs.state.join("exec")),
        "--pidfile".into(),
        s(dirs.state.join("docker.pid")),
        // Portas publicadas sem endereço vão para o localhost.
        "--ip".into(),
        "127.0.0.1".into(),
        // Os contêineres ficam no cgroup do `dockerd`, dentro do escopo com
        // os tetos da pasta. Com o driver do systemd, cada contêiner ganharia
        // um escopo próprio do systemd do usuário, fora do teto.
        "--exec-opt".into(),
        "native.cgroupdriver=cgroupfs".into(),
        // Sem barra no começo, o cgroup dos contêineres nasce dentro do cgroup
        // do `dockerd`; o padrão os punha ao lado do escopo, fora do teto.
        "--cgroup-parent".into(),
        "conteineres".into(),
    ];
    // O espelho do Docker Hub, alcançado pelo gateway como serviço da máquina.
    if let Some(p) = mirror {
        v.push(format!("--registry-mirror=http://10.0.2.254:{p}"));
    }
    v
}

/// Ambiente do `dockerd`, dentro da visão.
pub fn dockerd_env(dirs: &Dirs) -> Vec<(String, String)> {
    vec![
        (
            "PATH".into(),
            format!(
                "{}:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                sandmux_policy::compose::DOCKER_BIN
            ),
        ),
        ("HOME".into(), dirs.state.join("home").display().to_string()),
        (
            "XDG_RUNTIME_DIR".into(),
            dirs.state.join("xdg").display().to_string(),
        ),
    ]
}

/// Argumentos do `rootlesskit` até o `sandmux-run --view-file`.
pub fn rootlesskit_args(dirs: &Dirs, runner: &Path) -> Vec<String> {
    vec![
        format!("--state-dir={}", dirs.rootlesskit().display()),
        // Sem rede própria: a visão cria o TUN do gateway do sandmux.
        "--net=none".into(),
        "--port-driver=builtin".into(),
        // Sem --copy-up: a visão monta o próprio /etc e o próprio /run, e o
        // copy-up troca os arquivos de /etc por links, que o lançador recusa
        // como destino de montagem.
        "--propagation=rslave".into(),
        "--pidns".into(),
        "--".into(),
        runner.display().to_string(),
        "--view-file".into(),
        dirs.view_file().display().to_string(),
    ]
}

/// Prefixo do `systemd-run` com os tetos da pasta, quando há suporte.
/// Slice do Docker de uma pasta. O teto fica nela, e não no escopo: o
/// `dockerd` sem root roda sem gerenciar cgroups ("Cgroup Driver: none"), e o
/// runc põe cada contêiner num cgroup ao lado do escopo, no pai dele.
pub fn slice_name(id: &str) -> String {
    format!("sandmux-docker-{id}.slice")
}

/// Propriedades do teto para `systemctl --user set-property` na slice.
pub fn slice_properties(limits: &Limits) -> Vec<String> {
    let mut p = Vec::new();
    if let Some(m) = limits.memory {
        p.push(format!("MemoryMax={m}"));
        p.push("MemorySwapMax=0".into());
    }
    if let Some(n) = limits.processes {
        p.push(format!("TasksMax={n}"));
    }
    p
}

pub fn scope_prefix(limits: &Limits, workdir: &Path, slice: &str) -> Vec<String> {
    let mut a = limits::scope_args(limits, &format!("sandmux: docker {}", workdir.display()));
    a.insert(a.len() - 1, format!("--slice={slice}"));
    // Os contêineres ficam em cgroups filhos do escopo.
    a.insert(a.len() - 1, "-p".into());
    a.insert(a.len() - 1, "Delegate=yes".into());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faixas_subordinadas() {
        let t = "root:100000:65536\nana:165536:65536\n1001:231072:1000\n";
        assert!(has_subid(t, "ana", 1000));
        assert!(!has_subid(t, "bia", 1002));
        assert!(!has_subid(t, "bia", 1001), "faixa pequena demais");
        assert!(has_subid("1000:100000:65536\n", "ana", 1000));
        assert!(!has_subid("", "ana", 1000));
    }

    #[test]
    fn identificador_estavel() {
        let a = engine_id(Path::new("/home/ana/proj"));
        assert_eq!(a.len(), 16);
        assert_eq!(a, engine_id(Path::new("/home/ana/proj")));
        assert_ne!(a, engine_id(Path::new("/home/ana/proj2")));
    }

    #[test]
    fn teto_na_slice() {
        let l = Limits {
            memory: Some(512 << 20),
            processes: Some(100),
        };
        assert_eq!(slice_name("abc"), "sandmux-docker-abc.slice");
        assert_eq!(
            slice_properties(&l),
            vec!["MemoryMax=536870912", "MemorySwapMax=0", "TasksMax=100"]
        );
        let a = scope_prefix(&l, Path::new("/w"), "sandmux-docker-abc.slice").join(" ");
        assert!(
            a.contains("--slice=sandmux-docker-abc.slice") && a.ends_with("--"),
            "{a}"
        );
    }

    #[test]
    fn comandos() {
        let d = Dirs::new(Path::new("/d"), Path::new("/r"), "abc");
        assert_eq!(d.data, PathBuf::from("/d/docker/abc"));
        assert_eq!(d.socket(), PathBuf::from("/r/docker/abc/docker.sock"));
        let c = dockerd_command(&d, None).join(" ");
        assert!(!c.contains("registry-mirror"), "{c}");
        assert!(c.contains("--exec-opt native.cgroupdriver=cgroupfs"), "{c}");
        assert!(c.contains("--cgroup-parent conteineres"), "{c}");
        let m = dockerd_command(&d, Some(5000)).join(" ");
        assert!(
            m.contains("--registry-mirror=http://10.0.2.254:5000"),
            "{m}"
        );
        let cfg = registry_config(Path::new("/d/cache"), 5000);
        assert!(cfg.contains("rootdirectory: /d/cache"), "{cfg}");
        assert!(cfg.contains("addr: 127.0.0.1:5000"), "{cfg}");
        assert!(
            cfg.contains("remoteurl: https://registry-1.docker.io"),
            "{cfg}"
        );
        assert!(
            c.contains("--ip 127.0.0.1") && c.contains("--data-root /d/docker/abc"),
            "{c}"
        );
        let r = rootlesskit_args(&d, Path::new("/bin/sandmux-run")).join(" ");
        assert!(r.contains("--net=none") && r.contains("--pidns"), "{r}");
        assert!(r.contains("--port-driver=builtin"), "{r}");
        assert!(
            r.ends_with("-- /bin/sandmux-run --view-file /r/docker/abc/view.json"),
            "{r}"
        );
        let s = scope_prefix(
            &Limits {
                memory: Some(1 << 30),
                processes: Some(100),
            },
            Path::new("/home/ana/proj"),
            "sandmux-docker-x.slice",
        )
        .join(" ");
        assert!(
            s.contains("--slice=sandmux-docker-x.slice -p Delegate=yes --")
                && s.contains("TasksMax=100"),
            "{s}"
        );
    }
}
