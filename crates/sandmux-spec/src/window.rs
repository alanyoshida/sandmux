//! Acesso do sandbox às janelas e ao áudio do desktop (`window`).

use serde::{Deserialize, Serialize};

/// Modo da opção `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    /// Sem janela nem áudio.
    #[default]
    Off,
    /// Socket próprio da aba, registrado no compositor pelo
    /// `wp_security_context_v1`.
    On,
    /// Socket comum do compositor.
    Direct,
}

impl WindowMode {
    pub const ALL: [WindowMode; 3] = [WindowMode::Off, WindowMode::On, WindowMode::Direct];

    pub fn is_on(self) -> bool {
        self != WindowMode::Off
    }

    /// Lê o valor escrito na configuração ou no `config set`.
    pub fn parse(s: &str) -> Result<WindowMode, String> {
        match s {
            "false" => Ok(WindowMode::Off),
            "true" => Ok(WindowMode::On),
            "direct" => Ok(WindowMode::Direct),
            _ => Err(crate::trf!(
                "`window` precisa ser true, false ou \"direct\": {s}",
                "`window` must be true, false or \"direct\": {s}"
            )),
        }
    }

    /// O valor no formato do TOML.
    pub fn toml(self) -> &'static str {
        match self {
            WindowMode::Off => "false",
            WindowMode::On => "true",
            WindowMode::Direct => "\"direct\"",
        }
    }

    /// Descrição curta, para listagens.
    pub fn label(self) -> &'static str {
        match self {
            WindowMode::Off => crate::tr!("desligada", "off"),
            WindowMode::On => crate::tr!(
                "ligada, pelo security-context do compositor",
                "on, through the compositor's security-context"
            ),
            WindowMode::Direct => crate::tr!(
                "ligada, pelo socket direto do compositor",
                "on, through the compositor's direct socket"
            ),
        }
    }
}

impl std::fmt::Display for WindowMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WindowMode::Off => "false",
            WindowMode::On => "true",
            WindowMode::Direct => "direct",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        for m in WindowMode::ALL {
            assert_eq!(WindowMode::parse(&m.to_string()), Ok(m));
        }
        assert!(WindowMode::parse("sim").is_err());
    }
}
