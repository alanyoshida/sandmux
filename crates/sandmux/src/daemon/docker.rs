//! Docker privado por pasta no daemon: subida sob demanda, parada com a
//! última sessão da pasta, socket de cada sessão e repasse das conexões.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use sandmux_policy::limits;
use sandmux_proto::LaunchOptions;
use sandmux_proto::msg::DockerStats;

use super::Shared;
use crate::{docker, launch, paths};

struct Engine {
    dirs: docker::Dirs,
    child: Child,
    /// cgroup do escopo do systemd, quando há tetos.
    cgroup: Option<PathBuf>,
    /// Gateway de rede do Docker da pasta e o arquivo da política dele.
    gateway: Option<sandmux_net::Gateway>,
    policy_file: PathBuf,
}

type SpawnJob = (Command, std::sync::mpsc::Sender<std::io::Result<Child>>);

/// Os Docker privados vivos, por pasta de trabalho canônica.
#[derive(Default)]
pub struct Engines {
    map: Mutex<HashMap<PathBuf, Arc<Mutex<Option<Engine>>>>>,
    /// Thread fixa que cria os processos do Docker: o `PR_SET_PDEATHSIG` é
    /// ligado à thread que faz o fork, e uma thread de conexão termina logo.
    spawner: Mutex<Option<std::sync::mpsc::Sender<SpawnJob>>>,
    /// Espelho do Docker Hub (processo e porta), comum a todas as pastas.
    mirror: Mutex<Option<(Child, u16)>>,
}

fn support(shared: &Shared) -> &'static Result<docker::Support, String> {
    static S: OnceLock<Result<docker::Support, String>> = OnceLock::new();
    S.get_or_init(|| docker::support(&shared.base.user, shared.base.uid))
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn private_dir(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)?;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))
}

