//! Auxiliar de entrada: um processo que fica nos namespaces de usuário e de
//! pid do sandbox e, a pedido do supervisor, abre um shell lá dentro, com a
//! mesma visão de arquivos, rede, Landlock e seccomp do comando, e com as
//! aberturas de arquivo passando pelo supervisor como as do comando.
//!
//! Cada pedido traz dois descritores: a conexão de quem pediu, que recebe o
//! código de saída do shell (4 bytes), e o terminal do shell. O auxiliar
//! nasce antes das threads do supervisor (o `setns` num namespace de usuário
//! exige um processo de uma thread só) e antes de o processo 1 fechar o
//! `/proc` dele.

use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use nix::sys::signal::{SigHandler, SigSet, SigmaskHow, Signal, signal, sigprocmask};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, fork};
use sandmux_spec::LaunchSpec;

use crate::{landlock_rules, net, seccomp, sys};

/// Lado do supervisor.
pub struct Enterer {
    sock: OwnedFd,
}

impl Enterer {
    /// Pede um shell no terminal `pty`; o código de saída vai para `conn`.
    pub fn call(&self, conn: &OwnedFd, pty: &OwnedFd) -> io::Result<()> {
        net::send_fd_tagged(&self.sock, conn, b'C')?;
        net::send_fd_tagged(&self.sock, pty, b'P')
    }
}

fn setns(f: &File, kind: libc::c_int) -> io::Result<()> {
    // SAFETY: descritor válido de um namespace.
    if unsafe { libc::setns(f.as_raw_fd(), kind) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Namespaces do sandbox, abertos pelo `/proc` do processo 1.
struct Namespaces {
    user: File,
    pid: File,
    mnt: File,
    ipc: File,
    uts: File,
    net: Option<File>,
}

/// Cria o auxiliar para o sandbox cujo processo 1 é `child`. As aberturas
/// do shell (o descritor do filtro) vão por `files_w` ao supervisor.
///
/// # Safety
/// O processo precisa ter uma única thread (`fork`).
pub unsafe fn spawn(child: Pid, spec: &LaunchSpec, files_w: OwnedFd) -> io::Result<Enterer> {
    let pid = child.as_raw();
    let open = |n: &str| File::open(format!("/proc/{pid}/ns/{n}"));
    let ns = Namespaces {
        user: open("user")?,
        pid: open("pid")?,
        mnt: open("mnt")?,
        ipc: open("ipc")?,
        uts: open("uts")?,
        net: if spec.network.is_some() {
            Some(open("net")?)
        } else {
            None
        },
    };
    let (mine, theirs) = net::pair()?;
    // SAFETY: o chamador garante uma única thread.
    match unsafe { fork() }.map_err(io::Error::from)? {
        ForkResult::Parent { .. } => {
            drop(theirs);
            Ok(Enterer { sock: mine })
        }
        ForkResult::Child => {
            drop(mine);
            helper(theirs, files_w, ns, spec)
        }
    }
}

/// O auxiliar: espera os pedidos e cria um processo por shell.
fn helper(sock: OwnedFd, files_w: OwnedFd, ns: Namespaces, spec: &LaunchSpec) -> ! {
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) };
    let _ = sigprocmask(SigmaskHow::SIG_SETMASK, Some(&SigSet::empty()), None);
    // Os processos de cada shell são recolhidos sozinhos.
    // SAFETY: troca de disposição, sem handler.
    unsafe {
        let _ = signal(Signal::SIGCHLD, SigHandler::SigIgn);
    }
    // No namespace de usuário do sandbox, com as capabilities de lá, e no de
    // pid: os filhos nascem dentro dele.
    if setns(&ns.user, libc::CLONE_NEWUSER).is_err() || setns(&ns.pid, libc::CLONE_NEWPID).is_err()
    {
        // SAFETY: saída do auxiliar.
        unsafe { libc::_exit(1) };
    }
    // A memória do auxiliar guarda a especificação: o agente, com as mesmas
    // credenciais, não lê.
    // SAFETY: prctl com argumentos numéricos.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    loop {
        let conn = match net::recv_fd(&sock) {
            Ok(Some(c)) => c,
            // O supervisor terminou.
            _ => unsafe { libc::_exit(0) },
        };
        let Ok(Some(pty)) = net::recv_fd(&sock) else {
            continue;
        };
        // SAFETY: o auxiliar tem uma única thread.
        match unsafe { fork() } {
            Ok(ForkResult::Child) => waiter(conn, pty, &files_w, &ns, spec),
            Ok(ForkResult::Parent { .. }) => {}
            Err(_) => {
                let _ = (&File::from(conn)).write_all(&1i32.to_ne_bytes());
            }
        }
    }
}

