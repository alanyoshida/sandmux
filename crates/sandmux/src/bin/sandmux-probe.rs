//! Sonda dos testes de fuga do sandmux.
//!
//! Cada subcomando faz uma tentativa e imprime uma linha JSON:
//! `{"check": ..., "ok": <a tentativa funcionou>, "errno": ..., "detail": ...}`.
//! Fora do sandbox, as tentativas que o host permite funcionam; dentro, cada
//! uma precisa falhar. A sonda não altera nada fora do caminho indicado.

use std::ffi::CString;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use serde_json::json;

fn emit(check: &str, result: Result<String, std::io::Error>) {
    let v = match result {
        Ok(detail) => json!({"check": check, "ok": true, "errno": null, "detail": detail}),
        Err(e) => json!({
            "check": check,
            "ok": false,
            "errno": e.raw_os_error(),
            "detail": e.to_string(),
        }),
    };
    println!("{v}");
}

fn last_err() -> std::io::Error {
    std::io::Error::last_os_error()
}

fn sys_result(r: libc::c_long, detail: &str) -> Result<String, std::io::Error> {
    if r < 0 {
        Err(last_err())
    } else {
        Ok(format!("{detail}: {r}"))
    }
}

fn read_path(p: &str) -> Result<String, std::io::Error> {
    let meta = std::fs::metadata(p)?;
    if meta.is_dir() {
        let n = std::fs::read_dir(p)?.count();
        Ok(format!("pasta com {n} entradas"))
    } else {
        let mut buf = Vec::new();
        std::fs::File::open(p)?.read_to_end(&mut buf)?;
        Ok(format!("{} bytes", buf.len()))
    }
}

fn write_path(p: &str) -> Result<String, std::io::Error> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)?;
    f.write_all(b"")?;
    Ok("escrita aceita".into())
}

fn syscall_attempt(name: &str) -> Result<String, std::io::Error> {
    // Argumentos inválidos de propósito: se o filtro deixar passar, o kernel
    // devolve outro erro (EINVAL, EFAULT, EBADF...) ou sucesso; só o filtro e a
    // falta de privilégio devolvem EPERM.
    let null = std::ptr::null::<libc::c_void>();
    // SAFETY: chamadas com argumentos inválidos ou nulos, sem efeito colateral.
    let r = unsafe {
        match name {
            "mount" => libc::syscall(libc::SYS_mount, null, null, null, 0, null),
            "umount2" => libc::syscall(libc::SYS_umount2, c"/nao-existe".as_ptr(), 0),
            "pivot_root" => libc::syscall(libc::SYS_pivot_root, null, null),
            "open_tree" => libc::syscall(libc::SYS_open_tree, -1, null, 0),
            "move_mount" => libc::syscall(libc::SYS_move_mount, -1, null, -1, null, 0),
            "fsopen" => libc::syscall(libc::SYS_fsopen, c"tmpfs".as_ptr(), 0),
            "fsmount" => libc::syscall(libc::SYS_fsmount, -1, 0, 0),
            "fsconfig" => libc::syscall(libc::SYS_fsconfig, -1, 0, null, null, 0),
            "fspick" => libc::syscall(libc::SYS_fspick, -1, null, 0),
            "mount_setattr" => libc::syscall(libc::SYS_mount_setattr, -1, null, 0, null, 0),
            "bpf" => libc::syscall(libc::SYS_bpf, -1, null, 0),
            "perf_event_open" => libc::syscall(libc::SYS_perf_event_open, null, 0, -1, -1, 0),
            "init_module" => libc::syscall(libc::SYS_init_module, null, 0, null),
            "finit_module" => libc::syscall(libc::SYS_finit_module, -1, null, 0),
            "delete_module" => libc::syscall(libc::SYS_delete_module, c"nao_existe".as_ptr(), 0),
            "kexec_load" => libc::syscall(libc::SYS_kexec_load, 0, 0, null, 0),
            "kexec_file_load" => libc::syscall(libc::SYS_kexec_file_load, -1, -1, 0, null, 0),
            // KEYCTL_GET_KEYRING_ID do chaveiro de sessão, sem criar.
            "keyctl" => libc::syscall(libc::SYS_keyctl, 0, -3, 0),
            "add_key" => libc::syscall(libc::SYS_add_key, null, null, null, 0, 0),
            "request_key" => libc::syscall(libc::SYS_request_key, null, null, null, 0),
            // UFFD_USER_MODE_ONLY: permitido sem privilégio fora do sandbox.
            "userfaultfd" => libc::syscall(libc::SYS_userfaultfd, 1),
            "io_uring_setup" => {
                let mut params = [0u8; 120];
                libc::syscall(libc::SYS_io_uring_setup, 1, params.as_mut_ptr())
            }
            "io_uring_enter" => libc::syscall(libc::SYS_io_uring_enter, -1, 0, 0, 0, null, 0),
            "io_uring_register" => libc::syscall(libc::SYS_io_uring_register, -1, 0, null, 0),
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("chamada desconhecida na sonda: {other}"),
                ));
            }
        }
    };
    if r >= 0 && matches!(name, "io_uring_setup" | "userfaultfd" | "fsopen") {
        // SAFETY: fecha o descritor que a chamada devolveu.
        unsafe { libc::close(r as i32) };
    }
    sys_result(r, name)
}

