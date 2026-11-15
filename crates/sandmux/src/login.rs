//! O login do usuário e o linger, pelo systemd-logind: o daemon que sobe no
//! boot espera o primeiro login para relançar as abas.

use std::path::PathBuf;

/// O arquivo do estado do usuário no logind (`SANDMUX_LOGIND_USER` nos
/// testes).
fn user_file() -> PathBuf {
    match std::env::var_os("SANDMUX_LOGIND_USER") {
        Some(p) => PathBuf::from(p),
        // SAFETY: getuid não falha.
        None => PathBuf::from(format!("/run/systemd/users/{}", unsafe { libc::getuid() })),
    }
}

/// O usuário tem uma sessão de login (`active` ou `online`). Sem o logind,
/// conta como logado.
pub fn logged_in() -> bool {
    let Ok(text) = std::fs::read_to_string(user_file()) else {
        return true;
    };
    text.lines()
        .find_map(|l| l.strip_prefix("STATE="))
        .is_none_or(|s| matches!(s.trim(), "active" | "online"))
}

/// A pasta dos marcadores de linger (`SANDMUX_LINGER_DIR` nos testes).
fn linger_dir() -> PathBuf {
    std::env::var_os("SANDMUX_LINGER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/systemd/linger"))
}

/// O linger do usuário está ligado: o gerenciador do usuário sobe no boot.
pub fn linger_on(user: &str) -> bool {
    linger_dir().join(user).exists()
}

/// O comando do logind (`SANDMUX_LOGINCTL` nos testes).
pub fn loginctl() -> String {
    std::env::var("SANDMUX_LOGINCTL").unwrap_or_else(|_| "loginctl".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn estado_do_login_pelo_arquivo() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("1000");
        let with = |text: &str| {
            std::fs::write(&f, text).unwrap();
            // SAFETY: teste de uma thread só para esta variável.
            unsafe { std::env::set_var("SANDMUX_LOGIND_USER", &f) };
            super::logged_in()
        };
        assert!(!with("NAME=ana\nSTATE=lingering\n"));
        assert!(with("STATE=active\nSESSIONS=2\n"));
        assert!(with("STATE=online\n"));
        assert!(with("NAME=ana\n"));
        unsafe { std::env::set_var("SANDMUX_LOGIND_USER", d.path().join("nada")) };
        assert!(super::logged_in());
    }
}
