//! Biblioteca compartilhada pelo cliente `sandmux` e pelo daemon `sandmuxd`.

pub mod chat;

/// Define o idioma do processo: `SANDMUX_LANG`, depois `language` do
/// `config.toml`, depois o sistema.
pub fn init_language() -> sandmux_spec::lang::Lang {
    match sandmux_policy::Host::detect() {
        Ok(h) => sandmux_policy::config::init_language(&sandmux_policy::config::path(&h)),
        Err(_) => {
            let l = sandmux_spec::lang::resolve_env(None);
            sandmux_spec::lang::set(l);
            l
        }
    }
}
pub mod client;
pub mod daemon;
pub mod discover;
pub mod docker;
pub mod keyring;
pub mod launch;
pub mod paths;
pub mod secretdomains;
pub mod sshagent;
