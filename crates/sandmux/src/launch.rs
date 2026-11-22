//! Preparo de um lançamento: verificação do suporte, trava da camada,
//! composição da especificação e memfd. Usado pelo `run` sem terminal e pelo
//! daemon, para que a política seja composta num lugar só.

use std::fs::File;
use std::io::{Seek, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use sandmux_launcher::support::{self, SupportReport};
use sandmux_policy::limits::{self, Limits, MemoryLimit, ProcessLimit};
use sandmux_policy::{Host, LayerMode, Layout, compose, config, home};
use sandmux_proto::LaunchOptions;
use sandmux_spec::{tr, trf};

/// Caminho de um binário irmão (na mesma pasta do executável atual), com o
/// nome puro como recuo, para ser procurado no `PATH`.
/// Os textos do resumo da proteção (nível, camada e rede) são gravados no
/// idioma do lançamento; na exibição, vão para o idioma atual.
pub fn localize_summary(s: &str) -> String {
    const PAIRS: &[(&str, &str)] = &[
        ("persistente", "persistent"),
        (
            "temporária (outra execução usa a persistente)",
            "temporary (another run is using the persistent one)",
        ),
        (
            "recuo: caches vazios, toolchains em somente leitura",
            "fallback: empty caches, read-only toolchains",
        ),
        ("aberta, registrada", "open, logged"),
        ("só inferência, registrada", "inference only, logged"),
        (
            "custom (allowlist), registrada",
            "custom (allowlist), logged",
        ),
        ("sem rede", "no network"),
    ];
    let en = sandmux_spec::lang::is_en();
    for (pt, e) in PAIRS {
        if s == *pt || s == *e {
            return if en { e } else { pt }.to_string();
        }
    }
    let (pt, e) = ("nível ", "level ");
    if let Some(rest) = s.strip_prefix(pt).or_else(|| s.strip_prefix(e)) {
        return format!("{}{rest}", if en { e } else { pt });
    }
    s.to_string()
}

/// "Harness" das abas de shell dentro do sandbox de outra aba (Ctrl+b t,
/// `sandmux enter`): o primeiro argumento é o número da aba alvo.
pub const ENTER: &str = "dentro";

pub fn sibling(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// O lançador `sandmux-run`.
pub fn runner_path() -> PathBuf {
    sibling("sandmux-run")
}

/// Motivo curto de uma máquina sem suporte.
pub fn short_reason(r: &SupportReport) -> String {
    if !r.landlock_ok() {
        if let Some(Err(e)) = &r.helper {
            return trf!(
                "Landlock ABI {} encontrado, e o auxiliar root não completa o confinamento: {e}",
                "Landlock ABI {} found, and the root helper does not complete the confinement: {e}",
                r.landlock_abi
            );
        }
        return trf!(
            "Landlock ABI {} encontrado, exige {}",
            "Landlock ABI {} found, requires {}",
            r.landlock_abi,
            sandmux_launcher::REQUIRED_ABI
        );
    }
    match &r.userns {
        Err(e) => e.clone(),
        Ok(()) => tr!("motivo desconhecido", "unknown reason").into(),
    }
}

/// Tenta a trava da camada de escrita. O descritor sai com `CLOEXEC`; quem
/// lança o libera só no processo do lançador, que mantém a trava enquanto o
/// sandbox vive.
fn try_layer_lock(path: &Path) -> Result<Option<File>> {
    if let Some(parent) = path.parent() {
        home::private_dir(parent)?;
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| trf!("trava {}", "lock {}", path.display()))?;
    // SAFETY: flock sobre um descritor válido.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Ok(None);
    }
    Ok(Some(f))
}

/// Um lançamento pronto: a especificação num memfd e a trava da camada.
#[derive(Debug)]
pub struct Prepared {
    spec: File,
    lock: Option<File>,
    pub warnings: Vec<String>,
    pub runner: PathBuf,
    /// Escrita no clipboard liberada pela política efetiva.
    pub clipboard: bool,
    /// Nomes das variáveis de `pass-env` da política efetiva.
    pub pass_env: Vec<String>,
    /// Tetos aplicados pelo escopo do systemd; `None` sem suporte.
    pub limits: Option<Limits>,
    /// Resumo do que foi aplicado, para o painel e o `stats`.
    pub summary: sandmux_proto::msg::PolicySummary,
    /// Política de rede do lançamento (`None` sem rede).
    pub net_policy: Option<sandmux_spec::net::NetPolicy>,
    /// Descrição do escopo.
    scope_name: String,
    /// Socket da janela da aba, que precisa viver enquanto ela vive.
    pub window: Option<crate::window::WindowSocket>,
    /// Barramento do portal da aba, que também vive com ela.
    pub portal: Option<crate::portal::PortalSocket>,
}

impl Prepared {
    /// Descritores que o lançador precisa herdar.
    pub fn inherited_fds(&self) -> Vec<RawFd> {
        let mut v = vec![self.spec.as_raw_fd()];
        if let Some(l) = &self.lock {
            v.push(l.as_raw_fd());
        }
        v
    }

    /// O comando do lançador. Quem o executa precisa liberar
    /// [`Prepared::inherited_fds`] no processo filho.
    pub fn command(&self) -> Command {
        let spec_fd = self.spec.as_raw_fd().to_string();
        match &self.limits {
            Some(l) => {
                let mut c = limits::systemd_run();
                c.args(limits::scope_args(l, &self.scope_name))
                    .arg(&self.runner)
                    .arg("--spec-fd")
                    .arg(spec_fd)
                    .env(sandmux_spec::lang::ENV, sandmux_spec::lang::get().code());
                c
            }
            None => {
                let mut c = Command::new(&self.runner);
                c.arg("--spec-fd")
                    .arg(spec_fd)
                    .env(sandmux_spec::lang::ENV, sandmux_spec::lang::get().code());
                c
            }
        }
    }

    /// Substitui o processo atual pelo lançador.
    pub fn exec(self) -> anyhow::Error {
        for fd in self.inherited_fds() {
            // SAFETY: fcntl sobre descritores válidos deste processo.
            unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
        }
        let err = self.command().exec();
        anyhow!(trf!(
            "executar {}: {err}",
            "run {}: {err}",
            self.runner.display()
        ))
    }
}

/// Prepara o lançamento de `opts` para `host`. O HOME sintético é criado
/// vazio se não existir (a pergunta da importação é feita antes, por quem
/// tem terminal).
pub fn prepare(host: &Host, opts: &LaunchOptions) -> Result<Prepared> {
    // O repassador do Wayland e o barramento do portal vivem no daemon.
    if effective(host, opts)?.window.value.is_on() {
        bail!(tr!(
            "a janela só funciona nas abas do daemon: rode num terminal ou abra pela TUI",
            "the window only works in daemon tabs: run it in a terminal or open it from the TUI"
        ));
    }
    if opts.portal == Some(true) {
        bail!(tr!(
            "o portal só funciona nas abas do daemon: rode num terminal ou abra pela TUI",
            "the portal only works in daemon tabs: run it in a terminal or open it from the TUI"
        ));
    }
    // Sem o daemon, o padrão segue sem portal.
    let mut opts = opts.clone();
    opts.portal = Some(false);
    let opts = &opts;
    prepare_with(
        host,
        opts,
        None,
        &format!("run-{}", std::process::id()),
        None,
        VaultLaunch::default(),
    )
}

/// O que o cofre desbloqueado dá a um lançamento (só o daemon passa).
#[derive(Default)]
pub struct VaultLaunch {
    /// Acesso do gateway aos segredos.
    pub access: Option<sandmux_spec::VaultAccess>,
    /// Chaves falsas dos provedores de inferência.
    pub fake_env: Vec<(String, String)>,
    /// Valores falsos nas ferramentas.
    pub tool_secrets: Vec<sandmux_policy::toolsecrets::ToolSecret>,
    /// A pasta do `gcloud` da execução, com as identidades de GCP.
    pub gcloud_dir: Option<PathBuf>,
    /// A pasta do `kubectl` da execução, com as identidades de Kubernetes.
    pub kube_dir: Option<PathBuf>,
    /// Socket do agente SSH do lançamento e os servidores das chaves.
    pub ssh_agent_socket: Option<PathBuf>,
    pub ssh_servers: Vec<String>,
    /// Socket do MCP de banco do lançamento.
    pub db_socket: Option<PathBuf>,
    /// Aba de chat: o valor falso da chave de cada provedor próprio.
    pub chat_keys: Vec<(String, String)>,
    /// Modelos descobertos nos servidores locais do opencode.
    pub opencode_models: Vec<(String, Vec<String>)>,
}

/// Descobre os modelos dos servidores locais do opencode cujas portas estão
/// em `host-ports`. `real` troca os falsos do cofre pelos reais no valor da
/// autorização. Uma thread por provedor, com tempo curto.
pub fn discover_opencode_models(
    host: &Host,
    workdir: &Path,
    host_ports: &[u16],
    real: impl Fn(&str) -> String,
) -> Vec<(String, Vec<String>)> {
    let home = Layout::new(&host.data_dir).home("opencode");
    let jobs: Vec<_> = sandmux_policy::opencode_models::local_providers(&home, workdir)
        .into_iter()
        .filter(|p| host_ports.contains(&p.port))
        .map(|p| {
            let auth = p
                .authorization
                .clone()
                .or_else(|| p.api_key.as_ref().map(|k| format!("Bearer {k}")))
                .map(|a| real(&a));
            std::thread::spawn(move || {
                let m = crate::discover::models(&p.base_url, p.port, auth.as_deref());
                (p.id, m.unwrap_or_default())
            })
        })
        .collect();
    jobs.into_iter()
        .filter_map(|j| j.join().ok())
        .filter(|(_, m)| !m.is_empty())
        .collect()
}

/// Variável da chave de um provedor próprio no `sandmux-chat`.
pub fn chat_key_var(provider: &str) -> String {
    format!(
        "SANDMUX_CHAT_KEY_{}",
        provider.to_ascii_uppercase().replace('-', "_")
    )
}

/// As chaves dos provedores próprios: o falso do segredo dono do host.
pub fn chat_keys(host: &Host, fake_for: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let cfg = config::Config::load(&config::path(host)).unwrap_or_default();
    cfg.assistant
        .all_providers()
        .into_iter()
        .filter(|p| !p.is_known())
        .filter_map(|p| Some((p.name.clone(), fake_for(&p.host())?)))
        .collect()
}

/// As variáveis da aba de chat: os provedores (com a variável da chave de
/// cada um) e as chaves falsas dos provedores próprios.
pub fn chat_env(host: &Host, keys: &[(String, String)]) -> Vec<(String, String)> {
    use sandmux_policy::assistant::PROVIDERS_ENV;
    let cfg = config::Config::load(&config::path(host)).unwrap_or_default();
    let mut env = Vec::new();
    let providers: Vec<_> = cfg
        .assistant
        .all_providers()
        .into_iter()
        .map(|mut p| {
            if !p.is_known() {
                p.env = match keys.iter().find(|(n, _)| *n == p.name) {
                    Some((_, fake)) => {
                        let var = chat_key_var(&p.name);
                        env.push((var.clone(), fake.clone()));
                        var
                    }
                    None => String::new(),
                };
            }
            p
        })
        .collect();
    env.push((
        PROVIDERS_ENV.to_string(),
        serde_json::to_string(&providers).unwrap_or_default(),
    ));
    // As cores do tema ativo: a configuração não entra no sandbox.
    let theme: std::collections::BTreeMap<&str, String> = cfg
        .tui
        .theme
        .colors
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_text()))
        .collect();
    env.push((
        crate::chat::THEME_ENV.to_string(),
        serde_json::to_string(&theme).unwrap_or_default(),
    ));
    env
}

