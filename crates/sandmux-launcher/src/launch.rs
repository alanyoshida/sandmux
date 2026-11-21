//! Orquestração dos processos do sandbox.
//!
//! ```text
//! supervisor (fora)      espera, repassa TERM e HUP, devolve o código de saída
//!   `-- processo 1       namespaces novos; monta a raiz; recolhe órfãos
//!         `-- comando    sem capabilities, no_new_privs, Landlock, seccomp, exec
//! ```
//!
//! O supervisor cria o processo 1 com `clone` já nos namespaces novos e
//! escreve os mapas de UID e GID dele; um pipe sincroniza os dois. Erros de
//! montagem e de `exec` voltam ao supervisor por outro pipe, para que ele
//! termine com uma mensagem clara e sem executar nada.

use std::ffi::{CString, OsStr};
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use nix::sys::signal::{SigSet, SigmaskHow, Signal, kill, sigprocmask};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, getgid, getuid, pipe2};
use sandmux_spec::LaunchSpec;

use crate::{connector, landlock_rules, mounts, net, seccomp, sys};

/// Código de saída quando o confinamento não pôde ser montado.
pub const EXIT_SETUP: i32 = 125;
/// Código de saída quando o comando não pôde ser executado.
pub const EXIT_EXEC: i32 = 127;

/// Por que um lançamento não chegou a executar o comando.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Setup(String),
    #[error("{0}")]
    Exec(String),
}

impl LaunchError {
    pub fn exit_code(&self) -> i32 {
        match self {
            LaunchError::Exec(_) => EXIT_EXEC,
            _ => EXIT_SETUP,
        }
    }
}

const MSG_SETUP: u8 = b'S';
const MSG_EXEC: u8 = b'X';

fn forwarded_signals() -> SigSet {
    let mut s = SigSet::empty();
    s.add(Signal::SIGCHLD);
    s.add(Signal::SIGTERM);
    s.add(Signal::SIGHUP);
    s
}

/// Como o kernel confina o sandbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelLevel {
    /// O Landlock dá tudo (ABI 6 ou mais).
    Landlock,
    /// O Landlock na ABI dada (4 ou 5), com o auxiliar root completando os
    /// escopos e, na ABI 4, a restrição de `ioctl` em dispositivos.
    Helper { abi: i32 },
}

/// Confere o suporte mínimo do kernel antes de qualquer montagem.
pub fn check_kernel() -> Result<KernelLevel, LaunchError> {
    let abi = landlock_rules::effective_abi();
    // Só pode tornar a exigência mais rígida; usado pelos testes para simular um
    // kernel antigo.
    let required = std::env::var("SANDMUX_RUN_REQUIRE_LANDLOCK_ABI")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .map_or(landlock_rules::REQUIRED_ABI, |v| {
            v.max(landlock_rules::REQUIRED_ABI)
        });
    if abi >= required {
        return Ok(KernelLevel::Landlock);
    }
    if required == landlock_rules::REQUIRED_ABI && abi >= landlock_rules::MIN_ABI {
        return Ok(KernelLevel::Helper { abi });
    }
    let found = if abi < 0 {
        sandmux_spec::tr!("ausente", "missing").to_string()
    } else {
        format!("ABI {abi}")
    };
    Err(LaunchError::Unsupported(sandmux_spec::trf!(
        "o kernel não oferece Landlock com escopo de sockets abstratos e de sinais (exige ABI {required}, ou a {} com o auxiliar root; encontrado: {found}); rode `sandmux doctor`",
        "the kernel does not offer Landlock with abstract socket and signal scoping (requires ABI {required}, or {} with the root helper; found: {found}); run `sandmux doctor`",
        landlock_rules::MIN_ABI
    )))
}