fn ioctl_attempt(name: &str) -> Result<String, std::io::Error> {
    let cmd: libc::c_ulong = match name {
        "TIOCSTI" => 0x5412,
        "TIOCLINUX" => 0x541C,
        _ => return Err(std::io::Error::other("ioctl desconhecido")),
    };
    // Num descritor que não é terminal: sem o filtro, o kernel devolve ENOTTY;
    // nada é injetado em terminal nenhum.
    let f = std::fs::File::open("/dev/null")?;
    let c: u8 = b'x';
    // SAFETY: descritor válido e ponteiro para um byte.
    let r = unsafe { libc::ioctl(std::os::fd::AsRawFd::as_raw_fd(&f), cmd, &c as *const u8) };
    sys_result(r as libc::c_long, name)
}

fn procs() -> Result<String, std::io::Error> {
    let mut pids: Vec<i32> = std::fs::read_dir("/proc")?
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    pids.sort();
    Ok(serde_json::to_string(&pids).unwrap_or_default())
}

fn signal(pid: &str) -> Result<String, std::io::Error> {
    let pid: i32 = pid
        .parse()
        .map_err(|_| std::io::Error::other("pid inválido"))?;
    // Sinal 0: só a verificação de permissão, nada é entregue.
    // SAFETY: kill com sinal 0.
    let r = unsafe { libc::kill(pid, 0) };
    sys_result(r as libc::c_long, "kill 0")
}

fn connect_abstract(name: &str) -> Result<String, std::io::Error> {
    // SAFETY: estruturas zeradas e preenchidas com tamanhos conferidos.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(last_err());
        }
        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = name.as_bytes();
        if bytes.len() + 1 > addr.sun_path.len() {
            libc::close(fd);
            return Err(std::io::Error::other("nome longo demais"));
        }
        for (i, b) in bytes.iter().enumerate() {
            addr.sun_path[i + 1] = *b as libc::c_char;
        }
        let len = std::mem::size_of::<libc::sa_family_t>() + 1 + bytes.len();
        let r = libc::connect(
            fd,
            &addr as *const _ as *const libc::sockaddr,
            len as libc::socklen_t,
        );
        let res = if r < 0 {
            Err(last_err())
        } else {
            Ok("conectado".to_string())
        };
        libc::close(fd);
        res
    }
}

fn status_field(field: &str) -> String {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{field}:"))
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_default()
}

