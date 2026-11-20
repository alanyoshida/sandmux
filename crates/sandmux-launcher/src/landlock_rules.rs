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
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
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
            let Ok(fd) = PathFd::new(p) else { continue };
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
    fn abi_escolhida() {
        assert_eq!(abi_of(4), ABI::V4);
        assert_eq!(abi_of(5), ABI::V5);
        assert_eq!(abi_of(6), ABI::V6);
        assert_eq!(abi_of(9), ABI::V6);
    }
}
