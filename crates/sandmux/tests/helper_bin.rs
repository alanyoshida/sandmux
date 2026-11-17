//! O binário `sandmux-helper` sem root: sobe com `--socket`, responde ao
//! `status`, recusa o que não pode fazer e continua de pé.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sandmux::helper::Client;

const HELPER: &str = env!("CARGO_BIN_EXE_sandmux-helper");

#[test]
fn ajuda_e_versao() {
    let o = Command::new(HELPER).arg("--help").output().unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("--socket"));
    let o = Command::new(HELPER).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains(env!("CARGO_PKG_VERSION")));
    // Sem o socket do systemd e sem --socket: erro, sem ficar parado.
    let o = Command::new(HELPER)
        .env_remove("LISTEN_FDS")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("LISTEN_FDS"));
}

#[test]
fn sem_root_recusa_e_continua() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let sock = d.path().join("h.sock");
    let mut child = Command::new(HELPER)
        .arg("--socket")
        .arg(&sock)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !sock.exists() {
        assert!(Instant::now() < deadline, "o socket não apareceu");
        std::thread::sleep(Duration::from_millis(20));
    }
    let c = Client::connect(&sock).unwrap();
    let s = c.status().unwrap();
    assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
    // O cgroup do próprio teste: ou é de outro dono, ou a carga falha sem
    // privilégio. Nos dois casos, erro e nenhuma queda.
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    if let Some(rel) = cg.trim().strip_prefix("0::") {
        let dir = Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
        let e = c.confine(&dir).unwrap_err();
        assert!(!e.is_empty());
    }
    assert!(Client::connect(&sock).unwrap().status().is_ok());
    let _ = child.kill();
    let _ = child.wait();
}