fn identity() -> Result<String, std::io::Error> {
    // SAFETY: consultas sem efeito colateral.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    Ok(json!({
        "uid": uid,
        "gid": gid,
        "cap_eff": status_field("CapEff"),
        "cap_bnd": status_field("CapBnd"),
        "no_new_privs": status_field("NoNewPrivs"),
        "seccomp": status_field("Seccomp"),
    })
    .to_string())
}

fn nested_userns_mount() -> Result<String, std::io::Error> {
    // SAFETY: unshare só afeta este processo de sonda.
    if unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) } != 0 {
        return Err(last_err());
    }
    let tmp = CString::new("tmpfs").unwrap();
    let target = CString::new("/tmp").unwrap();
    // SAFETY: strings C válidas.
    let r = unsafe {
        libc::mount(
            tmp.as_ptr(),
            target.as_ptr(),
            tmp.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    sys_result(r as libc::c_long, "user namespace criado; mount")
}

fn nested_userns_create() -> Result<String, std::io::Error> {
    // SAFETY: unshare só afeta este processo de sonda.
    let r = unsafe { libc::unshare(libc::CLONE_NEWUSER) };
    sys_result(r as libc::c_long, "user namespace aninhado")
}

fn ptrace_child() -> Result<String, std::io::Error> {
    // SAFETY: o filho pede para ser rastreado e para; o pai confere e encerra.
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return Err(last_err());
        }
        if pid == 0 {
            libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0);
            libc::raise(libc::SIGSTOP);
            libc::_exit(0);
        }
        let mut st = 0;
        libc::waitpid(pid, &mut st, 0);
        let stopped = libc::WIFSTOPPED(st);
        let mut regs_ok = false;
        if stopped {
            let mut word = 0i64;
            let r = libc::ptrace(libc::PTRACE_PEEKUSER, pid, 0, &mut word as *mut i64);
            regs_ok = r != -1 || *libc::__errno_location() == 0;
            libc::ptrace(libc::PTRACE_CONT, pid, 0, 0);
        }
        libc::waitpid(pid, &mut st, 0);
        if stopped && regs_ok {
            Ok("depurador anexado ao filho".into())
        } else {
            Err(std::io::Error::other("não foi possível rastrear o filho"))
        }
    }
}

fn env_names() -> Result<String, std::io::Error> {
    let mut names: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(serde_json::to_string(&names).unwrap_or_default())
}

fn env_value(name: &str) -> Result<String, std::io::Error> {
    std::env::var(name).map_err(|_| std::io::Error::from_raw_os_error(libc::ENOENT))
}

fn rename(from: &str, to: &str) -> Result<String, std::io::Error> {
    std::fs::rename(from, to).map(|_| "renomeado".into())
}

fn unlink(p: &str) -> Result<String, std::io::Error> {
    std::fs::remove_file(p).map(|_| "apagado".into())
}

