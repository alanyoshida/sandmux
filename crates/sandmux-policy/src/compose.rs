//! Composição: baseline + preset + opções viram uma especificação de
//! lançamento.

use std::path::{Path, PathBuf};

use sandmux_spec::{EnvVar, LandlockRules, LaunchSpec, Layer, Mount, SPEC_VERSION, SeccompRules};

use crate::git::{self, GitLayout};
use crate::host::Host;
use crate::layout::{Layout, layer_key};
use crate::{PolicyError, hermes_config, lists, presets, toolchains};

/// Onde fica a camada de escrita de toolchains e caches nesta execução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerMode {
    /// Camada do harness no diretório de dados.
    Persistent,
    /// Camada em memória, descartada ao sair (outra execução usa a persistente).
    Temporary,
    /// Overlay indisponível: caches próprios vazios e toolchains em somente leitura.
    ReadOnlyFallback,
}

/// Opções de `sandmux run`.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub harness: String,
    pub workdir: Option<PathBuf>,
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub pass_env: Vec<String>,
    pub repo: bool,
    pub kvm: bool,
    pub no_gpu: bool,
    pub args: Vec<String>,
    pub layer: LayerMode,
}

impl RunOptions {
    pub fn new(harness: &str) -> Self {
        Self {
            harness: harness.into(),
            workdir: None,
            ro: vec![],
            rw: vec![],
            pass_env: vec![],
            repo: false,
            kvm: false,
            no_gpu: false,
            args: vec![],
            layer: LayerMode::Persistent,
        }
    }
}

/// Resultado da composição.
#[derive(Debug, Clone)]
pub struct Composed {
    pub spec: LaunchSpec,
    /// Avisos a mostrar antes do lançamento.
    pub warnings: Vec<String>,
    /// Pastas do host que precisam existir antes do lançamento.
    pub dirs_to_create: Vec<PathBuf>,
    pub workdir: PathBuf,
}

/// Recusa o HOME, a raiz, uma pasta que contém o HOME e o diretório de dados
/// do sandmux (ou uma pasta que o contém ou que está dentro dele).
pub fn check_dangerous(host: &Host, p: &Path) -> Result<(), PolicyError> {
    let reason = if p == Path::new("/") {
        Some("é a raiz do sistema")
    } else if p == host.home {
        Some("é o HOME do usuário")
    } else if host.home.starts_with(p) {
        Some("contém o HOME do usuário")
    } else if p.starts_with(&host.data_dir) {
        Some("fica dentro do diretório de dados do sandmux")
    } else if host.data_dir.starts_with(p) {
        Some("contém o diretório de dados do sandmux")
    } else {
        None
    };
    match reason {
        Some(r) => Err(PolicyError::Dangerous {
            path: p.to_path_buf(),
            reason: r.into(),
        }),
        None => Ok(()),
    }
}

fn canonical(p: &Path, base: &Path) -> Result<PathBuf, PolicyError> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    abs.canonicalize()
        .map_err(|_| PolicyError::MissingPath(abs))
}

/// Credenciais conhecidas que o caminho é, contém ou está dentro de.
pub fn credentials_touched(host: &Host, p: &Path) -> Vec<PathBuf> {
    lists::KNOWN_CREDENTIALS
        .iter()
        .map(|c| host.h(c))
        .filter(|c| p == c || p.starts_with(c) || (c.starts_with(p) && c.exists()))
        .collect()
}

struct Builder<'a> {
    host: &'a Host,
    layout: Layout,
    harness: &'a str,
    mode: LayerMode,
    mounts: Vec<Mount>,
    write: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl Builder<'_> {
    fn bind(&mut self, source: &Path, dest: &Path, writable: bool) {
        self.mounts.push(Mount::Bind {
            source: source.to_path_buf(),
            dest: dest.to_path_buf(),
            writable,
        });
    }

    fn tmpfs(&mut self, dest: &str, mode: u32) {
        self.mounts.push(Mount::Tmpfs {
            dest: dest.into(),
            mode,
        });
    }

    fn symlink(&mut self, dest: &str, target: &str) {
        self.mounts.push(Mount::Symlink {
            dest: dest.into(),
            target: target.into(),
        });
    }

    fn cover_if_exists(&mut self, p: &Path) {
        if self.host.sys(p).symlink_metadata().is_ok() {
            self.mounts.push(Mount::Cover {
                dest: p.to_path_buf(),
            });
        }
    }

    /// Pasta com camada de escrita (toolchain ou cache).
    fn layered(&mut self, path: &Path, cache: bool) {
        let Ok(lower) = path.canonicalize() else {
            return;
        };
        let key = layer_key(&self.host.home, path);
        let m = match self.mode {
            LayerMode::Persistent => {
                let (upper, work) = self.layout.layer(self.harness, &key);
                self.dirs.push(upper.clone());
                self.dirs.push(work.clone());
                Mount::Overlay {
                    lower,
                    dest: path.to_path_buf(),
                    layer: Layer::Persistent { upper, work },
                }
            }
            LayerMode::Temporary => Mount::Overlay {
                lower,
                dest: path.to_path_buf(),
                layer: Layer::Temporary,
            },
            LayerMode::ReadOnlyFallback if cache => {
                let own = self.layout.fallback_cache(self.harness, &key);
                self.dirs.push(own.clone());
                Mount::Bind {
                    source: own,
                    dest: path.to_path_buf(),
                    writable: true,
                }
            }
            LayerMode::ReadOnlyFallback => Mount::Bind {
                source: lower,
                dest: path.to_path_buf(),
                writable: false,
            },
        };
        if !path.starts_with(&self.host.home) {
            self.write.push(path.to_path_buf());
        }
        self.mounts.push(m);
    }
}

