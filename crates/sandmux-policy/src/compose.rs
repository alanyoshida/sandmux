//! Composição: baseline + preset + opções viram uma especificação de
//! lançamento.

use std::path::{Path, PathBuf};

use sandmux_spec::{EnvVar, LandlockRules, LaunchSpec, Layer, Mount, SPEC_VERSION, SeccompRules};
use sandmux_spec::{tr, trf};

use crate::git::{self, GitLayout};
use crate::host::Host;
use crate::layout::{Layout, layer_key};
use crate::{PolicyError, hermes_config, lists, presets, toolchains};

/// `resolv.conf` do sandbox com o gateway.
pub const RESOLV_CONF: &str = "nameserver 10.0.2.3\noptions edns0\n";
/// Nome da máquina, visto de dentro do sandbox.
pub const HOST_NAME: &str = "host.sandmux.internal";

/// Servidores de nome do host, do `resolv.conf` dele.
pub fn host_nameservers(host: &Host) -> Vec<String> {
    let text = std::fs::read_to_string(host.sys("/etc/resolv.conf")).unwrap_or_default();
    let v: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("nameserver"))
        .map(|s| s.trim().to_string())
        .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
        .collect();
    if v.is_empty() {
        vec!["127.0.0.53".into()]
    } else {
        v
    }
}

/// Onde o socket do Docker privado aparece dentro do sandbox.
pub const DOCKER_SOCKET: &str = "/run/sandmux/docker.sock";

/// Sockets do desktop que a opção `window` leva ao sandbox, já resolvidos
/// no host: o do compositor (o da aba, pelo security-context, ou o comum) e
/// os do áudio, quando existem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMounts {
    pub wayland: PathBuf,
    pub pipewire: Option<PathBuf>,
    pub pulse: Option<PathBuf>,
}

/// Permissões do opencode sem perguntas: o sandbox é a barreira.
pub const OPENCODE_ALLOW_ALL: &str = r#"{"*":"allow","edit":"allow","bash":"allow","webfetch":"allow","read":"allow","external_directory":"allow","doom_loop":"allow"}"#;

/// Opções do Claude Code sem perguntas, sem a confirmação do modo.
pub const CLAUDE_NO_ASKS: &[&str] = &[
    "--dangerously-skip-permissions",
    "--settings",
    r#"{"skipDangerousModePermissionPrompt":true}"#,
];

/// Lança o harness no modo em que ele não pede confirmação, pelo mecanismo
/// de cada um. O `shell` e o `chat` não mudam.
fn no_asks(harness: &str, plan: &mut presets::PresetPlan) {
    let after_exe = |plan: &mut presets::PresetPlan, args: &[&str]| {
        let at = 1.min(plan.command.len());
        for (i, a) in args.iter().enumerate() {
            plan.command.insert(at + i, (*a).to_string());
        }
    };
    match harness {
        "opencode" => plan
            .env
            .push(("OPENCODE_PERMISSION".into(), OPENCODE_ALLOW_ALL.into())),
        "hermes" => plan.env.push(("HERMES_YOLO_MODE".into(), "1".into())),
        "claude" => after_exe(plan, CLAUDE_NO_ASKS),
        "omp" => after_exe(plan, &["--auto-approve"]),
        "agy" => after_exe(plan, &["--dangerously-skip-permissions"]),
        _ => {}
    }
}

/// `XDG_RUNTIME_DIR` dentro do sandbox, com a opção `window`.
pub fn window_runtime_dir(uid: u32) -> PathBuf {
    PathBuf::from(format!("/run/user/{uid}"))
}

/// O que a opção `portal` monta no `XDG_RUNTIME_DIR` do sandbox: o socket
/// do barramento do sandmux e o `flatpak-info` vazio, que faz o `xdg-open`
/// abrir links pelo portal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalMounts {
    pub bus: PathBuf,
    pub marker: PathBuf,
}

/// Variáveis da opção `portal`.
pub fn portal_env(uid: u32) -> Vec<(String, String)> {
    let rt = window_runtime_dir(uid);
    vec![
        ("XDG_RUNTIME_DIR".into(), rt.to_string_lossy().into_owned()),
        (
            "DBUS_SESSION_BUS_ADDRESS".into(),
            format!("unix:path={}", rt.join("bus").display()),
        ),
        ("GIO_USE_PORTALS".into(), "1".into()),
    ]
}

