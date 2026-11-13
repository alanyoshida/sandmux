//! Aplicação das regras de Landlock da especificação.
//!
//! A ABI usada é a 6 (kernel 6.12), a primeira com escopo de sockets
//! abstratos e de sinais, e o nível de compatibilidade é exigência rígida: se
//! o kernel não oferecer algum direito pedido, a restrição falha e o comando
//! não roda.

use std::os::fd::AsFd;

use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr, RulesetStatus, Scope,
};
use nix::sys::stat::{SFlag, fstat};
use sandmux_spec::LandlockRules;

/// ABI do Landlock que o sandmux exige.
pub const REQUIRED_ABI: i32 = 6;
const ABI_USED: ABI = ABI::V6;

pub fn apply(rules: &LandlockRules) -> Result<(), String> {
    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_all(ABI_USED))
        .map_err(|e| e.to_string())?
        .scope(Scope::from_all(ABI_USED))
        .map_err(|e| e.to_string())?
        .create()
        .map_err(|e| e.to_string())?;

    let groups = [
        (&rules.read, AccessFs::from_read(ABI_USED)),
        (&rules.write, AccessFs::from_all(ABI_USED)),
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
                access & AccessFs::from_file(ABI_USED)
            };
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, access))
                .map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    let status = ruleset.restrict_self().map_err(|e| e.to_string())?;
    if status.ruleset != RulesetStatus::FullyEnforced {
        return Err(format!(
            "Landlock não foi aplicado por completo: {:?}",
            status.ruleset
        ));
    }
    Ok(())
}
