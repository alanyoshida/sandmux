//! Confinamento do Docker privado por BPF LSM.
//!
//! Sem `clang` e sem toolchain nightly: o BTF do kernel é lido na carga
//! ([`btf`]), as instruções são montadas em Rust ([`asm`]) e carregadas pela
//! chamada `bpf()` ([`sys`]). Os programas e o que eles decidem estão em
//! [`prog`]. Carregar exige root; ler o BTF e montar os programas, não.

pub mod asm;
pub mod btf;
pub mod prog;
pub mod ring;
pub mod sys;

/// Lista dos LSMs ativos no kernel.
pub const LSM_LIST: &str = "/sys/kernel/security/lsm";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("BTF: {0}")]
    Btf(String),
    #[error("montagem do programa BPF: {0}")]
    Asm(String),
    #[error("bpf {what}: {err}{}", if log.is_empty() { String::new() } else { format!("\n{log}") })]
    Sys {
        what: String,
        err: std::io::Error,
        log: String,
    },
}

/// O BPF LSM está entre os LSMs ativos? `Err` com o motivo quando não dá para
/// saber.
pub fn lsm_active() -> Result<bool, String> {
    let s = std::fs::read_to_string(LSM_LIST).map_err(|e| format!("{LSM_LIST}: {e}"))?;
    Ok(lsm_has_bpf(&s))
}

/// `bpf` está na lista (separada por vírgulas)?
pub fn lsm_has_bpf(list: &str) -> bool {
    list.trim().split(',').any(|x| x == "bpf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_de_lsms() {
        assert!(lsm_has_bpf(
            "lockdown,capability,landlock,yama,apparmor,bpf,ima,evm\n"
        ));
        assert!(!lsm_has_bpf("lockdown,capability,landlock,yama,apparmor"));
        assert!(!lsm_has_bpf("bpfx,apparmor"));
    }
}