/// Lança o comando da especificação dentro do sandbox e devolve o código de
/// saída dele.
///
/// Precisa ser chamado num processo com uma única thread.
pub fn launch(spec: &LaunchSpec) -> Result<i32, LaunchError> {
    let level = check_kernel()?;
    // Valida o filtro antes de criar qualquer processo.
    seccomp::build(&spec.seccomp).map_err(LaunchError::Setup)?;

    let (sync_r, sync_w) =
        pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(|e| LaunchError::Setup(e.to_string()))?;
    let (err_r, err_w) =
        pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(|e| LaunchError::Setup(e.to_string()))?;
    // O TUN do modo aberta volta do filho por este par.
    let open = spec
        .network
        .as_ref()
        .is_some_and(|n| n.mode == sandmux_spec::NetMode::Open);
    let (tun_r, tun_w) = if open {
        let (r, w) = net::pair().map_err(|e| LaunchError::Setup(e.to_string()))?;
        (Some(r), Some(w))
    } else {
        (None, None)
    };

    // O descritor do filtro das aberturas volta do comando por este par.
    let (files_r, files_w) = net::pair().map_err(|e| LaunchError::Setup(e.to_string()))?;

    let mask = forwarded_signals();
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&mask), None)
        .map_err(|e| LaunchError::Setup(e.to_string()))?;
    ignore(Signal::SIGINT);
    ignore(Signal::SIGQUIT);

    let mut flags = libc::CLONE_NEWUSER
        | libc::CLONE_NEWNS
        | libc::CLONE_NEWPID
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUTS;
    if spec.network.is_some() {
        flags |= libc::CLONE_NEWNET;
    }
    // SAFETY: o lançador tem uma única thread neste ponto.
    let pid = match unsafe { sys::clone_fork(flags) } {
        Ok(p) => p,
        Err(e) => {
            return Err(LaunchError::Unsupported(sandmux_spec::trf!(
                "não foi possível criar os namespaces do sandbox ({e}); user namespaces sem privilégio podem estar bloqueados; rode `sandmux doctor`",
                "could not create the sandbox namespaces ({e}); unprivileged user namespaces may be blocked; run `sandmux doctor`"
            )));
        }
    };
    if pid == 0 {
        drop(sync_w);
        drop(err_r);
        drop(tun_r);
        drop(files_r);
        init_process(spec, sync_r, err_w, tun_w, files_w);
    }
    let child = Pid::from_raw(pid);
    drop(sync_r);
    drop(err_w);
    drop(tun_w);
    drop(files_w);

    if let Err(e) = write_id_maps(child) {
        let _ = kill(child, Signal::SIGKILL);
        let _ = waitpid(child, None);
        return Err(LaunchError::Unsupported(sandmux_spec::trf!(
            "não foi possível mapear o usuário no user namespace ({e}); rode `sandmux doctor`",
            "could not map the user in the user namespace ({e}); run `sandmux doctor`"
        )));
    }
    // Sem o Landlock ABI 6, o auxiliar root completa o confinamento: o
    // sandbox é registrado antes de o comando rodar, e a conexão fica aberta
    // até o fim (fechada, o registro sai). A vigia do escopo (depois da
    // liberação) registra de novo quando o auxiliar cai, ou deixa para o
    // daemon.
    let scope_reg = match level {
        KernelLevel::Landlock => None,
        KernelLevel::Helper { abi } => match crate::scope::register(child, abi, spec) {
            Ok(c) => Some(c),
            Err(e) => {
                let _ = kill(child, Signal::SIGKILL);
                let _ = waitpid(child, None);
                return Err(LaunchError::Unsupported(sandmux_spec::trf!(
                    "o kernel tem o Landlock ABI {abi}, e o auxiliar root não completou o confinamento: {e}; rode `sandmux doctor`",
                    "the kernel has Landlock ABI {abi}, and the root helper did not complete the confinement: {e}; run `sandmux doctor`"
                )));
            }
        },
    };
    // O auxiliar de conexão das portas precisa de uma só thread e do /proc
    // do processo 1 aberto, antes de ele se fechar: nasce antes da liberação.
    let connector = if open {
        // SAFETY: o lançador ainda tem uma única thread.
        unsafe { connector::spawn(child) }.ok()
    } else {
        None
    };
    // O auxiliar de nomes também: entra nos namespaces antes da liberação.
    // SAFETY: o lançador ainda tem uma única thread.
    let namer = unsafe { crate::namer::spawn(child, &spec.landlock) }
        .ok()
        .map(|n| std::sync::Arc::new(std::sync::Mutex::new(n)));
    // E o de montagem, nas sessões com socket de controle (pastas ao vivo).
    let control = spec.files.as_ref().and_then(|f| f.control_socket.clone());
    let mounter = if control.is_some() {
        // SAFETY: o lançador ainda tem uma única thread.
        unsafe { crate::mounter::spawn(child) }.ok()
    } else {
        None
    };
    // E o de entrada (shell dentro do sandbox, Ctrl+b t), também só nas
    // sessões do daemon. As aberturas do shell voltam por `enter_files`.
    let (enter_files, enterer) = match (&control, net::pair()) {
        (Some(_), Ok((r, w))) => {
            // SAFETY: o lançador ainda tem uma única thread.
            let e = unsafe { crate::enter::spawn(child, spec, w) }.ok();
            (Some(r), e)
        }
        _ => (None, None),
    };
    let mut sync_w = fs::File::from(sync_w);
    let _ = sync_w.write_all(b"1");
    drop(sync_w);

    // Quem refaz o escopo numa queda do auxiliar: numa sessão do daemon, a
    // resposta dele decide; sem o daemon, o lançador.
    let keeper = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(
        if kernel_files(spec).is_some() {
            crate::scope::KEEPER_UNKNOWN
        } else {
            crate::scope::KEEPER_SELF
        },
    ));
    let scope_info = scope_reg.as_ref().map(|r| (r.params.clone(), r.ns.clone()));
    if let Some(r) = scope_reg {
        crate::scope::keep(r, child, keeper.clone());
    }

    // O modo dos arquivos da aba, para os shells de entrada: eles seguem o
    // comando.
    let enter_mode =
        std::sync::Arc::new(std::sync::atomic::AtomicU8::new(crate::enter::MODE_UNKNOWN));

    // As aberturas de arquivo do comando: o descritor do filtro chega quando
    // o comando o instala; uma thread espera por ele.
    {
        let spec = spec.clone();
        let child_pid = child.as_raw();
        let keeper = keeper.clone();
        let enter_mode = enter_mode.clone();
        let _ = std::thread::Builder::new()
            .name("sandmux-files".into())
            .spawn(move || {
                // Os arquivos pelo kernel e o escopo: com a raiz pronta, o
                // processo 1 pergunta o modo antes de criar o comando.
                if let Some((access, tag, f)) = kernel_files(&spec)
                    && recv_byte(&files_r) == Some(MODE_ASK)
                {
                    let a = confine_tab(child_pid, access, tag, f, scope_info.as_ref());
                    keeper.store(
                        if a.scope {
                            crate::scope::KEEPER_DAEMON
                        } else {
                            crate::scope::KEEPER_SELF
                        },
                        std::sync::atomic::Ordering::SeqCst,
                    );
                    send_byte(
                        &files_r,
                        if a.kernel.is_ok() {
                            MODE_KERNEL
                        } else {
                            MODE_SUPERVISOR
                        },
                    );
                    enter_mode.store(
                        if a.kernel.is_ok() {
                            crate::enter::MODE_KERNEL
                        } else {
                            crate::enter::MODE_SUPERVISOR
                        },
                        std::sync::atomic::Ordering::SeqCst,
                    );
                } else {
                    keeper.store(
                        crate::scope::KEEPER_SELF,
                        std::sync::atomic::Ordering::SeqCst,
                    );
                    enter_mode.store(
                        crate::enter::MODE_SUPERVISOR,
                        std::sync::atomic::Ordering::SeqCst,
                    );
                }
                if let Ok(Some(listener)) = net::recv_fd(&files_r) {
                    // A raiz está pronta: o auxiliar de nomes aplica o Landlock.
                    if let Some(n) = &namer {
                        n.lock().unwrap_or_else(|e| e.into_inner()).ready();
                    }
                    let _ = crate::files::spawn(listener, &spec, namer.clone());
                }
                // As aberturas de cada shell de entrada, com o mesmo auxiliar.
                if let Some(r) = enter_files {
                    while let Ok(Some(listener)) = net::recv_fd(&r) {
                        let _ = crate::files::spawn(listener, &spec, namer.clone());
                    }
                }
            });
    }

    // Controle das pastas ao vivo: o daemon manda o pedido do auxiliar de
    // montagem, numa linha JSON, e recebe a resposta.
    if let (Some(path), Some(m)) = (control.clone(), mounter) {
        let _ = std::fs::remove_file(&path);
        if let Ok(l) = std::os::unix::net::UnixListener::bind(&path) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _ = std::thread::Builder::new()
                .name("sandmux-control".into())
                .spawn(move || {
                    for c in l.incoming().flatten() {
                        let mut line = String::new();
                        let mut r = std::io::BufReader::new(&c);
                        if std::io::BufRead::read_line(&mut r, &mut line).is_err() {
                            continue;
                        }
                        let reply = match serde_json::from_str::<crate::mounter::Request>(&line) {
                            Ok(req) => m.call(&req),
                            Err(e) => Err(sandmux_spec::trf!(
                                "pedido inválido: {e}",
                                "invalid request: {e}"
                            )),
                        };
                        let out =
                            serde_json::to_string(&crate::mounter::Reply { error: reply.err() })
                                .unwrap_or_default();
                        let _ = (&c).write_all(format!("{out}\n").as_bytes());
                    }
                });
        }
    }

    // Entrada: quem pede manda o terminal pelo socket `<controle>.enter` e
    // recebe o código de saída do shell pela mesma conexão.
    let enter_path = control.as_ref().map(|p| p.with_extension("enter"));
    if let (Some(path), Some(en)) = (enter_path.clone(), enterer) {
        let _ = std::fs::remove_file(&path);
        if let Ok(l) = std::os::unix::net::UnixListener::bind(&path) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _ = std::thread::Builder::new()
                .name("sandmux-enter".into())
                .spawn(move || {
                    for c in l.incoming().flatten() {
                        let c = OwnedFd::from(c);
                        if let Ok(Some(pty)) = net::recv_fd(&c) {
                            // Um pedido antes da resposta do daemon espera o
                            // modo; sem ele, o supervisor.
                            let kernel =
                                crate::enter::wait_mode(&enter_mode, crate::enter::MODE_WAIT);
                            let _ = en.call(&c, &pty, kernel);
                        }
                    }
                });
        }
    }

    // O gateway nasce depois do clone: até aqui o lançador tem uma só thread.
    // As threads dele herdam a máscara de sinais bloqueados.
    let mut gateway = None;
    if let (Some(r), Some(n)) = (tun_r, spec.network.as_ref())
        && let Ok(Some(tun)) = net::recv_fd(&r)
    {
        // O socket de sock_diag, quando o filho conseguiu criar (o canal
        // fecha logo depois do TUN).
        let sock_diag = net::recv_fd(&r).ok().flatten();
        let cfg = sandmux_net::Config {
            log_dir: n.log_dir.clone(),
            tag: n.tag.clone(),
            workdir: n.workdir.clone(),
            nameservers: n
                .nameservers
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
            policy: n.policy.clone(),
            policy_file: n.policy_file.clone(),
            ports: connector.map(|c| sandmux_net::ports::Ports {
                pid: child.as_raw(),
                file: n.ports_file.clone(),
                connector: std::sync::Arc::new(sandmux_net::ports::Connector::new(c)),
            }),
            aliases: Vec::new(),
            tls: n.tls.clone(),
            vault: n.vault.clone(),
            harness: n.harness.clone(),
            sandbox_pid: Some(child.as_raw() as u32),
            sock_diag,
        };
        match sandmux_net::spawn(tun, cfg) {
            Ok(g) => gateway = Some(g),
            Err(e) => {
                let _ = kill(child, Signal::SIGKILL);
                let _ = waitpid(child, None);
                return Err(LaunchError::Setup(sandmux_spec::trf!(
                    "não foi possível subir o gateway de rede: {e}",
                    "could not start the network gateway: {e}"
                )));
            }
        }
    }

    let code = supervise(child, &mask);
    if let Some(p) = &control {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = &enter_path {
        let _ = fs::remove_file(p);
    }
    // As conexões que ainda estavam abertas gravam a linha delas.
    if let Some(g) = gateway {
        g.shutdown(std::time::Duration::from_secs(1));
    }

    let mut msg = Vec::new();
    let _ = fs::File::from(err_r).read_to_end(&mut msg);
    match msg.split_first() {
        Some((&MSG_SETUP, rest)) => Err(LaunchError::Setup(
            String::from_utf8_lossy(rest).into_owned(),
        )),
        Some((&MSG_EXEC, rest)) => Err(LaunchError::Exec(
            String::from_utf8_lossy(rest).into_owned(),
        )),
        _ => Ok(code),
    }
}

