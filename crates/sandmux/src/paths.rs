//! Caminhos do daemon: socket, trava, lista de sessões e log.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

fn uid() -> u32 {
    // SAFETY: getuid não falha.
    unsafe { libc::getuid() }
}

fn env_abs(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// Pasta do socket: `$XDG_RUNTIME_DIR/sandmux`, ou `/tmp/sandmux-<uid>`.
pub fn runtime_dir() -> PathBuf {
    match env_abs("XDG_RUNTIME_DIR") {
        Some(d) => d.join("sandmux"),
        None => PathBuf::from(format!("/tmp/sandmux-{}", uid())),
    }
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("sandmuxd.sock")
}

pub fn lock_path() -> PathBuf {
    runtime_dir().join("sandmuxd.lock")
}

/// Pasta de estado: `$XDG_STATE_HOME/sandmux`, ou `~/.local/state/sandmux`.
pub fn state_dir() -> PathBuf {
    match env_abs("XDG_STATE_HOME") {
        Some(d) => d.join("sandmux"),
        None => env_abs("HOME")
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".local/state/sandmux"),
    }
}

/// Lista de sessões, no diretório de dados.
pub fn sessions_file(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions.json")
}

/// Confere uma pasta privada já existente: diretório de verdade (não link),
/// do dono `owner`, sem permissão para grupo e outros.
pub fn check_private_dir(p: &Path, owner: u32) -> Result<()> {
    let m = std::fs::symlink_metadata(p)?;
    if m.file_type().is_symlink() {
        bail!("{} é um link simbólico", p.display());
    }
    if !m.is_dir() {
        bail!("{} não é uma pasta", p.display());
    }
    if m.uid() != owner {
        bail!("{} pertence a outro usuário (UID {})", p.display(), m.uid());
    }
    if m.mode() & 0o077 != 0 {
        bail!(
            "{} tem permissão {:o}; a pasta do daemon precisa ser 700",
            p.display(),
            m.mode() & 0o777
        );
    }
    Ok(())
}

/// Cria a pasta do socket com 700 ou confere a que já existe.
pub fn ensure_runtime_dir(p: &Path) -> Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(p)?;
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))?;
        }
        Err(e) => return Err(e.into()),
    }
    check_private_dir(p, uid()).map_err(|e| anyhow::anyhow!("pasta do socket recusada: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cria_com_700() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sandmux");
        ensure_runtime_dir(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o700);
        // Chamar de novo aceita a pasta existente.
        ensure_runtime_dir(&p).unwrap();
    }

    #[test]
    fn recusa_pasta_aberta() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sandmux");
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let err = ensure_runtime_dir(&p).unwrap_err().to_string();
        assert!(err.contains("755"), "{err}");
    }

    #[test]
    fn recusa_link_e_arquivo() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(
            ensure_runtime_dir(&link)
                .unwrap_err()
                .to_string()
                .contains("link")
        );
        let file = d.path().join("arquivo");
        std::fs::write(&file, "").unwrap();
        assert!(ensure_runtime_dir(&file).is_err());
    }

    #[test]
    fn recusa_outro_dono() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sandmux");
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        let err = check_private_dir(&p, uid() + 1).unwrap_err().to_string();
        assert!(err.contains("outro usuário"), "{err}");
    }
}
