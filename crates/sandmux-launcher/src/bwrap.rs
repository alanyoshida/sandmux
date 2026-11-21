//! O `bwrap` do sandmux: um substituto do bubblewrap dentro do sandbox.
//!
//! Harnesses isolam os próprios comandos com o bubblewrap (o Claude Code no
//! sandbox do bash dele), e o bubblewrap precisa montar, o que o sandbox
//! nega. O substituto executa o comando já confinado pelo sandmux, sem montar
//! nada, e aplica do pedido o que não precisa de montagem:
//!
//! - as montagens de arquivos no mesmo caminho viram um Landlock a mais,
//!   empilhado só no comando (o Landlock só restringe: o pedido nunca dá mais
//!   que a aba);
//! - os namespaces sem montagem (`--unshare-user`, `-net`, `-ipc`, `-uts`,
//!   `-pid`), o ambiente, o diretório, o `--seccomp` e os descritores de
//!   sincronização;
//! - as montagens em si (`--proc`, `--dev`, `--dir`...) são ignoradas: o
//!   comando já vê a visão da aba.

use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::path::PathBuf;

use sandmux_spec::{LandlockRules, trf};

/// Um pedido lido das opções.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Request {
    /// Caminhos com leitura (origem = destino).
    pub read: Vec<PathBuf>,
    /// Caminhos com leitura e escrita.
    pub write: Vec<PathBuf>,
    /// Houve alguma montagem de arquivos (então o Landlock é empilhado).
    pub binds: bool,
    pub unshare_user: bool,
    pub unshare_net: bool,
    pub unshare_ipc: bool,
    pub unshare_uts: bool,
    pub unshare_pid: bool,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub hostname: Option<String>,
    pub chdir: Option<PathBuf>,
    pub clearenv: bool,
    /// `--setenv` e `--unsetenv`, na ordem (`None` remove).
    pub env: Vec<(String, Option<String>)>,
    pub die_with_parent: bool,
    pub new_session: bool,
    pub seccomp_fds: Vec<RawFd>,
    pub info_fd: Option<RawFd>,
    pub json_status_fd: Option<RawFd>,
    pub sync_fd: Option<RawFd>,
    pub block_fd: Option<RawFd>,
    pub command: Vec<String>,
}

/// Quantos valores cada opção leva, e o que fazer com ela.
fn arity(opt: &str) -> Option<usize> {
    Some(match opt {
        "--unshare-all"
        | "--share-net"
        | "--unshare-user"
        | "--unshare-user-try"
        | "--unshare-ipc"
        | "--unshare-pid"
        | "--unshare-net"
        | "--unshare-uts"
        | "--unshare-cgroup"
        | "--unshare-cgroup-try"
        | "--clearenv"
        | "--die-with-parent"
        | "--as-pid-1"
        | "--new-session"
        | "--disable-userns"
        | "--assert-userns-disabled"
        | "--level-prefix" => 0,
        "--args" | "--argv0" | "--userns" | "--userns2" | "--pidns" | "--uid" | "--gid"
        | "--hostname" | "--chdir" | "--unsetenv" | "--lock-file" | "--sync-fd"
        | "--remount-ro" | "--exec-label" | "--file-label" | "--proc" | "--dev" | "--tmpfs"
        | "--mqueue" | "--dir" | "--seccomp" | "--add-seccomp-fd" | "--block-fd"
        | "--userns-block-fd" | "--info-fd" | "--json-status-fd" | "--cap-add" | "--cap-drop"
        | "--perms" | "--size" | "--overlay-src" | "--tmp-overlay" | "--ro-overlay" => 1,
        "--setenv" | "--bind" | "--bind-try" | "--dev-bind" | "--dev-bind-try" | "--ro-bind"
        | "--ro-bind-try" | "--file" | "--bind-data" | "--ro-bind-data" | "--symlink"
        | "--bind-fd" | "--ro-bind-fd" | "--chmod" | "--overlay" => 2,
        _ => return None,
    })
}

fn fd_arg(v: &str) -> Result<RawFd, String> {
    v.parse::<RawFd>()
        .map_err(|_| trf!("descritor inválido: {v}", "invalid descriptor: {v}"))
}

