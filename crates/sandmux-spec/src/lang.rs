//! Idioma da interface: português (o padrão) ou inglês.
//!
//! Cada texto ao usuário fica no código nos dois idiomas, lado a lado:
//! `tr!("Proteção", "Protection")` devolve um deles, e
//! `trf!("aba {id}", "tab {id}")` faz o `format!` de um deles.

use std::sync::atomic::{AtomicU8, Ordering};

/// Um idioma da interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Pt,
    En,
}

impl Lang {
    /// `"pt"` ou `"en"`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Pt => "pt",
            Lang::En => "en",
        }
    }

    /// Lê `"pt"` ou `"en"`.
    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim() {
            "pt" => Some(Lang::Pt),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// Variável que passa por cima da configuração e do sistema.
pub const ENV: &str = "SANDMUX_LANG";

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// O idioma atual (português até alguém escolher outro).
pub fn get() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Pt
    }
}

pub fn set(l: Lang) {
    CURRENT.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

/// O idioma está em inglês?
pub fn is_en() -> bool {
    get() == Lang::En
}

/// O idioma do sistema pelos valores de `LC_ALL`, `LC_MESSAGES` e `LANG`:
/// o primeiro não vazio decide; `pt*` é português, o resto (inclusive `C` e
/// nenhum) é inglês.
pub fn from_locale(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Lang {
    let v = [lc_all, lc_messages, lang]
        .into_iter()
        .flatten()
        .find(|s| !s.is_empty())
        .unwrap_or("");
    if v.starts_with("pt") {
        Lang::Pt
    } else {
        Lang::En
    }
}

/// Resolve o idioma: `SANDMUX_LANG`, depois o da configuração, depois o do
/// sistema.
pub fn resolve(env_override: Option<&str>, configured: Option<Lang>, system: Lang) -> Lang {
    env_override
        .and_then(Lang::parse)
        .or(configured)
        .unwrap_or(system)
}

/// O idioma do ambiente do processo (`SANDMUX_LANG` e o sistema), com o da
/// configuração no meio.
pub fn resolve_env(configured: Option<Lang>) -> Lang {
    let var = |k: &str| std::env::var(k).ok();
    resolve(
        var(ENV).as_deref(),
        configured,
        from_locale(
            var("LC_ALL").as_deref(),
            var("LC_MESSAGES").as_deref(),
            var("LANG").as_deref(),
        ),
    )
}

/// Um texto nos dois idiomas: `tr!("português", "english")`.
#[macro_export]
macro_rules! tr {
    ($pt:expr, $en:expr $(,)?) => {
        if $crate::lang::is_en() { $en } else { $pt }
    };
}

/// `format!` nos dois idiomas, com os mesmos argumentos:
/// `trf!("aba {id}", "tab {id}")` ou `trf!("{} de {}", "{} of {}", a, b)`.
#[macro_export]
macro_rules! trf {
    ($pt:literal, $en:literal $(, $a:expr)* $(,)?) => {
        if $crate::lang::is_en() {
            format!($en $(, $a)*)
        } else {
            format!($pt $(, $a)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sistema_configuracao_e_variavel() {
        assert_eq!(from_locale(None, None, Some("pt_BR.UTF-8")), Lang::Pt);
        assert_eq!(from_locale(None, None, Some("en_US.UTF-8")), Lang::En);
        assert_eq!(from_locale(None, None, Some("C")), Lang::En);
        assert_eq!(from_locale(None, None, None), Lang::En);
        assert_eq!(
            from_locale(Some(""), Some("pt_PT"), Some("en_US")),
            Lang::Pt
        );
        assert_eq!(from_locale(Some("en_GB"), None, Some("pt_BR")), Lang::En);
        assert_eq!(resolve(None, Some(Lang::Pt), Lang::En), Lang::Pt);
        assert_eq!(resolve(Some("en"), Some(Lang::Pt), Lang::Pt), Lang::En);
        assert_eq!(resolve(Some("xx"), None, Lang::Pt), Lang::Pt);
    }

    #[test]
    fn macros() {
        set(Lang::Pt);
        let n = 2;
        assert_eq!(tr!("Proteção", "Protection"), "Proteção");
        assert_eq!(trf!("{n} abas", "{n} tabs"), "2 abas");
        set(Lang::En);
        assert_eq!(tr!("Proteção", "Protection"), "Protection");
        assert_eq!(trf!("{} de {}", "{} of {}", 1, n), "1 of 2");
        set(Lang::Pt);
    }
}