/// Variáveis da opção `window`: o Wayland e as dicas para os toolkits o
/// usarem. Nunca `DISPLAY`.
pub fn window_env(uid: u32) -> Vec<(String, String)> {
    [
        (
            "XDG_RUNTIME_DIR",
            window_runtime_dir(uid).to_string_lossy().into_owned(),
        ),
        ("WAYLAND_DISPLAY", "wayland-0".into()),
        ("XDG_SESSION_TYPE", "wayland".into()),
        ("GDK_BACKEND", "wayland".into()),
        ("QT_QPA_PLATFORM", "wayland".into()),
        ("MOZ_ENABLE_WAYLAND", "1".into()),
        ("ELECTRON_OZONE_PLATFORM_HINT", "wayland".into()),
        ("SDL_VIDEODRIVER", "wayland".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}
/// Caminho do socket do agente SSH dentro do sandbox.
pub const SSH_AGENT_SOCKET: &str = "/run/sandmux/ssh-agent.sock";
/// Socket do MCP de banco e o repassador, dentro do sandbox.
pub const DB_SOCKET: &str = "/run/sandmux/db.sock";
/// O socket do barramento do portal, dentro do sandbox.
pub const PORTAL_SOCKET: &str = "/run/sandmux/portal.sock";
pub const DB_MCP: &str = "/run/sandmux/bin/sandmux-db-mcp";
/// Nome do MCP de banco no harness.
pub const DB_MCP_NAME: &str = "sandmux-db";
/// Programa das abas de chat, dentro do sandbox.
pub const CHAT_BIN: &str = "/run/sandmux/bin/sandmux-chat";
/// O `bwrap` do sandmux dentro do sandbox (o `sandmux-run` com outro nome).
pub const BWRAP_SHIM: &str = "/run/sandmux/bin/bwrap";

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
    /// Deixa o harness pedir confirmação para as ações (`harness-asks`).
    /// Sem ela, o sandbox é a barreira, e o harness roda sem perguntas.
    pub harness_asks: bool,
    pub args: Vec<String>,
    pub layer: LayerMode,
    /// Socket do Docker privado da sessão, montado em
    /// `/run/sandmux/docker.sock` com `DOCKER_HOST`. Só nas sessões do daemon.
    pub docker_socket: Option<PathBuf>,
    /// Socket do agente SSH do lançamento, montado em
    /// `/run/sandmux/ssh-agent.sock` com `SSH_AUTH_SOCK`. Só nas sessões do
    /// daemon com chaves escolhidas.
    pub ssh_agent_socket: Option<PathBuf>,
    /// Servidores das chaves SSH escolhidas, para o `known_hosts` da sessão.
    pub ssh_servers: Vec<String>,
    /// Sockets do desktop (`window`), quando a opção está ligada.
    pub window: Option<WindowMounts>,
    /// O portal do desktop (`portal`).
    pub portal: Option<PortalMounts>,
    /// Pasta do registro de arquivos (`None`: sem registro).
    pub files_log: Option<PathBuf>,
    /// Política de arquivos compilada e o arquivo que o daemon regrava.
    pub file_policy: sandmux_spec::files::FilePolicy,
    pub file_policy_file: Option<PathBuf>,
    /// Socket do MCP de banco do lançamento e o binário `sandmux-db-mcp`.
    pub db_socket: Option<PathBuf>,
    pub db_mcp: Option<PathBuf>,
    /// Binário `sandmux-chat` e as variáveis da aba de chat.
    pub chat_bin: Option<PathBuf>,
    /// O `sandmux-run`, montado como o `bwrap` do sandbox (o substituto do
    /// bubblewrap, que não monta).
    pub bwrap_bin: Option<PathBuf>,
    pub chat_env: Vec<(String, String)>,
    /// Modelos descobertos nos servidores locais do opencode: (provedor,
    /// modelos), somados ao `OPENCODE_CONFIG_CONTENT`.
    pub opencode_models: Vec<(String, Vec<String>)>,
    /// Servidores MCP desabilitados e a allowlist (não vazia: só ela).
    pub mcp_deny: Vec<String>,
    pub mcp_allow: Vec<String>,
    /// Pastas da aba em `/mnt/sandmux` e o socket de controle do supervisor.
    pub folders: Vec<sandmux_spec::LiveFolder>,
    pub control_socket: Option<PathBuf>,
    /// Modo de rede.
    pub network: crate::config::NetworkMode,
    /// Política de rede compilada (`None` no modo sem rede).
    pub net_policy: Option<sandmux_spec::net::NetPolicy>,
    /// Variáveis com valores falsos de segredos do cofre (trocados no
    /// tráfego); vencem o `pass-env` de mesmo nome.
    pub fake_env: Vec<(String, String)>,
    /// Segredos do cofre para as ferramentas (valor falso por domínio).
    pub tool_secrets: Vec<crate::toolsecrets::ToolSecret>,
    /// Registro de rede deste lançamento (pasta e etiqueta). Sem ele, o
    /// sandbox usa a rede do host (testes da composição).
    pub net: Option<NetLaunch>,
}

/// Onde o gateway registra este lançamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetLaunch {
    pub log_dir: PathBuf,
    pub tag: String,
    /// Arquivo da política atual, que o daemon regrava (só nas sessões).
    pub policy_file: Option<PathBuf>,
    /// Arquivo da lista de portas publicadas.
    pub ports_file: Option<PathBuf>,
    /// CA do lançamento, para decifrar o TLS.
    pub tls: Option<sandmux_spec::TlsSpec>,
    /// Acesso ao daemon para os segredos (só nas sessões).
    pub vault: Option<sandmux_spec::VaultAccess>,
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
            harness_asks: false,
            args: vec![],
            layer: LayerMode::Persistent,
            docker_socket: None,
            ssh_agent_socket: None,
            ssh_servers: Vec::new(),
            window: None,
            portal: None,
            files_log: None,
            file_policy: Default::default(),
            file_policy_file: None,
            folders: Vec::new(),
            control_socket: None,
            mcp_deny: Vec::new(),
            mcp_allow: Vec::new(),
            db_socket: None,
            db_mcp: None,
            chat_bin: None,
            bwrap_bin: None,
            chat_env: Vec::new(),
            opencode_models: Vec::new(),
            network: crate::config::NetworkMode::Aberta,
            net_policy: Some(sandmux_spec::net::NetPolicy::default()),
            fake_env: Vec::new(),
            tool_secrets: Vec::new(),
            net: None,
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
/// do sandmux (ou uma pasta que o contém ou que está dentro dele). Com
/// `writable`, recusa também a pasta de configuração do sandmux, uma pasta
/// dentro dela ou uma que a contém: o agente poderia alterar a própria
/// política.
pub fn check_dangerous(host: &Host, p: &Path, writable: bool) -> Result<(), PolicyError> {
    let reason = if p == Path::new("/") {
        Some(tr!("é a raiz do sistema", "it is the system root"))
    } else if p == host.home {
        Some(tr!("é o HOME do usuário", "it is the user's HOME"))
    } else if host.home.starts_with(p) {
        Some(tr!(
            "contém o HOME do usuário",
            "it contains the user's HOME"
        ))
    } else if p.starts_with(&host.data_dir) {
        Some(tr!(
            "fica dentro do diretório de dados do sandmux",
            "it is inside the sandmux data directory"
        ))
    } else if host.data_dir.starts_with(p) {
        Some(tr!(
            "contém o diretório de dados do sandmux",
            "it contains the sandmux data directory"
        ))
    } else {
        None
    };
    if let Some(r) = reason {
        return Err(PolicyError::Dangerous {
            path: p.to_path_buf(),
            reason: r.into(),
            hint: tr!("Use uma subpasta", "Use a subfolder").into(),
        });
    }
    let config = if !writable {
        None
    } else if p.starts_with(&host.config_dir) {
        Some(tr!(
            "fica dentro da pasta de configuração do sandmux",
            "it is inside the sandmux configuration folder"
        ))
    } else if host.config_dir.starts_with(p) {
        Some(tr!(
            "contém a pasta de configuração do sandmux",
            "it contains the sandmux configuration folder"
        ))
    } else {
        None
    };
    match config {
        Some(r) => Err(PolicyError::Dangerous {
            path: p.to_path_buf(),
            reason: trf!(
                "{r}, e com escrita o agente poderia alterar a configuração do sandmux",
                "{r}, and with write access the agent could change the sandmux configuration"
            ),
            hint: tr!(
                "Monte em somente leitura (--ro) ou escolha uma pasta fora dela",
                "Mount it read-only (--ro) or choose a folder outside it"
            )
            .into(),
        }),
        None => check_host_exec(host, p, writable),
    }
}

/// Arquivos e pastas do HOME que o host executa fora do sandbox: com
/// escrita neles, o agente plantaria algo que roda no próximo terminal ou
/// login.
pub const HOST_EXEC: &[&str] = &[
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".bash_logout",
    ".profile",
    ".zshrc",
    ".zprofile",
    ".zshenv",
    ".zlogin",
    ".zlogout",
    ".xprofile",
    ".xinitrc",
    ".xsession",
    ".xsessionrc",
    ".pam_environment",
    ".bashrc.d",
    ".oh-my-zsh",
    ".local/bin",
    ".local/share/applications",
    ".config/autostart",
    ".config/systemd",
    ".config/environment.d",
    ".config/fish",
    ".config/plasma-workspace",
];

/// Recusa a escrita num caminho que o host executa, dentro de um ou numa
/// pasta que contém um.
fn check_host_exec(host: &Host, p: &Path, writable: bool) -> Result<(), PolicyError> {
    if !writable {
        return Ok(());
    }
    let Some(hit) = HOST_EXEC
        .iter()
        .map(|c| host.h(c))
        .find(|c| p == c || p.starts_with(c) || c.starts_with(p))
    else {
        return Ok(());
    };
    let shown = hit
        .strip_prefix(&host.home)
        .map(|r| format!("~/{}", r.display()))
        .unwrap_or_else(|_| hit.display().to_string());
    let reason = if p == hit {
        trf!(
            "o host executa {shown} fora do sandbox",
            "the host runs {shown} outside the sandbox"
        )
    } else if p.starts_with(&hit) {
        trf!(
            "fica dentro de {shown}, que o host executa fora do sandbox",
            "it is inside {shown}, which the host runs outside the sandbox"
        )
    } else {
        trf!(
            "contém {shown}, que o host executa fora do sandbox",
            "it contains {shown}, which the host runs outside the sandbox"
        )
    };
    Err(PolicyError::Dangerous {
        path: p.to_path_buf(),
        reason: trf!(
            "{reason}; com escrita, o agente poderia plantar algo que roda no próximo terminal ou login",
            "{reason}; with write access, the agent could plant something that runs on the next terminal or login"
        ),
        hint: tr!(
            "Monte em somente leitura (--ro) ou escolha outra pasta",
            "Mount it read-only (--ro) or choose another folder"
        )
        .into(),
    })
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

    /// Sistema em somente leitura, contas geradas e arquivos sensíveis de
    /// `/etc` cobertos. Igual no sandbox e na visão do Docker privado.
    fn system(&mut self, shell: &Path) {
        for d in lists::SYSTEM_DIRS {
            let p = self.host.sys(d);
            if let Ok(target) = std::fs::read_link(&p) {
                self.mounts.push(Mount::Symlink {
                    dest: PathBuf::from(d),
                    target,
                });
            } else if p.is_dir() {
                self.bind(Path::new(d), Path::new(d), false);
            }
        }
        if self.host.sys("/sys").is_dir() {
            self.bind(Path::new("/sys"), Path::new("/sys"), false);
        }
        self.mounts.push(Mount::File {
            dest: "/etc/passwd".into(),
            contents: passwd(self.host, shell),
            mode: 0o644,
        });
        self.mounts.push(Mount::File {
            dest: "/etc/group".into(),
            contents: group(self.host),
            mode: 0o644,
        });
        for p in lists::ETC_PRIVILEGE
            .iter()
            .chain(lists::ETC_SERVICE_SECRETS)
            .chain(lists::ETC_NETWORK)
        {
            self.cover_if_exists(Path::new(p));
        }
        if let Ok(rd) = std::fs::read_dir(self.host.sys("/etc/ssh")) {
            let mut keys: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("ssh_host_") && n.ends_with("_key"))
                .map(|n| PathBuf::from("/etc/ssh").join(n))
                .collect();
            keys.sort();
            for k in keys {
                self.mounts.push(Mount::Cover { dest: k });
            }
        }
    }

    /// Pasta de trabalho com escrita e a proteção do `.git`. Devolve as pastas
    /// que precisam de escrita no Landlock.
    fn workdir(
        &mut self,
        workdir: &Path,
        repo: bool,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<PathBuf>, PolicyError> {
        let mut workdir_rw = vec![workdir.to_path_buf()];
        match git::detect(workdir) {
            GitLayout::None => self.bind(workdir, workdir, true),
            GitLayout::Repo { git_dir } => {
                self.bind(workdir, workdir, true);
                let (m, create) = git::protect(&git_dir);
                self.dirs.extend(create);
                self.mounts.extend(m);
            }
            GitLayout::Worktree {
                git_file,
                common_dir,
            } => {
                self.bind(workdir, workdir, true);
                self.bind(&git_file, &git_file, false);
                let (m, create) = git::protect(&common_dir);
                self.dirs.extend(create);
                self.mounts.extend(m);
                workdir_rw.push(common_dir);
            }
            GitLayout::Subfolder { repo_root, git_dir } => {
                if repo {
                    check_dangerous(self.host, &repo_root, false)?;
                    self.bind(&repo_root, &repo_root, false);
                    if git_dir.is_dir() {
                        let (m, create) = git::protect(&git_dir);
                        self.dirs.extend(create);
                        self.mounts.extend(m);
                        workdir_rw.push(git_dir);
                    }
                    self.bind(workdir, workdir, true);
                } else {
                    warnings.push(trf!(
                        "a pasta de trabalho fica dentro do repositório {}; o git não funciona no sandbox (use --repo para montar o repositório inteiro em somente leitura)",
                        "the work folder is inside the repository {}; git does not work in the sandbox (use --repo to mount the whole repository read-only)",
                        repo_root.display()
                    ));
                    self.bind(workdir, workdir, true);
                }
            }
        }
        Ok(workdir_rw)
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

/// O `/dev` do sandbox e da visão do Docker: um tmpfs novo com os seis nós
/// básicos do host, pseudoterminais novos, `/dev/shm` próprio e os links de
/// `fd`; a GPU e o `/dev/kvm` quando pedidos. Nenhum outro dispositivo do
/// host. Devolve falso quando o KVM foi pedido e a máquina não o tem.
fn minimal_dev(b: &mut Builder, gpu: bool, kvm: bool) -> bool {
    let host = b.host;
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
    if gpu {
        for dev in gpu_devices(host) {
            b.mounts.push(Mount::Device {
                source: dev.clone(),
                dest: dev,
            });
        }
    }
    if !kvm {
        return true;
    }
    if !host.sys("/dev/kvm").exists() {
        return false;
    }
    b.mounts.push(Mount::Device {
        source: "/dev/kvm".into(),
        dest: "/dev/kvm".into(),
    });
    true
}

/// Compõe a especificação. O HOME sintético do harness precisa existir.
pub fn compose(host: &Host, opts: &RunOptions) -> Result<Composed, PolicyError> {
    if !presets::exists(&opts.harness) && opts.harness != crate::assistant::HARNESS {
        return Err(PolicyError::UnknownHarness(opts.harness.clone()));
    }
    let mut warnings = Vec::new();
    // Uma variável com o valor falso do cofre vence o `pass-env` de mesmo nome:
    // o valor real não entra.
    let fake_names: Vec<&str> = opts.fake_env.iter().map(|(n, _)| n.as_str()).collect();
    let tool_env = crate::toolsecrets::env_for(&opts.tool_secrets);
    let replaced = |n: &str| fake_names.contains(&n) || tool_env.iter().any(|(t, _)| t == n);
    let pass_env: Vec<String> = opts
        .pass_env
        .iter()
        .filter(|n| !replaced(n))
        .cloned()
        .collect();
    if pass_env.len() != opts.pass_env.len() {
        warnings.push(trf!(
            "--pass-env ignorado para {}: o cofre tem a chave, e o agente recebe o valor falso",
            "--pass-env ignored for {}: the vault has the key, and the agent gets the fake value",
            opts.pass_env
                .iter()
                .filter(|n| replaced(n))
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    // No opencode, a configuração injetada aponta também para as variáveis
    // falsas (o `auth.json` importado venceria a variável); no Antigravity
    // CLI, a chave do Gemini falsa troca o login pela variável.
    let mut plan_names = pass_env.clone();
    if opts.harness == "opencode" || opts.harness == "agy" {
        plan_names.extend(fake_names.iter().map(|n| n.to_string()));
    }
    let mut plan = presets::plan(&opts.harness, host, &opts.args, &plan_names)?;
    if !opts.harness_asks {
        no_asks(&opts.harness, &mut plan);
    }

    let workdir = canonical(opts.workdir.as_deref().unwrap_or(&host.cwd), &host.cwd)?;
    check_dangerous(host, &workdir, true)?;
    let mut extras = Vec::new();
    for (list, writable) in [(&opts.ro, false), (&opts.rw, true)] {
        for p in list {
            let c = canonical(p, &host.cwd)?;
            check_dangerous(host, &c, writable)?;
            let creds = credentials_touched(host, &c);
            if !creds.is_empty() {
                let names: Vec<String> = creds.iter().map(|c| c.display().to_string()).collect();
                warnings.push(trf!(
                    "{} dá ao agente acesso a credenciais reais: {}",
                    "{} gives the agent access to real credentials: {}",
                    c.display(),
                    names.join(", ")
                ));
            }
            extras.push((c, writable));
        }
    }

    let mut env_extra = Vec::new();
    for name in &pass_env {
        let v = host
            .env
            .get(name)
            .ok_or_else(|| PolicyError::MissingEnv(name.clone()))?;
        env_extra.push((name.clone(), v.clone()));
    }
    if !pass_env.is_empty() {
        warnings.push(trf!(
            "--pass-env deixa ao alcance do agente: {}",
            "--pass-env puts within the agent's reach: {}",
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

    b.system(&plan.shell);

    // /proc, /dev, áreas temporárias.
    b.mounts.push(Mount::Proc {
        dest: "/proc".into(),
    });
    if !minimal_dev(&mut b, !opts.no_gpu, opts.kvm) {
        warnings.push(
            tr!(
                "--kvm pedido, mas a máquina não tem /dev/kvm",
                "--kvm requested, but the machine has no /dev/kvm"
            )
            .into(),
        );
    }
    b.tmpfs("/tmp", 0o1777);
    b.tmpfs("/run", 0o755);
    b.tmpfs("/var/tmp", 0o1777);
    // Pastas novas: liberadas para escrita desde o início, montadas ao vivo
    // pelo supervisor; as da aba, já no lançamento.
    b.tmpfs(sandmux_spec::FOLDERS_DIR, 0o755);
    for f in &opts.folders {
        let p = canonical(&f.path, &host.cwd)?;
        check_dangerous(host, &p, f.rw)?;
        b.bind(&p, &f.dest(), f.rw);
    }
    b.mounts.push(Mount::File {
        dest: Path::new(sandmux_spec::FOLDERS_DIR).join("PASTAS.txt"),
        contents: sandmux_spec::folders_text(&opts.folders),
        mode: 0o444,
    });
    let gateway = opts.net.is_some() && opts.network != crate::config::NetworkMode::SemRede;
    resolv(&mut b, host, gateway);
    // Confiança na CA do lançamento: o pacote do sistema mais a CA, por cima
    // do pacote do sistema, e as variáveis dos runtimes comuns.
    let mut tls_env = Vec::new();
    if gateway
        && let Some(t) = opts.net.as_ref().and_then(|n| n.tls.as_ref())
        && let Some(dest) = trust_ca(&mut b, host, &t.ca_cert_pem)
    {
        for v in CA_VARS {
            tls_env.push((v.to_string(), dest.display().to_string()));
        }
    }

    // HOME sintético e pasta de entrada.
    b.bind(&synthetic_home, &host.home, true);
    // Valores falsos dos segredos nas ferramentas: arquivos só desta execução
    // por cima dos guardados.
    // O lançador cria o ponto de montagem (e as pastas) quando falta.
    for (rel, contents) in crate::toolsecrets::files_for(&opts.tool_secrets, &synthetic_home) {
        b.mounts.push(Mount::File {
            dest: host.home.join(rel),
            contents: contents.clone(),
            mode: 0o600,
        });
    }
    let inbox = layout.inbox();
    b.dirs.push(inbox.clone());
    b.bind(&inbox, &host.h("inbox"), false);

    // Toolchains e caches.
    let chains = toolchains::detect(host);
    let mut path_dirs = vec![host.h(".local/bin")];
    if opts.bwrap_bin.is_some() {
        path_dirs.insert(0, PathBuf::from("/run/sandmux/bin"));
    }
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
            tr!(
                "a camada de escrita não está disponível nesta máquina: caches começam vazios e as toolchains ficam em somente leitura",
                "the write layer is not available on this machine: caches start empty and toolchains are read-only"
            )
            .into(),
        );
    }
    if opts.layer == LayerMode::Temporary {
        warnings.push(trf!(
            "outra execução do {} já usa a camada de escrita; o que for instalado ou baixado agora não será guardado",
            "another run of {} already uses the write layer; what is installed or downloaded now will not be kept",
            opts.harness
        ));
    }

    // Código do harness.
    for d in &plan.layered {
        b.layered(d, false);
    }
    b.mounts.extend(plan.mounts.iter().cloned());
    // MCPs desabilitados no hermes e o MCP de banco: um `config.yaml` só
    // desta execução.
    if opts.harness == "hermes" {
        let saved = synthetic_home.join(".hermes/config.yaml");
        let text = std::fs::read_to_string(&saved).unwrap_or_default();
        let disabled: Vec<String> = crate::mcp::hermes_servers(&text)
            .into_iter()
            .map(|s| s.name)
            .filter(|n| !crate::mcp::enabled(n, &opts.mcp_deny, &opts.mcp_allow))
            .collect();
        let mut out = None;
        if !disabled.is_empty() {
            match crate::mcp::hermes_filtered(&text, &disabled) {
                Some(f) => out = Some(f),
                None => warnings.push(trf!(
                    "MCPs não desabilitados no hermes ({}): o config.yaml não foi entendido",
                    "MCPs not disabled in hermes ({}): the config.yaml was not understood",
                    disabled.join(", ")
                )),
            }
        }
        if opts.db_socket.is_some() && opts.db_mcp.is_some() {
            let base = out.clone().unwrap_or_else(|| text.clone());
            out = Some(crate::mcp::hermes_with_server(&base, DB_MCP_NAME, DB_MCP));
        }
        if let Some(contents) = out {
            b.mounts.push(Mount::File {
                dest: host.h(".hermes/config.yaml"),
                contents,
                mode: 0o600,
            });
        }
    }
    // Hermes com --pass-env: `.env` só desta execução, por cima do sintético.
    // O hermes lê o `.env` por cima do ambiente: as chaves falsas vão nele.
    let hermes_pairs: Vec<(String, String)> = env_extra
        .iter()
        .cloned()
        .chain(opts.fake_env.iter().cloned())
        .chain(crate::toolsecrets::channel_env(&opts.tool_secrets))
        .collect();
    if opts.harness == "hermes" && !hermes_pairs.is_empty() {
        let existing = std::fs::read(synthetic_home.join(".hermes/.env"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        b.mounts.push(Mount::File {
            dest: host.h(".hermes/.env"),
            contents: hermes_config::env_with_values(&existing, &hermes_pairs)?,
            mode: 0o600,
        });
        if !env_extra.is_empty() {
            warnings.push(
                tr!(
                    "nesta execução o ~/.hermes/.env tem os valores de --pass-env e fica em somente leitura; o .env guardado continua com os valores falsos",
                    "in this run ~/.hermes/.env has the --pass-env values and is read-only; the saved .env keeps the fake values"
                )
                .into(),
            );
        }
    }

    let mut workdir_rw = b.workdir(&workdir, opts.repo, &mut warnings)?;

    if let Some(sock) = &opts.docker_socket {
        b.bind(sock, Path::new(DOCKER_SOCKET), true);
    }
    // Janela e portal: uma pasta privada em memória como `XDG_RUNTIME_DIR`,
    // com o socket do compositor e os do áudio, e o barramento do portal.
    if opts.window.is_some() || opts.portal.is_some() {
        b.tmpfs(&window_runtime_dir(host.uid).to_string_lossy(), 0o700);
    }
    // O socket do daemon fica em `/run/sandmux`; o `bus` do `XDG_RUNTIME_DIR`
    // é criado lá dentro pelo repassador do processo 1.
    if let Some(p) = &opts.portal {
        b.bind(&p.bus, Path::new(PORTAL_SOCKET), true);
        b.bind(
            &p.marker,
            &window_runtime_dir(host.uid).join("flatpak-info"),
            false,
        );
    }
    if let Some(w) = &opts.window {
        let rt = window_runtime_dir(host.uid);
        b.bind(&w.wayland, &rt.join("wayland-0"), true);
        if let Some(p) = &w.pipewire {
            b.bind(p, &rt.join("pipewire-0"), true);
        }
        if let Some(p) = &w.pulse {
            b.bind(p, &rt.join("pulse/native"), true);
        }
    }
    // MCP de banco: o socket do daemon e o repassador, em somente leitura.
    if let (Some(sock), Some(bin)) = (&opts.db_socket, &opts.db_mcp) {
        b.bind(sock, Path::new(DB_SOCKET), true);
        b.bind(bin, Path::new(DB_MCP), false);
    }
    // O `bwrap` do sandmux, na frente do `PATH` e por cima do `bwrap` do
    // sistema, se a visão tiver um.
    if let Some(bin) = &opts.bwrap_bin {
        b.bind(bin, Path::new(BWRAP_SHIM), false);
        for sys in ["/usr/bin/bwrap", "/usr/local/bin/bwrap"] {
            if host.sys(sys).is_file() {
                b.bind(bin, Path::new(sys), false);
            }
        }
    }
    // Aba de chat: o programa, em somente leitura.
    if let Some(bin) = &opts.chat_bin {
        b.bind(bin, Path::new(CHAT_BIN), false);
    }
    // Agente SSH: o socket e o `known_hosts` só desta execução, com as linhas
    // do host para os servidores das chaves.
    if let Some(sock) = &opts.ssh_agent_socket {
        b.bind(sock, Path::new(SSH_AGENT_SOCKET), true);
        // O `ssh` recusa um arquivo incluído cujo dono não é o root nem o
        // usuário, e no namespace o root do host aparece como `nobody`: os
        // arquivos de `/etc/ssh/ssh_config.d` viram cópias do usuário.
        ssh_config_copies(&mut b, host);
        let saved =
            std::fs::read_to_string(synthetic_home.join(".ssh/known_hosts")).unwrap_or_default();
        let from_host =
            std::fs::read_to_string(host.home.join(".ssh/known_hosts")).unwrap_or_default();
        b.mounts.push(Mount::File {
            dest: host.h(".ssh/known_hosts"),
            contents: crate::knownhosts::session_file(&saved, &from_host, &opts.ssh_servers),
            mode: 0o600,
        });
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
    if opts.docker_socket.is_some() {
        env.push(("DOCKER_HOST".into(), format!("unix://{DOCKER_SOCKET}")));
    }
    if opts.ssh_agent_socket.is_some() {
        env.push(("SSH_AUTH_SOCK".into(), SSH_AGENT_SOCKET.into()));
    }
    if opts.portal.is_some() {
        env.extend(portal_env(host.uid));
    }
    if opts.window.is_some() {
        // O `XDG_RUNTIME_DIR` é o mesmo dos dois.
        env.retain(|(k, _)| k != "XDG_RUNTIME_DIR");
        env.extend(window_env(host.uid));
    }
    let db = opts.db_socket.is_some() && opts.db_mcp.is_some();
    if db {
        env.push(("SANDMUX_DB_SOCKET".into(), DB_SOCKET.into()));
    }
    env.push(("SANDMUX_FOLDERS".into(), sandmux_spec::FOLDERS_DIR.into()));
    if gateway {
        env.push(("SANDMUX_HOST".into(), HOST_NAME.into()));
    }
    env.extend(tls_env);
    env.extend(env_extra);
    env.extend(opts.fake_env.iter().cloned());
    env.extend(opts.chat_env.iter().cloned());
    env.extend(tool_env);
    // MCPs desabilitados no opencode: `"enabled": false` no
    // `OPENCODE_CONFIG_CONTENT`, que vence as outras configurações.
    if opts.harness == "opencode" {
        let disabled: Vec<String> = crate::mcp::opencode_servers(&synthetic_home, &workdir)
            .into_iter()
            .map(|s| s.name)
            .filter(|n| !crate::mcp::enabled(n, &opts.mcp_deny, &opts.mcp_allow))
            .collect();
        let mut mcp: serde_json::Map<String, serde_json::Value> =
            crate::mcp::opencode_disabled_json(&disabled)
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
        if db {
            mcp.insert(
                DB_MCP_NAME.into(),
                serde_json::json!({"type": "local", "command": [DB_MCP], "enabled": true}),
            );
        }
        if !mcp.is_empty() {
            let mcp = serde_json::Value::Object(mcp).to_string();
            let current = env
                .iter()
                .rev()
                .find(|(n, _)| n == "OPENCODE_CONFIG_CONTENT")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "{}".into());
            let mut v: serde_json::Value =
                serde_json::from_str(&current).unwrap_or_else(|_| serde_json::json!({}));
            if let (Some(o), Ok(m)) = (v.as_object_mut(), serde_json::from_str(&mcp)) {
                o.insert("mcp".into(), m);
            }
            env.push(("OPENCODE_CONFIG_CONTENT".into(), v.to_string()));
        }
        // Modelos dos servidores locais, descobertos pelo daemon.
        if opts.opencode_models.iter().any(|(_, m)| !m.is_empty()) {
            let current = env
                .iter()
                .rev()
                .find(|(n, _)| n == "OPENCODE_CONFIG_CONTENT")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "{}".into());
            env.push((
                "OPENCODE_CONFIG_CONTENT".into(),
                crate::opencode_models::merge_models(&current, &opts.opencode_models),
            ));
        }
    }
    // A última definição de cada nome vale.
    let mut dedup: Vec<EnvVar> = Vec::new();
    for (name, value) in env {
        dedup.retain(|v| v.name != name);
        dedup.push(EnvVar { name, value });
    }

    let mut write = vec![host.home.clone()];
    write.extend(workdir_rw);
    write.extend(
        [
            "/tmp",
            "/var/tmp",
            "/run",
            "/dev",
            "/proc",
            sandmux_spec::FOLDERS_DIR,
        ]
        .iter()
        .map(PathBuf::from),
    );
    write.extend(b.write.iter().cloned());

    // Registro de arquivos: a pasta de trabalho, o HOME e as pastas extras;
    // as camadas e as montagens de caches e toolchains no HOME ficam fora.
    let files_spec = match (&opts.files_log, &opts.net) {
        (Some(dir), Some(n)) => {
            let mut roots = vec![workdir.clone(), host.home.clone()];
            roots.extend(extras.iter().map(|(p, _)| p.clone()));
            roots.push(PathBuf::from(sandmux_spec::FOLDERS_DIR));
            let exclude = b
                .mounts
                .iter()
                .filter_map(|m| match m {
                    Mount::Overlay { dest, .. } | Mount::Bind { dest, .. }
                        if dest.starts_with(&host.home)
                            && *dest != host.home
                            && !roots.contains(dest) =>
                    {
                        Some(dest.clone())
                    }
                    _ => None,
                })
                .collect();
            Some(sandmux_spec::FilesSpec {
                log_dir: dir.clone(),
                tag: n.tag.clone(),
                workdir: workdir.clone(),
                roots,
                exclude,
                policy: opts.file_policy.clone(),
                policy_file: opts.file_policy_file.clone(),
                state_dir: Some(dir.clone()),
                control_socket: opts.control_socket.clone(),
                folders: opts.folders.clone(),
                exec_log: true,
            })
        }
        _ => None,
    };
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
            enosys_syscalls: Vec::new(),
        },
        env: dedup,
        command: plan.command.clone(),
        cwd: workdir.clone(),
        network: opts.net.as_ref().map(|n| sandmux_spec::NetworkSpec {
            mode: match opts.network {
                crate::config::NetworkMode::SemRede => sandmux_spec::NetMode::None,
                _ => sandmux_spec::NetMode::Open,
            },
            log_dir: n.log_dir.clone(),
            tag: n.tag.clone(),
            workdir: workdir.clone(),
            nameservers: host_nameservers(host),
            policy: opts.net_policy.clone().unwrap_or_default(),
            policy_file: n.policy_file.clone(),
            ports_file: n.ports_file.clone(),
            tun_socket: None,
            tls: n.tls.clone().filter(|_| gateway),
            vault: n.vault.clone().filter(|_| gateway),
            harness: opts.harness.clone(),
        }),
        files: files_spec,
        bus_relay: opts.portal.as_ref().map(|_| sandmux_spec::BusRelay {
            listen: window_runtime_dir(host.uid).join("bus"),
            target: PORTAL_SOCKET.into(),
        }),
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

/// O `runc` da visão: põe `oomScoreAdj` 900 no `config.json` do contêiner
/// (e no `process.json` do `docker exec`), porque o `dockerd` sem root escreve
/// lá o valor dele, e sobe o do próprio processo, que o contêiner herda.
pub fn runc_wrapper(real: &str) -> String {
    format!(
        r#"#!/bin/sh
echo 900 > /proc/self/oom_score_adj 2>/dev/null
b=""; p=""; prev=""
for a in "$@"; do
  case "$prev" in --bundle|-b) b="$a" ;; --process|-p) p="$a" ;; esac
  prev="$a"
done
for f in "${{b:+$b/config.json}}" "$p"; do
  [ -n "$f" ] && [ -f "$f" ] && sed -i 's/"oomScoreAdj": *-\{{0,1\}}[0-9]*/"oomScoreAdj":900/' "$f"
done
exec {real} "$@"
"#
    )
}

/// Pasta dos executáveis próprios da visão do Docker, na frente do `PATH`.
pub const DOCKER_BIN: &str = "/run/sandmux/bin";

/// Variáveis que apontam para o pacote de CAs com a CA do lançamento.
pub const CA_VARS: &[&str] = &[
    "SSL_CERT_FILE",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "GIT_SSL_CAINFO",
];

/// Pacotes de CAs do sistema, na ordem de procura.
const BUNDLES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/cert.pem",
];

/// Troca `/etc/ssh/ssh_config.d` por uma pasta nova com cópias dos arquivos
/// `.conf` (links seguidos), do usuário do sandbox.
fn ssh_config_copies(b: &mut Builder, host: &Host) {
    let dir = host.sys("/etc/ssh/ssh_config.d");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if !name.ends_with(".conf") {
                return None;
            }
            Some((name, std::fs::read_to_string(e.path()).ok()?))
        })
        .collect();
    files.sort();
    b.tmpfs("/etc/ssh/ssh_config.d", 0o755);
    for (name, contents) in files {
        b.mounts.push(Mount::File {
            dest: Path::new("/etc/ssh/ssh_config.d").join(name),
            contents,
            mode: 0o644,
        });
    }
}

/// Cobre o pacote de CAs do sistema com ele mais `ca_pem`. Devolve o caminho
/// do pacote.
fn trust_ca(b: &mut Builder, host: &Host, ca_pem: &str) -> Option<PathBuf> {
    let (dest, mut bundle) = system_bundle(host)?;
    if !bundle.ends_with('\n') {
        bundle.push('\n');
    }
    bundle.push_str(ca_pem);
    b.mounts.push(Mount::File {
        dest: dest.clone(),
        contents: bundle,
        mode: 0o644,
    });
    Some(dest)
}

/// O pacote de CAs do sistema: o caminho real (sem links) e o conteúdo.
fn system_bundle(host: &Host) -> Option<(PathBuf, String)> {
    BUNDLES.iter().find_map(|p| {
        let real = host.sys(p).canonicalize().ok()?;
        let text = std::fs::read_to_string(&real).ok()?;
        let dest = if host.sys_root == Path::new("/") {
            real
        } else {
            Path::new("/").join(real.strip_prefix(&host.sys_root).ok()?)
        };
        Some((dest, text))
    })
}

/// O `/etc/resolv.conf`: com o gateway, um arquivo gerado com o DNS dele; sem
/// o gateway, o do host.
fn resolv(b: &mut Builder<'_>, host: &Host, gateway: bool) {
    if let Ok(target) = host.sys("/etc/resolv.conf").canonicalize() {
        let real = if host.sys_root == Path::new("/") {
            target
        } else {
            Path::new("/").join(target.strip_prefix(&host.sys_root).unwrap_or(&target))
        };
        if gateway {
            // O DNS é o do gateway. Um link para dentro de /run recebe o
            // arquivo no destino (o /run é tmpfs); um arquivo comum é coberto
            // por um gerado.
            let dest = if real.starts_with("/run") {
                real
            } else {
                PathBuf::from("/etc/resolv.conf")
            };
            b.mounts.push(Mount::File {
                dest,
                contents: RESOLV_CONF.into(),
                mode: 0o644,
            });
        } else if real.starts_with("/run") {
            b.bind(&real, &real, false);
        }
    }
}

/// O que a visão do Docker de uma pasta recebe da configuração dela.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DockerView {
    /// Dispositivos de GPU (chave `gpu` da pasta).
    pub gpu: bool,
    /// `/dev/kvm` (chave `kvm` da pasta).
    pub kvm: bool,
    /// O Docker roda num cgroup namespace próprio (o do `rootlesskit`): a
    /// visão monta um cgroup2 novo, cuja raiz é o cgroup do Docker. Falso só
    /// nos testes do modo de visão, que rodam sem cgroup namespace.
    pub cgroup: bool,
    /// Arquivos e pastas da pasta de trabalho cobertos na visão: vazios e em
    /// somente leitura ([`docker_covers`]).
    pub covers: Vec<PathBuf>,
}

/// Teto de coberturas da visão do Docker: cada uma é uma montagem, copiada a
/// cada contêiner criado.
pub const MAX_DOCKER_COVERS: usize = 4096;

/// As coberturas da visão do Docker de uma pasta: cada arquivo da pasta de
/// trabalho com veredito de segredo ou de negação pela política de arquivos
/// (a mesma decisão do supervisor), e cada pasta negada, sem descer nela. Os
/// links simbólicos são pulados: não dá para montar por cima de um link, e o
/// alvo, se estiver na pasta, é decidido pelo próprio caminho. Acima de
/// [`MAX_DOCKER_COVERS`], devolve `Err` com a contagem.
pub fn docker_covers(
    policy: &sandmux_spec::files::FilePolicy,
    created: &std::collections::HashSet<PathBuf>,
    workdir: &Path,
) -> Result<Vec<PathBuf>, usize> {
    use sandmux_spec::files::FileVerdict;
    let mut covers = Vec::new();
    let mut stack = vec![workdir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            let path = e.path();
            match policy.decide(&path, |q| created.contains(q)) {
                FileVerdict::Deny(_) | FileVerdict::Secret => covers.push(path),
                FileVerdict::Pass if t.is_dir() => stack.push(path),
                FileVerdict::Pass => {}
            }
        }
    }
    if covers.len() > MAX_DOCKER_COVERS {
        return Err(covers.len());
    }
    covers.sort();
    Ok(covers)
}

/// O que o confinamento do Docker por eBPF marca, tirado da mesma fonte das
/// montagens da visão: as coberturas (negadas) e a proteção do `.git` de
/// `git::protect` (os binds em somente leitura, protegidos contra escrita, e
/// os diretórios git).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DockerMarks {
    pub deny: Vec<PathBuf>,
    pub protect: Vec<PathBuf>,
    pub gitdir: Vec<PathBuf>,
}