/// Como [`prepare`], com o socket do Docker privado da sessão (só o daemon
/// passa um).
pub fn prepare_with(
    host: &Host,
    opts: &LaunchOptions,
    docker_socket: Option<&Path>,
    net_tag: &str,
    policy_file: Option<&Path>,
    vault: VaultLaunch,
) -> Result<Prepared> {
    let eff = effective(host, opts)?;
    let layout = Layout::new(&host.data_dir);
    let mut warnings = Vec::new();
    for r in &eff.ports {
        if let Some(a) = r.addr {
            warnings.push(trf!(
                "a porta {} fica exposta à rede em {a}:{}, além do 127.0.0.1",
                "port {} is exposed to the network at {a}:{}, besides 127.0.0.1",
                r.port,
                r.port
            ));
        }
    }
    if !layout.home(&opts.harness).exists() {
        let setup = home::ensure(host, &layout, &opts.harness)?;
        warnings.extend(setup.warnings);
    }
    if opts.harness == "hermes" {
        sandmux_policy::hermes_config::ensure_update_check_off(&layout.home("hermes"))?;
    }

    let runner = runner_path();
    let report = support::check_via(&runner);
    if !report.can_launch() {
        bail!(trf!(
            "esta máquina não permite lançar o sandbox ({}); rode `sandmux doctor`",
            "this machine cannot launch the sandbox ({}); run `sandmux doctor`",
            short_reason(&report)
        ));
    }
    // SANDMUX_DISABLE_OVERLAY só desliga a camada (modo de recuo): nunca afrouxa.
    let overlay_ok =
        report.overlay.is_ok() && std::env::var_os("SANDMUX_DISABLE_OVERLAY").is_none();
    let lock = if overlay_ok {
        try_layer_lock(&layout.layer_lock(&opts.harness))?
    } else {
        None
    };
    let mode = match (overlay_ok, &lock) {
        (false, _) => LayerMode::ReadOnlyFallback,
        (true, Some(_)) => LayerMode::Persistent,
        (true, None) => LayerMode::Temporary,
    };
    let mut run = eff.run_options(&opts.harness, &opts.workdir, &opts.args, mode);
    run.docker_socket = docker_socket.map(Path::to_path_buf);
    run.fake_env = vault.fake_env;
    run.opencode_models = vault.opencode_models;
    run.tool_secrets = vault.tool_secrets;
    run.gcloud_dir = vault.gcloud_dir;
    run.kube_dir = vault.kube_dir;
    run.ssh_agent_socket = vault.ssh_agent_socket;
    run.ssh_servers = vault.ssh_servers;
    let window = crate::window::open(eff.window.value, &host.env, net_tag)?;
    run.window = window.as_ref().map(|w| w.mounts.clone());
    // O portal: um barramento por aba no daemon. Se ele não sobe, a aba
    // segue sem ele, com o aviso.
    let portal = if eff.portal.value {
        let tab = crate::portal::Tab {
            label: crate::portal::label(&opts.harness, &opts.workdir),
            tag: net_tag.to_string(),
            workdir: std::fs::canonicalize(&opts.workdir).unwrap_or_else(|_| opts.workdir.clone()),
            policy: eff.net_policy(),
            policy_file: policy_file.map(Path::to_path_buf),
            log_dir: crate::paths::state_dir().join("net"),
        };
        match crate::portal::open(net_tag, tab) {
            Ok(p) => Some(p),
            Err(e) => {
                warnings.push(crate::portal::warning(&e));
                None
            }
        }
    } else {
        None
    };
    run.portal = portal.as_ref().map(|p| p.mounts.clone());
    if let Some(sock) = vault.db_socket {
        // O repassador fica ao lado do `sandmux-run`.
        let bin = runner_path().with_file_name("sandmux-db-mcp");
        if bin.is_file() {
            run.db_socket = Some(sock);
            run.db_mcp = Some(bin);
        } else {
            warnings.push(trf!(
                "MCP de banco sem o repassador: falta {}",
                "database MCP without the relay: {} is missing",
                bin.display()
            ));
        }
    }
    // O substituto do bubblewrap: o próprio lançador, com outro nome.
    let shim = runner_path();
    if shim.is_file() {
        run.bwrap_bin = Some(shim);
    }
    if opts.harness == sandmux_policy::assistant::HARNESS {
        let bin = runner_path().with_file_name("sandmux-chat");
        if !bin.is_file() {
            bail!(trf!(
                "falta o programa da aba de chat: {}",
                "the chat tab program is missing: {}",
                bin.display()
            ));
        }
        run.chat_bin = Some(bin);
        run.chat_env = chat_env(host, &vault.chat_keys);
        // A aba de chat fala o idioma do sandmux.
        run.chat_env.push((
            sandmux_spec::lang::ENV.to_string(),
            sandmux_spec::lang::get().code().to_string(),
        ));
        // A tecla de prefixo, para a aba dizer como sair (Ctrl+b d).
        let prefix = config::Config::load(&config::path(host))
            .map(|c| c.tui.prefix)
            .unwrap_or('b');
        run.chat_env
            .push(("SANDMUX_PREFIX".to_string(), format!("Ctrl+{prefix}")));
    }
    run.files_log = Some(crate::paths::state_dir().join("files"));
    // A política de arquivos: no daemon, num arquivo que ele regrava quando a
    // configuração ou a aba mudam.
    let workdir = std::fs::canonicalize(&opts.workdir).unwrap_or_else(|_| opts.workdir.clone());
    run.file_policy = eff.file_policy(&host.home, &workdir);
    if policy_file.is_some() {
        let f = file_policy_path(net_tag);
        write_file_policy(&f, &run.file_policy)?;
        run.file_policy_file = Some(f);
        run.control_socket = Some(control_socket_path(net_tag));
    }
    run.folders = opts.folders.clone();
    run.net = Some(sandmux_policy::compose::NetLaunch {
        log_dir: crate::paths::state_dir().join("net"),
        tag: net_tag.to_string(),
        policy_file: policy_file.map(Path::to_path_buf),
        ports_file: Some(ports_file(net_tag)?),
        tls: launch_ca(),
        vault: vault.access,
    });
    let composed = compose(host, &run)?;
    warnings.extend(composed.warnings);
    let limits = scope_limits(&eff, &mut warnings);
    let summary = sandmux_proto::msg::PolicySummary {
        level: trf!(
            "nível 1: namespaces, Landlock ABI {}, seccomp",
            "level 1: namespaces, Landlock ABI {}, seccomp",
            report.landlock_abi
        ),
        layer: match mode {
            LayerMode::Persistent => tr!("persistente", "persistent").into(),
            LayerMode::Temporary => tr!(
                "temporária (outra execução usa a persistente)",
                "temporary (another run is using the persistent one)"
            )
            .into(),
            LayerMode::ReadOnlyFallback => tr!(
                "recuo: caches vazios, toolchains em somente leitura",
                "fallback: empty caches, read-only toolchains"
            )
            .into(),
        },
        workdir: composed.workdir.clone(),
        ro: eff.ro(),
        rw: eff.rw(),
        gpu: eff.gpu.value,
        kvm: eff.kvm.value,
        clipboard: eff.clipboard.value,
        pass_env: eff.pass_env_names(),
        limits: limits.is_some(),
        memory_max: limits.and_then(|l| l.memory),
        processes_max: limits.and_then(|l| l.processes),
        network: eff.network.value.describe().into(),
        window: eff.window.value,
        portal: portal.is_some(),
        // O daemon troca por `kernel` quando o auxiliar assume a aba.
        exec: Some(
            tr!(
                "supervisor: aguardando o escopo da aba",
                "supervisor: waiting for the tab's scope"
            )
            .into(),
        ),
        // E por `kernel` quando o lançador pede os arquivos da aba e o
        // auxiliar aceita (o motivo, quando não aceita).
        files: policy_file.is_some().then(|| {
            tr!(
                "supervisor: aguardando o lançamento",
                "supervisor: waiting for the launch"
            )
            .into()
        }),
    };
    for d in &composed.dirs_to_create {
        if d.starts_with(&host.data_dir) {
            home::private_dir(d)?;
        } else {
            std::fs::create_dir_all(d)
                .with_context(|| trf!("criar {}", "create {}", d.display()))?;
        }
    }

    let fd = nix::sys::memfd::memfd_create(c"sandmux-spec", nix::sys::memfd::MFdFlags::MFD_CLOEXEC)
        .map_err(|e| anyhow!("memfd: {e}"))?;
    let mut spec = File::from(fd);
    spec.write_all(composed.spec.to_json().as_bytes())?;
    spec.rewind()?;
    Ok(Prepared {
        spec,
        lock,
        warnings,
        runner,
        clipboard: eff.clipboard.value,
        pass_env: eff.pass_env_names(),
        limits,
        summary,
        net_policy: eff.net_policy(),
        scope_name: format!(
            "sandmux: {}:{}",
            opts.harness,
            opts.workdir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        ),
        window: window.and_then(|w| w.socket),
        portal,
    })
}

