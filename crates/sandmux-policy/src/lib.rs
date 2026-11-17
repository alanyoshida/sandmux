//! Política do sandmux: baseline de segurança, toolchains, proteção do git e
//! presets de harness, compostos numa [`sandmux_spec::LaunchSpec`].

pub mod assistant;
pub mod compose;
pub mod config;
pub mod git;
pub mod hermes_config;
pub mod home;
pub mod host;
pub mod knownhosts;
pub mod layout;
pub mod limits;
pub mod lists;
pub mod mcp;
pub mod netcatalog;
pub mod opencode_models;
pub mod presets;
pub mod toolchains;
pub mod toolsecrets;

use std::path::PathBuf;

pub use compose::{
    Composed, DockerView, LayerMode, MAX_DOCKER_COVERS, RunOptions, check_dangerous, compose,
    compose_docker_view, docker_covers,
};
pub use host::Host;
pub use layout::Layout;

#[derive(Debug)]
pub enum PolicyError {
    UnknownHarness(String),
    /// O preset não tem atualizador (`sandmux update`).
    NoUpdater(String),
    NotInstalled {
        harness: String,
        searched: Vec<PathBuf>,
        hint: String,
    },
    Dangerous {
        path: PathBuf,
        reason: String,
        hint: String,
    },
    MissingPath(PathBuf),
    MissingEnv(String),
    UnrepresentableEnv(String),
    Io(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use sandmux_spec::trf;
        let s = match self {
            PolicyError::UnknownHarness(h) => trf!(
                "harness desconhecido: {h}. Presets disponíveis: {}",
                "unknown harness: {h}. Available presets: {}",
                presets::PRESETS.join(", ")
            ),
            PolicyError::NoUpdater(h) => trf!(
                "{h} não tem atualizador. Harnesses com atualizador: opencode, hermes, claude, omp, agy",
                "{h} has no updater. Harnesses with an updater: opencode, hermes, claude, omp, agy"
            ),
            PolicyError::NotInstalled {
                harness,
                searched,
                hint,
            } => trf!(
                "{harness} não está instalado (procurado em: {}). Para instalar: {hint}",
                "{harness} is not installed (searched in: {}). To install: {hint}",
                searched
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            PolicyError::Dangerous { path, reason, hint } => trf!(
                "{} não pode ser usado: {reason}. {hint}",
                "{} cannot be used: {reason}. {hint}",
                path.display()
            ),
            PolicyError::MissingPath(p) => trf!(
                "caminho não existe: {}",
                "path does not exist: {}",
                p.display()
            ),
            PolicyError::MissingEnv(v) => trf!(
                "a variável {v}, pedida em --pass-env, não existe no ambiente",
                "the variable {v}, requested in --pass-env, does not exist in the environment"
            ),
            PolicyError::UnrepresentableEnv(v) => trf!(
                "o valor de {v}, pedido em --pass-env, tem quebra de linha ou `${{` e não pode ser escrito no .env do hermes",
                "the value of {v}, requested in --pass-env, has a line break or `${{` and cannot be written to the hermes .env"
            ),
            PolicyError::Io(m) => return f.write_str(m),
        };
        f.write_str(&s)
    }
}

impl std::error::Error for PolicyError {}
