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

use crate::{landlock_rules, mounts, seccomp, sys};

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

/// Confere o suporte mínimo do kernel antes de qualquer montagem.
pub fn check_kernel() -> Result<(), LaunchError> {
    let abi = sys::landlock_abi();
    // Só pode tornar a exigência mais rígida; usado pelos testes para simular um
    // kernel antigo.
    let required = std::env::var("SANDMUX_RUN_REQUIRE_LANDLOCK_ABI")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .map_or(landlock_rules::REQUIRED_ABI, |v| {
            v.max(landlock_rules::REQUIRED_ABI)
        });
    if abi < required {
        let found = if abi < 0 {
            "ausente".to_string()
        } else {
            format!("ABI {abi}")
        };
        return Err(LaunchError::Unsupported(format!(
            "o kernel não oferece Landlock com escopo de sockets abstratos e de sinais (exige ABI {required}, encontrado: {found}); rode `sandmux doctor`"
        )));
    }
    Ok(())
}

/// Lança o comando da especificação dentro do sandbox e devolve o código de
/// saída dele.
///
/// Precisa ser chamado num processo com uma única thread.
pub fn launch(spec: &LaunchSpec) -> Result<i32, LaunchError> {
    check_kernel()?;
    // Valida o filtro antes de criar qualquer processo.
    seccomp::build(&spec.seccomp).map_err(LaunchError::Setup)?;

    let (sync_r, sync_w) =
        pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(|e| LaunchError::Setup(e.to_string()))?;
    let (err_r, err_w) =
        pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(|e| LaunchError::Setup(e.to_string()))?;

    let mask = forwarded_signals();
    sigprocmask(SigmaskHow::SIG_BLOCK, Some(&mask), None)
        .map_err(|e| LaunchError::Setup(e.to_string()))?;
    ignore(Signal::SIGINT);
    ignore(Signal::SIGQUIT);

    let flags = libc::CLONE_NEWUSER
        | libc::CLONE_NEWNS
        | libc::CLONE_NEWPID
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUTS;
    // SAFETY: o lançador tem uma única thread neste ponto.
    let pid = match unsafe { sys::clone_fork(flags) } {
        Ok(p) => p,
        Err(e) => {
            return Err(LaunchError::Unsupported(format!(
                "não foi possível criar os namespaces do sandbox ({e}); user namespaces sem privilégio podem estar bloqueados; rode `sandmux doctor`"
            )));
        }
    };
    if pid == 0 {
        drop(sync_w);
        drop(err_r);
        init_process(spec, sync_r, err_w);
    }
    let child = Pid::from_raw(pid);
    drop(sync_r);
    drop(err_w);

    if let Err(e) = write_id_maps(child) {
        let _ = kill(child, Signal::SIGKILL);
        let _ = waitpid(child, None);
        return Err(LaunchError::Unsupported(format!(
            "não foi possível mapear o usuário no user namespace ({e}); rode `sandmux doctor`"
        )));
    }
    let mut sync_w = fs::File::from(sync_w);
    let _ = sync_w.write_all(b"1");
    drop(sync_w);

    let code = supervise(child, &mask);

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

fn report(err_w: &OwnedFd, kind: u8, msg: &str) {
    let mut buf = vec![kind];
    buf.extend_from_slice(msg.as_bytes());
    // SAFETY: descritor válido e buffer válido; escrita única e curta.
    unsafe { libc::write(err_w.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
}

/// Processo 1 do sandbox. Nunca retorna.
fn init_process(spec: &LaunchSpec, sync_r: OwnedFd, err_w: OwnedFd) -> ! {
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

    if let Err(e) = mounts::setup_root(spec) {
        report(
            &err_w,
            MSG_SETUP,
            &format!("falha ao montar o confinamento: {e}"),
        );
        // SAFETY: saída imediata do filho.
        unsafe { libc::_exit(EXIT_SETUP) };
    }

    // SAFETY: o processo 1 tem uma única thread.
    let cmd = match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => command_process(spec, &err_w),
        Ok(nix::unistd::ForkResult::Parent { child }) => child,
        Err(e) => {
            report(&err_w, MSG_SETUP, &format!("fork do comando: {e}"));
            // SAFETY: saída imediata do filho.
            unsafe { libc::_exit(EXIT_SETUP) };
        }
    };
    drop(err_w);

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
fn command_process(spec: &LaunchSpec, err_w: &OwnedFd) -> ! {
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
            format!("entrar na pasta inicial {}: {e}", spec.cwd.display()),
        );
    }
    if let Err(e) = sys::drop_all_capabilities() {
        fail(MSG_SETUP, format!("descartar capabilities: {e}"));
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
    if let Err(e) = sys::cloexec_from(3) {
        fail(MSG_SETUP, format!("fechar descritores herdados: {e}"));
    }

    let program = match resolve_program(&spec.command[0], spec.env_value("PATH")) {
        Some(p) => p,
        None => fail(
            MSG_EXEC,
            format!("comando não encontrado no sandbox: {}", spec.command[0]),
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
    fail(MSG_EXEC, format!("executar {}: {err}", program.display()));
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