/// Os tetos da política, ou `None` com o aviso, quando a máquina não permite.
fn scope_limits(eff: &config::Effective, warnings: &mut Vec<String>) -> Option<Limits> {
    match limits::support() {
        Ok(()) => {
            let total = limits::mem_total().unwrap_or(0);
            Some(eff.limits(total))
        }
        Err(why) => {
            warnings.push(trf!(
                "esta sessão roda sem teto de memória e de processos: {why}; veja `sandmux doctor`",
                "this session runs with no memory or process cap: {why}; see `sandmux doctor`"
            ));
            None
        }
    }
}

/// CA nova para o lançamento, e a CA adicional dos destinos de
/// `SANDMUX_UPSTREAM_CA` (um arquivo PEM, como a de um servidor local com
/// HTTPS). Sem CA, o gateway não decifra nada.
pub fn launch_ca() -> Option<sandmux_spec::TlsSpec> {
    let (ca_cert_pem, ca_key_pem) = sandmux_net::tls::generate_ca().ok()?;
    let upstream_ca_pem =
        std::env::var_os("SANDMUX_UPSTREAM_CA").and_then(|p| std::fs::read_to_string(p).ok());
    Some(sandmux_spec::TlsSpec {
        ca_cert_pem,
        ca_key_pem,
        upstream_ca_pem,
    })
}

