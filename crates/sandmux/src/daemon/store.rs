//! Lista de sessões guardada no diretório de dados: opções e estado, nunca
//! valores de variáveis nem saída.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use sandmux_proto::{LaunchOptions, SessionState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: u32,
    pub name: String,
    pub opts: LaunchOptions,
    pub state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub next_id: u32,
    pub sessions: Vec<SavedSession>,
}

impl Default for Saved {
    fn default() -> Self {
        Saved {
            next_id: 1,
            sessions: vec![],
        }
    }
}

/// Lê a lista; sem arquivo, devolve uma lista vazia.
pub fn load(path: &Path) -> Result<Saved> {
    match std::fs::read(path) {
        Ok(b) => serde_json::from_slice(&b).with_context(|| format!("ler {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
        Err(e) => Err(e).with_context(|| format!("ler {}", path.display())),
    }
}

/// Grava de forma atômica, com modo 600.
pub fn save(path: &Path, saved: &Saved) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| format!("gravar {}", tmp.display()))?;
    f.write_all(&serde_json::to_vec_pretty(saved)?)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path).with_context(|| format!("gravar {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn ida_e_volta_com_modo_600() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sessions.json");
        assert_eq!(load(&p).unwrap(), Saved::default());
        let mut opts = LaunchOptions::new("opencode");
        opts.pass_env = vec!["OPENROUTER_API_KEY".into()];
        let s = Saved {
            next_id: 3,
            sessions: vec![SavedSession {
                id: 2,
                name: "opencode:proj".into(),
                opts,
                state: SessionState::Encerrada { codigo: 2 },
            }],
        };
        save(&p, &s).unwrap();
        save(&p, &s).unwrap();
        assert_eq!(load(&p).unwrap(), s);
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
