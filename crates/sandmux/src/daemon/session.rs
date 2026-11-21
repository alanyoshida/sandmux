//! Sessões: lançamento num PTY, leitura da saída, reenvio ao anexar e
//! tamanho da tela.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::num::NonZeroU16;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender, TrySendError};

use anyhow::{Context, Result, anyhow, bail};
use sandmux_policy::limits::{self, Counters, Limits};
use sandmux_proto::msg::{Stats, Usage};
use sandmux_proto::term::{ModeTracker, OscFilter};
use sandmux_proto::{LaunchOptions, Msg, SessionInfo, SessionState, Size};
use sandmux_spec::{tr, trf};

use super::Shared;
use crate::launch;

/// O que vai para a fila de um cliente anexado.
#[derive(Debug)]
pub enum Out {
    Data(Vec<u8>),
    Msg(Msg),
}

/// Fila de cada cliente; cheia, o cliente é desanexado.
pub const CLIENT_QUEUE: usize = 4096;

pub struct Client {
    pub id: u64,
    pub size: Size,
    pub tx: SyncSender<Out>,
}

pub struct Live {
    pub pid: i32,
    pub master: Arc<File>,
}

pub struct Session {
    pub id: u32,
    pub name: String,
    /// Grupo de abas (protocolo 30).
    pub group: String,
    pub opts: LaunchOptions,
    pub state: SessionState,
    pub parser: vt100::Parser,
    pub tracker: ModeTracker,
    pub filter: OscFilter,
    pub clients: Vec<Client>,
    pub size: Size,
    pub live: Option<Live>,
    pub killing: bool,
    /// Encerrada para um reinício forçado: o fim não notifica nem relança
    /// o gateway.
    pub restarting: bool,
    /// Escrita no clipboard liberada no último lançamento.
    pub clipboard: bool,
    /// Nomes de `pass-env` do último lançamento.
    pub pass_env: Vec<String>,
    /// Tetos do último lançamento; `None` sem suporte.
    pub limits: Option<Limits>,
    /// cgroup do escopo do lançamento atual, quando já conhecido.
    pub cgroup: Option<std::path::PathBuf>,
    /// A observação das execuções pelo kernel (o auxiliar root).
    pub exec_obs: Option<super::execlog::ExecObs>,
    /// A árvore de processos do lançamento atual, pela observação.
    pub proc_tree: super::proctree::ProcTree,
    /// Os arquivos da aba confinados pelo kernel (change 78); `None`, com o
    /// supervisor.
    pub tab_kernel: Option<super::tabfiles::TabKernel>,
    /// As conexões TCP do lançamento atual vistas pelo kernel.
    pub conns: super::execlog::Conns,
    /// Mortes por falta de memória e recusas de processo do último lançamento.
    pub oom_kills: u64,
    /// Chaves reais movidas ao cofre pelo vigia desde o lançamento.
    pub keys_moved: u32,
    /// Saída ainda não gravada no histórico em disco.
    pub hist_buf: Vec<u8>,
    /// Há histórico em disco esperando o cofre abrir.
    pub hist_pending: bool,
    pub fork_refusals: u64,
    /// Socket do Docker privado do lançamento atual.
    pub docker: Option<super::docker::SessionSocket>,
    /// Socket do agente SSH do lançamento atual.
    pub ssh_agent: Option<super::sshagent::AgentSocket>,
    /// Socket do MCP de banco do lançamento atual.
    pub db: Option<super::db::DbSocket>,
    /// Socket da janela do lançamento atual.
    pub window: Option<crate::window::WindowSocket>,
    /// Barramento do portal do lançamento atual.
    pub portal: Option<crate::portal::PortalSocket>,
    /// Proteção aplicada no último lançamento.
    pub policy: Option<sandmux_proto::msg::PolicySummary>,
    /// Lançamentos feitos, para numerar as etiquetas de rede.
    pub launches: u32,
    /// Etiquetas de rede dos últimos lançamentos (`s<id>-<n>`).
    pub tags: Vec<String>,
    /// Arquivo de política de rede do lançamento atual e a política gravada
    /// nele (`None` sem rede).
    pub policy_file: Option<std::path::PathBuf>,
    pub net_policy: Option<sandmux_spec::net::NetPolicy>,
    /// Os valores de `pass-env` do último lançamento estão no cofre.
    pub vault_env: bool,
    history: usize,
    /// Início do lançamento atual e a última espera de reinício (gateway).
    pub started: Option<std::time::Instant>,
    pub restart_delay: u64,
    /// O mesmo começo, em segundos Unix, para os clientes.
    pub started_at: Option<u64>,
}

/// Etiquetas guardadas por sessão.
pub const MAX_TAGS: usize = 50;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v.max(1)).unwrap_or(NonZeroU16::MIN)
}

impl Session {
    pub fn new(
        id: u32,
        name: String,
        opts: LaunchOptions,
        state: SessionState,
        history: usize,
        size: Option<Size>,
    ) -> Self {
        let size = size.unwrap_or(DEFAULT_SIZE);
        let clipboard = opts.allow_clipboard == Some(true);
        let filter = OscFilter::new(clipboard);
        let pass_env = opts.pass_env.clone();
        Session {
            id,
            name,
            group: sandmux_proto::msg::main_group(),
            opts,
            state,
            parser: vt100::Parser::new(nz(size.rows), nz(size.cols), history),
            tracker: ModeTracker::new(),
            filter,
            clients: Vec::new(),
            size,
            live: None,
            killing: false,
            restarting: false,
            clipboard,
            pass_env,
            limits: None,
            cgroup: None,
            exec_obs: None,
            proc_tree: Default::default(),
            tab_kernel: None,
            conns: Default::default(),
            oom_kills: 0,
            keys_moved: 0,
            hist_buf: Vec::new(),
            hist_pending: false,
            fork_refusals: 0,
            docker: None,
            ssh_agent: None,
            db: None,
            window: None,
            portal: None,
            policy: None,
            launches: 0,
            tags: Vec::new(),
            policy_file: None,
            net_policy: None,
            vault_env: false,
            history,
            started: None,
            started_at: None,
            restart_delay: 0,
        }
    }