/// Dentro do pid namespace: cria o shell, espera e manda o código.
fn waiter(conn: OwnedFd, pty: OwnedFd, files_w: &OwnedFd, ns: &Namespaces, spec: &LaunchSpec) -> ! {
    // SAFETY: troca de disposição, sem handler.
    unsafe {
        let _ = signal(Signal::SIGCHLD, SigHandler::SigDfl);
    }
    // SAFETY: este processo tem uma única thread.
    let code = match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            drop(conn);
            shell(pty, files_w, ns, spec)
        }
        Ok(ForkResult::Parent { child }) => {
            drop(pty);
            loop {
                match waitpid(child, None) {
                    Ok(WaitStatus::Exited(_, c)) => break c,
                    Ok(WaitStatus::Signaled(_, s, _)) => break 128 + s as i32,
                    Err(nix::errno::Errno::EINTR) => continue,
                    Ok(_) => continue,
                    Err(_) => break 1,
                }
            }
        }
        Err(_) => 1,
    };
    let _ = (&File::from(conn)).write_all(&code.to_ne_bytes());
    // SAFETY: saída do processo de espera.
    unsafe { libc::_exit(0) };
}

/// Escreve a falha no terminal e sai.
fn fail(msg: String) -> ! {
    let line = format!("sandmux: {msg}\r\n");
    // SAFETY: escrita curta no descritor 2 (o terminal).
    unsafe { libc::write(2, line.as_ptr().cast(), line.len()) };
    // SAFETY: saída imediata do shell que não subiu.
    unsafe { libc::_exit(126) };
}