/// Lê as opções. `read_args` lê o conteúdo de um `--args FD` (opções
/// separadas por NUL).
pub fn parse(
    args: &[String],
    read_args: &mut dyn FnMut(RawFd) -> Result<Vec<String>, String>,
) -> Result<Request, String> {
    let mut r = Request::default();
    let mut q: std::collections::VecDeque<String> = args.iter().cloned().collect();
    while let Some(a) = q.pop_front() {
        if a == "--" {
            r.command = q.into_iter().collect();
            break;
        }
        if !a.starts_with("--") {
            q.push_front(a);
            r.command = q.into_iter().collect();
            break;
        }
        let Some(n) = arity(&a) else {
            return Err(trf!("opção desconhecida: {a}", "unknown option: {a}"));
        };
        if q.len() < n {
            return Err(trf!(
                "a opção {a} precisa de {n} valores",
                "option {a} needs {n} values"
            ));
        }
        let v: Vec<String> = q.drain(..n).collect();
        match a.as_str() {
            "--args" => {
                let mut extra = read_args(fd_arg(&v[0])?)?;
                extra.extend(q.drain(..));
                q = extra.into();
            }
            "--unshare-all" => {
                r.unshare_user = true;
                r.unshare_net = true;
                r.unshare_ipc = true;
                r.unshare_uts = true;
                r.unshare_pid = true;
            }
            "--share-net" => r.unshare_net = false,
            "--unshare-user" | "--unshare-user-try" => r.unshare_user = true,
            "--unshare-net" => r.unshare_net = true,
            "--unshare-ipc" => r.unshare_ipc = true,
            "--unshare-uts" => r.unshare_uts = true,
            "--unshare-pid" => r.unshare_pid = true,
            "--uid" => r.uid = v[0].parse().ok(),
            "--gid" => r.gid = v[0].parse().ok(),
            "--hostname" => r.hostname = Some(v[0].clone()),
            "--chdir" => r.chdir = Some(PathBuf::from(&v[0])),
            "--clearenv" => {
                r.clearenv = true;
                r.env.clear();
            }
            "--setenv" => r.env.push((v[0].clone(), Some(v[1].clone()))),
            "--unsetenv" => r.env.push((v[0].clone(), None)),
            "--die-with-parent" => r.die_with_parent = true,
            "--new-session" => r.new_session = true,
            "--seccomp" | "--add-seccomp-fd" => r.seccomp_fds.push(fd_arg(&v[0])?),
            "--info-fd" => r.info_fd = Some(fd_arg(&v[0])?),
            "--json-status-fd" => r.json_status_fd = Some(fd_arg(&v[0])?),
            "--sync-fd" => r.sync_fd = Some(fd_arg(&v[0])?),
            "--block-fd" => r.block_fd = Some(fd_arg(&v[0])?),
            "--ro-bind" | "--ro-bind-try" | "--bind" | "--bind-try" | "--dev-bind"
            | "--dev-bind-try" => {
                r.binds = true;
                let (src, dst) = (PathBuf::from(&v[0]), PathBuf::from(&v[1]));
                // Só no mesmo caminho dá para imitar sem montar: a origem
                // ganha o acesso. Um bind para outro lugar (o `/dev/null` sobre
                // um arquivo, para negá-lo) não concede nada.
                if src == dst && !(a.ends_with("-try") && !src.exists()) {
                    if a.starts_with("--ro-") {
                        r.read.push(src);
                    } else {
                        r.write.push(src);
                    }
                }
            }
            "--tmpfs" => {
                r.binds = true;
                let p = PathBuf::from(&v[0]);
                if p.exists() {
                    r.write.push(p);
                }
            }
            // As montagens em si e o resto: o comando já vê a visão da aba.
            _ => {}
        }
    }
    if r.command.is_empty() {
        return Err(trf!("falta o comando", "the command is missing").to_string());
    }
    Ok(r)
}