    pub fn pid(&self) -> Option<i32> {
        self.live.as_ref().map(|l| l.pid)
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id,
            name: self.name.clone(),
            harness: self.opts.harness.clone(),
            workdir: self.opts.workdir.clone(),
            state: self.state.clone(),
            clients: self.clients.len(),
            pass_env: self.pass_env.clone(),
            opts: self.opts.clone(),
            usage: self.counters().map(|c| usage(&c)),
            oom_kills: self.oom_kills,
            started_at: self.started_at,
            keys: sandmux_proto::msg::KeyNote {
                moved: self.keys_moved,
                ..Default::default()
            },
            group: self.group.clone(),
            window: self.policy.as_ref().map(|p| p.window).unwrap_or_default(),
        }
    }

    /// Contadores atuais do cgroup, quando a sessão está viva.
    pub fn counters(&self) -> Option<Counters> {
        if self.state != SessionState::Viva {
            return None;
        }
        limits::read_counters(self.cgroup.as_deref()?)
    }

    /// Detalhe do consumo, para `sandmux stats`.
    pub fn stats(&self) -> Msg {
        let c = self.counters();
        Msg::StatsOf {
            id: self.id,
            state: self.state.clone(),
            stats: c.map(|c| Stats {
                usage: usage(&c),
                memory_peak: c.memory_peak,
                processes_peak: c.processes_peak,
                cpu_usec: c.cpu_usec,
            }),
            oom_kills: self.oom_kills.max(c.map_or(0, |c| c.oom_kills)),
            fork_refusals: self.fork_refusals.max(c.map_or(0, |c| c.fork_refusals)),
            docker: None,
            policy: self.policy.clone().map(Box::new),
            ports: Vec::new(),
        }
    }

    /// Começa um processo novo: terminal limpo, histórico novo.
    fn reset_terminal(&mut self, size: Size) {
        self.size = size;
        self.parser = vt100::Parser::new(nz(size.rows), nz(size.cols), self.history);
        self.tracker = ModeTracker::new();
        self.filter = OscFilter::new(self.clipboard);
    }

    /// Um parser novo, do tamanho atual, com o histórico do disco e a tela
    /// atual por cima.
    pub fn rebuild_parser(&mut self, history: &[u8]) {
        let (rows, cols) = (self.size.rows, self.size.cols);
        let mut p = vt100::Parser::new(nz(rows), nz(cols), self.history);
        p.process(history);
        self.parser = p;
    }

    /// Envia a todos os clientes; quem tem a fila cheia é desanexado.
    pub fn broadcast(&mut self, out: impl Fn() -> Out) {
        self.clients
            .retain(|c| !matches!(c.tx.try_send(out()), Err(TrySendError::Full(_))));
    }

    /// Desanexa todos os clientes com uma mensagem final.
    pub fn drop_clients(&mut self, last: Msg) {
        for c in self.clients.drain(..) {
            let _ = c.tx.try_send(Out::Msg(last.clone()));
        }
    }

    /// Aplica o menor tamanho entre os clientes anexados.
    pub fn recompute_size(&mut self) {
        let Some(size) = Size::min_of(self.clients.iter().map(|c| c.size)) else {
            return;
        };
        if size == self.size {
            return;
        }
        self.size = size;
        self.parser
            .screen_mut()
            .set_size(nz(size.rows), nz(size.cols));
        if let Some(l) = &self.live {
            set_winsize(l.master.as_raw_fd(), size);
        }
    }

    /// Bytes que reproduzem, num terminal recém-anexado, o histórico, a tela
    /// e os modos da sessão.
    pub fn replay(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let screen = self.parser.screen_mut();
        let (rows, cols) = screen.size();
        let alt = screen.alternate_screen();
        if !alt {
            screen.set_scrollback(usize::MAX);
            let total = screen.scrollback();
            let mut done = 0;
            while done < total {
                screen.set_scrollback(total - done);
                let take = (usize::from(rows.get())).min(total - done);
                for line in screen.rows(0, cols.get()).take(take) {
                    out.extend_from_slice(line.trim_end().as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
                done += take;
            }
            screen.set_scrollback(0);
            if total > 0 {
                // Empurra o histórico para a rolagem antes de redesenhar.
                for _ in 0..rows.get() {
                    out.extend_from_slice(b"\r\n");
                }
            }
        } else {
            out.extend_from_slice(b"\x1b[?1049h");
        }
        out.extend_from_slice(screen.state_formatted().as_bytes());
        out.extend(self.tracker.replay());
        out
    }
}

/// A pasta atual do grupo de processos em primeiro plano do PTY, em geral o
/// shell da aba. A pasta de trabalho entra no sandbox no mesmo caminho, e o
/// caminho lido de fora é o mesmo que o processo vê.
pub fn foreground_cwd(master: &File) -> Option<std::path::PathBuf> {
    // SAFETY: tcgetpgrp sobre o descritor válido do mestre do PTY.
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    if pgid <= 0 {
        return None;
    }
    std::fs::read_link(format!("/proc/{pgid}/cwd")).ok()
}

pub fn set_winsize(fd: RawFd, size: Size) {
    let ws = libc::winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ioctl com uma estrutura winsize válida.
    unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) };
}

pub fn signal(pid: i32, sig: i32) {
    // SAFETY: kill num PID de um filho nosso.
    unsafe { libc::kill(pid, sig) };
}

/// Pede ao harness que termine: SIGTERM e SIGHUP, como um terminal que
/// fecha (shells interativos ignoram SIGTERM). O lançador repassa os dois.
pub fn terminate(pid: i32) {
    signal(pid, libc::SIGTERM);
    signal(pid, libc::SIGHUP);
}

// ---------------------------------------------------------------- lançamento

pub struct LaunchJob {
    cmd: Command,
    inherit: Vec<RawFd>,
    reply: mpsc::Sender<std::io::Result<Child>>,
}

/// Thread de vida longa que cria os processos dos sandboxes.
pub fn start_launcher() -> mpsc::Sender<LaunchJob> {
    let (tx, rx) = mpsc::channel::<LaunchJob>();
    std::thread::Builder::new()
        .name("lancador".into())
        .spawn(move || {
            // SAFETY: getpid não falha.
            let me = unsafe { libc::getpid() };
            for mut job in rx {
                let inherit = job.inherit.clone();
                // SAFETY: só chamadas de sistema seguras depois de fork.
                unsafe {
                    job.cmd.pre_exec(move || child_setup(me, &inherit));
                }
                let _ = job.reply.send(job.cmd.spawn());
            }
        })
        .expect("thread de lançamento");
    tx
}

