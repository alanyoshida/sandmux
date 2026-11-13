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

impl SupportReport {
    /// Formato de linhas que o `sandmux-run --check` escreve e o cliente lê.
    pub fn to_wire(&self) -> String {
        let s = |r: &Result<(), String>| match r {
            Ok(()) => "ok".to_string(),
            Err(e) => e.replace('\n', " "),
        };
        let aa = match self.apparmor_restrict_userns {
            Some(true) => "1",
            Some(false) => "0",
            None => "-",
        };
        format!(
            "landlock:{}\nuserns:{}\noverlay:{}\napparmor:{aa}\n",
            self.landlock_abi,
            s(&self.userns),
            s(&self.overlay)
        )
    }

    /// Lê o formato de [`SupportReport::to_wire`]. Campos ausentes contam como
    /// falha.
    pub fn from_wire(out: &str) -> SupportReport {
        let (userns, overlay) = parse_probe(out);
        let mut landlock_abi = -1;
        let mut apparmor_restrict_userns = None;
        for line in out.lines() {
            match line.split_once(':') {
                Some(("landlock", v)) => landlock_abi = v.trim().parse().unwrap_or(-1),
                Some(("apparmor", "1")) => apparmor_restrict_userns = Some(true),
                Some(("apparmor", "0")) => apparmor_restrict_userns = Some(false),
                _ => {}
            }
        }
        SupportReport {
            landlock_abi,
            userns,
            overlay,
            apparmor_restrict_userns,
        }
    }
}

/// Roda as verificações dentro do lançador (`<runner> --check`). Assim a
/// verificação vale com o perfil AppArmor do lançador, que é o processo que
/// cria o sandbox.
pub fn check_via(runner: &std::path::Path) -> SupportReport {
    match std::process::Command::new(runner)
        .arg("--check")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .output()
    {
        Ok(out) => SupportReport::from_wire(&String::from_utf8_lossy(&out.stdout)),
        Err(e) => SupportReport {
            landlock_abi: sys::landlock_abi(),
            userns: Err(format!("executar o lançador {}: {e}", runner.display())),
            overlay: Err("não verificado".into()),
            apparmor_restrict_userns: apparmor_restricts_userns(),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_de_linhas_ida_e_volta() {
        let r = SupportReport {
            landlock_abi: 8,
            userns: Ok(()),
            overlay: Err("sem\noverlay".into()),
            apparmor_restrict_userns: Some(true),
        };
        let back = SupportReport::from_wire(&r.to_wire());
        assert_eq!(back.landlock_abi, 8);
        assert!(back.userns.is_ok());
        assert_eq!(back.overlay, Err("sem overlay".into()));
        assert_eq!(back.apparmor_restrict_userns, Some(true));
    }

    #[test]
    fn resposta_vazia_e_falha() {
        let r = SupportReport::from_wire("");
        assert!(!r.can_launch());
        assert_eq!(r.apparmor_restrict_userns, None);
    }
}