/// O shell: entra no resto dos namespaces, aplica o confinamento do comando
/// e executa.
fn shell(pty: OwnedFd, files_w: &OwnedFd, ns: &Namespaces, spec: &LaunchSpec) -> ! {
    let _ = sigprocmask(SigmaskHow::SIG_SETMASK, Some(&SigSet::empty()), None);
    for sig in [
        Signal::SIGINT,
        Signal::SIGQUIT,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGPIPE,
        Signal::SIGTSTP,
        Signal::SIGTTIN,
        Signal::SIGTTOU,
    ] {
        // SAFETY: troca de disposição, sem handler.
        unsafe {
            let _ = signal(sig, SigHandler::SigDfl);
        }
    }
    // O terminal vira o do shell, e a saída de erro já mostra as falhas.
    // SAFETY: chamadas com descritores válidos.
    unsafe {
        libc::setsid();
        libc::ioctl(pty.as_raw_fd(), libc::TIOCSCTTY as _, 0);
        for fd in 0..3 {
            libc::dup2(pty.as_raw_fd(), fd);
        }
    }
    drop(pty);
    if let Some(n) = &ns.net
        && let Err(e) = setns(n, libc::CLONE_NEWNET)
    {
        fail(sandmux_spec::trf!(
            "entrar na rede do sandbox: {e}",
            "enter the sandbox network: {e}"
        ));
    }
    for (f, k) in [(&ns.ipc, libc::CLONE_NEWIPC), (&ns.uts, libc::CLONE_NEWUTS)] {
        if let Err(e) = setns(f, k) {
            fail(sandmux_spec::trf!(
                "entrar no sandbox: {e}",
                "enter the sandbox: {e}"
            ));
        }
    }
    // Por último a visão de arquivos: a raiz passa a ser a do sandbox.
    if let Err(e) = setns(&ns.mnt, libc::CLONE_NEWNS) {
        fail(sandmux_spec::trf!(
            "entrar nos arquivos do sandbox: {e}",
            "enter the sandbox files: {e}"
        ));
    }
    if let Err(e) = nix::unistd::chdir(&spec.cwd) {
        fail(sandmux_spec::trf!(
            "entrar na pasta inicial {}: {e}",
            "enter the initial folder {}: {e}",
            spec.cwd.display()
        ));
    }
    if let Err(e) = sys::drop_all_capabilities() {
        fail(sandmux_spec::trf!(
            "descartar capabilities: {e}",
            "drop capabilities: {e}"
        ));
    }
    if let Err(e) = sys::set_no_new_privs() {
        fail(format!("no_new_privs: {e}"));
    }
    if let Err(e) = landlock_rules::apply(&spec.landlock) {
        fail(format!("Landlock: {e}"));
    }
    if let Err(e) = seccomp::install(&spec.seccomp) {
        fail(format!("seccomp: {e}"));
    }
    match seccomp::install_notify(true, spec.files.as_ref().is_some_and(|f| f.exec_log)) {
        Ok(listener) => {
            if let Err(e) = net::send_fd(files_w, &listener) {
                fail(sandmux_spec::trf!(
                    "entregar o filtro das aberturas: {e}",
                    "hand over the file-open filter: {e}"
                ));
            }
        }
        Err(e) => fail(sandmux_spec::trf!(
            "seccomp das aberturas: {e}",
            "file-open seccomp: {e}"
        )),
    }
    if let Err(e) = sys::cloexec_from(3) {
        fail(sandmux_spec::trf!(
            "fechar descritores herdados: {e}",
            "close inherited descriptors: {e}"
        ));
    }
    // O shell do usuário dentro do sandbox; sem ele, bash ou sh.
    let program = spec
        .env_value("SHELL")
        .map(Path::new)
        .filter(|p| p.is_file())
        .map(Path::to_path_buf)
        .or_else(|| {
            ["/bin/bash", "/usr/bin/bash", "/bin/sh"]
                .iter()
                .map(Path::new)
                .find(|p| p.is_file())
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| "/bin/sh".into());
    let to_c = |s: &[u8]| CString::new(s).unwrap_or_default();
    let prog_c = to_c(program.as_os_str().as_bytes());
    let argv = [prog_c.clone()];
    let envp: Vec<CString> = spec
        .env
        .iter()
        .map(|v| to_c(format!("{}={}", v.name, v.value).as_bytes()))
        .collect();
    let err = nix::unistd::execve(&prog_c, &argv, &envp).unwrap_err();
    fail(sandmux_spec::trf!(
        "executar {}: {err}",
        "execute {}: {err}",
        program.display()
    ));
}

/// Lado de quem pede (`sandmux-run --enter <socket>`): manda o terminal da
/// entrada padrão ao supervisor e espera o código de saída do shell.
pub fn client(socket: &Path) -> i32 {
    // O terminal vira o controle do shell: quem pede larga o dele antes (sem
    // isso, o shell fica sem controle de tarefas). O SIGHUP que o largar
    // manda ao próprio grupo é ignorado.
    // SAFETY: troca de disposição e ioctl no descritor 0.
    unsafe {
        let _ = signal(Signal::SIGHUP, SigHandler::SigIgn);
        libc::ioctl(0, libc::TIOCNOTTY as _, 0);
    }
    let conn = match std::os::unix::net::UnixStream::connect(socket) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{}",
                sandmux_spec::trf!(
                    "sandmux: o sandbox da aba não atende ({e}); ela ainda está viva?",
                    "sandmux: the tab's sandbox does not answer ({e}); is it still alive?"
                )
            );
            return 1;
        }
    };
    let owned = OwnedFd::from(conn);
    let stdin = io::stdin();
    if let Err(e) = net::send_fd_tagged(&owned, &stdin.as_fd(), b'P') {
        eprintln!(
            "{}",
            sandmux_spec::trf!(
                "sandmux: mandar o terminal ao sandbox: {e}",
                "sandmux: send the terminal to the sandbox: {e}"
            )
        );
        return 1;
    }
    let mut code = [0u8; 4];
    match File::from(owned).read_exact(&mut code) {
        Ok(()) => i32::from_ne_bytes(code),
        Err(_) => {
            eprintln!(
                "{}",
                sandmux_spec::tr!(
                    "sandmux: o sandbox da aba terminou",
                    "sandmux: the tab's sandbox ended"
                )
            );
            1
        }
    }
}