/// Preparo do filho, entre o fork e o exec do `sandmux-run`.
fn child_setup(parent: libc::pid_t, inherit: &[RawFd]) -> std::io::Result<()> {
    let check = |r: libc::c_int| {
        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    // SAFETY: chamadas de sistema sobre o próprio processo e seus descritores.
    unsafe {
        check(libc::setsid())?;
        // Com PTY, ele vira o terminal de controle; a pergunta sem aba
        // (`sandmux chat` sem terminal) roda com pipes.
        if libc::isatty(0) == 1 {
            check(libc::ioctl(0, libc::TIOCSCTTY, 0))?;
        }
        check(libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0))?;
        if libc::getppid() != parent {
            libc::_exit(1);
        }
        // Máscara e disposições de sinal limpas: o daemon bloqueia os de
        // término e ignora SIGPIPE.
        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        check(libc::sigprocmask(
            libc::SIG_SETMASK,
            &empty,
            std::ptr::null_mut(),
        ))?;
        for sig in [libc::SIGPIPE, libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(sig, libc::SIG_DFL);
        }
        // Nenhum descritor do daemon (PTYs de outras sessões, sockets) passa
        // para o sandbox: só a especificação e a trava.
        check(libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        ) as libc::c_int)?;
        for &fd in inherit {
            check(libc::fcntl(fd, libc::F_SETFD, 0))?;
        }
    }
    Ok(())
}

fn set_cloexec(fd: RawFd) {
    // SAFETY: fcntl sobre um descritor válido.
    unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
}

/// Host do lançamento: o do daemon, com a pasta, as variáveis e o shell do
/// pedido.
fn host_for(
    shared: &Shared,
    opts: &LaunchOptions,
    vars: &BTreeMap<String, String>,
) -> sandmux_policy::Host {
    let mut host = shared.base.clone();
    host.cwd = opts.workdir.clone();
    host.env = vars.clone();
    if let Some(sh) = vars
        .get("SHELL")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        host.user_shell = sh;
    }
    host
}

fn usage(c: &Counters) -> Usage {
    Usage {
        memory: c.memory,
        memory_max: c.memory_max,
        processes: c.processes,
        processes_max: c.processes_max,
    }
}

/// Processo lançado, ainda sem sessão registrada.
pub struct Spawned {
    pub child: Child,
    pub master: Arc<File>,
    pub warnings: Vec<String>,
    /// Política efetiva do lançamento que importa ao daemon.
    pub clipboard: bool,
    pub pass_env: Vec<String>,
    pub limits: Option<Limits>,
    pub docker: Option<super::docker::SessionSocket>,
    pub ssh_agent: Option<super::sshagent::AgentSocket>,
    pub db: Option<super::db::DbSocket>,
    pub window: Option<crate::window::WindowSocket>,
    pub portal: Option<crate::portal::PortalSocket>,
    pub policy: sandmux_proto::msg::PolicySummary,
    /// Número deste lançamento na sessão e a etiqueta de rede dele.
    pub launch: u32,
    pub tag: String,
    pub policy_file: std::path::PathBuf,
    pub net_policy: Option<sandmux_spec::net::NetPolicy>,
}