fn ignore(sig: Signal) {
    // SAFETY: troca de disposição para SIG_IGN, sem handler.
    unsafe {
        let _ = nix::sys::signal::signal(sig, nix::sys::signal::SigHandler::SigIgn);
    }
}

fn default(sig: Signal) {
    // SAFETY: troca de disposição para SIG_DFL, sem handler.
    unsafe {
        let _ = nix::sys::signal::signal(sig, nix::sys::signal::SigHandler::SigDfl);
    }
}

fn write_id_maps(child: Pid) -> std::io::Result<()> {
    let (uid, gid) = (getuid().as_raw(), getgid().as_raw());
    let base = PathBuf::from(format!("/proc/{}", child.as_raw()));
    fs::write(base.join("setgroups"), "deny")?;
    fs::write(base.join("uid_map"), format!("{uid} {uid} 1\n"))?;
    fs::write(base.join("gid_map"), format!("{gid} {gid} 1\n"))?;
    Ok(())
}

/// Espera o processo 1 terminar, repassando TERM e HUP.
fn supervise(child: Pid, mask: &SigSet) -> i32 {
    loop {
        match mask.wait() {
            Ok(Signal::SIGCHLD) => loop {
                match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(_, code)) => return code,
                    Ok(WaitStatus::Signaled(_, sig, _)) => return 128 + sig as i32,
                    Ok(WaitStatus::StillAlive) => break,
                    Ok(_) => continue,
                    Err(nix::errno::Errno::EINTR) => continue,
                    Err(_) => return EXIT_SETUP,
                }
            },
            Ok(sig) => {
                let _ = kill(child, sig);
            }
            Err(_) => {}
        }
    }
}