/// A pasta do `gcloud` de um lançamento (só falsos), apagada no fim dele.
pub fn gcloud_dir_path(tag: &str) -> PathBuf {
    crate::paths::runtime_dir().join("cloud").join(tag)
}

/// A pasta do `kubectl` de um lançamento (só falsos), apagada no fim dele.
pub fn kube_dir_path(tag: &str) -> PathBuf {
    crate::paths::runtime_dir().join("kube").join(tag)
}

/// Arquivo da política de arquivos de um lançamento, numa pasta 700 do
/// diretório de execução.
pub fn file_policy_path(tag: &str) -> PathBuf {
    crate::paths::runtime_dir()
        .join("files")
        .join(format!("{tag}.json"))
}

/// Socket de controle do supervisor de um lançamento (pastas ao vivo).
pub fn control_socket_path(tag: &str) -> PathBuf {
    crate::paths::runtime_dir()
        .join("files")
        .join(format!("{tag}.ctl"))
}

/// Grava a política de arquivos de forma atômica (arquivo 600, pasta 700).
pub fn write_file_policy(path: &Path, p: &sandmux_spec::files::FilePolicy) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .with_context(|| trf!("criar {}", "create {}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| trf!("gravar {}", "write {}", tmp.display()))?;
    std::io::Write::write_all(&mut f, &serde_json::to_vec(p)?)?;
    std::fs::rename(&tmp, path).with_context(|| trf!("gravar {}", "write {}", path.display()))?;
    Ok(())
}

/// Arquivo da lista de portas publicadas de um lançamento, numa pasta 700 do
/// diretório de execução.
pub fn ports_file(tag: &str) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir = crate::paths::runtime_dir().join("net");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .with_context(|| trf!("criar {}", "create {}", dir.display()))?;
    Ok(dir.join(format!("{tag}.ports.json")))
}