/// Compõe e lança um sandbox num PTY novo, como o lançamento `launch` da
/// sessão `id`.
pub fn spawn(
    shared: &Arc<Shared>,
    id: u32,
    launch: u32,
    opts: &LaunchOptions,
    vars: &BTreeMap<String, String>,
    size: Size,
) -> Result<Spawned> {
    if opts.harness == launch::ENTER {
        return spawn_enter(shared, id, launch, opts, size);
    }
    let host = host_for(shared, opts, vars);
    let chat = opts.harness == sandmux_policy::assistant::HARNESS;
    // A aba de chat não tem Docker privado.
    let docker = if chat {
        None
    } else {
        Some(
            super::docker::session_socket(shared, &opts.workdir).context(tr!(
                "socket do Docker privado da sessão",
                "socket of the session private Docker"
            ))?,
        )
    };
    let tag = format!("s{id}-{launch}");
    let policy_file = super::net::policy_path(&tag);
    // As chaves falsas dos provedores com segredo no cofre, nos presets que as
    // usam, e os falsos nas ferramentas.
    let (fake_env, tool_secrets) = {
        let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        let fake = if sandmux_policy::presets::FAKE_KEY_HARNESSES.contains(&opts.harness.as_str()) {
            v.fake_env()
        } else {
            Vec::new()
        };
        (fake, v.tool_secrets())
    };
    // Agente SSH: as chaves escolhidas pela configuração e pelo `run`.
    let ssh_names = {
        let mut h = host.clone();
        h.cwd = opts.workdir.clone();
        launch::effective(&h, opts)
            .map(|e| e.ssh_keys)
            .unwrap_or_default()
    };
    let mut ssh_warnings = Vec::new();
    let (ssh_agent, ssh_servers) = if ssh_names.is_empty() {
        (None, Vec::new())
    } else {
        let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        match v.unknown_ssh(&ssh_names) {
            Some(u) if !u.is_empty() => ssh_warnings.push(trf!(
                "chave SSH que não está no cofre: {}",
                "SSH key that is not in the vault: {}",
                u.join(", ")
            )),
            None => ssh_warnings.push(
                tr!(
                    "o cofre está trancado: o agente SSH assina depois do desbloqueio",
                    "the vault is locked: the SSH agent signs after the unlock"
                )
                .into(),
            ),
            _ => {}
        }
        let servers = v.ssh_servers(&ssh_names);
        drop(v);
        let sock = super::sshagent::agent_socket(shared, &tag, &opts.workdir, ssh_names.clone())
            .context(tr!("socket do agente SSH", "SSH agent socket"))?;
        (Some(sock), servers)
    };
    // MCP de banco: as conexões escolhidas pela configuração e pelo `run`.
    let db_names = {
        let mut h = host.clone();
        h.cwd = opts.workdir.clone();
        launch::effective(&h, opts)
            .map(|e| e.db)
            .unwrap_or_default()
    };
    let db = if db_names.is_empty() {
        None
    } else {
        if let Ok(list) = shared
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .db_list()
        {
            let unknown: Vec<&String> = db_names
                .iter()
                .filter(|n| *n != "*" && !list.iter().any(|c| &c.name == *n))
                .collect();
            if !unknown.is_empty() {
                ssh_warnings.push(trf!(
                    "conexão de banco que não está no cofre: {}",
                    "database connection that is not in the vault: {}",
                    unknown
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        Some(
            super::db::db_socket(shared, &tag, &opts.workdir, db_names)
                .context(tr!("socket do MCP de banco", "database MCP socket"))?,
        )
    };
    let access = sandmux_spec::VaultAccess {
        socket: shared.socket.clone(),
        data_dir: shared.base.data_dir.clone(),
        config_dir: shared.base.config_dir.clone(),
    };
    let prepared = launch::prepare_with(
        &host,
        opts,
        docker.as_ref().map(|d| d.path.as_path()),
        &tag,
        Some(&policy_file),
        launch::VaultLaunch {
            access: Some(access),
            fake_env,
            tool_secrets,
            ssh_agent_socket: ssh_agent.as_ref().map(|a| a.path.clone()),
            ssh_servers,
            db_socket: db.as_ref().map(|d| d.path.clone()),
            chat_keys: if chat {
                let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
                launch::chat_keys(&host, |h| v.fake_for(h))
            } else {
                Vec::new()
            },
            opencode_models: if opts.harness == "opencode" {
                let ports = {
                    let mut h = host.clone();
                    h.cwd = opts.workdir.clone();
                    launch::effective(&h, opts)
                        .map(|e| e.host_ports)
                        .unwrap_or_default()
                };
                // Os segredos do host, para a consulta levar o valor real.
                let secrets = sandmux_net::inject::for_host(
                    shared
                        .vault
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .injections(),
                    sandmux_net::HOST_NAME,
                );
                launch::discover_opencode_models(&host, &opts.workdir, &ports, |a| {
                    secrets
                        .iter()
                        .filter(|s| !s.fake.is_empty())
                        .fold(a.to_string(), |acc, s| acc.replace(&s.fake, &s.value))
                })
            } else {
                Vec::new()
            },
        },
    )?;
    let net_policy = prepared
        .net_policy
        .clone()
        .map(|p| super::net::with_vault(shared, p, &opts.harness))
        .map(|p| super::net::with_ssh(shared, p, &ssh_names));
    if let Some(p) = &net_policy {
        super::net::write_policy(&policy_file, p)
            .context(tr!("gravar a política de rede", "write the network policy"))?;
    }
    let ws = nix::pty::Winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = nix::pty::openpty(Some(&ws), None).context(tr!("abrir o PTY", "open the PTY"))?;
    set_cloexec(pty.master.as_raw_fd());
    set_cloexec(pty.slave.as_raw_fd());
    let slave: OwnedFd = pty.slave;
    let mut cmd = prepared.command();
    cmd.stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    let (tx, rx) = mpsc::channel();
    shared
        .launcher
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(LaunchJob {
            cmd,
            inherit: prepared.inherited_fds(),
            reply: tx,
        })
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?;
    let child = rx
        .recv()
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?
        .with_context(|| trf!("executar {}", "run {}", prepared.runner.display()))?;
    Ok(Spawned {
        child,
        master: Arc::new(File::from(pty.master)),
        warnings: prepared.warnings.into_iter().chain(ssh_warnings).collect(),
        clipboard: prepared.clipboard,
        pass_env: prepared.pass_env,
        limits: prepared.limits,
        docker,
        ssh_agent,
        db,
        window: prepared.window,
        portal: prepared.portal,
        policy: prepared.summary,
        launch,
        tag,
        policy_file,
        net_policy,
    })
}

/// A aba alvo de um shell de dentro: o número e a pasta. Sem número, a única
/// aba viva que não é ela mesma um shell de dentro.
pub fn enter_target(shared: &Arc<Shared>, arg: Option<&str>) -> Result<(u32, std::path::PathBuf)> {
    let st = shared.lock();
    let alive = |s: &Session| {
        s.live.is_some() && s.state == SessionState::Viva && s.opts.harness != launch::ENTER
    };
    let s = match arg.filter(|a| !a.is_empty()) {
        Some(a) => {
            let id: u32 = a
                .parse()
                .map_err(|_| anyhow!(trf!("aba inválida: {a}", "invalid tab: {a}")))?;
            st.sessions.get(&id).ok_or_else(|| {
                anyhow!(trf!(
                    "não existe a sessão {id}",
                    "session {id} does not exist"
                ))
            })?
        }
        None => {
            let live: Vec<&Session> = st.sessions.values().filter(|s| alive(s)).collect();
            match live.as_slice() {
                [only] => *only,
                [] => bail!(tr!("não há sessão viva", "there is no live session")),
                _ => bail!(tr!(
                    "há mais de uma sessão viva; escolha uma com `sandmux enter <id>`",
                    "there is more than one live session; pick one with `sandmux enter <id>`"
                )),
            }
        }
    };
    if s.opts.harness == launch::ENTER {
        bail!(tr!(
            "essa aba já é um shell de dentro de outra",
            "that tab is already a shell inside another one"
        ));
    }
    if !alive(s) {
        bail!(trf!(
            "a aba {} não está viva: o sandbox dela não existe agora",
            "tab {} is not alive: its sandbox does not exist now",
            s.id
        ));
    }
    Ok((s.id, s.opts.workdir.clone()))
}

/// Um shell dentro do sandbox da aba `opts.args[0]`: `sandmux-run --enter`
/// manda o terminal ao supervisor dela, que abre o shell com o mesmo
/// confinamento do agente.
fn spawn_enter(
    shared: &Arc<Shared>,
    id: u32,
    launch: u32,
    opts: &LaunchOptions,
    size: Size,
) -> Result<Spawned> {
    let (target, _) = enter_target(shared, opts.args.first().map(String::as_str))?;
    let (tag, policy) = {
        let st = shared.lock();
        let s = st.sessions.get(&target);
        (
            s.and_then(|s| s.tags.last().cloned()),
            s.and_then(|s| s.policy.clone()),
        )
    };
    let (Some(target_tag), Some(policy)) = (tag, policy) else {
        bail!(trf!(
            "a aba {target} não tem sandbox vivo",
            "tab {target} has no live sandbox"
        ));
    };
    let socket = launch::control_socket_path(&target_tag).with_extension("enter");
    if !socket.exists() {
        bail!(trf!(
            "o sandbox da aba {target} não aceita entrar (aba lançada por uma versão anterior? reinicie-a com Ctrl+b r)",
            "tab {target}'s sandbox does not accept entering (tab launched by an older version? restart it with Ctrl+b r)"
        ));
    }
    let ws = nix::pty::Winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = nix::pty::openpty(Some(&ws), None).context(tr!("abrir o PTY", "open the PTY"))?;
    set_cloexec(pty.master.as_raw_fd());
    set_cloexec(pty.slave.as_raw_fd());
    let slave: OwnedFd = pty.slave;
    let runner = launch::runner_path();
    let mut cmd = std::process::Command::new(&runner);
    cmd.arg("--enter")
        .arg(&socket)
        .env(sandmux_spec::lang::ENV, sandmux_spec::lang::get().code())
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    let (tx, rx) = mpsc::channel();
    shared
        .launcher
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(LaunchJob {
            cmd,
            inherit: Vec::new(),
            reply: tx,
        })
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?;
    let child = rx
        .recv()
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?
        .with_context(|| trf!("executar {}", "run {}", runner.display()))?;
    // Etiqueta própria: o fim desta aba não apaga os arquivos da alvo.
    let tag = format!("s{id}-{launch}");
    Ok(Spawned {
        child,
        master: Arc::new(File::from(pty.master)),
        warnings: Vec::new(),
        clipboard: false,
        pass_env: Vec::new(),
        limits: None,
        docker: None,
        ssh_agent: None,
        db: None,
        window: None,
        portal: None,
        policy,
        launch,
        policy_file: super::net::policy_path(&tag),
        tag,
        net_policy: None,
    })
}

/// Responde uma pergunta numa execução confinada como a da aba de chat, sem
/// aba: `sandmux-chat --once` numa pasta temporária, com a saída em pipe.
pub fn ask(
    shared: &Arc<Shared>,
    model: &str,
    text: &str,
    system: Option<&str>,
    vars: &BTreeMap<String, String>,
) -> Result<String> {
    let state = crate::paths::state_dir();
    let mut conv = crate::chat::Conversa::new(model, Some(text));
    conv.sistema = system.map(str::to_string);
    let dir = crate::chat::new_dir(&state, &conv)?;
    let r = ask_in(shared, &dir, vars, "--once").map(|(out, _)| out);
    crate::chat::remove_dir(&state, &dir);
    r
}

/// Os modelos dos provedores do assistente, pela aba de chat num sandbox
/// (`sandmux-chat --models`), e os avisos dos provedores que falharam.
pub fn list_models(shared: &Arc<Shared>) -> Result<(Vec<String>, Vec<String>)> {
    let state = crate::paths::state_dir();
    let conv = crate::chat::Conversa::new("lista/modelos", None);
    let dir = crate::chat::new_dir(&state, &conv)?;
    let r = ask_in(shared, &dir, &BTreeMap::new(), "--models");
    crate::chat::remove_dir(&state, &dir);
    let (out, notes) = r?;
    Ok((
        out.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        notes,
    ))
}

/// Roda `sandmux-chat <mode>` num sandbox na pasta `dir`: a saída e os
/// avisos (as linhas `sandmux-chat: ` da saída de erro).
fn ask_in(
    shared: &Arc<Shared>,
    dir: &std::path::Path,
    vars: &BTreeMap<String, String>,
    mode: &str,
) -> Result<(String, Vec<String>)> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(1);
    let mut opts = LaunchOptions::new(sandmux_policy::assistant::HARNESS);
    opts.workdir = dir.to_path_buf();
    opts.args = vec![mode.into()];
    let host = host_for(shared, &opts, vars);
    let tag = format!("ask{}", N.fetch_add(1, Ordering::Relaxed));
    let policy_file = super::net::policy_path(&tag);
    let (fake_env, chat_keys) = {
        let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        (v.fake_env(), launch::chat_keys(&host, |h| v.fake_for(h)))
    };
    let access = sandmux_spec::VaultAccess {
        socket: shared.socket.clone(),
        data_dir: shared.base.data_dir.clone(),
        config_dir: shared.base.config_dir.clone(),
    };
    let prepared = launch::prepare_with(
        &host,
        &opts,
        None,
        &tag,
        Some(&policy_file),
        launch::VaultLaunch {
            access: Some(access),
            fake_env,
            chat_keys,
            ..Default::default()
        },
    )?;
    let cleanup = || {
        let _ = std::fs::remove_file(&policy_file);
        let _ = std::fs::remove_file(crate::launch::file_policy_path(&tag));
    };
    if let Some(p) = prepared
        .net_policy
        .clone()
        .map(|p| super::net::with_vault(shared, p, &opts.harness))
    {
        super::net::write_policy(&policy_file, &p)
            .context(tr!("gravar a política de rede", "write the network policy"))?;
    }
    let mut cmd = prepared.command();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (tx, rx) = mpsc::channel();
    shared
        .launcher
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(LaunchJob {
            cmd,
            inherit: prepared.inherited_fds(),
            reply: tx,
        })
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?;
    let child = rx
        .recv()
        .map_err(|_| {
            anyhow!(tr!(
                "a thread de lançamento parou",
                "the launch thread stopped"
            ))
        })?
        .with_context(|| trf!("executar {}", "run {}", prepared.runner.display()));
    let child = match child {
        Ok(c) => c,
        Err(e) => {
            cleanup();
            return Err(e);
        }
    };
    let pid = child.id() as i32;
    let (otx, orx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = otx.send(child.wait_with_output());
    });
    let out = match orx.recv_timeout(std::time::Duration::from_secs(300)) {
        Ok(o) => o,
        Err(_) => {
            terminate(pid);
            cleanup();
            bail!(tr!(
                "o modelo não respondeu em 5 minutos",
                "the model did not answer within 5 minutes"
            ));
        }
    };
    drop(prepared);
    cleanup();
    let out = out.context(tr!("esperar a resposta", "wait for the answer"))?;
    let err = String::from_utf8_lossy(&out.stderr);
    let msg: Vec<&str> = err
        .lines()
        .filter_map(|l| l.strip_prefix("sandmux-chat: "))
        .collect();
    if out.status.success() {
        return Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            msg.iter().map(|s| s.to_string()).collect(),
        ));
    }
    if msg.is_empty() {
        bail!("{}", err.trim());
    }
    bail!("{}", msg.join("\n"))
}

