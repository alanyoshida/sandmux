//! Utilitários dos testes de integração: especificações mínimas e execução do
//! lançador e da sonda.
#![allow(dead_code)]

use std::io::{Seek, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use sandmux_spec::{EnvVar, LandlockRules, LaunchSpec, Mount, SPEC_VERSION, SeccompRules};

pub const RUN: &str = env!("CARGO_BIN_EXE_sandmux-run");
pub const PROBE: &str = env!("CARGO_BIN_EXE_sandmux-probe");
pub const SANDMUX: &str = env!("CARGO_BIN_EXE_sandmux");

pub fn bin_dir() -> PathBuf {
    Path::new(PROBE).parent().unwrap().to_path_buf()
}

pub fn default_seccomp() -> SeccompRules {
    SeccompRules {
        deny_syscalls: [
            "mount",
            "umount2",
            "pivot_root",
            "move_mount",
            "open_tree",
            "fsopen",
            "fsconfig",
            "fsmount",
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
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        deny_ioctls: vec!["TIOCSTI".into(), "TIOCLINUX".into()],
    }
}

/// Especificação mínima: sistema em somente leitura, /proc, /dev mínimo, /tmp
/// privado e a pasta dos binários de teste em somente leitura.
pub fn base_spec(command: Vec<String>) -> LaunchSpec {
    let mut mounts = Vec::new();
    for d in ["usr", "etc", "opt"] {
        let p = Path::new("/").join(d);
        if p.is_dir() && !p.is_symlink() {
            mounts.push(Mount::Bind {
                source: p.clone(),
                dest: p,
                writable: false,
            });
        }
    }
    for d in ["bin", "sbin", "lib", "lib64", "lib32"] {
        let p = Path::new("/").join(d);
        if let Ok(target) = std::fs::read_link(&p) {
            mounts.push(Mount::Symlink { dest: p, target });
        } else if p.is_dir() {
            mounts.push(Mount::Bind {
                source: p.clone(),
                dest: p,
                writable: false,
            });
        }
    }
    mounts.push(Mount::Proc {
        dest: "/proc".into(),
    });
    mounts.push(Mount::Tmpfs {
        dest: "/dev".into(),
        mode: 0o755,
    });
    for d in ["null", "zero", "full", "random", "urandom", "tty"] {
        let p = Path::new("/dev").join(d);
        mounts.push(Mount::Device {
            source: p.clone(),
            dest: p,
        });
    }
    mounts.push(Mount::Devpts {
        dest: "/dev/pts".into(),
    });
    mounts.push(Mount::Symlink {
        dest: "/dev/ptmx".into(),
        target: "pts/ptmx".into(),
    });
    mounts.push(Mount::Tmpfs {
        dest: "/tmp".into(),
        mode: 0o1777,
    });
    let bins = bin_dir();
    mounts.push(Mount::Bind {
        source: bins.clone(),
        dest: bins,
        writable: false,
    });
    LaunchSpec {
        version: SPEC_VERSION,
        mounts,
        landlock: LandlockRules {
            read: vec!["/".into()],
            write: vec!["/tmp".into(), "/dev".into(), "/proc".into()],
        },
        seccomp: default_seccomp(),
        env: vec![
            EnvVar {
                name: "PATH".into(),
                value: "/usr/local/bin:/usr/bin:/bin".into(),
            },
            EnvVar {
                name: "HOME".into(),
                value: "/tmp".into(),
            },
        ],
        command,
        cwd: "/tmp".into(),
    }
}

pub fn probe_cmd(args: &[&str]) -> Vec<String> {
    std::iter::once(PROBE.to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect()
}

/// Prepara o comando do lançador com a especificação num memfd herdado.
pub fn run_command(json: &[u8]) -> (Command, std::fs::File) {
    let fd = nix::sys::memfd::memfd_create(c"spec", nix::sys::memfd::MFdFlags::empty()).unwrap();
    let mut f = std::fs::File::from(fd);
    f.write_all(json).unwrap();
    f.rewind().unwrap();
    let mut cmd = Command::new(RUN);
    cmd.arg("--spec-fd").arg(f.as_raw_fd().to_string());
    cmd.stdin(Stdio::null());
    (cmd, f)
}

pub fn run_json(json: &[u8]) -> Output {
    let (mut cmd, _keep) = run_command(json);
    cmd.output().unwrap()
}

pub fn run_spec(spec: &LaunchSpec) -> Output {
    run_json(spec.to_json().as_bytes())
}

/// Roda a sonda dentro do sandbox e devolve a linha JSON dela.
pub fn probe_in(spec: &LaunchSpec) -> serde_json::Value {
    let out = run_spec(spec);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().last().unwrap_or_else(|| {
        panic!(
            "sonda sem saída (código {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    serde_json::from_str(line).unwrap_or_else(|e| panic!("saída inválida {line:?}: {e}"))
}

pub fn probe(args: &[&str]) -> serde_json::Value {
    probe_in(&base_spec(probe_cmd(args)))
}

pub fn probe_outside(args: &[&str]) -> serde_json::Value {
    let out = Command::new(PROBE).args(args).output().unwrap();
    serde_json::from_slice(out.stdout.trim_ascii()).unwrap()
}

pub fn ok(v: &serde_json::Value) -> bool {
    v["ok"].as_bool().unwrap()
}

pub fn errno(v: &serde_json::Value) -> Option<i64> {
    v["errno"].as_i64()
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap())
}

pub mod cli;

/// Roda um comando cuja preparação cria um user namespace no processo de
/// teste, que não tem perfil AppArmor. Com a restrição do AppArmor ligada,
/// essa preparação é negada: devolve `None` e o teste deve ser pulado.
#[allow(dead_code)]
pub fn output_unless_restricted(cmd: &mut Command, test: &str) -> Option<Output> {
    match cmd.output() {
        Ok(out) => Some(out),
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied
                && std::fs::read_to_string(
                    "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
                )
                .is_ok_and(|s| s.trim() == "1") =>
        {
            eprintln!(
                "{test}: pulado, a restrição de user namespaces do AppArmor impede a preparação no processo de teste"
            );
            None
        }
        Err(e) => panic!("{e}"),
    }
}