/// A camada das opções dadas no `run`.
pub fn run_layer(opts: &LaunchOptions) -> Result<config::Layer> {
    let memory = opts
        .memory
        .as_deref()
        .map(MemoryLimit::parse)
        .transpose()
        .map_err(|e| anyhow!("--memory: {e}"))?;
    let processes = opts
        .processes
        .as_deref()
        .map(ProcessLimit::parse)
        .transpose()
        .map_err(|e| anyhow!("--processes: {e}"))?;
    let network = opts
        .network
        .as_deref()
        .map(config::NetworkMode::parse)
        .transpose()
        .map_err(|e| anyhow!("--network: {e}"))?;
    let pats = |v: &[String], what: &str| -> Result<Vec<sandmux_spec::net::Pattern>> {
        v.iter()
            .map(|s| s.parse().map_err(|e| anyhow!("{what}: {e}")))
            .collect()
    };
    let files = |v: &[String], what: &str| -> Result<Vec<String>> {
        for s in v {
            config::valid_file_pattern(s).map_err(|e| anyhow!("{what}: {e}"))?;
        }
        Ok(v.to_vec())
    };
    Ok(config::Layer {
        network,
        window: opts.window,
        // `harness-asks` é só de configuração.
        harness_asks: None,
        deny: pats(&opts.deny, "--deny")?,
        allow: pats(&opts.allow, "--allow")?,
        allow_off: vec![],
        catalog: vec![],
        catalog_off: vec![],
        decrypt: pats(&opts.decrypt, "--decrypt")?,
        ssh_keys: {
            if let Some(k) = opts
                .ssh_keys
                .iter()
                .find(|k| !config::valid_ssh_key_name(k))
            {
                bail!(trf!(
                    "--ssh-key: nome inválido {k:?}",
                    "--ssh-key: invalid name {k:?}"
                ));
            }
            opts.ssh_keys.clone()
        },
        file_deny: files(&opts.file_deny, "--file-deny")?,
        file_allow: files(&opts.file_allow, "--file-allow")?,
        secret_files: vec![],
        secret_files_off: vec![],
        db: {
            if let Some(k) = opts.db.iter().find(|k| !config::valid_ssh_key_name(k)) {
                bail!(trf!(
                    "--db: nome inválido {k:?}",
                    "--db: invalid name {k:?}"
                ));
            }
            opts.db.clone()
        },
        ports: opts
            .ports
            .iter()
            .map(|p| p.parse().map_err(|e| anyhow!("--port: {e}")))
            .collect::<Result<_>>()?,
        host_ports: {
            if opts.host_ports.contains(&0) {
                bail!(tr!(
                    "--host-port: a porta precisa ser de 1 a 65535",
                    "--host-port: the port must be from 1 to 65535"
                ));
            }
            opts.host_ports.clone()
        },
        mcp_deny: opts.mcp_deny.clone(),
        mcp_allow: opts.mcp_allow.clone(),
        ro: opts.ro.clone(),
        rw: opts.rw.clone(),
        pass_env: opts.pass_env.clone(),
        repo: opts.repo,
        kvm: opts.kvm,
        gpu: opts.gpu,
        clipboard: opts.allow_clipboard,
        portal: opts.portal,
        memory,
        processes,
    })
}