/// As regras do Landlock empilhado: as do pedido mais o mínimo para um
/// comando rodar.
pub fn rules(r: &Request) -> LandlockRules {
    let mut read: Vec<PathBuf> = [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/lib32",
        "/etc",
        "/run/sandmux/bin",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    read.extend(r.read.iter().cloned());
    let mut write: Vec<PathBuf> = [
        "/dev/null",
        "/dev/zero",
        "/dev/tty",
        "/dev/pts",
        "/dev/full",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    write.extend(r.write.iter().cloned());
    LandlockRules { read, write }
}

fn args_from_fd(fd: RawFd) -> Result<Vec<String>, String> {
    // SAFETY: o descritor foi passado pelo chamador para nós.
    let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

fn check(r: libc::c_int, what: &str) -> Result<(), String> {
    if r < 0 {
        return Err(format!("{what}: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

/// O processo atual no user namespace novo, com o uid e o gid pedidos (ou os
/// de antes).
fn map_ids(uid_out: u32, gid_out: u32, uid_in: u32, gid_in: u32) -> Result<(), String> {
    let w = |p: &str, s: String| std::fs::write(p, s).map_err(|e| format!("{p}: {e}"));
    w("/proc/self/setgroups", "deny".into())?;
    w("/proc/self/uid_map", format!("{uid_in} {uid_out} 1"))?;
    w("/proc/self/gid_map", format!("{gid_in} {gid_out} 1"))
}

fn install_seccomp(fd: RawFd) -> Result<(), String> {
    // SAFETY: o descritor foi passado pelo chamador para nós.
    let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let n = std::mem::size_of::<libc::sock_filter>();
    if buf.is_empty() || buf.len() % n != 0 {
        return Err(tr_bad_seccomp());
    }
    let prog = libc::sock_fprog {
        len: (buf.len() / n) as u16,
        filter: buf.as_mut_ptr().cast(),
    };
    // SAFETY: o programa aponta para o buffer vivo; o kernel o copia.
    check(
        unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER,
                &prog as *const libc::sock_fprog,
            )
        },
        "seccomp",
    )
}

fn tr_bad_seccomp() -> String {
    sandmux_spec::tr!("filtro seccomp inválido", "invalid seccomp filter").into()
}

/// Escreve o JSON de estado do bubblewrap e fecha o descritor.
fn status(fd: Option<RawFd>, pid: i32) {
    if let Some(fd) = fd {
        // SAFETY: o descritor foi passado pelo chamador para nós.
        let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
        let _ = writeln!(f, "{{\n    \"child-pid\": {pid}\n}}");
    }
}

/// Registra as regras no supervisor de arquivos do sandbox (por uma abertura
/// num caminho especial que ele intercepta). Sem supervisor, não há o que
/// registrar: as aberturas vão direto ao kernel, sob o Landlock.
fn register(rl: &LandlockRules) {
    let Ok(json) = serde_json::to_vec(rl) else {
        return;
    };
    let path = format!(
        "{}{:x}-{}",
        String::from_utf8_lossy(crate::files::BWRAP_RULES),
        json.as_ptr() as usize,
        json.len()
    );
    let Ok(c) = std::ffi::CString::new(path) else {
        return;
    };
    // SAFETY: caminho terminado em NUL; o JSON vive até o fim da chamada.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd >= 0 {
        // SAFETY: descritor recém-aberto.
        unsafe { libc::close(fd) };
    }
    std::hint::black_box(&json);
}

/// Prepara tudo e executa o comando (só volta com erro).
fn run(r: &Request) -> Result<std::convert::Infallible, String> {
    if let Some(fd) = r.block_fd {
        // SAFETY: o descritor foi passado pelo chamador para nós.
        let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut b = [0u8; 1];
        let _ = f.read(&mut b);
    }
    // SAFETY: getuid/getgid não falham.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    let mut flags = 0;
    if r.unshare_user || r.unshare_net || r.unshare_ipc || r.unshare_uts || r.unshare_pid {
        // Sem capabilities no sandbox: os outros namespaces precisam de um
        // user namespace novo.
        flags |= libc::CLONE_NEWUSER;
    }
    if r.unshare_net {
        flags |= libc::CLONE_NEWNET;
    }
    if r.unshare_ipc {
        flags |= libc::CLONE_NEWIPC;
    }
    if r.unshare_uts {
        flags |= libc::CLONE_NEWUTS;
    }
    if r.unshare_pid {
        flags |= libc::CLONE_NEWPID;
    }
    if flags != 0 {
        // SAFETY: unshare com flags numéricas.
        check(unsafe { libc::unshare(flags) }, "unshare")?;
        map_ids(uid, gid, r.uid.unwrap_or(uid), r.gid.unwrap_or(gid))?;
    }
    if let (Some(h), true) = (&r.hostname, r.unshare_uts) {
        // SAFETY: o buffer vive até o fim da chamada.
        check(
            unsafe { libc::sethostname(h.as_ptr().cast(), h.len()) },
            "sethostname",
        )?;
    }
    if r.unshare_pid {
        // O processo 1 do namespace novo é o filho; este espera e devolve o
        // código dele.
        // SAFETY: o processo tem uma só thread.
        let pid = unsafe { libc::fork() };
        check(pid, "fork")?;
        if pid > 0 {
            status(r.info_fd, pid);
            status(r.json_status_fd, pid);
            let mut st = 0;
            // SAFETY: espera o filho que acabamos de criar.
            unsafe { libc::waitpid(pid, &mut st, 0) };
            let code = if libc::WIFEXITED(st) {
                libc::WEXITSTATUS(st)
            } else {
                128 + libc::WTERMSIG(st)
            };
            std::process::exit(code);
        }
    } else {
        // SAFETY: getpid não falha.
        let me = unsafe { libc::getpid() };
        status(r.info_fd, me);
        status(r.json_status_fd, me);
    }
    if r.die_with_parent {
        // SAFETY: prctl com argumentos numéricos.
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
    }
    if r.new_session {
        // SAFETY: setsid não tem argumentos.
        unsafe { libc::setsid() };
    }
    if let Some(d) = &r.chdir {
        std::env::set_current_dir(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    if r.binds {
        let rl = rules(r);
        // O supervisor de arquivos abre pelo processo nas pastas que ele
        // registra: ele precisa conhecer as regras, que o Landlock deste
        // processo não alcança ali.
        register(&rl);
        crate::landlock_rules::apply(&rl)?;
    }
    for &fd in &r.seccomp_fds {
        install_seccomp(fd)?;
    }
    if let Some(fd) = r.sync_fd {
        // O comando segura o descritor até terminar, como o bubblewrap.
        // SAFETY: tira o CLOEXEC de um descritor nosso.
        unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
    }
    let mut cmd = std::process::Command::new(&r.command[0]);
    cmd.args(&r.command[1..]);
    if r.clearenv {
        cmd.env_clear();
    }
    for (k, v) in &r.env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    let e = std::os::unix::process::CommandExt::exec(&mut cmd);
    Err(format!("{}: {e}", r.command[0]))
}

/// O ponto de entrada (o `sandmux-run` chamado como `bwrap`). Devolve o
/// código de saída quando não chegou a executar o comando.
pub fn main(args: &[String]) -> i32 {
    let r = match parse(args, &mut args_from_fd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("bwrap: {e}");
            return 1;
        }
    };
    match run(&r) {
        Ok(never) => match never {},
        Err(e) => {
            eprintln!("bwrap: {e}");
            1
        }
    }
}

/// O caminho do binário do substituto na visão (no `PATH`).
pub const SHIM: &str = "/run/sandmux/bin/bwrap";

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Request, String> {
        let v: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        parse(&v, &mut |_| Ok(vec![]))
    }

    #[test]
    fn opcoes_do_claude_code() {
        let r = p(&[
            "--new-session",
            "--die-with-parent",
            "--unshare-net",
            "--unshare-pid",
            "--ro-bind",
            "/",
            "/",
            "--bind",
            "/tmp/proj",
            "/tmp/proj",
            "--ro-bind",
            "/dev/null",
            "/tmp/proj/.env",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--tmpfs",
            "/nao/existe",
            "--setenv",
            "A",
            "1",
            "--unsetenv",
            "B",
            "--chdir",
            "/tmp/proj",
            "--",
            "bash",
            "-c",
            "echo oi",
        ])
        .unwrap();
        assert_eq!(r.read, vec![PathBuf::from("/")]);
        assert_eq!(r.write, vec![PathBuf::from("/tmp/proj")]);
        assert!(r.binds && r.unshare_net && r.unshare_pid && r.new_session && r.die_with_parent);
        assert_eq!(
            r.env,
            vec![("A".into(), Some("1".into())), ("B".into(), None)]
        );
        assert_eq!(r.command, vec!["bash", "-c", "echo oi"]);
    }

    #[test]
    fn args_por_descritor_e_comando_sem_separador() {
        let v: Vec<String> = ["--args", "7", "true"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let r = parse(&v, &mut |fd| {
            assert_eq!(fd, 7);
            Ok(vec!["--unshare-user".into(), "--uid".into(), "0".into()])
        })
        .unwrap();
        assert!(r.unshare_user);
        assert_eq!(r.uid, Some(0));
        assert_eq!(r.command, vec!["true"]);
    }

    #[test]
    fn erros() {
        assert!(p(&["--nao-existe", "--", "true"]).is_err());
        assert!(p(&["--bind", "/a"]).is_err());
        assert!(p(&["--unshare-net"]).is_err());
        assert!(p(&["--seccomp", "x", "--", "true"]).is_err());
    }

    #[test]
    fn sem_montagem_de_arquivos_nao_empilha() {
        let r = p(&["--unshare-net", "--proc", "/proc", "--", "true"]).unwrap();
        assert!(!r.binds);
        let r = p(&["--dev-bind", "/", "/", "--", "true"]).unwrap();
        assert_eq!(rules(&r).write.last(), Some(&PathBuf::from("/")));
    }
}
