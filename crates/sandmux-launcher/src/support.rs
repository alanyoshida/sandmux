//! Verificação do suporte da máquina, usada pelo `sandmux doctor` e pelo
//! `sandmux run` para decidir o recuo da camada de escrita.
//!
//! A verificação de user namespace cria um de verdade num processo filho e
//! tenta montar um tmpfs: em distros que restringem user namespaces por
//! AppArmor, criar o namespace funciona, mas montar não.

use std::io::{Read, Write};
use std::os::fd::AsFd;

use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork, getgid, getuid, pipe};

use crate::landlock_rules::REQUIRED_ABI;
use crate::sys::{self, FsContext, MOUNT_ATTR_NODEV, MOUNT_ATTR_NOSUID};

/// Resultado das verificações.
#[derive(Debug, Clone)]
pub struct SupportReport {
    /// ABI do Landlock (negativa quando ausente).
    pub landlock_abi: i32,
    /// Criação de user namespace com montagem dentro dele.
    pub userns: Result<(), String>,
    /// Overlay montado dentro do user namespace.
    pub overlay: Result<(), String>,
    /// Valor de `kernel.apparmor_restrict_unprivileged_userns`, quando existe.
    pub apparmor_restrict_userns: Option<bool>,
}

impl SupportReport {
    pub fn landlock_ok(&self) -> bool {
        self.landlock_abi >= REQUIRED_ABI
    }

    /// O sandbox pode ser lançado (o overlay é opcional).
    pub fn can_launch(&self) -> bool {
        self.landlock_ok() && self.userns.is_ok()
    }
}

pub fn apparmor_restricts_userns() -> Option<bool> {
    std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
        .ok()
        .map(|s| s.trim() == "1")
}

/// Roda todas as verificações. Precisa ser chamado num processo com uma única
/// thread.
pub fn check() -> SupportReport {
    let (userns, overlay) = probe_namespaces();
    SupportReport {
        landlock_abi: sys::landlock_abi(),
        userns,
        overlay,
        apparmor_restrict_userns: apparmor_restricts_userns(),
    }
}

fn probe_namespaces() -> (Result<(), String>, Result<(), String>) {
    let (r, w) = match pipe() {
        Ok(p) => p,
        Err(e) => return (Err(e.to_string()), Err("não verificado".into())),
    };
    let (uid, gid) = (getuid().as_raw(), getgid().as_raw());
    // SAFETY: chamado num processo de uma thread; o filho só faz chamadas de
    // sistema e sai.
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            drop(r);
            let result = child_probe(uid, gid);
            let mut f = std::fs::File::from(w);
            let _ = f.write_all(result.as_bytes());
            drop(f);
            // SAFETY: saída imediata do filho.
            unsafe { libc::_exit(0) };
        }
        Ok(ForkResult::Parent { child }) => {
            drop(w);
            let mut out = String::new();
            let _ = std::fs::File::from(r).read_to_string(&mut out);
            let _ = waitpid(child, None).map(|s| matches!(s, WaitStatus::Exited(..)));
            parse_probe(&out)
        }
        Err(e) => (Err(e.to_string()), Err("não verificado".into())),
    }
}

/// Saída do filho: duas linhas, `userns:<ok|erro>` e `overlay:<ok|erro>`.
fn parse_probe(out: &str) -> (Result<(), String>, Result<(), String>) {
    let mut userns = Err("a verificação não respondeu".to_string());
    let mut overlay = Err("não verificado".to_string());
    for line in out.lines() {
        let (k, v) = line.split_once(':').unwrap_or((line, ""));
        let r = if v == "ok" {
            Ok(())
        } else {
            Err(v.to_string())
        };
        match k {
            "userns" => userns = r,
            "overlay" => overlay = r,
            _ => {}
        }
    }
    (userns, overlay)
}

fn child_probe(uid: u32, gid: u32) -> String {
    let fmt = |u: Result<(), String>, o: Result<(), String>| {
        let s = |r: &Result<(), String>| match r {
            Ok(()) => "ok".to_string(),
            Err(e) => e.replace('\n', " "),
        };
        format!("userns:{}\noverlay:{}\n", s(&u), s(&o))
    };
    if let Err(e) = nix::sched::unshare(
        nix::sched::CloneFlags::CLONE_NEWUSER | nix::sched::CloneFlags::CLONE_NEWNS,
    ) {
        return fmt(
            Err(format!("criar user namespace: {e}")),
            Err("não verificado".into()),
        );
    }
    let maps = std::fs::write("/proc/self/setgroups", "deny")
        .and_then(|_| std::fs::write("/proc/self/uid_map", format!("{uid} {uid} 1\n")))
        .and_then(|_| std::fs::write("/proc/self/gid_map", format!("{gid} {gid} 1\n")));
    if let Err(e) = maps {
        return fmt(
            Err(format!("mapear o usuário: {e}")),
            Err("não verificado".into()),
        );
    }
    let tmp = (|| {
        let fsc = FsContext::open("tmpfs")?;
        fsc.set_string("mode", "0755")?;
        fsc.mount(MOUNT_ATTR_NOSUID | MOUNT_ATTR_NODEV)
    })();
    let tmp = match tmp {
        Ok(fd) => fd,
        Err(e) => {
            return fmt(
                Err(format!("montar dentro do user namespace: {e}")),
                Err("não verificado".into()),
            );
        }
    };
    let overlay = (|| -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        for d in ["l", "u", "w", "m"] {
            nix::sys::stat::mkdirat(
                tmp.as_fd(),
                d,
                nix::sys::stat::Mode::from_bits_truncate(0o755),
            )?;
        }
        let base = format!("/proc/self/fd/{}", tmp.as_raw_fd());
        let fsc = FsContext::open("overlay")?;
        fsc.set_string("lowerdir+", &format!("{base}/l"))?;
        fsc.set_string("upperdir", &format!("{base}/u"))?;
        fsc.set_string("workdir", &format!("{base}/w"))?;
        fsc.mount(MOUNT_ATTR_NODEV).map(|_| ())
    })()
    .map_err(|e| e.to_string());
    fmt(Ok(()), overlay)
}