/// Todas as tentativas de fuga numa execução, com um relatório único.
///
/// Argumentos em `chave=valor`: `ssh` (pasta de chaves do HOME real),
/// `other` (outra pasta do HOME real), `hostsock` (socket do host por
/// caminho), `abstract` (nome de socket abstrato do host), `covered` (arquivo
/// coberto, repetível), `git` (pasta `.git` da pasta de trabalho), `inbox`,
/// `daemonsock` (socket do daemon do sandmux) e `daemonabstract` (nome
/// abstrato com o nome do daemon).
fn suite(args: &[String]) -> i32 {
    let get = |k: &str| -> Vec<String> {
        args.iter()
            .filter_map(|a| a.strip_prefix(&format!("{k}=")).map(str::to_string))
            .collect()
    };
    let one = |k: &str| get(k).into_iter().next().unwrap_or_default();
    let mut results: Vec<(String, Result<String, std::io::Error>)> = Vec::new();
    let mut add = |name: String, r: Result<String, std::io::Error>| results.push((name, r));

    add(
        format!("ler chaves do HOME real ({})", one("ssh")),
        read_path(&one("ssh")),
    );
    add(
        format!("ler outra pasta do HOME real ({})", one("other")),
        read_path(&one("other")),
    );
    add(
        "conectar no socket do host por caminho".into(),
        UnixStream::connect(one("hostsock")).map(|_| "conectado".into()),
    );
    add(
        "conectar no socket abstrato do host".into(),
        connect_abstract(&one("abstract")),
    );
    for p in get("daemonsock") {
        add(
            format!("conectar no socket do daemon do sandmux ({p})"),
            UnixStream::connect(&p).map(|_| "conectado".into()),
        );
    }
    for n in get("daemonabstract") {
        add(
            format!("conectar num socket abstrato com o nome do daemon ({n})"),
            connect_abstract(&n),
        );
    }
    add(
        "sinalizar processo fora do domínio (pid 1)".into(),
        signal("1"),
    );
    add(
        "ler o ambiente do processo 1".into(),
        read_path("/proc/1/environ"),
    );
    let host_procs = procs()
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<i32>>(&s).ok())
        .map(|v| v.len())
        .unwrap_or(0);
    add(
        "enxergar processos do host".into(),
        if host_procs > 5 {
            Ok(format!("{host_procs} processos"))
        } else {
            Err(std::io::Error::other(format!(
                "{host_procs} processos visíveis"
            )))
        },
    );
    for name in [
        "mount",
        "umount2",
        "pivot_root",
        "open_tree",
        "move_mount",
        "fsopen",
        "fsmount",
        "fsconfig",
        "fspick",
        "mount_setattr",
        "bpf",
        "perf_event_open",
        "init_module",
        "finit_module",
        "delete_module",
        "kexec_load",
        "kexec_file_load",
        "keyctl",
        "add_key",
        "request_key",
        "userfaultfd",
        "io_uring_setup",
        "io_uring_enter",
        "io_uring_register",
    ] {
        let r = syscall_attempt(name);
        // Só EPERM conta como negado pelo filtro.
        let r = match r {
            Err(e) if e.raw_os_error() == Some(libc::EPERM) => Err(e),
            Err(e) => Ok(format!("passou pelo filtro ({e})")),
            ok => ok,
        };
        add(format!("chamada {name}"), r);
    }
    for name in ["TIOCSTI", "TIOCLINUX"] {
        let r = match ioctl_attempt(name) {
            Err(e) if e.raw_os_error() == Some(libc::EPERM) => Err(e),
            Err(e) => Ok(format!("passou pelo filtro ({e})")),
            ok => ok,
        };
        add(format!("ioctl {name}"), r);
    }
    for p in ["/usr/sandmux-fuga", "/etc/sandmux-fuga", "/sandmux-fuga"] {
        add(format!("escrever em {p}"), write_path(p));
    }
    let inbox = one("inbox");
    add(
        "escrever na pasta de entrada".into(),
        write_path(&format!("{inbox}/sandmux-fuga")),
    );
    for c in get("covered") {
        let r = read_path(&c).and_then(|d| {
            if d == "0 bytes" || d == "pasta com 0 entradas" {
                Err(std::io::Error::other("vazio"))
            } else {
                Ok(d)
            }
        });
        add(format!("ler conteúdo coberto {c}"), r);
        add(format!("apagar coberto {c}"), unlink(&c));
        add(
            format!("renomear coberto {c}"),
            rename(&c, &format!("{c}.x")),
        );
    }
    let git = one("git");
    if !git.is_empty() {
        add(
            "criar hook em .git/hooks".into(),
            write_path(&format!("{git}/hooks/pre-commit")),
        );
        add(
            "alterar .git/config".into(),
            write_path(&format!("{git}/config")),
        );
        add("renomear .git".into(), rename(&git, &format!("{git}2")));
    }
    add("montar em user namespace aninhado".into(), {
        // Num filho, para não mudar os namespaces da sonda.
        // SAFETY: fork simples; o filho só faz a tentativa e sai.
        unsafe {
            let pid = libc::fork();
            if pid == 0 {
                let ok = nested_userns_mount().is_ok();
                libc::_exit(if ok { 0 } else { 1 });
            }
            let mut st = 0;
            libc::waitpid(pid, &mut st, 0);
            if libc::WEXITSTATUS(st) == 0 {
                Ok("montou".into())
            } else {
                Err(std::io::Error::other("montagem negada"))
            }
        }
    });
    let id = identity().unwrap_or_default();
    let caps_zero =
        id.contains("\"cap_eff\":\"0000000000000000\"") && id.contains("\"no_new_privs\":\"1\"");
    add(
        "ter capabilities ou ganhar privilégio".into(),
        if caps_zero {
            Err(std::io::Error::other(
                "sem capabilities, no_new_privs ligado",
            ))
        } else {
            Ok(id)
        },
    );

    let mut escaped = 0;
    for (name, r) in &results {
        let line = match r {
            Ok(d) => {
                escaped += 1;
                json!({"tentativa": name, "resultado": "CONSEGUIU", "detalhe": d})
            }
            Err(e) => json!({"tentativa": name, "resultado": "negada", "detalhe": e.to_string()}),
        };
        println!("{line}");
    }
    println!(
        "{}",
        json!({"resumo": {"tentativas": results.len(), "conseguiram": escaped}})
    );
    if escaped == 0 { 0 } else { 1 }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("suite") {
        std::process::exit(suite(&args[1..]));
    }
    let a = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    let check = a(0);
    let result = match check {
        "read" => read_path(a(1)),
        "write" => write_path(a(1)),
        "exists" => std::fs::symlink_metadata(a(1)).map(|m| format!("{:?}", m.file_type())),
        "procs" => procs(),
        "signal" => signal(a(1)),
        "connect-unix" => UnixStream::connect(a(1)).map(|_| "conectado".into()),
        "connect-abstract" => connect_abstract(a(1)),
        "syscall" => syscall_attempt(a(1)),
        "ioctl" => ioctl_attempt(a(1)),
        "identity" => identity(),
        "nested-userns" => nested_userns_create(),
        "nested-userns-mount" => nested_userns_mount(),
        "ptrace-child" => ptrace_child(),
        "env-names" => env_names(),
        "env" => env_value(a(1)),
        "rename" => rename(a(1), a(2)),
        "unlink" => unlink(a(1)),
        "orphan" => {
            // Cria um neto que fica órfão e sai antes dele.
            // SAFETY: fork simples; o neto só dorme e sai.
            unsafe {
                if libc::fork() == 0 {
                    if libc::fork() == 0 {
                        libc::usleep(200_000);
                        libc::_exit(0);
                    }
                    libc::_exit(0);
                }
                let mut st = 0;
                libc::wait(&mut st);
            }
            Ok("órfão criado".into())
        }
        "background" => {
            // Deixa um processo em segundo plano e sai.
            // SAFETY: fork simples; o filho dorme.
            unsafe {
                if libc::fork() == 0 {
                    libc::setsid();
                    libc::sleep(300);
                    libc::_exit(0);
                }
            }
            Ok("processo em segundo plano criado".into())
        }
        "zombies" => {
            let mut z = Vec::new();
            for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
                let name = e.file_name();
                let Some(pid) = name.to_str().and_then(|s| s.parse::<i32>().ok()) else {
                    continue;
                };
                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
                if let Some(rest) = stat.rsplit_once(") ")
                    && rest.1.starts_with('Z')
                {
                    z.push(pid);
                }
            }
            Ok(serde_json::to_string(&z).unwrap_or_default())
        }
        "exit" => std::process::exit(a(1).parse().unwrap_or(0)),
        "sleep" => {
            std::thread::sleep(std::time::Duration::from_secs(a(1).parse().unwrap_or(1)));
            Ok("acordou".into())
        }
        other => {
            eprintln!("sandmux-probe: verificação desconhecida: {other}");
            std::process::exit(2);
        }
    };
    emit(check, result);
}