/// O processo 1 pergunta o modo dos arquivos; o supervisor responde.
const MODE_ASK: u8 = b'R';
const MODE_KERNEL: u8 = b'2';
const MODE_SUPERVISOR: u8 = b'1';

/// Os arquivos da aba podem ir ao kernel: uma sessão do daemon (com o
/// socket de controle) e o acesso ao daemon. O acesso, a etiqueta e as
/// pastas do registro.
fn kernel_files(
    spec: &LaunchSpec,
) -> Option<(&sandmux_spec::VaultAccess, &str, &sandmux_spec::FilesSpec)> {
    let f = spec.files.as_ref()?;
    f.control_socket.as_ref()?;
    let n = spec.network.as_ref()?;
    Some((n.vault.as_ref()?, n.tag.as_str(), f))
}

/// Pede ao daemon os arquivos da aba pelo kernel, com as montagens das
/// pastas do registro na visão do processo 1, e a vigia do escopo. Sem
/// montagens que caibam no mapa, vão vazias: os arquivos ficam com o
/// supervisor, e o escopo ainda vai.
fn confine_tab(
    pid: i32,
    access: &sandmux_spec::VaultAccess,
    tag: &str,
    f: &sandmux_spec::FilesSpec,
    scope: Option<&(crate::scope::TabScope, std::sync::Arc<OwnedFd>)>,
) -> sandmux_net::procinfo::TabAnswer {
    let mut mounts = crate::viewmounts::of_pid(pid, &f.roots, &f.exclude);
    if mounts.len() > crate::viewmounts::MAX_MOUNTS {
        mounts.clear();
    }
    sandmux_net::procinfo::confine_tab(
        access,
        sandmux_net::procinfo::TabRequest {
            tag,
            mounts,
            roots: f.roots.clone(),
            exclude: f.exclude.clone(),
            scope: scope.map(|(p, ns)| (p.clone(), &**ns)),
        },
    )
}