/// Liga um processo lançado à sessão `id` e começa a ler a saída.
pub fn attach_process(shared: &Arc<Shared>, id: u32, spawned: Spawned, size: Size) {
    let Spawned {
        child,
        master,
        clipboard,
        pass_env,
        limits: lim,
        docker,
        ssh_agent,
        db,
        window,
        portal,
        policy,
        launch,
        tag,
        policy_file,
        net_policy,
        ..
    } = spawned;
    let pid = child.id() as i32;
    // O histórico em disco, antes da trava do estado (pede o cofre).
    let preload = super::history::preload(shared, id);
    {
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.clipboard = clipboard;
            s.pass_env = pass_env;
            s.limits = lim;
            s.docker = docker;
            s.ssh_agent = ssh_agent;
            s.db = db;
            s.window = window;
            s.portal = portal;
            s.policy = Some(policy);
            s.launches = s.launches.max(launch);
            if let Some(old) = s.policy_file.take() {
                let _ = std::fs::remove_file(old);
            }
            s.policy_file = Some(policy_file);
            s.net_policy = net_policy;
            s.tags.push(tag);
            if s.tags.len() > MAX_TAGS {
                let extra = s.tags.len() - MAX_TAGS;
                s.tags.drain(..extra);
            }
            s.cgroup = None;
            s.exec_obs = None;
            s.proc_tree = Default::default();
            s.tab_kernel = None;
            s.conns = Default::default();
            s.oom_kills = 0;
            s.keys_moved = 0;
            s.fork_refusals = 0;
            s.reset_terminal(size);
            match preload {
                super::history::Preload::Bytes(b) => {
                    s.parser.process(&b);
                    let sep = super::history::separator();
                    s.parser.process(&sep);
                    s.hist_buf.extend_from_slice(&sep);
                }
                super::history::Preload::Pending => {
                    s.hist_pending = true;
                    s.hist_buf.extend_from_slice(&super::history::separator());
                }
                super::history::Preload::None => {}
            }
            s.state = SessionState::Viva;
            s.killing = false;
            s.started = Some(std::time::Instant::now());
            s.started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs());
            s.live = Some(Live {
                pid,
                master: master.clone(),
            });
        }
        shared.persist(&mut st);
    }
    // O Docker da pasta passa a considerar a política desta sessão.
    let wd = shared
        .lock()
        .sessions
        .get(&id)
        .map(|s| s.opts.workdir.clone());
    if let Some(w) = wd {
        super::net::refresh_engine(shared, &w);
    }
    if lim.is_some() {
        let shared = shared.clone();
        std::thread::Builder::new()
            .name(format!("cgroup-{id}"))
            .spawn(move || monitor(shared, id, pid))
            .expect("thread do cgroup");
    }
    let shared = shared.clone();
    std::thread::Builder::new()
        .name(format!("sessao-{id}"))
        .spawn(move || reader(shared, id, pid, master, child))
        .expect("thread da sessão");
}

