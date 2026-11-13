//! Grupo 10: linha de comando e diagnóstico.

mod common;
use common::cli::*;
use common::*;

use std::os::unix::process::CommandExt;
use std::process::Command;

#[test]
fn ajuda_do_run_lista_todas_as_opcoes() {
    let out = Command::new(SANDMUX)
        .args(["run", "--help"])
        .output()
        .unwrap();
    let s = stdout(&out);
    for opt in [
        "--workdir",
        "--ro",
        "--rw",
        "--pass-env",
        "--import",
        "--no-import",
        "--repo",
        "--kvm",
        "--no-gpu",
    ] {
        assert!(s.contains(opt), "{opt} ausente na ajuda:\n{s}");
    }
}

#[test]
fn doctor_com_suporte_completo() {
    let e = Env::new();
    let out = e.run(&e.work, &["doctor"]);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{s}");
    for l in [
        "[ok  ] Landlock",
        "[ok  ] user namespace",
        "[ok  ] camada de escrita",
    ] {
        assert!(s.contains(l), "{l}:\n{s}");
    }
    assert!(s.contains(&e.data_dir().join("inbox").display().to_string()));
    assert!(s.contains("sem filtro e sem registro"));
}

#[test]
fn doctor_sem_camada_de_escrita_avisa_e_termina_com_zero() {
    let e = Env::new();
    let out = e
        .cmd(&e.work, &["doctor"])
        .env("SANDMUX_DISABLE_OVERLAY", "1")
        .output()
        .unwrap();
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        s.contains("[FALHA] camada de escrita") && s.contains("somente leitura"),
        "{s}"
    );
}

#[test]
fn doctor_sem_user_namespace_termina_com_erro() {
    let e = Env::new();
    let mut cmd = e.cmd(&e.work, &["doctor"]);
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let uid_map = format!("{uid} {uid} 1\n");
    let gid_map = format!("{gid} {gid} 1\n");
    // SAFETY: o pre_exec só usa chamadas de sistema com buffers preparados antes.
    unsafe {
        cmd.pre_exec(move || {
            fn write(path: &std::ffi::CStr, data: &[u8]) -> std::io::Result<()> {
                // SAFETY: caminho e dados válidos.
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
            write(c"/proc/sys/user/max_user_namespaces", b"0")
        });
    }
    cmd.env("SANDMUX_ASSUME_APPARMOR_RESTRICT", "1");
    let Some(out) =
        output_unless_restricted(&mut cmd, "doctor_sem_user_namespace_termina_com_erro")
    else {
        return;
    };
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    assert!(s.contains("[FALHA] user namespace"), "{s}");
    // Com a restrição do AppArmor, mostra o perfil com o caminho real do lançador.
    let runner = std::fs::canonicalize(RUN).unwrap();
    assert!(s.contains("apparmor_restrict_unprivileged_userns=1"), "{s}");
    assert!(
        s.contains(&format!(
            "profile sandmux-run {} flags=(unconfined)",
            runner.display()
        )),
        "{s}"
    );
    assert!(
        s.contains("\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n"),
        "{s}"
    );
    // O comando sai sem recuo, para o heredoc terminar ao ser colado.
    assert!(
        s.contains("\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n")
            && s.contains("}\nEOF\n"),
        "{s}"
    );
}

#[test]
fn lancador_responde_a_verificacao() {
    let out = std::process::Command::new(RUN)
        .arg("--check")
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("\nuserns:ok\n") && s.starts_with("landlock:"),
        "{s}"
    );
}

#[test]
fn perfil_apparmor_tem_o_caminho_do_lancador() {
    let tpl = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/apparmor/sandmux-run"
    ))
    .unwrap();
    assert!(
        tpl.contains("@SANDMUX_RUN@")
            && tpl.contains("userns,")
            && tpl.contains("flags=(unconfined)")
    );
}

#[test]
fn ajuda_lista_os_comandos_de_sessao() {
    let out = std::process::Command::new(SANDMUX)
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    for c in ["run", "ls", "attach", "detach", "kill", "restart", "daemon"] {
        assert!(
            help.lines()
                .any(|l| l.trim_start().starts_with(&format!("{c} "))),
            "{c} ausente:\n{help}"
        );
    }
    let out = std::process::Command::new(SANDMUX)
        .args(["run", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--allow-clipboard") && help.contains("Ctrl+b d"),
        "{help}"
    );
}
