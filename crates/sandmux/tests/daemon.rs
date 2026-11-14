//! O daemon: instância única, socket, saudação, estado e parada.

mod common;
use common::cli::*;

use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

#[test]
fn segunda_instancia_recusada_e_primeira_continua() {
    let e = Env::new();
    let mut first = e.daemon_cmd().stderr(Stdio::null()).spawn().unwrap();
    e.wait_socket();
    let out = e.daemon_cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("já está rodando"), "{}", stderr(&out));
    let ls = e.run(&e.work, &["ls"]);
    assert!(ls.status.success() && stdout(&ls).contains("Não há sessões"));
    // Pasta e socket privados.
    let dir_mode = std::fs::metadata(e.runtime.join("sandmux"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(dir_mode & 0o777, 0o700);
    let _ = first.kill();
    let _ = first.wait();
}

#[test]
fn socket_orfao_de_daemon_morto_e_removido() {
    let e = Env::new();
    let mut first = e.daemon_cmd().stderr(Stdio::null()).spawn().unwrap();
    e.wait_socket();
    first.kill().unwrap();
    first.wait().unwrap();
    assert!(e.socket().exists(), "o SIGKILL deixa o socket para trás");
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("Não há sessões"));
}

#[test]
fn protocolo_diferente_recusado_com_as_duas_versoes() {
    let e = Env::new();
    let mut d = e
        .daemon_cmd()
        .env("SANDMUX_PROTOCOL_OVERRIDE", "999")
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    e.wait_socket();
    let out = e.run(&e.work, &["ls"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("protocolo 999")
            && err.contains("protocolo 2 ")
            && err.contains("sandmux daemon restart"),
        "{err}"
    );
    // O daemon continua rodando, intacto.
    assert!(d.try_wait().unwrap().is_none());
    assert!(e.socket().exists());
    let _ = d.kill();
    let _ = d.wait();
}

#[test]
fn diretorio_de_dados_diferente_recusado() {
    let e = Env::new();
    let other = e.dir.path().join("outros-dados");
    std::fs::create_dir_all(&other).unwrap();
    let mut d = e
        .daemon_cmd()
        .env("XDG_DATA_HOME", &other)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    e.wait_socket();
    let out = e.run(&e.work, &["ls"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(
        err.contains("outros-dados/sandmux") && err.contains(&e.data_dir().display().to_string()),
        "{err}"
    );
    let _ = d.kill();
    let _ = d.wait();
}

/// Espera o processo terminar (um zumbi ainda não colhido conta como fim).
fn gone(pid: i32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(_) => return true,
            Ok(s) if s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')) => return true,
            Ok(_) => std::thread::sleep(std::time::Duration::from_millis(30)),
        }
    }
    false
}

#[test]
fn status_e_stop() {
    let e = Env::new();
    let out = e.run(&e.work, &["daemon", "status"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stdout(&out).contains("não está rodando"),
        "{}",
        stdout(&out)
    );
    assert!(!e.socket().exists(), "o status não pode subir o daemon");

    assert!(e.run(&e.work, &["ls"]).status.success());
    let out = e.run(&e.work, &["daemon", "status"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let s = stdout(&out);
    assert!(
        s.contains("está rodando")
            && s.contains("protocolo 2")
            && s.contains("iniciado por: direct")
            && s.contains("sessões: 0"),
        "{s}"
    );
    let pid = e.daemon_pid().unwrap();

    let out = e.run(&e.work, &["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!e.socket().exists());
    assert!(gone(pid), "o daemon continua vivo");
    let out = e.run(&e.work, &["daemon", "status"]);
    assert_ne!(out.status.code(), Some(0));
}

#[test]
fn subida_direta_continua_depois_do_cliente() {
    let e = Env::new();
    let out = e.run(&e.work, &["ls"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let pid = e.daemon_pid().expect("o daemon subiu");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!gone_now(pid), "o daemon morreu junto com o cliente");
    assert!(e.state.join("sandmux/sandmuxd.log").exists());
    let out = e.run(&e.work, &["daemon", "status"]);
    assert!(
        stdout(&out).contains(&format!("PID: {pid}")),
        "{}",
        stdout(&out)
    );
}

fn gone_now(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')),
    }
}

#[test]
fn disable_remove_a_unidade_sem_systemctl() {
    let e = Env::new();
    let unit_dir = e.config.join("systemd/user");
    std::fs::create_dir_all(&unit_dir).unwrap();
    std::fs::write(unit_dir.join("sandmuxd.service"), "[Service]\n").unwrap();
    assert!(e.run(&e.work, &["ls"]).status.success());
    let out = e.run(&e.work, &["daemon", "disable"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!unit_dir.join("sandmuxd.service").exists());
    assert!(stderr(&out).contains("removida"), "{}", stderr(&out));
    assert!(!e.socket().exists(), "o disable para o daemon");
}

#[test]
fn doctor_mostra_o_daemon_sem_iniciar() {
    let e = Env::new();
    let unit_dir = e.config.join("systemd/user");
    std::fs::create_dir_all(&unit_dir).unwrap();
    std::fs::write(unit_dir.join("sandmuxd.service"), "[Service]\n").unwrap();
    let out = e.run(&e.work, &["doctor"]);
    let s = stdout(&out);
    assert!(
        s.contains("Daemon de sessões")
            && s.contains("estado: parado")
            && s.contains(&e.socket().display().to_string())
            && s.contains("unidade systemd de usuário: instalada"),
        "{s}"
    );
    assert!(!e.socket().exists(), "o doctor subiu o daemon");
    assert!(e.run(&e.work, &["ls"]).status.success());
    let s = stdout(&e.run(&e.work, &["doctor"]));
    assert!(
        s.contains("estado: rodando") && s.contains("iniciado por direct"),
        "{s}"
    );
}