/// Relança uma sessão existente (reinício ou restauração).
pub fn relaunch(
    shared: &Arc<Shared>,
    id: u32,
    vars: &BTreeMap<String, String>,
    size: Option<Size>,
    force: bool,
) -> Result<Vec<String>> {
    let live = shared
        .lock()
        .sessions
        .get(&id)
        .is_some_and(|s| s.live.is_some());
    if live && force {
        stop_for_restart(shared, id)?;
    }
    let (opts, cur, launch) = {
        let st = shared.lock();
        let s = st.sessions.get(&id).ok_or_else(|| {
            anyhow!(trf!(
                "não existe a sessão {id}",
                "session {id} does not exist"
            ))
        })?;
        if s.live.is_some() {
            return Err(anyhow!(trf!(
                "a sessão {id} está viva; `sandmux restart {id} --force` a encerra e relança",
                "session {id} is alive; `sandmux restart {id} --force` ends and relaunches it"
            )));
        }
        (s.opts.clone(), s.size, s.launches + 1)
    };
    let size = size.unwrap_or(cur);
    let spawned = spawn(shared, id, launch, &opts, vars, size)?;
    let warnings = spawned.warnings.clone();
    attach_process(shared, id, spawned, size);
    Ok(warnings)
}

/// Encerra o processo da sessão para um reinício: TERM, até 5 s, KILL e
/// mais 5 s. A sessão fica na lista, marcada como reiniciando.
fn stop_for_restart(shared: &Arc<Shared>, id: u32) -> Result<()> {
    let pid = {
        let mut st = shared.lock();
        let Some(s) = st.sessions.get_mut(&id) else {
            return Ok(());
        };
        let Some(pid) = s.pid() else { return Ok(()) };
        s.restarting = true;
        pid
    };
    let ended = |st: &super::State| st.sessions.get(&id).is_none_or(|s| s.pid() != Some(pid));
    terminate(pid);
    if !shared.wait_until(std::time::Duration::from_secs(5), ended) {
        signal(pid, libc::SIGKILL);
        if !shared.wait_until(std::time::Duration::from_secs(5), ended) {
            if let Some(s) = shared.lock().sessions.get_mut(&id) {
                s.restarting = false;
            }
            return Err(anyhow!(trf!(
                "a sessão {id} não terminou",
                "session {id} did not end"
            )));
        }
    }
    Ok(())
}

