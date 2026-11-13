//! Grupo 4: o lançador confina o comando. Cada teste faz uma tentativa de fuga
//! com a sonda dentro do sandbox e confere que ela falha; alguns conferem
//! antes que a mesma tentativa funciona fora, para provar que a sonda detecta.

mod common;
use common::*;

use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

fn abstract_listener(name: &str) -> std::os::fd::OwnedFd {
    use std::os::fd::FromRawFd;
    // SAFETY: estruturas zeradas e preenchidas com tamanhos conferidos.
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        assert!(fd >= 0);
        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (i, b) in name.as_bytes().iter().enumerate() {
            addr.sun_path[i + 1] = *b as libc::c_char;
        }
        let len = std::mem::size_of::<libc::sa_family_t>() + 1 + name.len();
        assert_eq!(
            libc::bind(fd, &addr as *const _ as *const libc::sockaddr, len as u32),
            0
        );
        assert_eq!(libc::listen(fd, 8), 0);
        std::os::fd::OwnedFd::from_raw_fd(fd)
    }
}

// ---- 4.1 a sonda detecta, fora do sandbox, o que o host permite ----

#[test]
fn sonda_fora_do_sandbox_detecta_o_que_o_host_permite() {
    let home = home();
    let ssh = home.join(".ssh");
    let target = if ssh.exists() { ssh } else { home.clone() };
    assert!(ok(&probe_outside(&["read", target.to_str().unwrap()])));
    let procs: Vec<i64> =
        serde_json::from_str(probe_outside(&["procs"])["detail"].as_str().unwrap()).unwrap();
    assert!(
        procs.len() > 10,
        "fora do sandbox a sonda deveria ver os processos do host"
    );
    let me = std::process::id().to_string();
    assert!(ok(&probe_outside(&["signal", &me])));
    assert!(ok(&probe_outside(&["syscall", "keyctl"])));
    let io_uring_off = std::fs::read_to_string("/proc/sys/kernel/io_uring_disabled")
        .map(|s| s.trim() != "0")
        .unwrap_or(false);
    if !io_uring_off {
        assert!(ok(&probe_outside(&["syscall", "io_uring_setup"])));
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(ok(&probe_outside(&[
        "write",
        dir.path().join("x").to_str().unwrap()
    ])));
    let name = format!("sandmux-teste-41-{}", std::process::id());
    let _l = abstract_listener(&name);
    assert!(ok(&probe_outside(&["connect-abstract", &name])));
    assert!(
        probe_outside(&["env-names"])["detail"]
            .as_str()
            .unwrap()
            .contains("\"HOME\"")
    );
    // Sem o filtro, mount com argumentos inválidos devolve EFAULT, não EPERM.
    assert_eq!(
        errno(&probe_outside(&["syscall", "mount"])),
        Some(libc::EFAULT as i64)
    );
    assert!(ok(&probe_outside(&["ptrace-child"])));
}

// ---- 4.2 falha fechada ----

fn marker_spec(dir: &std::path::Path) -> sandmux_spec::LaunchSpec {
    let mut s = base_spec(vec![
        "/bin/sh".into(),
        "-c".into(),
        format!("touch {}/executou", dir.display()),
    ]);
    s.mounts.push(sandmux_spec::Mount::Bind {
        source: dir.into(),
        dest: dir.into(),
        writable: true,
    });
    s.landlock.write.push(dir.into());
    s
}

#[test]
fn landlock_insuficiente_recusa_sem_executar() {
    let dir = tempfile::tempdir().unwrap();
    let (mut cmd, _keep) = run_command(marker_spec(dir.path()).to_json().as_bytes());
    let out = cmd
        .env("SANDMUX_RUN_REQUIRE_LANDLOCK_ABI", "99")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(125));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Landlock") && err.contains("exige ABI 99"),
        "{err}"
    );
    assert!(!dir.path().join("executou").exists());
}

#[test]
fn user_namespace_bloqueado_recusa_e_aponta_o_doctor() {
    let dir = tempfile::tempdir().unwrap();
    let (mut cmd, _keep) = run_command(marker_spec(dir.path()).to_json().as_bytes());
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let uid_map = format!("{uid} {uid} 1\n");
    let gid_map = format!("{gid} {gid} 1\n");
    // SAFETY: o pre_exec só usa chamadas de sistema com buffers preparados antes.
    unsafe {
        cmd.pre_exec(move || {
            fn write(path: &std::ffi::CStr, data: &[u8]) -> std::io::Result<()> {
                // SAFETY: caminho e dados válidos; só chamadas de sistema.
                unsafe {
                    let fd = libc::open(path.as_ptr(), libc::O_WRONLY);
                    if fd < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    let r = libc::write(fd, data.as_ptr().cast(), data.len());
                    libc::close(fd);
                    if r < 0 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                }
            }
            if libc::unshare(libc::CLONE_NEWUSER) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            write(c"/proc/self/setgroups", b"deny")?;
            write(c"/proc/self/uid_map", uid_map.as_bytes())?;
            write(c"/proc/self/gid_map", gid_map.as_bytes())?;
            // Neste user namespace, nenhum user namespace filho pode ser criado.
            write(c"/proc/sys/user/max_user_namespaces", b"0")
        });
    }
    let Some(out) = output_unless_restricted(
        &mut cmd,
        "user_namespace_bloqueado_recusa_e_aponta_o_doctor",
    ) else {
        return;
    };
    assert_eq!(out.status.code(), Some(125));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("user namespaces") && err.contains("sandmux doctor"),
        "{err}"
    );
    assert!(!dir.path().join("executou").exists());
}