fn send_byte(sock: &OwnedFd, b: u8) -> bool {
    // SAFETY: descritor válido, um byte de um buffer vivo.
    unsafe { libc::send(sock.as_raw_fd(), [b].as_ptr().cast(), 1, libc::MSG_NOSIGNAL) == 1 }
}

fn recv_byte(sock: &OwnedFd) -> Option<u8> {
    let mut b = [0u8];
    loop {
        // SAFETY: descritor válido, buffer de um byte.
        let n = unsafe { libc::recv(sock.as_raw_fd(), b.as_mut_ptr().cast(), 1, 0) };
        if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return (n == 1).then_some(b[0]);
    }
}

fn report(err_w: &OwnedFd, kind: u8, msg: &str) {
    let mut buf = vec![kind];
    buf.extend_from_slice(msg.as_bytes());
    // SAFETY: descritor válido e buffer válido; escrita única e curta.
    unsafe { libc::write(err_w.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
}

/// Processo 1 do sandbox. Nunca retorna.
fn init_process(
    spec: &LaunchSpec,
    sync_r: OwnedFd,
    err_w: OwnedFd,
    tun_w: Option<OwnedFd>,
    files_w: OwnedFd,
) -> ! {
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) };
    // Dentro do pid namespace novo, getppid() devolve 0. Se o supervisor tiver
    // morrido antes do PDEATHSIG, a leitura abaixo devolve fim de arquivo.
    let mut b = [0u8; 1];
    let ok = matches!(fs::File::from(sync_r).read(&mut b), Ok(1));
    if !ok {
        // SAFETY: saída imediata do filho.
        unsafe { libc::_exit(EXIT_SETUP) };
    }
    // O processo 1 é uma cópia do supervisor: guarda na memória o ambiente
    // completo do usuário e a especificação. Sem isto, o comando, com as mesmas
    // credenciais e sem capabilities, poderia ler /proc/1/environ e /proc/1/mem.
    // Só depois da sincronização: o supervisor precisa escrever os mapas de UID.
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };

    // A rede antes da raiz nova: o /dev/net/tun do host ainda está visível.
    if let Some(n) = &spec.network {
        let res =
            net::setup(n.mode == sandmux_spec::NetMode::Open).and_then(|tun| match (tun, &tun_w) {
                (Some(t), Some(w)) => {
                    net::send_fd(w, &t)?;
                    // Um socket de sock_diag criado aqui fica preso ao
                    // namespace de rede do sandbox: o gateway acha o socket
                    // de cada conexão pela consulta exata do kernel, sem ler
                    // a tabela inteira de /proc/<pid>/net. Sem ele, o
                    // gateway lê o /proc.
                    if let Ok(d) = net::diag_socket() {
                        let _ = net::send_fd_tagged(w, &d, b'D');
                    }
                    Ok(())
                }
                _ => Ok(()),
            });
        if let Err(e) = res {
            report(
                &err_w,
                MSG_SETUP,
                &sandmux_spec::trf!(
                    "falha ao montar a rede: {e}",
                    "failed to set up the network: {e}"
                ),
            );
            // SAFETY: saída imediata do filho.
            unsafe { libc::_exit(EXIT_SETUP) };
        }
    }
    drop(tun_w);

    if let Err(e) = mounts::setup_root(spec) {
        report(
            &err_w,
            MSG_SETUP,
            &sandmux_spec::trf!(
                "falha ao montar o confinamento: {e}",
                "failed to set up the confinement: {e}"
            ),
        );
        // SAFETY: saída imediata do filho.
        unsafe { libc::_exit(EXIT_SETUP) };
    }

    // O barramento do portal, criado dentro do sandbox antes do comando.
    if let Some(r) = &spec.bus_relay {
        // SAFETY: o processo 1 tem uma única thread.
        if let Err(e) = unsafe { crate::relay::spawn(r, &spec.seccomp) } {
            report(
                &err_w,
                MSG_SETUP,
                &sandmux_spec::trf!(
                    "falha ao criar o barramento do portal: {e}",
                    "failed to create the portal bus: {e}"
                ),
            );
            // SAFETY: saída imediata do filho.
            unsafe { libc::_exit(EXIT_SETUP) };
        }
    }

    // Os arquivos pelo kernel: com a raiz pronta, o supervisor pede ao daemon
    // e responde o modo. Sem resposta, o supervisor de aberturas.
    let kernel = kernel_files(spec).is_some()
        && send_byte(&files_w, MODE_ASK)
        && recv_byte(&files_w) == Some(MODE_KERNEL);

    // SAFETY: o processo 1 tem uma única thread.
    let cmd = match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => command_process(spec, &err_w, &files_w, kernel),
        Ok(nix::unistd::ForkResult::Parent { child }) => child,
        Err(e) => {
            report(
                &err_w,
                MSG_SETUP,
                &sandmux_spec::trf!("fork do comando: {e}", "fork of the command: {e}"),
            );
            // SAFETY: saída imediata do filho.
            unsafe { libc::_exit(EXIT_SETUP) };
        }
    };
    drop(err_w);
    drop(files_w);

    // O processo 1 não precisa de mais nada: sem capabilities e com o mesmo
    // filtro do comando.
    let _ = sys::drop_all_capabilities();
    let _ = sys::set_no_new_privs();
    let _ = seccomp::install(&spec.seccomp);

    let mask = forwarded_signals();
    let code = loop {
        match mask.wait() {
            Ok(Signal::SIGCHLD) => {
                let mut done = None;
                loop {
                    match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::Exited(p, c)) if p == cmd => done = Some(c),
                        Ok(WaitStatus::Signaled(p, s, _)) if p == cmd => {
                            done = Some(128 + s as i32)
                        }
                        Ok(WaitStatus::StillAlive) | Err(_) => break,
                        Ok(_) => continue,
                    }
                }
                if let Some(c) = done {
                    break c;
                }
            }
            Ok(sig) => {
                let _ = kill(cmd, sig);
            }
            Err(_) => {}
        }
    };
    // Ao terminar, o processo 1 leva com ele tudo que restou no pid namespace.
    // SAFETY: saída do processo 1.
    unsafe { libc::_exit(code) };
}