/// Vigia os contadores do escopo de um lançamento: acha o cgroup e, por
/// inotify em `memory.events` e `pids.events`, guarda as mortes por falta de
/// memória e as recusas de processo enquanto o escopo existe. Assim a
/// contagem sobrevive ao fim do escopo, que some com o último processo.
fn monitor(shared: Arc<Shared>, id: u32, pid: i32) {
    use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
    let own = limits::cgroup_of(std::process::id());
    let same = |st: &super::State| st.sessions.get(&id).and_then(|s| s.pid()) == Some(pid);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let cg = loop {
        match limits::cgroup_of(pid as u32) {
            Some(p) if Some(&p) != own.as_ref() && p.extension().is_some_and(|e| e == "scope") => {
                break p;
            }
            _ if std::time::Instant::now() > deadline => return,
            None => return,
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    };
    let update = |cg: &std::path::Path| {
        let Some(c) = limits::read_counters(cg) else {
            return;
        };
        let mut st = shared.lock();
        if same(&st)
            && let Some(s) = st.sessions.get_mut(&id)
        {
            s.oom_kills = s.oom_kills.max(c.oom_kills);
            s.fork_refusals = s.fork_refusals.max(c.fork_refusals);
        }
    };
    {
        let mut st = shared.lock();
        if !same(&st) {
            return;
        }
        if let Some(s) = st.sessions.get_mut(&id) {
            s.cgroup = Some(cg.clone());
        }
    }
    // O registro de execuções pelo kernel, no escopo achado.
    super::execlog::start(shared.clone(), id, pid, cg.clone());
    let Ok(ino) = Inotify::init(InitFlags::IN_CLOEXEC) else {
        return;
    };
    for f in ["memory.events", "pids.events"] {
        let _ = ino.add_watch(&cg.join(f), AddWatchFlags::IN_MODIFY);
    }
    update(&cg);
    loop {
        match ino.read_events() {
            Ok(evs) => {
                update(&cg);
                if evs
                    .iter()
                    .any(|e| e.mask.contains(AddWatchFlags::IN_IGNORED))
                {
                    break;
                }
            }
            Err(nix::errno::Errno::EINTR) => {}
            Err(_) => break,
        }
    }
}

fn reader(shared: Arc<Shared>, id: u32, pid: i32, master: Arc<File>, mut child: Child) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match (&*master).read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut st = shared.lock();
                if let Some(s) = st.sessions.get_mut(&id) {
                    let data = s.filter.filter(&buf[..n]);
                    if data.is_empty() {
                        continue;
                    }
                    s.parser.process(&data);
                    s.tracker.feed(&data);
                    if shared.disk_history.enabled() {
                        s.hist_buf.extend_from_slice(&data);
                        let over = s.hist_buf.len().saturating_sub(super::history::BUF_CAP);
                        if over > 0 {
                            s.hist_buf.drain(..over);
                        }
                    }
                    s.broadcast(|| Out::Data(data.clone()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let code = match child.wait() {
        Ok(status) => status.code().unwrap_or_else(|| {
            128 + std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)
        }),
        Err(_) => 1,
    };
    let mut st = shared.lock();
    let stopping = st.stopping;
    let mut remove = false;
    if let Some(s) = st.sessions.get_mut(&id)
        && s.pid() == Some(pid)
    {
        s.live = None;
        if stopping {
            s.drop_clients(Msg::Detached {
                reason: tr!("o daemon parou", "the daemon stopped").into(),
            });
        } else if s.restarting {
            s.restarting = false;
            s.state = SessionState::Encerrada { codigo: code };
            s.drop_clients(Msg::Detached {
                reason: tr!("reiniciando", "restarting").into(),
            });
        } else if s.killing {
            s.drop_clients(Msg::Detached {
                reason: trf!(
                    "a sessão {id} foi encerrada por `sandmux kill`",
                    "session {id} was ended by `sandmux kill`"
                ),
            });
            remove = true;
        } else if s.opts.gateway {
            // O gateway volta sozinho, com espera crescente.
            let ran = s.started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
            let delay = gateway_delay(s.restart_delay, ran);
            s.restart_delay = delay;
            s.state = SessionState::Reiniciando { segundos: delay };
            s.drop_clients(Msg::Exited { id, code });
            let shared = shared.clone();
            std::thread::spawn(move || restart_gateway(shared, id, delay));
        } else {
            s.state = SessionState::Encerrada { codigo: code };
            s.drop_clients(Msg::Exited { id, code });
            shared
                .notify
                .send(super::notify::Event::Ended { session: id, code });
        }
    }
    // O socket do Docker some com o lançamento; o Docker da pasta para
    // quando nenhuma sessão viva da pasta resta.
    let mut workdir = None;
    if let Some(s) = st.sessions.get_mut(&id)
        && s.live.is_none()
    {
        s.docker = None;
        s.ssh_agent = None;
        s.db = None;
        s.window = None;
        s.portal = None;
        if let Some(f) = s.policy_file.take() {
            let _ = std::fs::remove_file(f);
        }
        if let Some(t) = s.tags.last() {
            let _ = std::fs::remove_file(crate::launch::file_policy_path(t));
        }
        workdir = Some(s.opts.workdir.clone());
    }
    if remove && let Some(s) = st.sessions.remove(&id) {
        remove_chat_dir(&s.opts);
        workdir = Some(s.opts.workdir.clone());
    }
    if !stopping {
        shared.persist(&mut st);
    }
    let unused = workdir.clone().filter(|w| {
        !stopping
            && !st
                .sessions
                .values()
                .any(|s| s.live.is_some() && same_dir(&s.opts.workdir, w))
    });
    let still = workdir.clone().filter(|_| unused.is_none() && !stopping);
    drop(st);
    if let Some(w) = unused {
        shared.docker.stop(&w);
    }
    // Outras sessões da pasta seguem: o Docker volta à política delas.
    if let Some(w) = still {
        super::net::refresh_engine(&shared, &w);
    }
    shared.changed.notify_all();
}

pub fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.canonicalize()
        .ok()
        .zip(b.canonicalize().ok())
        .is_some_and(|(x, y)| x == y)
        || a == b
}

/// Uma sessão do opencode cujos provedores locais (com a porta em
/// `host-ports`) mandam uma chave falsa do cofre: a lista de modelos dela só
/// vem com o cofre desbloqueado.
pub fn needs_vault_for_models(shared: &Shared, opts: &LaunchOptions) -> bool {
    if opts.harness != "opencode" {
        return false;
    }
    let host = host_for(shared, opts, &BTreeMap::new());
    let ports = launch::effective(&host, opts)
        .map(|e| e.host_ports)
        .unwrap_or_default();
    let home = sandmux_policy::Layout::new(&host.data_dir).home("opencode");
    sandmux_policy::opencode_models::local_providers(&home, &opts.workdir)
        .iter()
        .filter(|p| ports.contains(&p.port))
        .any(|p| {
            [&p.authorization, &p.api_key]
                .into_iter()
                .flatten()
                .any(|v| v.contains("sandmux-fake-"))
        })
}

/// A sessão é uma aba de chat?
pub fn is_chat(opts: &LaunchOptions) -> bool {
    opts.harness == sandmux_policy::assistant::HARNESS
}

/// Apaga a pasta de uma aba de chat que deixou de existir.
pub fn remove_chat_dir(opts: &LaunchOptions) {
    if is_chat(opts) {
        crate::chat::remove_dir(&crate::paths::state_dir(), &opts.workdir);
    }
}

/// Nome da sessão, `<harness>:<pasta>` (`hermes-gateway:<pasta>` no
/// gateway, `chat:<modelo>` na aba de chat). Pode repetir; o identificador
/// não.
pub fn session_name(opts: &LaunchOptions) -> String {
    if is_chat(opts) {
        let model = crate::chat::Conversa::load(&opts.workdir)
            .ok()
            .flatten()
            .map(|c| c.modelo)
            .unwrap_or_default();
        return format!("chat:{model}");
    }
    let folder = opts
        .workdir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    if opts.gateway {
        return format!("{}-gateway:{folder}", opts.harness);
    }
    format!("{}:{folder}", opts.harness)
}

/// Espera antes de relançar o gateway: 5 s, dobrando a cada saída seguida até
/// 5 min; um gateway que viveu 10 min volta aos 5 s.
pub fn gateway_delay(prev: u64, ran_secs: u64) -> u64 {
    if prev == 0 || ran_secs >= 600 {
        5
    } else {
        (prev * 2).min(300)
    }
}

/// A sessão de gateway viva, reiniciando ou aguardando o cofre, se houver.
pub fn gateway_session(shared: &Shared) -> Option<u32> {
    shared
        .lock()
        .sessions
        .values()
        .find(|s| {
            s.opts.gateway
                && (s.live.is_some()
                    || matches!(
                        s.state,
                        SessionState::AguardandoCofre | SessionState::Reiniciando { .. }
                    ))
        })
        .map(|s| s.id)
}

/// Relança o gateway depois da espera, se a sessão continua esperando.
fn restart_gateway(shared: Arc<Shared>, id: u32, delay: u64) {
    std::thread::sleep(std::time::Duration::from_secs(delay));
    let waiting = {
        let st = shared.lock();
        !st.stopping
            && st.sessions.get(&id).is_some_and(|s| {
                s.live.is_none() && matches!(s.state, SessionState::Reiniciando { .. })
            })
    };
    if !waiting {
        return;
    }
    let mut vars = shared.default_vars();
    if let Some(saved) = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pass_env(id)
    {
        vars.extend(saved);
    }
    if let Err(e) = relaunch(&shared, id, &vars, None, false) {
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.state = SessionState::Parada {
                motivo: trf!(
                    "o gateway não voltou: {e:#}",
                    "the gateway did not come back: {e:#}"
                ),
            };
        }
        shared.persist(&mut st);
        shared.changed.notify_all();
    }
}

