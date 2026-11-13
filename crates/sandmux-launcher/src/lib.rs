//! Lançador do sandbox do sandmux.
//!
//! Recebe uma [`sandmux_spec::LaunchSpec`] pronta, monta o confinamento que
//! ela descreve (user, mount, pid, ipc e uts namespaces; raiz nova; Landlock;
//! seccomp) e executa o comando dentro dele. Não conhece harnesses nem
//! política.

mod landlock_rules;
mod launch;
mod mounts;
pub mod seccomp;
pub mod support;
mod sys;

pub use landlock_rules::REQUIRED_ABI;
pub use launch::{EXIT_EXEC, EXIT_SETUP, LaunchError, check_kernel, launch};