fn tail(p: &Path) -> String {
    let t = std::fs::read_to_string(p).unwrap_or_default();
    let lines: Vec<&str> = t.lines().rev().take(3).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

impl Engines {
    fn spawn(&self, cmd: Command) -> std::io::Result<Child> {
        let tx = {
            let mut g = self.spawner.lock().unwrap_or_else(|e| e.into_inner());
            if g.is_none() {
                let (tx, rx) = std::sync::mpsc::channel::<SpawnJob>();
                std::thread::Builder::new()
                    .name("docker-spawn".into())
                    .spawn(move || {
                        for (mut cmd, reply) in rx {
                            let _ = reply.send(cmd.spawn());
                        }
                    })?;
                *g = Some(tx);
            }
            g.as_ref().expect("spawner").clone()
        };
        let (rtx, rrx) = std::sync::mpsc::channel();
        tx.send((cmd, rtx))
            .map_err(|_| std::io::Error::other("a thread do Docker parou"))?;
        rrx.recv()
            .map_err(|_| std::io::Error::other("a thread do Docker parou"))?
    }

    fn slot(&self, workdir: &Path) -> Arc<Mutex<Option<Engine>>> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(workdir.to_path_buf())
            .or_default()
            .clone()
    }

    /// O socket do Docker da pasta, subindo-o se preciso.
    pub fn ensure(&self, shared: &Shared, workdir: &Path) -> Result<PathBuf, String> {
        let workdir = canon(workdir);
        let sup = support(shared).clone().map_err(|e| {
            format!(
                "o Docker privado não está disponível nesta máquina: {e}; veja `sandmux doctor`"
            )
        })?;
        let slot = self.slot(&workdir);
        let mut g = slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = g.as_mut() {
            let alive = matches!(e.child.try_wait(), Ok(None));
            if alive && UnixStream::connect(e.dirs.socket()).is_ok() {
                return Ok(e.dirs.socket());
            }
            stop_engine(g.take().expect("engine"));
        }
        let engine = start(self, shared, &workdir, &sup)?;
        let sock = engine.dirs.socket();
        *g = Some(engine);
        Ok(sock)
    }

    /// A porta do espelho do Docker Hub, subindo-o se preciso; `None` sem o
    /// binário `registry` ou se ele não subiu.
    fn mirror_port(&self, shared: &Shared) -> Option<u16> {
        let bin = docker::registry_binary()?;
        let mut g = self.mirror.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((c, p)) = g.as_mut()
            && matches!(c.try_wait(), Ok(None))
        {
            return Some(*p);
        }
        let dir = paths::state_dir().join("registry");
        let storage = shared.base.data_dir.join("registry-cache");
        private_dir(&dir).ok()?;
        private_dir(&storage).ok()?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .ok()?
            .local_addr()
            .ok()?
            .port();
        let cfg = dir.join("config.yml");
        std::fs::write(&cfg, docker::registry_config(&storage, port)).ok()?;
        let log = std::fs::File::create(dir.join("registry.log")).ok()?;
        let mut cmd = Command::new(bin);
        cmd.arg("serve")
            .arg(&cfg)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().ok()?))
            .stderr(Stdio::from(log));
        // SAFETY: só chamadas seguras depois do fork.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
                Ok(())
            });
        }
        let mut child = self.spawn(cmd).ok()?;
        let end = Instant::now() + Duration::from_secs(10);
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            if Instant::now() > end || !matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!(
                    "sandmuxd: o espelho do Docker Hub não subiu: {}",
                    tail(&dir.join("registry.log"))
                );
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        *g = Some((child, port));
        Some(port)
    }

    /// O Docker da pasta está rodando? Devolve o arquivo da política dele.
    pub fn policy_file(&self, workdir: &Path) -> Option<PathBuf> {
        let slot = self.slot(&canon(workdir));
        let g = slot.lock().unwrap_or_else(|e| e.into_inner());
        g.as_ref().map(|e| e.policy_file.clone())
    }

    /// Para o Docker da pasta.
    pub fn stop(&self, workdir: &Path) {
        let slot = self.slot(&canon(workdir));
        let taken = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(e) = taken {
            stop_engine(e);
        }
    }

    /// Para todos.
    pub fn stop_all(&self) {
        let slots: Vec<_> = self
            .map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        for s in slots {
            if let Some(e) = s.lock().unwrap_or_else(|e| e.into_inner()).take() {
                stop_engine(e);
            }
        }
        if let Some((mut c, _)) = self.mirror.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Portas publicadas pelos contêineres do Docker da pasta.
    pub fn ports(&self, workdir: &Path) -> Vec<sandmux_proto::msg::PortInfo> {
        let slot = self.slot(&canon(workdir));
        let g = slot.lock().unwrap_or_else(|e| e.into_inner());
        g.as_ref()
            .map(|e| container_ports(&e.dirs.socket()))
            .unwrap_or_default()
    }

    /// Consumo do Docker da pasta, quando ele está rodando.
    pub fn stats(&self, workdir: &Path) -> Option<DockerStats> {
        let slot = self.slot(&canon(workdir));
        let g = slot.lock().unwrap_or_else(|e| e.into_inner());
        let e = g.as_ref()?;
        let containers = count_containers(&e.dirs.socket()).unwrap_or(0);
        let c = e
            .cgroup
            .as_deref()
            .and_then(limits::read_counters)
            .unwrap_or_default();
        Some(DockerStats {
            memory: c.memory,
            memory_max: c.memory_max,
            processes: c.processes,
            processes_max: c.processes_max,
            containers,
        })
    }
}