/// Reserva o identificador de uma sessão nova, antes do lançamento: a
/// etiqueta de rede dela leva o identificador.
pub fn reserve(shared: &Arc<Shared>) -> u32 {
    let mut st = shared.lock();
    let id = st.next_id;
    st.next_id += 1;
    shared.persist(&mut st);
    id
}

/// Registro da sessão nova, com o identificador reservado.
pub fn register(
    shared: &Arc<Shared>,
    id: u32,
    opts: LaunchOptions,
    spawned: Spawned,
    size: Size,
    group: String,
) -> String {
    let name = session_name(&opts);
    {
        let mut st = shared.lock();
        let mut s = Session::new(
            id,
            name.clone(),
            opts,
            SessionState::Viva,
            shared.history,
            Some(size),
        );
        s.group = group;
        st.sessions.insert(id, s);
    }
    attach_process(shared, id, spawned, size);
    name
}

/// Guarda no cofre os valores de `pass-env` da sessão `id` (os nomes vêm da
/// política efetiva). Devolve um aviso quando há valores e o cofre não pôde
/// guardá-los.
pub fn store_pass_env(
    shared: &Shared,
    id: u32,
    names: &[String],
    vars: &BTreeMap<String, String>,
) -> Option<String> {
    let values: BTreeMap<String, String> = names
        .iter()
        .filter_map(|n| vars.get(n).map(|v| (n.clone(), v.clone())))
        .collect();
    let stored = !values.is_empty()
        && shared
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .put_pass_env(id, values.clone());
    {
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.vault_env = stored;
        }
        shared.persist(&mut st);
    }
    (!values.is_empty() && !stored).then(|| {
        tr!(
            "o cofre não existe ou está trancado: os valores de --pass-env não foram guardados, e num reboot esta sessão fica parada até um `sandmux restart` (veja `sandmux vault`)",
            "the vault does not exist or is locked: the --pass-env values were not saved, and on a reboot this session stays stopped until a `sandmux restart` (see `sandmux vault`)"
        )
        .to_string()
    })
}

/// Depois do desbloqueio: limpa do cofre os valores de sessões que não
/// existem mais e relança as sessões que aguardavam o cofre, com os valores
/// guardados.
pub fn vault_unlocked(shared: &Arc<Shared>) {
    let (alive, waiting): (Vec<u32>, Vec<u32>) = {
        let st = shared.lock();
        (
            st.sessions.keys().copied().collect(),
            st.sessions
                .values()
                .filter(|s| s.state == SessionState::AguardandoCofre)
                .map(|s| s.id)
                .collect(),
        )
    };
    shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .prune(&alive);
    for id in waiting {
        let mut saved = shared
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pass_env(id);
        // O gateway sem `pass-env` esperava só o cofre (os tokens dos canais).
        let bare = shared.lock().sessions.get(&id).is_some_and(|s| {
            (s.opts.gateway || is_chat(&s.opts) || needs_vault_for_models(shared, &s.opts))
                && s.pass_env.is_empty()
        });
        if saved.is_none() && bare {
            saved = Some(BTreeMap::new());
        }
        let shared = shared.clone();
        // O relançamento leva tempo; a resposta do desbloqueio não espera.
        std::thread::spawn(move || {
            let Some(values) = saved else {
                let mut st = shared.lock();
                if let Some(s) = st.sessions.get_mut(&id) {
                    s.state = SessionState::Parada {
                        motivo: trf!(
                            "os valores de --pass-env não estão no cofre; rode `sandmux restart {id}` num terminal com as variáveis",
                            "the --pass-env values are not in the vault; run `sandmux restart {id}` in a terminal with the variables"
                        ),
                    };
                }
                shared.persist(&mut st);
                return;
            };
            let mut vars = shared.default_vars();
            vars.extend(values);
            if let Err(e) = relaunch(&shared, id, &vars, None, false) {
                let mut st = shared.lock();
                if let Some(s) = st.sessions.get_mut(&id) {
                    s.state = SessionState::Parada {
                        motivo: trf!("falhou ao restaurar: {e:#}", "restore failed: {e:#}"),
                    };
                }
                shared.persist(&mut st);
            }
        });
    }
}