/// Processo do comando. Nunca retorna.
fn command_process(spec: &LaunchSpec, err_w: &OwnedFd, files_w: &OwnedFd, kernel: bool) -> ! {
    let fail = |kind: u8, msg: String| -> ! {
        report(err_w, kind, &msg);
        // SAFETY: saída imediata do filho.
        unsafe {
            libc::_exit(if kind == MSG_EXEC {
                EXIT_EXEC
            } else {
                EXIT_SETUP
            })
        };
    };

    let _ = sigprocmask(SigmaskHow::SIG_SETMASK, Some(&SigSet::empty()), None);
    for sig in [
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGCHLD,
        Signal::SIGPIPE,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
    ] {
        default(sig);
    }
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) };

    if let Err(e) = nix::unistd::chdir(&spec.cwd) {
        fail(
            MSG_SETUP,
            sandmux_spec::trf!(
                "entrar na pasta inicial {}: {e}",
                "enter the initial folder {}: {e}",
                spec.cwd.display()
            ),
        );
    }
    if let Err(e) = sys::drop_all_capabilities() {
        fail(
            MSG_SETUP,
            sandmux_spec::trf!("descartar capabilities: {e}", "drop capabilities: {e}"),
        );
    }
    if let Err(e) = sys::set_no_new_privs() {
        fail(MSG_SETUP, format!("no_new_privs: {e}"));
    }
    if let Err(e) = landlock_rules::apply(&spec.landlock) {
        fail(MSG_SETUP, format!("Landlock: {e}"));
    }
    if let Err(e) = seccomp::install(&spec.seccomp) {
        fail(MSG_SETUP, format!("seccomp: {e}"));
    }
    // As aberturas de arquivo passam pelo supervisor, menos com os arquivos
    // pelo kernel (aí só as execuções, com o registro delas pelo supervisor).
    // O descritor vai logo depois da instalação: uma abertura antes dele
    // esperaria o supervisor.
    let exec = spec.files.as_ref().is_some_and(|f| f.exec_log);
    match seccomp::install_files_notify(kernel, exec) {
        Ok(None) => {}
        Ok(Some(listener)) => {
            if let Err(e) = net::send_fd(files_w, &listener) {
                fail(
                    MSG_SETUP,
                    sandmux_spec::trf!(
                        "entregar o filtro das aberturas: {e}",
                        "hand over the file-open filter: {e}"
                    ),
                );
            }
        }
        Err(e) => fail(
            MSG_SETUP,
            sandmux_spec::trf!("seccomp das aberturas: {e}", "file-open seccomp: {e}"),
        ),
    }
    if let Err(e) = sys::cloexec_from(3) {
        fail(
            MSG_SETUP,
            sandmux_spec::trf!(
                "fechar descritores herdados: {e}",
                "close inherited descriptors: {e}"
            ),
        );
    }

    let program = match resolve_program(&spec.command[0], spec.env_value("PATH")) {
        Some(p) => p,
        None => fail(
            MSG_EXEC,
            sandmux_spec::trf!(
                "comando não encontrado no sandbox: {}",
                "command not found in the sandbox: {}",
                spec.command[0]
            ),
        ),
    };
    let to_c = |s: &[u8]| CString::new(s).unwrap_or_default();
    let prog_c = to_c(program.as_os_str().as_bytes());
    let argv: Vec<CString> = spec.command.iter().map(|a| to_c(a.as_bytes())).collect();
    let envp: Vec<CString> = spec
        .env
        .iter()
        .map(|v| to_c(format!("{}={}", v.name, v.value).as_bytes()))
        .collect();
    let err = nix::unistd::execve(&prog_c, &argv, &envp).unwrap_err();
    fail(
        MSG_EXEC,
        sandmux_spec::trf!("executar {}: {err}", "execute {}: {err}", program.display()),
    );
}