#[test]
fn falha_de_montagem_nao_executa() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = marker_spec(dir.path());
    s.mounts.push(sandmux_spec::Mount::Bind {
        source: "/nao/existe/no/host".into(),
        dest: "/x".into(),
        writable: false,
    });
    let out = run_spec(&s);
    assert_eq!(out.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&out.stderr).contains("/nao/existe/no/host"));
    assert!(!dir.path().join("executou").exists());
}

// ---- 4.3 namespaces e processo 1 ----

#[test]
fn so_os_processos_do_sandbox_aparecem() {
    let v = probe(&["procs"]);
    let pids: Vec<i64> = serde_json::from_str(v["detail"].as_str().unwrap()).unwrap();
    assert!(pids.contains(&1), "{pids:?}");
    assert!(pids.len() <= 3, "processos demais visíveis: {pids:?}");
}

#[test]
fn orfao_e_recolhido() {
    let script = format!("{PROBE} orphan >/dev/null; sleep 0.5; {PROBE} zombies");
    let v = probe_in(&base_spec(vec!["/bin/sh".into(), "-c".into(), script]));
    assert_eq!(v["detail"], "[]", "zumbis no sandbox: {v}");
}

#[test]
fn identidade_e_a_do_usuario() {
    let v = probe(&["identity"]);
    let id: serde_json::Value = serde_json::from_str(v["detail"].as_str().unwrap()).unwrap();
    assert_eq!(id["uid"], unsafe { libc::getuid() });
    assert_eq!(id["gid"], unsafe { libc::getgid() });
}

// ---- 4.4 raiz nova ----

#[test]
fn caminhos_do_host_fora_da_especificacao_nao_existem() {
    let host_tmp = tempfile::Builder::new()
        .prefix("sandmux-host-")
        .tempfile_in("/tmp")
        .unwrap();
    for p in [
        home().join(".ssh"),
        "/var/log".into(),
        "/mnt".into(),
        "/root".into(),
        "/run/user".into(),
        host_tmp.path().to_path_buf(),
    ] {
        let v = probe(&["exists", p.to_str().unwrap()]);
        assert!(!ok(&v), "{} visível no sandbox: {v}", p.display());
    }
}

#[test]
fn raiz_e_sistema_sao_somente_leitura() {
    for p in ["/novo-arquivo", "/usr/sandmux-teste", "/etc/sandmux-teste"] {
        let v = probe(&["write", p]);
        assert!(!ok(&v), "escrita em {p} aceita");
    }
    assert!(ok(&probe(&["write", "/tmp/aceito"])));
}

#[test]
fn link_simbolico_no_destino_e_recusado() {
    let dir = tempfile::tempdir().unwrap();
    let home_like = dir.path().join("home");
    std::fs::create_dir(&home_like).unwrap();
    std::os::unix::fs::symlink("/etc", home_like.join("projeto")).unwrap();
    let mut s = base_spec(probe_cmd(&["exists", "/x"]));
    s.mounts.push(sandmux_spec::Mount::Bind {
        source: home_like.clone(),
        dest: "/h".into(),
        writable: true,
    });
    s.mounts.push(sandmux_spec::Mount::Tmpfs {
        dest: "/h/projeto/app".into(),
        mode: 0o755,
    });
    let out = run_spec(&s);
    assert_eq!(out.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&out.stderr).contains("link simbólico"));
}

// ---- 4.5 capabilities, no_new_privs, Landlock ----

#[test]
fn sem_capabilities_e_com_no_new_privs() {
    let v = probe(&["identity"]);
    let id: serde_json::Value = serde_json::from_str(v["detail"].as_str().unwrap()).unwrap();
    assert_eq!(id["cap_eff"], "0000000000000000");
    assert_eq!(id["cap_bnd"], "0000000000000000");
    assert_eq!(id["no_new_privs"], "1");
    assert_eq!(id["seccomp"], "2");
}

