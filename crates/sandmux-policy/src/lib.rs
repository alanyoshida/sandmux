//! Política do sandmux: baseline de segurança, toolchains, proteção do git e
//! presets de harness, compostos numa [`sandmux_spec::LaunchSpec`].

pub mod compose;
pub mod config;
pub mod git;
pub mod hermes_config;
pub mod home;
pub mod host;
pub mod layout;
pub mod lists;
pub mod presets;
pub mod toolchains;

use std::path::PathBuf;

pub use compose::{Composed, LayerMode, RunOptions, check_dangerous, compose};
pub use host::Host;
pub use layout::Layout;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("harness desconhecido: {0}. Presets disponíveis: {list}", list = presets::PRESETS.join(", "))]
    UnknownHarness(String),
    #[error("{harness} não está instalado (procurado em: {}). Para instalar: {hint}", .searched.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))]
    NotInstalled {
        harness: String,
        searched: Vec<PathBuf>,
        hint: String,
    },
    #[error("{} não pode ser usado: {reason}. {hint}", .path.display())]
    Dangerous {
        path: PathBuf,
        reason: String,
        hint: String,
    },
    #[error("caminho não existe: {}", .0.display())]
    MissingPath(PathBuf),
    #[error("a variável {0}, pedida em --pass-env, não existe no ambiente")]
    MissingEnv(String),
    #[error(
        "o valor de {0}, pedido em --pass-env, tem quebra de linha ou `${{` e não pode ser escrito no .env do hermes"
    )]
    UnrepresentableEnv(String),
    #[error("{0}")]
    Io(String),
}