fn passwd(host: &Host, shell: &Path) -> String {
    format!(
        "root:x:0:0:root:/root:/usr/sbin/nologin\n{}:x:{}:{}:{}:{}:{}\nnobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n",
        host.user,
        host.uid,
        host.gid,
        host.user,
        host.home.display(),
        shell.display()
    )
}

fn group(host: &Host) -> String {
    format!(
        "root:x:0:\n{}:x:{}:\nnogroup:x:65534:\n",
        host.group, host.gid
    )
}

fn gpu_devices(host: &Host) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(host.sys("/dev/dri")) {
        let mut v: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("renderD"))
            .map(|e| PathBuf::from("/dev/dri").join(e.file_name()))
            .collect();
        v.sort();
        out.extend(v);
    }
    if let Ok(rd) = std::fs::read_dir(host.sys("/dev")) {
        let mut v: Vec<PathBuf> = rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                let numbered = n
                    .strip_prefix("nvidia")
                    .is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit()));
                let extra = matches!(n.as_str(), "nvidiactl" | "nvidia-uvm" | "nvidia-uvm-tools");
                (numbered || extra).then(|| PathBuf::from("/dev").join(n))
            })
            .collect();
        v.sort();
        out.extend(v);
    }
    out
}

/// Compõe a especificação. O HOME sintético do harness precisa existir.
pub fn compose(host: &Host, opts: &RunOptions) -> Result<Composed, PolicyError> {
    if !presets::exists(&opts.harness) {
        return Err(PolicyError::UnknownHarness(opts.harness.clone()));
    }
    let plan = presets::plan(&opts.harness, host, &opts.args, &opts.pass_env)?;
    let mut warnings = Vec::new();

    let workdir = canonical(opts.workdir.as_deref().unwrap_or(&host.cwd), &host.cwd)?;
    check_dangerous(host, &workdir)?;
    let mut extras = Vec::new();
    for (list, writable) in [(&opts.ro, false), (&opts.rw, true)] {
        for p in list {
            let c = canonical(p, &host.cwd)?;
            check_dangerous(host, &c)?;
            let creds = credentials_touched(host, &c);
            if !creds.is_empty() {
                let names: Vec<String> = creds.iter().map(|c| c.display().to_string()).collect();
                warnings.push(format!(
                    "{} dá ao agente acesso a credenciais reais: {}",
                    c.display(),
                    names.join(", ")
                ));
            }
            extras.push((c, writable));
        }
    }

    let mut env_extra = Vec::new();
    for name in &opts.pass_env {
        let v = host
            .env
            .get(name)
            .ok_or_else(|| PolicyError::MissingEnv(name.clone()))?;
        env_extra.push((name.clone(), v.clone()));
    }
    if !opts.pass_env.is_empty() {
        warnings.push(format!(
            "--pass-env deixa ao alcance do agente: {}",
            opts.pass_env.join(", ")
        ));
    }

    let layout = Layout::new(&host.data_dir);
    let synthetic_home = layout.home(&opts.harness);
    let mut b = Builder {
        host,
        layout: layout.clone(),
        harness: &opts.harness,
        mode: opts.layer,
        mounts: Vec::new(),
        write: Vec::new(),
        dirs: Vec::new(),
    };

    // Sistema em somente leitura.
    for d in lists::SYSTEM_DIRS {
        let p = host.sys(d);
        if let Ok(target) = std::fs::read_link(&p) {
            b.mounts.push(Mount::Symlink {
                dest: PathBuf::from(d),
                target,
            });
        } else if p.is_dir() {
            b.bind(Path::new(d), Path::new(d), false);
        }
    }
    if host.sys("/sys").is_dir() {
        b.bind(Path::new("/sys"), Path::new("/sys"), false);
    }
    b.mounts.push(Mount::File {
        dest: "/etc/passwd".into(),
        contents: passwd(host, &plan.shell),
        mode: 0o644,
    });
    b.mounts.push(Mount::File {
        dest: "/etc/group".into(),
        contents: group(host),
        mode: 0o644,
    });
    for p in lists::ETC_PRIVILEGE
        .iter()
        .chain(lists::ETC_SERVICE_SECRETS)
        .chain(lists::ETC_NETWORK)
    {
        b.cover_if_exists(Path::new(p));
    }
    if let Ok(rd) = std::fs::read_dir(host.sys("/etc/ssh")) {
        let mut keys: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("ssh_host_") && n.ends_with("_key"))
            .map(|n| PathBuf::from("/etc/ssh").join(n))
            .collect();
        keys.sort();
        for k in keys {
            b.mounts.push(Mount::Cover { dest: k });
        }
    }

    // /proc, /dev, áreas temporárias.
    b.mounts.push(Mount::Proc {
        dest: "/proc".into(),
    });
    b.tmpfs("/dev", 0o755);
    for d in ["null", "zero", "full", "random", "urandom", "tty"] {
        let p = PathBuf::from("/dev").join(d);
        if host.sys(&p).exists() {
            b.mounts.push(Mount::Device {
                source: p.clone(),
                dest: p,
            });
        }
    }
    b.mounts.push(Mount::Devpts {
        dest: "/dev/pts".into(),
    });
    b.symlink("/dev/ptmx", "pts/ptmx");
    b.tmpfs("/dev/shm", 0o1777);
    b.symlink("/dev/fd", "/proc/self/fd");
    b.symlink("/dev/stdin", "/proc/self/fd/0");
    b.symlink("/dev/stdout", "/proc/self/fd/1");
    b.symlink("/dev/stderr", "/proc/self/fd/2");
    if !opts.no_gpu {
        for dev in gpu_devices(host) {
            b.mounts.push(Mount::Device {
                source: dev.clone(),
                dest: dev,
            });
        }
    }
    if opts.kvm {
        if host.sys("/dev/kvm").exists() {
            b.mounts.push(Mount::Device {
                source: "/dev/kvm".into(),
                dest: "/dev/kvm".into(),
            });
        } else {
            warnings.push("--kvm pedido, mas a máquina não tem /dev/kvm".into());
        }
    }
    b.tmpfs("/tmp", 0o1777);
    b.tmpfs("/run", 0o755);
    b.tmpfs("/var/tmp", 0o1777);
    if let Ok(target) = host.sys("/etc/resolv.conf").canonicalize() {
        let real = if host.sys_root == Path::new("/") {
            target
        } else {
            Path::new("/").join(target.strip_prefix(&host.sys_root).unwrap_or(&target))
        };
        if real.starts_with("/run") {
            b.bind(&real, &real, false);
        }
    }

    // HOME sintético e pasta de entrada.
    b.bind(&synthetic_home, &host.home, true);
    let inbox = layout.inbox();
    b.dirs.push(inbox.clone());
    b.bind(&inbox, &host.h("inbox"), false);

    // Toolchains e caches.
    let chains = toolchains::detect(host);
    let mut path_dirs = vec![host.h(".local/bin")];
    for t in &chains {
        for d in &t.dirs {
            b.layered(d, false);
        }
        for f in &t.files {
            if let Ok(real) = f.canonicalize() {
                b.bind(&real, f, false);
            }
        }
        for c in &t.covers {
            b.cover_if_exists(c);
        }
        path_dirs.extend(t.path.iter().cloned());
    }
    for c in lists::CACHES {
        let p = host.h(c);
        if p.is_dir() {
            b.layered(&p, true);
        }
    }
    for c in lists::CACHE_COVERS {
        b.cover_if_exists(&host.h(c));
    }
    if opts.layer == LayerMode::ReadOnlyFallback {
        warnings.push(
            "a camada de escrita não está disponível nesta máquina: caches começam vazios e as toolchains ficam em somente leitura".into(),
        );
    }
    if opts.layer == LayerMode::Temporary {
        warnings.push(format!(
            "outra execução do {} já usa a camada de escrita; o que for instalado ou baixado agora não será guardado",
            opts.harness
        ));
    }

    // Código do harness.
    for d in &plan.layered {
        b.layered(d, false);
    }
    b.mounts.extend(plan.mounts.iter().cloned());
    // Hermes com --pass-env: `.env` só desta execução, por cima do sintético.
    if opts.harness == "hermes" && !env_extra.is_empty() {
        let existing = std::fs::read(synthetic_home.join(".hermes/.env"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        b.mounts.push(Mount::File {
            dest: host.h(".hermes/.env"),
            contents: hermes_config::env_with_values(&existing, &env_extra)?,
            mode: 0o600,
        });
        warnings.push(
            "nesta execução o ~/.hermes/.env tem os valores de --pass-env e fica em somente leitura; o .env guardado continua com os valores falsos".into(),
        );
    }

    // Pasta de trabalho e git.
    let mut workdir_rw = vec![workdir.clone()];
    match git::detect(&workdir) {
        GitLayout::None => b.bind(&workdir, &workdir, true),
        GitLayout::Repo { git_dir } => {
            b.bind(&workdir, &workdir, true);
            let (m, create) = git::protect(&git_dir);
            b.dirs.extend(create);
            b.mounts.extend(m);
        }
        GitLayout::Worktree {
            git_file,
            common_dir,
        } => {
            b.bind(&workdir, &workdir, true);
            b.bind(&git_file, &git_file, false);
            let (m, create) = git::protect(&common_dir);
            b.dirs.extend(create);
            b.mounts.extend(m);
            workdir_rw.push(common_dir);
        }
        GitLayout::Subfolder { repo_root, git_dir } => {
            if opts.repo {
                check_dangerous(host, &repo_root)?;
                b.bind(&repo_root, &repo_root, false);
                if git_dir.is_dir() {
                    let (m, create) = git::protect(&git_dir);
                    b.dirs.extend(create);
                    b.mounts.extend(m);
                    workdir_rw.push(git_dir);
                }
                b.bind(&workdir, &workdir, true);
            } else {
                warnings.push(format!(
                    "a pasta de trabalho fica dentro do repositório {}; o git não funciona no sandbox (use --repo para montar o repositório inteiro em somente leitura)",
                    repo_root.display()
                ));
                b.bind(&workdir, &workdir, true);
            }
        }
    }

    for (p, writable) in &extras {
        b.bind(p, p, *writable);
        if *writable {
            workdir_rw.push(p.clone());
        }
    }

    // Ambiente.
    let mut env: Vec<(String, String)> = Vec::new();
    for k in lists::SAFE_ENV {
        if let Some(v) = host.env.get(*k) {
            env.push(((*k).into(), v.clone()));
        }
    }
    for (k, v) in &host.env {
        if k.starts_with("LC_") {
            env.push((k.clone(), v.clone()));
        }
    }
    let home = host.home.to_string_lossy().into_owned();
    let mut seen = std::collections::BTreeSet::new();
    let path: Vec<String> = path_dirs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .chain(lists::SYSTEM_PATH.iter().map(|s| s.to_string()))
        .filter(|p| seen.insert(p.clone()))
        .collect();
    env.extend([
        ("USER".into(), host.user.clone()),
        ("LOGNAME".into(), host.user.clone()),
        ("SHELL".into(), plan.shell.to_string_lossy().into_owned()),
        ("HOME".into(), home.clone()),
        ("PATH".into(), path.join(":")),
        ("XDG_CONFIG_HOME".into(), format!("{home}/.config")),
        ("XDG_DATA_HOME".into(), format!("{home}/.local/share")),
        ("XDG_CACHE_HOME".into(), format!("{home}/.cache")),
        ("XDG_STATE_HOME".into(), format!("{home}/.local/state")),
        ("SANDMUX_INBOX".into(), format!("{home}/inbox")),
    ]);
    env.extend(plan.env.iter().cloned());
    env.extend(env_extra);
    // A última definição de cada nome vale.
    let mut dedup: Vec<EnvVar> = Vec::new();
    for (name, value) in env {
        dedup.retain(|v| v.name != name);
        dedup.push(EnvVar { name, value });
    }

    let mut write = vec![host.home.clone()];
    write.extend(workdir_rw);
    write.extend(
        ["/tmp", "/var/tmp", "/run", "/dev", "/proc"]
            .iter()
            .map(PathBuf::from),
    );
    write.extend(b.write.iter().cloned());

    let spec = LaunchSpec {
        version: SPEC_VERSION,
        mounts: b.mounts,
        landlock: LandlockRules {
            read: vec!["/".into()],
            write,
        },
        seccomp: SeccompRules {
            deny_syscalls: lists::DENY_SYSCALLS.iter().map(|s| s.to_string()).collect(),
            deny_ioctls: lists::DENY_IOCTLS.iter().map(|s| s.to_string()).collect(),
        },
        env: dedup,
        command: plan.command.clone(),
        cwd: workdir.clone(),
    };
    spec.validate()
        .map_err(|e| PolicyError::Io(e.to_string()))?;
    Ok(Composed {
        spec,
        warnings,
        dirs_to_create: b.dirs,
        workdir,
    })
}