/// Modo de visão: monta a raiz da especificação no mount namespace atual e
/// executa o comando, sem criar namespaces e sem Landlock. É chamado dentro
/// de um user namespace que já confina o processo (o do `rootlesskit`, no
/// Docker privado): o que limita o comando é a visão de arquivos e o filtro
/// seccomp da especificação.
///
/// O filtro é instalado sem `no_new_privs`, o que o kernel aceita porque o
/// processo tem `CAP_SYS_ADMIN` no próprio user namespace. Ele passa a todos
/// os descendentes e não pode ser removido: vale para todo contêiner, mesmo
/// com `--privileged` ou `seccomp=unconfined`. O `no_new_privs` ficaria
/// herdado também, e quebraria o `su` e o `sudo` dentro dos contêineres.
pub fn exec_view(spec: &LaunchSpec) -> LaunchError {
    if let Err(e) = seccomp::build(&spec.seccomp) {
        return LaunchError::Setup(e);
    }
    // A rede antes da raiz nova, com o /dev/net/tun do host à vista: o TUN
    // vai para o daemon, que roda o gateway do Docker.
    if let Some(sock) = spec.network.as_ref().and_then(|n| n.tun_socket.as_ref()) {
        let sent = net::setup(true).and_then(|tun| {
            let tun =
                tun.ok_or_else(|| std::io::Error::other(sandmux_spec::tr!("sem TUN", "no TUN")))?;
            let s = std::os::unix::net::UnixStream::connect(sock)?;
            net::send_fd_tagged(&OwnedFd::from(s), &tun, b'T')
        });
        if let Err(e) = sent {
            return LaunchError::Setup(sandmux_spec::trf!(
                "falha ao montar a rede da visão: {e}",
                "failed to set up the view's network: {e}"
            ));
        }
    }
    if let Err(e) = mounts::setup_root(spec) {
        return LaunchError::Setup(sandmux_spec::trf!(
            "falha ao montar a visão: {e}",
            "failed to set up the view: {e}"
        ));
    }
    if let Err(e) = nix::unistd::chdir(&spec.cwd) {
        return LaunchError::Setup(sandmux_spec::trf!(
            "entrar na pasta inicial {}: {e}",
            "enter the initial folder {}: {e}",
            spec.cwd.display()
        ));
    }
    if let Err(e) = sys::cloexec_from(3) {
        return LaunchError::Setup(sandmux_spec::trf!(
            "fechar descritores herdados: {e}",
            "close inherited descriptors: {e}"
        ));
    }
    let Some(program) = resolve_program(&spec.command[0], spec.env_value("PATH")) else {
        return LaunchError::Exec(sandmux_spec::trf!(
            "comando não encontrado: {}",
            "command not found: {}",
            spec.command[0]
        ));
    };
    let to_c = |s: &[u8]| CString::new(s).unwrap_or_default();
    let prog_c = to_c(program.as_os_str().as_bytes());
    let argv: Vec<CString> = spec.command.iter().map(|a| to_c(a.as_bytes())).collect();
    // As variáveis do rootlesskit, que o dockerd usa para o modo sem root,
    // mais as da especificação.
    let inherited = std::env::vars()
        .filter(|(k, _)| k.starts_with("ROOTLESSKIT_") || k.starts_with("_ROOTLESSKIT_"))
        .filter(|(k, _)| !spec.env.iter().any(|v| &v.name == k));
    let envp: Vec<CString> = inherited
        .map(|(k, v)| to_c(format!("{k}={v}").as_bytes()))
        .chain(
            spec.env
                .iter()
                .map(|v| to_c(format!("{}={}", v.name, v.value).as_bytes())),
        )
        .collect();
    // Por último, depois das montagens: o filtro não nega montagem, mas assim
    // nada da preparação passa por ele.
    if let Err(e) = seccomp::install(&spec.seccomp) {
        return LaunchError::Setup(sandmux_spec::trf!(
            "falha ao instalar o filtro seccomp da visão: {e}",
            "failed to install the view's seccomp filter: {e}"
        ));
    }
    let err = nix::unistd::execve(&prog_c, &argv, &envp).unwrap_err();
    LaunchError::Exec(sandmux_spec::trf!(
        "executar {}: {err}",
        "execute {}: {err}",
        program.display()
    ))
}

/// Procura o programa no `PATH` do ambiente do sandbox, como `execvp`.
fn resolve_program(cmd: &str, path: Option<&str>) -> Option<PathBuf> {
    if cmd.contains('/') {
        return Some(PathBuf::from(cmd));
    }
    for dir in path
        .unwrap_or("/usr/bin:/bin")
        .split(':')
        .filter(|d| !d.is_empty())
    {
        let candidate = Path::new(dir).join(OsStr::new(cmd));
        let c = CString::new(candidate.as_os_str().as_bytes()).ok()?;
        // SAFETY: string C válida.
        if unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0 && !candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}
