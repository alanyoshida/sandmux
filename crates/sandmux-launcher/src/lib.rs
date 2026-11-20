//! Lançador do sandbox do sandmux.
//!
//! Recebe uma [`sandmux_spec::LaunchSpec`] pronta, monta o confinamento que
//! ela descreve (user, mount, pid, ipc, uts e, com rede, net namespaces; raiz
//! nova; gateway de rede no supervisor; Landlock;
//! seccomp) e executa o comando dentro dele. Não conhece harnesses nem
//! política.

mod connector;
pub mod enter;
pub mod files;
mod landlock_rules;
mod launch;
pub mod mounter;
mod mounts;
mod namer;
mod net;
mod relay;
pub mod scope;
pub mod seccomp;
pub mod support;
mod sys;

pub use landlock_rules::REQUIRED_ABI;
pub use launch::{
    EXIT_EXEC, EXIT_SETUP, KernelLevel, LaunchError, check_kernel, exec_view, launch,
};