/// Lê o arquivo de configuração de `host` e resolve as camadas de `opts`.
pub fn effective(host: &Host, opts: &LaunchOptions) -> Result<config::Effective> {
    let cfg = config::Config::load(&config::path(host))?;
    let run = run_layer(opts)?;
    let req = config::Request {
        harness: &opts.harness,
        workdir: &opts.workdir,
        profile: opts.profile.as_deref(),
        run: &run,
    };
    Ok(config::resolve(&cfg, host, &req)?)
}

/// Caminho absoluto, relativo a `base` quando preciso. Não resolve links nem
/// exige existência: a composição confere e dá o erro certo.
pub fn absolute(p: &Path, base: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

/// Os servidores MCP do harness de `opts` com o estado pela configuração
/// efetiva.
pub fn mcp_entries(host: &Host, opts: &LaunchOptions) -> Result<Vec<sandmux_proto::msg::McpEntry>> {
    let eff = effective(host, opts)?;
    let home = Layout::new(&host.data_dir).home(&opts.harness);
    let workdir = std::fs::canonicalize(&opts.workdir).unwrap_or_else(|_| opts.workdir.clone());
    let servers = match opts.harness.as_str() {
        "opencode" => sandmux_policy::mcp::opencode_servers(&home, &workdir),
        "hermes" => std::fs::read_to_string(home.join(".hermes/config.yaml"))
            .map(|t| sandmux_policy::mcp::hermes_servers(&t))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    Ok(servers
        .into_iter()
        .map(|s| sandmux_proto::msg::McpEntry {
            enabled: sandmux_policy::mcp::enabled(&s.name, &eff.mcp_deny, &eff.mcp_allow),
            name: s.name,
            kind: s.kind,
            target: s.target,
            origin: s.origin,
        })
        .collect())
}
