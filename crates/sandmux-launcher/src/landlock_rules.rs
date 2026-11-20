//! Aplicação das regras de Landlock da especificação.
//!
//! A ABI usada é a 6 (kernel 6.12), a primeira com escopo de sockets
//! abstratos e de sinais, e o nível de compatibilidade é exigência rígida: se
//! o kernel não oferecer algum direito pedido, a restrição falha e o comando
//! não roda. Num kernel com a ABI 4 ou 5 (o Ubuntu 24.04 com o 6.8), usa a
//! ABI do kernel, e o auxiliar root completa o que falta por BPF LSM (o
//! lançador registra o sandbox nele antes de liberar o comando).

use std::os::fd::AsFd;

use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, Ruleset, RulesetAttr,
    RulesetCreatedAttr, RulesetStatus, Scope,
};
use nix::sys::stat::{SFlag, fstat};
use sandmux_spec::LandlockRules;

/// ABI do Landlock que o sandmux exige sem o auxiliar root.
pub const REQUIRED_ABI: i32 = 6;
/// A menor ABI aceita com o auxiliar root completando o resto.
pub const MIN_ABI: i32 = 4;
/// Variável de teste que faz o lançador agir como num kernel com uma ABI
/// menor (só baixa).
pub const ABI_ENV: &str = "SANDMUX_RUN_LANDLOCK_ABI";

/// A ABI usada: a do kernel, limitada a [`REQUIRED_ABI`], ou menor pela
/// variável de teste.
pub fn effective_abi() -> i32 {
    let k = crate::sys::landlock_abi().min(REQUIRED_ABI);
    std::env::var(ABI_ENV)
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .map_or(k, |v| v.min(k))
}

/// Abre o caminho com `O_PATH | O_CLOEXEC`, seguindo links, como o `PathFd`
/// do crate `landlock`, mas pelo `open` direto: no musl (os binários das
/// releases), o `O_ACCMODE` inclui o `O_PATH`, e o `custom_flags` do
/// `OpenOptions` o tira. O caminho era aberto para leitura: um caminho sem
/// leitura ficava sem regra, e um FIFO travava o lançador.
pub fn open_path(p: &std::path::Path) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: caminho terminado em NUL; o descritor devolvido é nosso.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: descritor recém-aberto.
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

fn abi_of(n: i32) -> ABI {
    match n {
        ..=4 => ABI::V4,
        5 => ABI::V5,
        _ => ABI::V6,
    }
}

pub fn apply(rules: &LandlockRules) -> Result<(), String> {
    let n = effective_abi();
    if n < MIN_ABI {
        return Err(sandmux_spec::trf!(
            "o kernel não oferece o Landlock ABI {MIN_ABI} (encontrado: {n})",
            "the kernel does not offer Landlock ABI {MIN_ABI} (found: {n})"
        ));
    }
    let abi_used = abi_of(n);
    let ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(abi_used))
        .map_err(|e| e.to_string())?;
    // Os escopos só existem a partir da ABI 6; abaixo dela, vêm do auxiliar.
    let ruleset = if n >= REQUIRED_ABI {
        ruleset
            .scope(Scope::from_all(abi_used))
            .map_err(|e| e.to_string())?
    } else {
        ruleset
    };
    let mut ruleset = ruleset.create().map_err(|e| e.to_string())?;

    let groups = [
        (&rules.read, AccessFs::from_read(abi_used)),
        (&rules.write, AccessFs::from_all(abi_used)),
    ];
    for (paths, access) in groups {
        for p in paths {
            // Um caminho que não existe não concede nada; ignorá-lo só reduz acesso.
            let Ok(fd) = open_path(p) else { continue };
            let is_dir = fstat(fd.as_fd())
                .map(|st| st.st_mode & SFlag::S_IFMT.bits() == SFlag::S_IFDIR.bits())
                .unwrap_or(false);
            let access = if is_dir {
                access
            } else {
                access & AccessFs::from_file(abi_used)
            };
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, access))
                .map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    let status = ruleset.restrict_self().map_err(|e| e.to_string())?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(sandmux_spec::trf!(
            "Landlock não foi aplicado por completo: {:?}",
            "Landlock was not fully enforced: {:?}",
            status.ruleset
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminho_sem_leitura_e_fifo_abrem_sem_conteudo() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let fifo = d.path().join("cano");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        // Um FIFO sem ninguém do outro lado: o `open` para leitura travaria.
        let fd = open_path(&fifo).unwrap();
        let st = nix::sys::stat::fstat(&fd).unwrap();
        assert_eq!(st.st_mode & libc::S_IFMT, libc::S_IFIFO);
        // Uma pasta que o usuário atravessa mas não lista.
        let fechada = d.path().join("fechada");
        std::fs::create_dir(&fechada).unwrap();
        std::fs::set_permissions(&fechada, std::fs::Permissions::from_mode(0o111)).unwrap();
        if std::fs::read_dir(&fechada).is_err() {
            assert!(open_path(&fechada).is_ok());
        }
        std::fs::set_permissions(&fechada, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn regra_vale_numa_pasta_sem_leitura() {
        use std::os::unix::fs::PermissionsExt;
        if crate::sys::landlock_abi() < MIN_ABI {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let fechada = d.path().join("fechada");
        std::fs::create_dir(&fechada).unwrap();
        std::fs::write(fechada.join("dentro"), "ok").unwrap();
        std::fs::set_permissions(&fechada, std::fs::Permissions::from_mode(0o311)).unwrap();
        if std::fs::read_dir(&fechada).is_ok() {
            std::fs::set_permissions(&fechada, std::fs::Permissions::from_mode(0o755)).unwrap();
            return; // root lista tudo; o teste não diz nada
        }
        let rules = LandlockRules {
            read: vec![fechada.clone()],
            write: vec![],
        };
        // Num filho: o Landlock vale para o processo inteiro.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => {
                let ok = apply(&rules).is_ok()
                    && std::fs::read_to_string(fechada.join("dentro"))
                        .ok()
                        .as_deref()
                        == Some("ok")
                    && std::fs::read_to_string("/etc/hostname").is_err();
                unsafe { libc::_exit(if ok { 0 } else { 1 }) };
            }
            nix::unistd::ForkResult::Parent { child } => {
                let st = nix::sys::wait::waitpid(child, None).unwrap();
                std::fs::set_permissions(&fechada, std::fs::Permissions::from_mode(0o755)).unwrap();
                assert_eq!(st, nix::sys::wait::WaitStatus::Exited(child, 0));
            }
        }
    }

    #[test]
    fn abi_escolhida() {
        assert_eq!(abi_of(4), ABI::V4);
        assert_eq!(abi_of(5), ABI::V5);
        assert_eq!(abi_of(6), ABI::V6);
        assert_eq!(abi_of(9), ABI::V6);
    }
}