impl DockerMarks {
    pub fn len(&self) -> usize {
        self.deny.len() + self.protect.len() + self.gitdir.len()
    }

    /// Acrescenta às pastas negadas e protegidas tudo o que existe dentro
    /// delas (sem seguir links simbólicos): o kernel decide pelo próprio inode
    /// e pelo diretório de cada operação, sem subir pela árvore. Acima de
    /// `max` marcas, devolve `Err` com a contagem.
    pub fn expand(&mut self, max: usize) -> Result<(), usize> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>, max: usize) -> bool {
            let mut stack = vec![dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let Ok(t) = e.file_type() else { continue };
                    let p = e.path();
                    if t.is_dir() {
                        stack.push(p.clone());
                    }
                    out.push(p);
                    if out.len() > max {
                        return false;
                    }
                }
            }
            true
        }
        let mut n = self.len();
        for v in [&mut self.deny, &mut self.protect] {
            let mut more = Vec::new();
            for p in v.iter() {
                let is_dir = std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
                if is_dir && !walk(p, &mut more, max.saturating_sub(n)) {
                    return Err(n + more.len());
                }
            }
            n += more.len();
            v.extend(more);
            v.sort();
            v.dedup();
        }
        if self.len() > max {
            return Err(self.len());
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// As marcas do confinamento para a visão `spec` do Docker da pasta
/// `workdir` (já canônica).
pub fn docker_marks(spec: &LaunchSpec, workdir: &Path) -> DockerMarks {
    let mut m = DockerMarks::default();
    for x in &spec.mounts {
        if let Mount::Cover { dest } = x {
            m.deny.push(dest.clone());
        }
    }
    let git_dir = match git::detect(workdir) {
        GitLayout::Repo { git_dir } => Some(git_dir),
        GitLayout::Worktree {
            git_file,
            common_dir,
        } => {
            m.protect.push(git_file);
            Some(common_dir)
        }
        GitLayout::None | GitLayout::Subfolder { .. } => None,
    };
    if let Some(g) = git_dir {
        for x in git::protect(&g).0 {
            if let Mount::Bind {
                source, writable, ..
            } = x
            {
                if writable {
                    m.gitdir.push(source);
                } else {
                    m.protect.push(source);
                }
            }
        }
    }
    for v in [&mut m.deny, &mut m.protect, &mut m.gitdir] {
        v.sort();
        v.dedup();
    }
    m
}

/// Marca de um caminho no confinamento do Docker: os bits de
/// `sandmux_ebpf` (1 nega, 2 protege, 4 diretório git), por caminho.
pub type MarkSet = std::collections::BTreeMap<PathBuf, u32>;

/// As marcas como conjunto, com os bits de cada caminho.
pub fn mark_set(m: &DockerMarks) -> MarkSet {
    let mut s = MarkSet::new();
    for (v, bit) in [(&m.deny, 1u32), (&m.protect, 2), (&m.gitdir, 4)] {
        for p in v {
            *s.entry(p.clone()).or_default() |= bit;
        }
    }
    s
}

/// O que muda de `old` para `new`: os bits a acrescentar e a tirar, por
/// caminho.
pub fn mark_diff(old: &MarkSet, new: &MarkSet) -> (MarkSet, MarkSet) {
    let mut add = MarkSet::new();
    let mut del = MarkSet::new();
    for (p, &b) in new {
        let o = old.get(p).copied().unwrap_or(0);
        if b & !o != 0 {
            add.insert(p.clone(), b & !o);
        }
    }
    for (p, &o) in old {
        let b = new.get(p).copied().unwrap_or(0);
        if o & !b != 0 {
            del.insert(p.clone(), o & !b);
        }
    }
    (add, del)
}

/// Um caminho que apareceu na pasta de trabalho com o Docker rodando é
/// negado aos contêineres? Sim quando a política de arquivos diz que é
/// segredo (e o agente não o criou) ou negado, ou quando ele fica dentro de
/// uma das pastas negadas `denied_dirs`.
pub fn docker_mark_for(
    policy: &sandmux_spec::files::FilePolicy,
    created: impl Fn(&Path) -> bool,
    denied_dirs: &[PathBuf],
    path: &Path,
) -> bool {
    use sandmux_spec::files::FileVerdict;
    if denied_dirs.iter().any(|d| path != d && path.starts_with(d)) {
        return true;
    }
    matches!(
        policy.decide(path, created),
        FileVerdict::Deny(_) | FileVerdict::Secret
    )
}

/// Visão de arquivos do Docker privado de uma pasta: sistema em somente
/// leitura com as coberturas de `/etc`, a pasta de trabalho com a proteção do
/// `.git`, a pasta de entrada, o `/dev` mínimo do sandbox, `/proc` novo,
/// áreas temporárias e as pastas do próprio Docker (`docker_dirs`), com
/// escrita. Sem HOME, caches, toolchains nem as pastas extras das sessões: um
/// `-v` do Docker só alcança o que está aqui. Quem aplica é o `sandmux-run
/// --view-fd`, sem Landlock, com o filtro seccomp herdado
/// ([`docker_view_seccomp`]).
/// Com `ca_pem` (a CA do gateway do Docker), o pacote de CAs do sistema
/// ganha a CA, para o `dockerd` confiar na decifração.
#[allow(clippy::too_many_arguments)]
pub fn compose_docker_view(
    host: &Host,
    workdir: &Path,
    docker_dirs: &[PathBuf],
    command: Vec<String>,
    env: Vec<(String, String)>,
    tun_socket: Option<&Path>,
    ca_pem: Option<&str>,
    view: &DockerView,
) -> Result<LaunchSpec, PolicyError> {
    let workdir = canonical(workdir, &host.cwd)?;
    check_dangerous(host, &workdir, true)?;
    let layout = Layout::new(&host.data_dir);
    let mut b = Builder {
        host,
        layout: layout.clone(),
        harness: "docker",
        mode: LayerMode::ReadOnlyFallback,
        mounts: Vec::new(),
        write: Vec::new(),
        dirs: Vec::new(),
    };
    b.system(Path::new("/bin/sh"));
    b.mounts.push(Mount::Proc {
        dest: "/proc".into(),
    });
    // O runc monta nos contêineres os nós que acha no /dev desta visão (sem
    // root real, ele não cria nós): só os do sandbox. O TUN já foi criado
    // pelo `exec_view`, com o /dev do host, antes da raiz nova.
    minimal_dev(&mut b, view.gpu, view.kvm);
    // Um /sys novo, com a rede do namespace do Docker e sem o securityfs do
    // host (com ele, o dockerd tenta carregar perfis do AppArmor e falha).
    if tun_socket.is_some() {
        b.mounts.push(Mount::Sysfs {
            dest: "/sys".into(),
        });
    }
    // O cgroup do Docker: um cgroup2 novo, cuja raiz é a do cgroup namespace
    // do `rootlesskit` (o escopo da pasta, quando há tetos), e não o do host.
    // A slice com o teto fica acima, fora do alcance. O runc não cria
    // contêiner sem um cgroup montado.
    if view.cgroup {
        b.mounts.push(Mount::Cgroup2 {
            dest: "/sys/fs/cgroup".into(),
        });
    }
    b.tmpfs("/tmp", 0o1777);
    b.tmpfs("/run", 0o755);
    b.tmpfs("/var/tmp", 0o1777);
    // O `runc` da visão sobe o `oom_score_adj` para 900 antes de criar o
    // contêiner, e os processos do contêiner o herdam: no OOM do teto da
    // pasta, o kernel escolhe sempre um contêiner, e não o `dockerd`, o
    // `containerd` ou os shims (que ficam com o valor da sessão). Sem
    // privilégio, só dá para subir o valor, nunca baixar.
    if let Some(real) = ["/usr/bin/runc", "/usr/local/bin/runc", "/usr/sbin/runc"]
        .iter()
        .find(|p| host.sys(p).is_file())
    {
        b.mounts.push(Mount::File {
            dest: Path::new(DOCKER_BIN).join("runc"),
            contents: runc_wrapper(real),
            mode: 0o755,
        });
    }
    resolv(&mut b, host, tun_socket.is_some());
    if tun_socket.is_some()
        && let Some(pem) = ca_pem
    {
        trust_ca(&mut b, host, pem);
    }
    let mut warnings = Vec::new();
    b.workdir(&workdir, false, &mut warnings)?;
    // Os segredos e os caminhos negados da pasta, que o agente não abre:
    // vazios e em somente leitura para os contêineres. Um contêiner com
    // `--privileged` consegue desmontar uma cobertura (limite documentado).
    for c in &view.covers {
        b.mounts.push(Mount::Cover { dest: c.clone() });
    }
    let inbox = layout.inbox();
    if inbox.is_dir() {
        b.bind(&inbox, &inbox, false);
    }
    for d in docker_dirs {
        b.bind(d, d, true);
    }
    let spec = LaunchSpec {
        version: SPEC_VERSION,
        mounts: b.mounts,
        landlock: LandlockRules::default(),
        seccomp: docker_view_seccomp(),
        env: env
            .into_iter()
            .map(|(name, value)| EnvVar { name, value })
            .collect(),
        command,
        cwd: "/".into(),
        files: None,
        bus_relay: None,
        // A visão cria o TUN e o manda ao daemon, que roda o gateway.
        network: tun_socket.map(|t| sandmux_spec::NetworkSpec {
            mode: sandmux_spec::NetMode::Open,
            log_dir: PathBuf::new(),
            tag: String::new(),
            workdir: workdir.clone(),
            nameservers: Vec::new(),
            policy: Default::default(),
            policy_file: None,
            ports_file: None,
            tun_socket: Some(t.to_path_buf()),
            tls: None,
            vault: None,
            harness: String::new(),
        }),
    };
    spec.validate()
        .map_err(|e| PolicyError::Io(e.to_string()))?;
    Ok(spec)
}

/// O filtro seccomp da visão do Docker, herdado pelo `dockerd` e por todo
/// contêiner: o do sandbox sem as chamadas de montagem, com o chaveiro em
/// `ENOSYS`.
fn docker_view_seccomp() -> SeccompRules {
    let names = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    SeccompRules {
        deny_syscalls: lists::DENY_SYSCALLS
            .iter()
            .filter(|s| !lists::MOUNT_SYSCALLS.contains(s) && !lists::KEYRING_SYSCALLS.contains(s))
            .map(|s| s.to_string())
            .collect(),
        deny_ioctls: names(lists::DENY_IOCTLS),
        enosys_syscalls: names(lists::KEYRING_SYSCALLS),
    }
}

#[cfg(test)]
mod docker_view_tests {
    use super::*;

    #[test]
    fn visao_do_docker_tem_so_o_necessario() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().canonicalize().unwrap();
        for p in ["proj/.git/hooks", ".ssh", ".cache/pip", "dados", "estado"] {
            std::fs::create_dir_all(home.join(p)).unwrap();
        }
        std::fs::write(home.join("proj/.git/config"), "").unwrap();
        let h = Host {
            home: home.clone(),
            uid: 1000,
            gid: 1000,
            user: "ana".into(),
            group: "ana".into(),
            user_shell: "/bin/bash".into(),
            data_dir: home.join(".local/share/sandmux"),
            config_dir: home.join(".config/sandmux"),
            cwd: home.join("proj"),
            env: Default::default(),
            sys_root: "/".into(),
        };
        let spec = compose_docker_view(
            &h,
            &home.join("proj"),
            &[home.join("dados"), home.join("estado")],
            vec!["dockerd".into()],
            vec![("XDG_RUNTIME_DIR".into(), "/x".into())],
            Some(Path::new("/r/tun.sock")),
            None,
            &DockerView::default(),
        )
        .unwrap();
        let n = spec.network.as_ref().unwrap();
        assert_eq!(n.tun_socket.as_deref(), Some(Path::new("/r/tun.sock")));
        let dests: Vec<&Path> = spec.mounts.iter().map(|m| m.dest()).collect();
        let bind = |p: &Path, w: bool| {
            spec.mounts.iter().any(|m| {
                matches!(m, Mount::Bind { source, dest, writable }
                if source == p && dest == p && *writable == w)
            })
        };
        assert!(bind(&home.join("proj"), true));
        assert!(bind(&home.join("dados"), true) && bind(&home.join("estado"), true));
        // O /dev mínimo, e não o do host.
        assert!(!bind(Path::new("/dev"), true));
        assert!(
            spec.mounts
                .iter()
                .any(|m| matches!(m, Mount::Tmpfs { dest, .. } if dest == Path::new("/dev")))
        );
        assert!(
            spec.mounts
                .iter()
                .any(|m| matches!(m, Mount::Devpts { dest } if dest == Path::new("/dev/pts")))
        );
        assert!(bind(Path::new("/usr"), false));
        // Nada do cgroup do host; sem o namespace, nenhum cgroup.
        assert!(!dests.contains(&Path::new("/sys/fs/cgroup")));
        assert!(dests.contains(&Path::new("/proc")));
        assert!(dests.contains(&Path::new("/etc/shadow")) || !Path::new("/etc/shadow").exists());
        // Proteção do .git.
        assert!(
            bind(&home.join("proj/.git/hooks"), false),
            "{:?}",
            spec.mounts
        );
        // Nada do HOME além da pasta de trabalho.
        for p in [home.clone(), home.join(".ssh"), home.join(".cache/pip")] {
            assert!(!dests.contains(&p.as_path()), "{} na visão", p.display());
        }
        assert!(spec.landlock.write.is_empty());
        // O filtro herdado: o do sandbox, sem as chamadas de montagem e com o
        // chaveiro em ENOSYS.
        let s = &spec.seccomp;
        for c in [
            "bpf",
            "perf_event_open",
            "userfaultfd",
            "io_uring_setup",
            "kexec_load",
        ] {
            assert!(s.deny_syscalls.iter().any(|x| x == c), "{c}");
        }
        for c in lists::MOUNT_SYSCALLS.iter().chain(lists::KEYRING_SYSCALLS) {
            assert!(!s.deny_syscalls.iter().any(|x| x == c), "{c}");
        }
        assert_eq!(s.enosys_syscalls, ["keyctl", "add_key", "request_key"]);
        assert_eq!(s.deny_ioctls, ["TIOCSTI", "TIOCLINUX"]);
        assert_eq!(spec.command, vec!["dockerd"]);
    }

    #[test]
    fn coberturas_pela_politica_de_arquivos() {
        use sandmux_spec::files::{FilePattern, FilePolicy, default_secrets};
        let d = tempfile::tempdir().unwrap();
        let w = d.path().canonicalize().unwrap();
        for p in ["privado/fundo", "src", "cfg"] {
            std::fs::create_dir_all(w.join(p)).unwrap();
        }
        for f in [
            ".env",
            ".env.example",
            "chave.pem",
            "cfg/liberado.key",
            "src/main.rs",
            "privado/fundo/x.pem",
            "privado/a.txt",
        ] {
            std::fs::write(w.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink("/etc/passwd", w.join("link.pem")).unwrap();
        let (secrets, secret_exceptions) = default_secrets();
        let pat = |s: &str| FilePattern::parse(s, None).unwrap();
        let policy = FilePolicy {
            deny: vec![pat(&format!("{}/privado", w.display()))],
            allow: vec![pat(&format!("{}/cfg/liberado.key", w.display()))],
            secrets,
            secret_exceptions,
            workdir: w.clone(),
            exec_log: None,
        };
        let created: std::collections::HashSet<PathBuf> = [w.join("chave.pem")].into();
        let covers = docker_covers(&policy, &created, &w).unwrap();
        // O .env do usuário e a pasta negada inteira, sem descer nela; o
        // exemplo, o segredo do agente, o file-allow e o link ficam de fora.
        assert_eq!(covers, [w.join(".env"), w.join("privado")]);

        // Acima do teto, a contagem: os dois de antes e mais 4095.
        let muitos = w.join("muitos");
        std::fs::create_dir_all(&muitos).unwrap();
        for i in 0..MAX_DOCKER_COVERS - 2 {
            std::fs::write(muitos.join(format!("{i}.pem")), "").unwrap();
        }
        assert_eq!(
            docker_covers(&policy, &created, &w).map(|c| c.len()),
            Ok(MAX_DOCKER_COVERS)
        );
        std::fs::write(muitos.join("mais.pem"), "").unwrap();
        assert_eq!(
            docker_covers(&policy, &created, &w),
            Err(MAX_DOCKER_COVERS + 1)
        );
    }

    #[test]
    fn visao_com_coberturas() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = rede_host(&sys, &home);
        let spec = compose_docker_view(
            &h,
            &home.join("proj"),
            &[],
            vec!["dockerd".into()],
            Vec::new(),
            None,
            None,
            &DockerView {
                covers: vec![home.join("proj/.env")],
                ..Default::default()
            },
        )
        .unwrap();
        let w = spec
            .mounts
            .iter()
            .position(|m| matches!(m, Mount::Bind { dest, .. } if *dest == home.join("proj")))
            .unwrap();
        let c = spec
            .mounts
            .iter()
            .position(|m| matches!(m, Mount::Cover { dest } if *dest == home.join("proj/.env")))
            .unwrap();
        assert!(c > w, "a cobertura vem depois da pasta de trabalho");
    }

    #[test]
    fn marcas_da_visao_do_docker() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        let proj = home.join("proj");
        let git = proj.join(".git");
        let sub = git.join("modules/lib");
        for p in [&git.join("hooks"), &sub.join("hooks")] {
            std::fs::create_dir_all(p).unwrap();
        }
        for p in [&git, &sub] {
            std::fs::write(p.join("HEAD"), "ref: refs/heads/main\n").unwrap();
            std::fs::write(p.join("config"), "").unwrap();
        }
        std::fs::write(proj.join(".env"), "").unwrap();
        let h = rede_host(&sys, &home);
        let spec = compose_docker_view(
            &h,
            &proj,
            &[],
            vec!["dockerd".into()],
            Vec::new(),
            None,
            None,
            &DockerView {
                covers: vec![proj.join(".env")],
                ..Default::default()
            },
        )
        .unwrap();
        let m = docker_marks(&spec, &proj);
        assert!(m.deny.contains(&proj.join(".env")), "{m:?}");
        for g in [&git, &sub] {
            assert!(m.gitdir.contains(g), "{m:?}");
            assert!(m.protect.contains(&g.join("hooks")), "{m:?}");
            assert!(m.protect.contains(&g.join("config")), "{m:?}");
        }
        // Nada do sistema nem da pasta inteira como protegido.
        assert!(m.protect.iter().all(|p| p.starts_with(&git)), "{m:?}");
        assert!(!m.gitdir.contains(&proj));
        // Sem git, só as coberturas.
        let outra = home.join("outra");
        std::fs::create_dir_all(&outra).unwrap();
        let spec = compose_docker_view(
            &h,
            &outra,
            &[],
            vec!["dockerd".into()],
            Vec::new(),
            None,
            None,
            &DockerView::default(),
        )
        .unwrap();
        let m = docker_marks(&spec, &outra);
        assert!(m.protect.is_empty() && m.gitdir.is_empty(), "{m:?}");
    }

    #[test]
    fn diferenca_das_marcas() {
        let p = |s: &str| PathBuf::from(s);
        let old: MarkSet = [(p("/a"), 1), (p("/b"), 3), (p("/c"), 4)].into();
        let new: MarkSet = [(p("/a"), 1), (p("/b"), 1), (p("/d"), 1)].into();
        let (add, del) = mark_diff(&old, &new);
        assert_eq!(add, [(p("/d"), 1)].into());
        assert_eq!(del, [(p("/b"), 2), (p("/c"), 4)].into());
        let m = DockerMarks {
            deny: vec![p("/x")],
            protect: vec![p("/x"), p("/h")],
            gitdir: vec![p("/g")],
        };
        assert_eq!(
            mark_set(&m),
            [(p("/x"), 3), (p("/h"), 2), (p("/g"), 4)].into()
        );
    }

    #[test]
    fn decisao_de_um_caminho_novo() {
        let (secrets, secret_exceptions) = sandmux_spec::files::default_secrets();
        let w = PathBuf::from("/w");
        let policy = sandmux_spec::files::FilePolicy {
            secrets,
            secret_exceptions,
            workdir: w.clone(),
            deny: vec![sandmux_spec::files::FilePattern::parse("*.sql", None).unwrap()],
            ..Default::default()
        };
        let none = |_: &Path| false;
        let dirs = vec![w.join("privado")];
        assert!(docker_mark_for(&policy, none, &dirs, &w.join(".env")));
        assert!(!docker_mark_for(
            &policy,
            none,
            &dirs,
            &w.join(".env.example")
        ));
        assert!(!docker_mark_for(
            &policy,
            |p: &Path| p.ends_with(".env"),
            &dirs,
            &w.join(".env")
        ));
        assert!(docker_mark_for(&policy, none, &dirs, &w.join("dados.sql")));
        assert!(docker_mark_for(
            &policy,
            none,
            &dirs,
            &w.join("privado/novo/x.txt")
        ));
        assert!(!docker_mark_for(&policy, none, &dirs, &w.join("privado")));
        assert!(!docker_mark_for(&policy, none, &dirs, &w.join("a.txt")));
    }

    #[test]
    fn marcas_descem_nas_pastas() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let privado = base.join("privado");
        std::fs::create_dir_all(privado.join("sub")).unwrap();
        std::fs::write(privado.join("a"), "").unwrap();
        std::fs::write(privado.join("sub/b"), "").unwrap();
        std::os::unix::fs::symlink("/etc", privado.join("link")).unwrap();
        let hooks = base.join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(hooks.join("pre-commit"), "").unwrap();
        let mut m = DockerMarks {
            deny: vec![privado.clone()],
            protect: vec![hooks.clone()],
            gitdir: vec![],
        };
        m.clone().expand(100).unwrap();
        let mut x = m.clone();
        x.expand(100).unwrap();
        for p in ["a", "sub", "sub/b", "link"] {
            assert!(x.deny.contains(&privado.join(p)), "{p}: {x:?}");
        }
        // O link é marcado, não o alvo dele.
        assert!(!x.deny.iter().any(|p| p.starts_with("/etc")));
        assert!(x.protect.contains(&hooks.join("pre-commit")));
        assert_eq!(m.expand(4), Err(5));
    }

    #[test]
    fn dispositivos_da_visao_seguem_a_pasta() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        for p in ["etc", "dev/dri"] {
            std::fs::create_dir_all(sys.join(p)).unwrap();
        }
        for p in [
            "dev/null",
            "dev/tty",
            "dev/dri/renderD128",
            "dev/kvm",
            "dev/video0",
            "dev/fuse",
        ] {
            std::fs::write(sys.join(p), "").unwrap();
        }
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = rede_host(&sys, &home);
        let devices = |gpu: bool, kvm: bool| -> Vec<PathBuf> {
            compose_docker_view(
                &h,
                &home.join("proj"),
                &[],
                vec!["dockerd".into()],
                Vec::new(),
                None,
                None,
                &DockerView {
                    gpu,
                    kvm,
                    ..Default::default()
                },
            )
            .unwrap()
            .mounts
            .into_iter()
            .filter_map(|m| match m {
                Mount::Device { dest, .. } => Some(dest),
                _ => None,
            })
            .collect()
        };
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(devices(false, false), [p("/dev/null"), p("/dev/tty")]);
        assert_eq!(
            devices(true, true),
            [
                p("/dev/null"),
                p("/dev/tty"),
                p("/dev/dri/renderD128"),
                p("/dev/kvm")
            ]
        );
    }

    fn rede_host(sys: &Path, home: &Path) -> Host {
        Host {
            home: home.into(),
            uid: 1000,
            gid: 1000,
            user: "ana".into(),
            group: "ana".into(),
            user_shell: "/bin/bash".into(),
            data_dir: home.join(".local/share/sandmux"),
            config_dir: home.join(".config/sandmux"),
            cwd: home.join("proj"),
            env: Default::default(),
            sys_root: sys.into(),
        }
    }

    #[test]
    fn chaves_falsas_no_opencode_vencem_o_pass_env() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let exe = home.join(".opencode/bin/opencode");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut h = rede_host(&sys, &home);
        h.env.insert("OPENROUTER_API_KEY".into(), "sk-REAL".into());
        std::fs::create_dir_all(Layout::new(&h.data_dir).home("opencode")).unwrap();
        let mut o = RunOptions::new("opencode");
        o.layer = LayerMode::ReadOnlyFallback;
        o.pass_env = vec!["OPENROUTER_API_KEY".into()];
        o.fake_env = vec![("OPENROUTER_API_KEY".into(), "sandmux-fake-x".into())];
        let c = compose(&h, &o).unwrap();
        assert_eq!(
            c.spec.env_value("OPENROUTER_API_KEY"),
            Some("sandmux-fake-x")
        );
        assert!(
            c.spec
                .env_value("OPENCODE_CONFIG_CONTENT")
                .is_some_and(|v| v.contains("{env:OPENROUTER_API_KEY}")),
            "{:?}",
            c.spec.env
        );
        assert!(
            c.warnings.iter().any(|w| w.contains("--pass-env ignorado")),
            "{:?}",
            c.warnings
        );
        let all = format!("{:?}", c.spec);
        assert!(!all.contains("sk-REAL"), "o valor real entrou");
    }

    #[test]
    fn visao_do_docker_confia_na_ca_do_gateway() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc/ssl/certs")).unwrap();
        std::fs::write(sys.join("etc/ssl/certs/ca-certificates.crt"), "SISTEMA").unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = rede_host(&sys, &home);
        let view = |ca: Option<&str>| {
            compose_docker_view(
                &h,
                &home.join("proj"),
                &[],
                vec!["dockerd".into()],
                Vec::new(),
                Some(Path::new("/r/tun.sock")),
                ca,
                &DockerView::default(),
            )
            .unwrap()
        };
        let bundle = |spec: &LaunchSpec| {
            spec.mounts.iter().find_map(|m| match m {
                Mount::File { dest, contents, .. }
                    if dest == Path::new("/etc/ssl/certs/ca-certificates.crt") =>
                {
                    Some(contents.clone())
                }
                _ => None,
            })
        };
        assert_eq!(
            bundle(&view(Some("CA-DO-GATEWAY"))).as_deref(),
            Some("SISTEMA\nCA-DO-GATEWAY")
        );
        assert_eq!(bundle(&view(None)), None);
    }

    #[test]
    fn runc_da_visao_poe_oom_900() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let w = d.path().join("runc");
        std::fs::write(&w, runc_wrapper("/bin/echo")).unwrap();
        std::fs::set_permissions(&w, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bundle = d.path().join("b");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            bundle.join("config.json"),
            r#"{"process":{"oomScoreAdj":200,"cwd":"/"}}"#,
        )
        .unwrap();
        let proc_json = d.path().join("p.json");
        std::fs::write(&proc_json, r#"{"oomScoreAdj": -5}"#).unwrap();
        let out = std::process::Command::new(&w)
            .args(["--root", "/x", "create", "--bundle"])
            .arg(&bundle)
            .arg("id1")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("create --bundle"));
        assert_eq!(
            std::fs::read_to_string(bundle.join("config.json")).unwrap(),
            r#"{"process":{"oomScoreAdj":900,"cwd":"/"}}"#
        );
        std::process::Command::new(&w)
            .args(["exec", "--process"])
            .arg(&proc_json)
            .arg("id1")
            .output()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&proc_json).unwrap(),
            r#"{"oomScoreAdj":900}"#
        );
    }

    #[test]
    fn mcps_desabilitados_nos_harnesses() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        for (exe, dir) in [
            (".opencode/bin/opencode", ""),
            (".local/bin/hermes", ".hermes/hermes-agent"),
        ] {
            let p = home.join(exe);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            if !dir.is_empty() {
                std::fs::create_dir_all(home.join(dir)).unwrap();
            }
        }
        let h = rede_host(&sys, &home);
        let layout = Layout::new(&h.data_dir);
        let oc = layout.home("opencode").join(".config/opencode");
        std::fs::create_dir_all(&oc).unwrap();
        std::fs::write(
            oc.join("opencode.json"),
            r#"{"mcp": {"github": {"type": "remote", "url": "https://x"}, "fs": {"type": "local", "command": ["fs"]}}}"#,
        )
        .unwrap();
        let hm = layout.home("hermes").join(".hermes");
        std::fs::create_dir_all(&hm).unwrap();
        let yaml = "mcp_servers:\n  fs:\n    command: fs\n  web:\n    url: https://web\n";
        std::fs::write(hm.join("config.yaml"), yaml).unwrap();

        let mut o = RunOptions::new("opencode");
        o.layer = LayerMode::ReadOnlyFallback;
        o.mcp_deny = vec!["github".into()];
        let c = compose(&h, &o).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(c.spec.env_value("OPENCODE_CONFIG_CONTENT").unwrap()).unwrap();
        assert_eq!(v["mcp"]["github"]["enabled"], false);
        assert!(v["mcp"].get("fs").is_none());

        let mut o = RunOptions::new("hermes");
        o.layer = LayerMode::ReadOnlyFallback;
        o.mcp_allow = vec!["fs".into()];
        let c = compose(&h, &o).unwrap();
        let cfg = c
            .spec
            .mounts
            .iter()
            .find_map(|m| match m {
                Mount::File { dest, contents, .. } if *dest == h.h(".hermes/config.yaml") => {
                    Some(contents.clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(cfg.contains("fs:") && !cfg.contains("web"), "{cfg}");
        assert_eq!(
            std::fs::read_to_string(hm.join("config.yaml")).unwrap(),
            yaml
        );
        // Canais: os falsos vão no `.env` da execução, por cima do guardado.
        std::fs::write(
            hm.join(".env"),
            "TELEGRAM_BOT_TOKEN=sandmux-fake-import\nOUTRA=1\n",
        )
        .unwrap();
        let mut o = RunOptions::new("hermes");
        o.layer = LayerMode::ReadOnlyFallback;
        o.tool_secrets = vec![crate::toolsecrets::ToolSecret {
            tool: "telegram".into(),
            domain: "api.telegram.org".into(),
            fake: "0000000000:sandmux-fake-t".into(),
        }];
        let c = compose(&h, &o).unwrap();
        assert_eq!(
            c.spec.env_value("TELEGRAM_BOT_TOKEN"),
            Some("0000000000:sandmux-fake-t")
        );
        let env = c
            .spec
            .mounts
            .iter()
            .find_map(|m| match m {
                Mount::File { dest, contents, .. } if *dest == h.h(".hermes/.env") => {
                    Some(contents.clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(
            env.contains("TELEGRAM_BOT_TOKEN=\"0000000000:sandmux-fake-t\"")
                && env.contains("OUTRA=1")
                && !env.contains("sandmux-fake-import"),
            "{env}"
        );
        std::fs::remove_file(hm.join(".env")).unwrap();
        // Sem nada desabilitado, nada muda.
        let mut o = RunOptions::new("hermes");
        o.layer = LayerMode::ReadOnlyFallback;
        let c = compose(&h, &o).unwrap();
        assert!(
            !c.spec
                .mounts
                .iter()
                .any(|m| m.dest() == h.h(".hermes/config.yaml"))
        );
        // MCP de banco: socket, repassador, variável e cadastro nos dois.
        let sock = base.join("db.sock");
        let bin = base.join("sandmux-db-mcp");
        std::fs::write(&sock, "").unwrap();
        std::fs::write(&bin, "").unwrap();
        for harness in ["opencode", "hermes"] {
            let mut o = RunOptions::new(harness);
            o.layer = LayerMode::ReadOnlyFallback;
            o.db_socket = Some(sock.clone());
            o.db_mcp = Some(bin.clone());
            let c = compose(&h, &o).unwrap();
            assert_eq!(c.spec.env_value("SANDMUX_DB_SOCKET"), Some(DB_SOCKET));
            assert!(c.spec.mounts.iter().any(|m| matches!(m,
                Mount::Bind { source, dest, writable, .. }
                    if *source == bin && dest == Path::new(DB_MCP) && !*writable)));
            assert!(
                c.spec
                    .mounts
                    .iter()
                    .any(|m| m.dest() == Path::new(DB_SOCKET))
            );
            if harness == "opencode" {
                let v: serde_json::Value =
                    serde_json::from_str(c.spec.env_value("OPENCODE_CONFIG_CONTENT").unwrap())
                        .unwrap();
                assert_eq!(v["mcp"][DB_MCP_NAME]["command"][0], DB_MCP);
            } else {
                let cfg = c
                    .spec
                    .mounts
                    .iter()
                    .find_map(|m| match m {
                        Mount::File { dest, contents, .. }
                            if *dest == h.h(".hermes/config.yaml") =>
                        {
                            Some(contents.clone())
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(cfg.contains("sandmux-db:") && cfg.contains("fs:"), "{cfg}");
            }
        }
    }

    #[test]
    fn pastas_em_mnt_sandmux() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        std::fs::create_dir_all(base.join("dados")).unwrap();
        let h = rede_host(&sys, &home);
        std::fs::create_dir_all(Layout::new(&h.data_dir).home("shell")).unwrap();
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        o.folders = vec![sandmux_spec::LiveFolder {
            name: "dados".into(),
            path: base.join("dados"),
            rw: true,
        }];
        let c = compose(&h, &o).unwrap();
        let mnt = Path::new(sandmux_spec::FOLDERS_DIR);
        assert!(
            c.spec
                .mounts
                .iter()
                .any(|m| matches!(m, Mount::Tmpfs { dest, .. } if dest == mnt))
        );
        assert!(c.spec.mounts.iter().any(|m| matches!(m,
            Mount::Bind { source, dest, writable, .. }
                if *source == base.join("dados") && *dest == mnt.join("dados") && *writable)));
        let txt = c
            .spec
            .mounts
            .iter()
            .find_map(|m| match m {
                Mount::File { dest, contents, .. } if *dest == mnt.join("PASTAS.txt") => {
                    Some(contents.clone())
                }
                _ => None,
            })
            .unwrap();
        assert!(txt.contains("dados\tleitura e escrita"), "{txt}");
        assert_eq!(c.spec.env_value("SANDMUX_FOLDERS"), Some("/mnt/sandmux"));
        assert!(c.spec.landlock.write.iter().any(|w| w == mnt));
        o.folders[0].path = home.clone();
        assert!(compose(&h, &o).is_err(), "o HOME é perigoso");
    }

    #[test]
    fn janela_monta_os_sockets_e_as_variaveis() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = rede_host(&sys, &home);
        std::fs::create_dir_all(Layout::new(&h.data_dir).home("shell")).unwrap();
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        let c = compose(&h, &o).unwrap();
        for k in [
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "DISPLAY",
            "GDK_BACKEND",
        ] {
            assert_eq!(c.spec.env_value(k), None, "{k}");
        }
        let rt = window_runtime_dir(h.uid);
        assert!(!c.spec.mounts.iter().any(|m| m.dest().starts_with(&rt)));
        o.window = Some(WindowMounts {
            wayland: base.join("wl.sock"),
            pipewire: Some(base.join("pw.sock")),
            pulse: None,
        });
        let c = compose(&h, &o).unwrap();
        assert_eq!(c.spec.env_value("WAYLAND_DISPLAY"), Some("wayland-0"));
        assert_eq!(
            c.spec.env_value("XDG_RUNTIME_DIR"),
            Some(rt.to_str().unwrap())
        );
        assert_eq!(c.spec.env_value("QT_QPA_PLATFORM"), Some("wayland"));
        assert_eq!(c.spec.env_value("DISPLAY"), None);
        let binds: Vec<(PathBuf, PathBuf)> = c
            .spec
            .mounts
            .iter()
            .filter_map(|m| match m {
                Mount::Bind { source, dest, .. } if dest.starts_with(&rt) => {
                    Some((source.clone(), dest.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            binds,
            [
                (base.join("wl.sock"), rt.join("wayland-0")),
                (base.join("pw.sock"), rt.join("pipewire-0")),
            ]
        );
        assert!(
            c.spec
                .mounts
                .iter()
                .any(|m| matches!(m, Mount::Tmpfs { dest, mode: 0o700 } if *dest == rt))
        );
    }

    #[test]
    fn portal_monta_o_socket_o_marcador_e_o_repassador() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = rede_host(&sys, &home);
        std::fs::create_dir_all(Layout::new(&h.data_dir).home("shell")).unwrap();
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        let rt = window_runtime_dir(h.uid);
        let c = compose(&h, &o).unwrap();
        assert_eq!(c.spec.env_value("DBUS_SESSION_BUS_ADDRESS"), None);
        assert_eq!(c.spec.bus_relay, None);
        o.portal = Some(PortalMounts {
            bus: base.join("portal.sock"),
            marker: base.join("flatpak-info"),
        });
        let c = compose(&h, &o).unwrap();
        let bus = format!("unix:path={}", rt.join("bus").display());
        assert_eq!(
            c.spec.env_value("DBUS_SESSION_BUS_ADDRESS"),
            Some(bus.as_str())
        );
        assert_eq!(c.spec.env_value("GIO_USE_PORTALS"), Some("1"));
        assert_eq!(
            c.spec.env_value("XDG_RUNTIME_DIR"),
            Some(rt.to_str().unwrap())
        );
        // O socket do daemon fica em /run/sandmux; o `bus` é do repassador.
        assert!(c.spec.mounts.iter().any(|m| matches!(
            m,
            Mount::Bind { source, dest, .. }
                if *source == base.join("portal.sock") && dest == Path::new(PORTAL_SOCKET)
        )));
        assert!(c.spec.mounts.iter().any(|m| matches!(
            m,
            Mount::Bind { source, dest, .. }
                if *source == base.join("flatpak-info") && *dest == rt.join("flatpak-info")
        )));
        assert_eq!(
            c.spec.bus_relay,
            Some(sandmux_spec::BusRelay {
                listen: rt.join("bus"),
                target: PORTAL_SOCKET.into(),
            })
        );
        // Com a janela, a mesma pasta e um só `XDG_RUNTIME_DIR`.
        o.window = Some(WindowMounts {
            wayland: base.join("wl.sock"),
            pipewire: None,
            pulse: None,
        });
        let c = compose(&h, &o).unwrap();
        assert_eq!(
            c.spec
                .mounts
                .iter()
                .filter(|m| matches!(m, Mount::Tmpfs { dest, .. } if *dest == rt))
                .count(),
            1
        );
        assert_eq!(
            c.spec
                .env
                .iter()
                .filter(|v| v.name == "XDG_RUNTIME_DIR")
                .count(),
            1
        );
        assert_eq!(c.spec.env_value("WAYLAND_DISPLAY"), Some("wayland-0"));
    }

    #[test]
    fn agente_ssh_e_known_hosts() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(
            home.join(".ssh/known_hosts"),
            "github.com ssh-ed25519 AAAA1\noutro.com ssh-ed25519 AAAA2\n",
        )
        .unwrap();
        std::fs::create_dir_all(sys.join("etc/ssh/ssh_config.d")).unwrap();
        std::fs::write(sys.join("etc/ssh/ssh_config.d/20-x.conf"), "Host x\n").unwrap();
        std::fs::write(sys.join("etc/ssh/ssh_config.d/LEIA"), "-").unwrap();
        let h = rede_host(&sys, &home);
        std::fs::create_dir_all(Layout::new(&h.data_dir).home("shell")).unwrap();
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        let c = compose(&h, &o).unwrap();
        assert_eq!(c.spec.env_value("SSH_AUTH_SOCK"), None);
        assert!(
            !c.spec
                .mounts
                .iter()
                .any(|m| m.dest() == Path::new("/etc/ssh/ssh_config.d"))
        );
        o.ssh_agent_socket = Some(base.join("a.sock"));
        o.ssh_servers = vec!["github.com".into()];
        let c = compose(&h, &o).unwrap();
        assert_eq!(c.spec.env_value("SSH_AUTH_SOCK"), Some(SSH_AGENT_SOCKET));
        assert!(c.spec.mounts.iter().any(|m| matches!(m,
            Mount::Bind { source, dest, .. } if *source == base.join("a.sock") && dest == Path::new(SSH_AGENT_SOCKET))));
        let kh = c
            .spec
            .mounts
            .iter()
            .find_map(|m| match m {
                Mount::File { dest, contents, .. } if *dest == h.h(".ssh/known_hosts") => {
                    Some(contents.clone())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(kh, "github.com ssh-ed25519 AAAA1\n");
        // `ssh_config.d` vira uma pasta nova com cópias do usuário.
        let dests: Vec<&Path> = c.spec.mounts.iter().map(|m| m.dest()).collect();
        assert!(dests.contains(&Path::new("/etc/ssh/ssh_config.d")));
        assert!(dests.contains(&Path::new("/etc/ssh/ssh_config.d/20-x.conf")));
        assert!(!dests.contains(&Path::new("/etc/ssh/ssh_config.d/LEIA")));
    }

    #[test]
    fn falsos_nas_ferramentas() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let mut h = rede_host(&sys, &home);
        h.env.insert("GH_TOKEN".into(), "ghp_REAL".into());
        let synth = Layout::new(&h.data_dir).home("shell");
        std::fs::create_dir_all(&synth).unwrap();
        std::fs::write(synth.join(".npmrc"), "save-exact=true\n").unwrap();
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        o.pass_env = vec!["GH_TOKEN".into()];
        let ts = |tool: &str, domain: &str| crate::toolsecrets::ToolSecret {
            tool: tool.into(),
            domain: domain.into(),
            fake: "sandmux-fake-1".into(),
        };
        o.tool_secrets = vec![
            ts("gh", "github.com"),
            ts("git", "github.com"),
            ts("npm", "npm.empresa.com"),
            ts("docker", "ghcr.io"),
        ];
        let c = compose(&h, &o).unwrap();
        assert_eq!(c.spec.env_value("GH_TOKEN"), Some("sandmux-fake-1"));
        assert_eq!(
            c.spec.env_value("GIT_CONFIG_KEY_0"),
            Some("credential.https://github.com.helper")
        );
        let file = |rel: &str| {
            c.spec.mounts.iter().find_map(|m| match m {
                Mount::File { dest, contents, .. } if *dest == h.h(rel) => Some(contents.clone()),
                _ => None,
            })
        };
        assert_eq!(
            file(".npmrc").as_deref(),
            Some("save-exact=true\n//npm.empresa.com/:_authToken=sandmux-fake-1\n")
        );
        assert!(file(".docker/config.json").unwrap().contains("ghcr.io"));
        // O guardado não muda.
        assert_eq!(
            std::fs::read_to_string(synth.join(".npmrc")).unwrap(),
            "save-exact=true\n"
        );
        assert!(c.warnings.iter().any(|w| w.contains("--pass-env ignorado")));
        assert!(!format!("{:?}", c.spec).contains("ghp_REAL"));
    }

    #[test]
    fn rede_com_resolv_em_link_e_em_arquivo() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let (sys, home) = (base.join("sys"), base.join("home"));
        std::fs::create_dir_all(sys.join("etc")).unwrap();
        std::fs::create_dir_all(sys.join("run/systemd/resolve")).unwrap();
        std::fs::create_dir_all(home.join("proj")).unwrap();
        std::fs::write(
            sys.join("run/systemd/resolve/stub-resolv.conf"),
            "nameserver 127.0.0.53\nnameserver 9.9.9.9\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            "../run/systemd/resolve/stub-resolv.conf",
            sys.join("etc/resolv.conf"),
        )
        .unwrap();
        std::fs::create_dir_all(sys.join("etc/ssl/certs")).unwrap();
        std::fs::write(
            sys.join("etc/ssl/certs/ca-certificates.crt"),
            "-----BEGIN CERTIFICATE-----\nDO-SISTEMA\n-----END CERTIFICATE-----",
        )
        .unwrap();
        let h = rede_host(&sys, &home);
        let mut o = RunOptions::new("shell");
        o.layer = LayerMode::ReadOnlyFallback;
        o.net = Some(NetLaunch {
            log_dir: base.join("net"),
            tag: "run-1".into(),
            policy_file: Some("/run/p.json".into()),
            ports_file: None,
            tls: Some(sandmux_spec::TlsSpec {
                ca_cert_pem:
                    "-----BEGIN CERTIFICATE-----\nCA-DE-TESTE\n-----END CERTIFICATE-----\n".into(),
                ca_key_pem: "chave".into(),
                upstream_ca_pem: None,
            }),
            vault: None,
        });
        let c = compose(&h, &o).unwrap();
        let n = c.spec.network.as_ref().unwrap();
        assert_eq!(n.policy_file.as_deref(), Some(Path::new("/run/p.json")));
        assert_eq!(n.policy.mode, sandmux_spec::net::PolicyMode::Open);
        let file = |dest: &str| {
            c.spec.mounts.iter().any(|m| {
                matches!(m, Mount::File { dest: d, contents, .. }
                    if d == Path::new(dest) && contents == RESOLV_CONF)
            })
        };
        assert!(
            file("/run/systemd/resolve/stub-resolv.conf"),
            "{:?}",
            c.spec.mounts
        );
        let n = c.spec.network.as_ref().unwrap();
        assert_eq!(n.mode, sandmux_spec::NetMode::Open);
        assert_eq!(n.tag, "run-1");
        assert_eq!(n.nameservers, vec!["127.0.0.53", "9.9.9.9"]);
        assert_eq!(c.spec.env_value("SANDMUX_HOST"), Some(HOST_NAME));
        // O pacote de CAs: o do sistema mais a CA do lançamento, no mesmo
        // caminho, e as variáveis apontando para ele.
        let bundle = c
            .spec
            .mounts
            .iter()
            .find_map(|m| match m {
                Mount::File { dest, contents, .. }
                    if dest == Path::new("/etc/ssl/certs/ca-certificates.crt") =>
                {
                    Some(contents.clone())
                }
                _ => None,
            })
            .expect("pacote de CAs");
        assert!(
            bundle.contains("DO-SISTEMA") && bundle.contains("CA-DE-TESTE"),
            "{bundle}"
        );
        for v in CA_VARS {
            assert_eq!(
                c.spec.env_value(v),
                Some("/etc/ssl/certs/ca-certificates.crt"),
                "{v}"
            );
        }
        assert_eq!(n.tls.as_ref().unwrap().ca_key_pem, "chave");

        // Arquivo comum: coberto por um gerado.
        std::fs::remove_file(sys.join("etc/resolv.conf")).unwrap();
        std::fs::write(sys.join("etc/resolv.conf"), "nameserver 1.1.1.1\n").unwrap();
        let c = compose(&h, &o).unwrap();
        assert!(c.spec.mounts.iter().any(
            |m| matches!(m, Mount::File { dest, .. } if dest == Path::new("/etc/resolv.conf"))
        ));
        assert_eq!(
            c.spec.network.as_ref().unwrap().nameservers,
            vec!["1.1.1.1"]
        );

        // Sem rede: sem resolv do gateway nem SANDMUX_HOST.
        o.network = crate::config::NetworkMode::SemRede;
        let c = compose(&h, &o).unwrap();
        assert_eq!(
            c.spec.network.as_ref().unwrap().mode,
            sandmux_spec::NetMode::None
        );
        assert_eq!(c.spec.env_value("SANDMUX_HOST"), None);
    }

    #[test]
    fn escrita_em_caminho_que_o_host_executa() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().to_path_buf();
        let h = Host {
            home: home.clone(),
            uid: 1000,
            gid: 1000,
            user: "ana".into(),
            group: "ana".into(),
            user_shell: "/bin/bash".into(),
            data_dir: home.join(".local/share/sandmux"),
            config_dir: home.join(".config/sandmux"),
            cwd: home.clone(),
            env: Default::default(),
            sys_root: "/".into(),
        };
        for (p, w, ok) in [
            (".bashrc", true, false),
            (".bashrc", false, true),
            (".local/bin", true, false),
            (".local/bin/x", true, false),
            (".config/systemd/user", true, false),
            (".config/fish", false, true),
            ("projetos/app", true, true),
        ] {
            let r = check_dangerous(&h, &h.h(p), w);
            assert_eq!(r.is_ok(), ok, "{p} {w}: {r:?}");
        }
        let e = check_dangerous(&h, &h.h(".bashrc"), true)
            .unwrap_err()
            .to_string();
        assert!(e.contains("~/.bashrc") && e.contains("--ro"), "{e}");
    }
}