#[test]
fn socket_abstrato_do_host_e_negado() {
    let name = format!("sandmux-teste-45-{}", std::process::id());
    let _l = abstract_listener(&name);
    assert!(ok(&probe_outside(&["connect-abstract", &name])));
    let v = probe(&["connect-abstract", &name]);
    assert!(!ok(&v), "conectou no socket abstrato do host: {v}");
    assert_eq!(errno(&v), Some(libc::EPERM as i64));
}

#[test]
fn socket_do_host_por_caminho_nao_existe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.sock");
    let _l = UnixListener::bind(&path).unwrap();
    assert!(ok(&probe_outside(&[
        "connect-unix",
        path.to_str().unwrap()
    ])));
    let v = probe(&["connect-unix", path.to_str().unwrap()]);
    assert_eq!(errno(&v), Some(libc::ENOENT as i64), "{v}");
}

#[test]
fn sinal_para_fora_do_dominio_e_negado() {
    // O processo 1 está fora do domínio do Landlock do comando.
    let v = probe(&["signal", "1"]);
    assert_eq!(errno(&v), Some(libc::EPERM as i64), "{v}");
}

#[test]
fn ambiente_do_processo_1_nao_e_legivel() {
    for f in ["/proc/1/environ", "/proc/1/mem", "/proc/1/cmdline"] {
        let v = probe(&["read", f]);
        if f.ends_with("cmdline") {
            continue;
        }
        assert!(!ok(&v), "{f} legível: {v}");
    }
}

#[test]
fn programa_setuid_nao_eleva() {
    if !std::path::Path::new("/usr/bin/sudo").exists() {
        return;
    }
    let s = base_spec(vec![
        "/bin/sh".into(),
        "-c".into(),
        "sudo -n id -u 2>/dev/null; echo fim".into(),
    ]);
    let out = run_spec(&s);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.lines().any(|l| l.trim() == "0"),
        "sudo elevou: {stdout}"
    );
}

// ---- 4.6 seccomp ----

#[test]
fn chamadas_negadas_devolvem_eperm_sem_matar() {
    for name in default_seccomp().deny_syscalls {
        let v = probe(&["syscall", &name]);
        assert_eq!(errno(&v), Some(libc::EPERM as i64), "{name}: {v}");
    }
}

#[test]
fn injecao_de_teclas_e_negada() {
    for name in ["TIOCSTI", "TIOCLINUX"] {
        assert_eq!(
            errno(&probe_outside(&["ioctl", name])),
            Some(libc::ENOTTY as i64)
        );
        let v = probe(&["ioctl", name]);
        assert_eq!(errno(&v), Some(libc::EPERM as i64), "{name}: {v}");
    }
}

#[test]
fn depurador_funciona_dentro_do_sandbox() {
    let v = probe(&["ptrace-child"]);
    assert!(ok(&v), "{v}");
}

// ---- 4.7 ciclo de vida ----

#[test]
fn codigo_de_saida_e_propagado() {
    let out = run_spec(&base_spec(probe_cmd(&["exit", "3"])));
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn comando_inexistente_devolve_127() {
    let out = run_spec(&base_spec(vec!["nao-existe-no-sandbox".into()]));
    assert_eq!(out.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&out.stderr).contains("não encontrado"));
}

#[test]
fn sinal_de_termino_chega_ao_comando() {
    let (mut cmd, _keep) = run_command(base_spec(probe_cmd(&["sleep", "30"])).to_json().as_bytes());
    let mut child = cmd.spawn().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let start = Instant::now();
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let status = child.wait().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(status.code(), Some(128 + libc::SIGTERM));
}

#[test]
fn processos_em_segundo_plano_morrem_com_o_comando() {
    let token = format!("sandmux-bg-{}", std::process::id());
    let out = run_spec(&base_spec(probe_cmd(&["background", &token])));
    assert!(out.status.success());
    std::thread::sleep(Duration::from_millis(200));
    for e in std::fs::read_dir("/proc").unwrap().flatten() {
        let cmdline = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        let text = String::from_utf8_lossy(&cmdline);
        assert!(
            !text.contains(&token),
            "processo em segundo plano sobreviveu: {}",
            e.path().display()
        );
    }
}

#[test]
fn ignora_sigint_no_supervisor_mas_nao_no_comando() {
    // O comando recebe SIGINT normalmente (o terminal manda para o grupo).
    let script = "trap 'echo recebeu; exit 7' INT; kill -INT $$; sleep 1; exit 0".to_string();
    let out = run_spec(&base_spec(vec!["/bin/sh".into(), "-c".into(), script]));
    assert_eq!(out.status.code(), Some(7));
}

// ---- 4.8 user namespace aninhado ----

#[test]
fn user_namespace_aninhado_e_criado_mas_nao_monta() {
    assert!(ok(&probe(&["nested-userns"])));
    let v = probe(&["nested-userns-mount"]);
    assert!(!ok(&v), "montou dentro de namespace aninhado: {v}");
}