fn start(
    engines: &Engines,
    shared: &Shared,
    workdir: &Path,
    sup: &docker::Support,
) -> Result<Engine, String> {
    let id = docker::engine_id(workdir);
    let dirs = docker::Dirs::new(&shared.base.data_dir, &paths::runtime_dir(), &id);
    let io = |what: &str, e: std::io::Error| format!("{what}: {e}");
    let _ = std::fs::remove_dir_all(&dirs.state);
    private_dir(&dirs.data).map_err(|e| io("pasta de dados do Docker", e))?;
    for d in [
        dirs.state.clone(),
        dirs.state.join("home"),
        dirs.state.join("xdg"),
    ] {
        private_dir(&d).map_err(|e| io("pasta de estado do Docker", e))?;
    }
    let _ = std::fs::write(dirs.data.join("pasta"), workdir.display().to_string());

    let mut host = shared.base.clone();
    host.cwd = workdir.to_path_buf();
    let mirror = engines.mirror_port(shared);
    // O TUN da visão chega por este socket; o gateway do Docker roda aqui.
    let tun_sock = dirs.state.join("tun.sock");
    // Um socket velho (a pasta de estado de uma subida anterior pode não ter
    // saído inteira) faria o `bind` falhar.
    let _ = std::fs::remove_file(&tun_sock);
    let tun_listener =
        UnixListener::bind(&tun_sock).map_err(|e| io("socket do TUN do Docker", e))?;
    tun_listener
        .set_nonblocking(true)
        .map_err(|e| io("socket do TUN do Docker", e))?;
    // A CA do gateway do Docker: decifra os registries dos segredos `docker`.
    let tls = launch::launch_ca();
    let spec = sandmux_policy::compose_docker_view(
        &host,
        workdir,
        &[dirs.data.clone(), dirs.state.clone()],
        docker::dockerd_command(&dirs, mirror),
        docker::dockerd_env(&dirs),
        Some(&tun_sock),
        tls.as_ref().map(|t| t.ca_cert_pem.as_str()),
    )
    .map_err(|e| format!("visão do Docker: {e}"))?;
    std::fs::write(dirs.view_file(), spec.to_json()).map_err(|e| io("gravar a visão", e))?;
    let _ = std::fs::set_permissions(dirs.view_file(), std::fs::Permissions::from_mode(0o600));

    // Tetos da pasta, pela configuração (sem opções de run).
    let mut opts = LaunchOptions::new("shell");
    opts.workdir = workdir.to_path_buf();
    let lim = limits::support().ok().map(|_| {
        launch::effective(&host, &opts)
            .map(|e| e.limits(limits::mem_total().unwrap_or(0)))
            .unwrap_or_default()
    });

    let runner = launch::runner_path();
    let _ = sup;
    let rk = docker::rootlesskit_args(&dirs, &runner);
    let mut cmd = match &lim {
        Some(l) => {
            let mut c = limits::systemd_run();
            // O teto vale na slice, que abraça o escopo e os contêineres.
            let slice = docker::slice_name(&id);
            let out = systemctl()
                .args(["--user", "set-property", "--runtime", &slice])
                .args(docker::slice_properties(l))
                .output();
            match out {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let _ = std::fs::remove_file(&tun_sock);
                    return Err(format!(
                        "não foi possível pôr o teto na slice {slice}: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    ));
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tun_sock);
                    return Err(format!("systemctl: {e}"));
                }
            }
            c.args(docker::scope_prefix(l, workdir, &slice))
                .arg("rootlesskit")
                .args(&rk);
            c
        }
        None => {
            let mut c = Command::new("rootlesskit");
            c.args(&rk);
            c
        }
    };
    let log = std::fs::File::create(dirs.log()).map_err(|e| io("log do Docker", e))?;
    let log2 = log.try_clone().map_err(|e| io("log do Docker", e))?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log2));
    // SAFETY: só chamadas seguras depois do fork.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
            Ok(())
        });
    }
    let mut child = engines
        .spawn(cmd)
        .map_err(|e| io("subir o rootlesskit", e))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let tag = format!("docker-{id}");
    let policy_file = super::net::policy_path(&tag);
    let mut gateway = None;
    loop {
        if gateway.is_none()
            && let Ok((s, _)) = tun_listener.accept()
        {
            let _ = s.set_nonblocking(false);
            if let Ok(Some((_, tun))) = sandmux_net::ports::recv_fd(&s) {
                let policy = super::net::engine_policy(shared, workdir);
                let _ = super::net::write_policy(&policy_file, &policy);
                let cfg = sandmux_net::Config {
                    log_dir: super::net::log_dir(),
                    tag: tag.clone(),
                    workdir: workdir.to_path_buf(),
                    nameservers: sandmux_policy::compose::host_nameservers(&host)
                        .iter()
                        .filter_map(|s| s.parse().ok())
                        .collect(),
                    policy,
                    policy_file: Some(policy_file.clone()),
                    ports: None,
                    aliases: mirror
                        .map(|p| {
                            vec![(
                                std::net::SocketAddr::from((sandmux_net::HOST, p)),
                                "registry-1.docker.io".to_string(),
                            )]
                        })
                        .unwrap_or_default(),
                    tls: tls.clone(),
                    vault: Some(sandmux_spec::VaultAccess {
                        socket: shared.socket.clone(),
                        data_dir: shared.base.data_dir.clone(),
                        config_dir: shared.base.config_dir.clone(),
                    }),
                };
                gateway = sandmux_net::spawn(tun, cfg).ok();
            }
        }
        if UnixStream::connect(dirs.socket()).is_ok() {
            break;
        }
        if let Ok(Some(st)) = child.try_wait() {
            return Err(format!(
                "o Docker privado não subiu ({st}): {}",
                tail(&dirs.log())
            ));
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "o Docker privado não respondeu em 30 s: {}",
                tail(&dirs.log())
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // O cgroup da slice: o consumo e o fim contam os contêineres.
    let slice = docker::slice_name(&id);
    let cgroup = lim
        .and_then(|_| limits::cgroup_of(child.id()))
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .filter(|p| p.file_name().is_some_and(|n| n == slice.as_str()));
    Ok(Engine {
        dirs,
        child,
        cgroup,
        gateway,
        policy_file,
    })
}

fn systemctl() -> Command {
    let mut c = Command::new("systemctl");
    if let Some(d) = limits::systemd_runtime_dir() {
        c.env("XDG_RUNTIME_DIR", d);
    }
    c.stdin(Stdio::null());
    c
}

/// Para as slices `sandmux-docker-*` sem nenhum processo: sobras de um
/// daemon derrubado antes de parar o Docker. Uma slice com processos (de outro
/// daemon) não é tocada.
pub fn stop_empty_slices() {
    let Ok(out) = systemctl()
        .args([
            "--user",
            "list-units",
            "--type=slice",
            "--all",
            "--no-legend",
            "--plain",
            "sandmux-docker-*",
        ])
        .output()
    else {
        return;
    };
    for unit in String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|u| u.starts_with("sandmux-docker-") && u.ends_with(".slice"))
    {
        let tasks = systemctl()
            .args(["--user", "show", unit, "-p", "TasksCurrent", "--value"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if tasks == "0" {
            let _ = systemctl().args(["--user", "stop", unit]).output();
        }
    }
}

fn stop_engine(mut e: Engine) {
    let pid = e.child.id() as i32;
    // SAFETY: sinal para o processo que este daemon criou.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !matches!(e.child.try_wait(), Ok(None)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if matches!(e.child.try_wait(), Ok(None)) {
        let _ = e.child.kill();
    }
    let _ = e.child.wait();
    // O que sobrar na slice (contêineres, containerd) vai junto, e a slice
    // sai do systemd do usuário.
    if let Some(cg) = &e.cgroup {
        let _ = std::fs::write(cg.join("cgroup.kill"), "1");
        if let Some(name) = cg.file_name().and_then(|n| n.to_str()) {
            let _ = systemctl().args(["--user", "stop", name]).output();
        }
    }
    if let Some(g) = e.gateway.take() {
        g.shutdown(Duration::from_secs(1));
    }
    let _ = std::fs::remove_file(&e.policy_file);
    let _ = std::fs::remove_dir_all(&e.dirs.state);
}

/// Portas publicadas pelos contêineres, pela API.
fn container_ports(sock: &Path) -> Vec<sandmux_proto::msg::PortInfo> {
    let mut out = Vec::new();
    let Ok(mut s) = UnixStream::connect(sock) else {
        return out;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    if s.write_all(b"GET /containers/json HTTP/1.0\r\nHost: docker\r\n\r\n")
        .is_err()
    {
        return out;
    }
    let mut body = String::new();
    let _ = s.read_to_string(&mut body);
    let json = body.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    let Ok(serde_json::Value::Array(list)) = serde_json::from_str(json) else {
        return out;
    };
    for c in list {
        let name = c["Names"][0]
            .as_str()
            .unwrap_or("")
            .trim_start_matches('/')
            .to_string();
        for p in c["Ports"].as_array().into_iter().flatten() {
            let (Some(g), Some(h)) = (p["PrivatePort"].as_u64(), p["PublicPort"].as_u64()) else {
                continue;
            };
            let info = sandmux_proto::msg::PortInfo {
                guest: g as u16,
                host: h as u16,
                source: name.clone(),
                addr: None,
            };
            if !out.contains(&info) {
                out.push(info);
            }
        }
    }
    out
}

/// Quantos contêineres estão rodando, pela API.
fn count_containers(sock: &Path) -> Option<u64> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    s.write_all(b"GET /containers/json HTTP/1.0\r\nHost: docker\r\n\r\n")
        .ok()?;
    let mut body = String::new();
    s.read_to_string(&mut body).ok()?;
    Some(body.matches("\"Id\":").count() as u64)
}

/// Socket do Docker de uma sessão: aceita as conexões do sandbox e repassa
/// ao Docker da pasta, subindo-o na primeira.
pub struct SessionSocket {
    pub path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for SessionSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Cria o socket de uma sessão na pasta `workdir`.
pub fn session_socket(shared: &Arc<Shared>, workdir: &Path) -> std::io::Result<SessionSocket> {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = paths::runtime_dir().join("s");
    private_dir(&dir)?;
    let path = dir.join(format!(
        "{}-{}.sock",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let (s2, stop2, wd) = (shared.clone(), stop.clone(), workdir.to_path_buf());
    std::thread::Builder::new()
        .name("docker-sock".into())
        .spawn(move || accept_loop(s2, listener, stop2, wd))?;
    Ok(SessionSocket { path, stop })
}

fn accept_loop(shared: Arc<Shared>, l: UnixListener, stop: Arc<AtomicBool>, workdir: PathBuf) {
    while !stop.load(Ordering::SeqCst) {
        match l.accept() {
            Ok((c, _)) => {
                let _ = c.set_nonblocking(false);
                let (s, w) = (shared.clone(), workdir.clone());
                let _ = std::thread::Builder::new()
                    .name("docker-conn".into())
                    .spawn(move || serve(&s, c, &w));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn serve(shared: &Shared, client: UnixStream, workdir: &Path) {
    match shared.docker.ensure(shared, workdir).and_then(|sock| {
        UnixStream::connect(sock).map_err(|e| format!("conectar ao Docker privado: {e}"))
    }) {
        Ok(upstream) => splice(client, upstream),
        Err(msg) => refuse(client, &msg),
    }
}

/// Copia os bytes nos dois sentidos até um dos lados fechar.
fn splice(a: UnixStream, b: UnixStream) {
    let (Ok(a2), Ok(b2)) = (a.try_clone(), b.try_clone()) else {
        return;
    };
    let t = std::thread::spawn(move || {
        let (mut r, mut w) = (a2, b2);
        let _ = std::io::copy(&mut r, &mut w);
        let _ = w.shutdown(std::net::Shutdown::Write);
    });
    let (mut r, mut w) = (b, a);
    let _ = std::io::copy(&mut r, &mut w);
    let _ = w.shutdown(std::net::Shutdown::Write);
    let _ = t.join();
}

/// Responde ao pedido com o formato de erro da API do Docker.
fn refuse(mut c: UnixStream, msg: &str) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0u8; 8192];
    let mut head = Vec::new();
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 65536 {
        match c.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let body = format!(
        "{{\"message\":{}}}",
        serde_json::to_string(msg).unwrap_or_else(|_| "\"erro\"".into())
    );
    let _ = write!(
        c,
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
