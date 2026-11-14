//! Lista de sessões guardada no diretório de dados: opções e estado, nunca
//! valores de variáveis nem saída.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use sandmux_proto::{LaunchOptions, SessionState};
use serde::{Deserialize, Serialize};

/// Versão do formato. Sem o campo, é o formato do change 2.
pub const VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: u32,
    pub name: String,
    /// Opções dadas no `run` e o perfil; nunca a política resolvida.
    pub opts: LaunchOptions,
    /// Nomes de `pass-env` do último lançamento.
    #[serde(default)]
    pub pass_env: Vec<String>,
    pub state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub version: u32,
    pub next_id: u32,
    pub sessions: Vec<SavedSession>,
}

impl Default for Saved {
    fn default() -> Self {
        Saved {
            version: VERSION,
            next_id: 1,
            sessions: vec![],
        }
    }
}

/// Converte as opções do formato do change 2: booleanos verdadeiros viram
/// `Some(true)`, falsos viram "não dado" (era o que significavam), e
/// `no_gpu` vira `gpu`.
fn upgrade_v1(v: &mut serde_json::Value) {
    let Some(sessions) = v.get_mut("sessions").and_then(|s| s.as_array_mut()) else {
        return;
    };
    for s in sessions {
        let names = s
            .pointer("/opts/pass_env")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        if let Some(o) = s.get_mut("opts").and_then(|o| o.as_object_mut()) {
            for k in ["repo", "kvm", "allow_clipboard"] {
                let on = o.get(k).and_then(|x| x.as_bool()) == Some(true);
                o.insert(
                    k.into(),
                    if on {
                        true.into()
                    } else {
                        serde_json::Value::Null
                    },
                );
            }
            let no_gpu = o.remove("no_gpu").and_then(|x| x.as_bool()) == Some(true);
            o.insert(
                "gpu".into(),
                if no_gpu {
                    false.into()
                } else {
                    serde_json::Value::Null
                },
            );
        }
        if let Some(o) = s.as_object_mut() {
            o.insert("pass_env".into(), names);
        }
    }
    if let Some(o) = v.as_object_mut() {
        o.insert("version".into(), VERSION.into());
    }
}

/// Lê a lista; sem arquivo, devolve uma lista vazia.
pub fn load(path: &Path) -> Result<Saved> {
    match std::fs::read(path) {
        Ok(b) => {
            let mut v: serde_json::Value =
                serde_json::from_slice(&b).with_context(|| format!("ler {}", path.display()))?;
            if v.get("version").is_none() {
                upgrade_v1(&mut v);
            }
            serde_json::from_value(v).with_context(|| format!("ler {}", path.display()))
        }
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
        opts.profile = Some("vm".into());
        let s = Saved {
            version: VERSION,
            next_id: 3,
            sessions: vec![SavedSession {
                id: 2,
                name: "opencode:proj".into(),
                opts,
                pass_env: vec!["OPENROUTER_API_KEY".into()],
                state: SessionState::Encerrada { codigo: 2 },
            }],
        };
        save(&p, &s).unwrap();
        save(&p, &s).unwrap();
        assert_eq!(load(&p).unwrap(), s);
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn le_a_lista_do_change_2() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sessions.json");
        let old = r#"{"next_id":4,"sessions":[
          {"id":1,"name":"shell:a","opts":{"harness":"shell","workdir":"/a","ro":[],"rw":["/x"],
            "pass_env":["K"],"repo":false,"kvm":true,"no_gpu":true,"allow_clipboard":false,"args":[]},
            "state":{"estado":"viva"}},
          {"id":3,"name":"opencode:b","opts":{"harness":"opencode","workdir":"/b","ro":[],"rw":[],
            "pass_env":[],"repo":true,"kvm":false,"no_gpu":false,"allow_clipboard":true,"args":["-c"]},
            "state":{"estado":"encerrada","codigo":2}}]}"#;
        std::fs::write(&p, old).unwrap();
        let s = load(&p).unwrap();
        assert_eq!(s.version, VERSION);
        assert_eq!(s.next_id, 4);
        let a = &s.sessions[0].opts;
        assert_eq!(
            (a.repo, a.kvm, a.gpu, a.allow_clipboard),
            (None, Some(true), Some(false), None)
        );
        assert_eq!(a.profile, None);
        assert_eq!(a.rw, vec![std::path::PathBuf::from("/x")]);
        assert_eq!(s.sessions[0].pass_env, vec!["K"]);
        let b = &s.sessions[1].opts;
        assert_eq!(
            (b.repo, b.kvm, b.gpu, b.allow_clipboard),
            (Some(true), None, None, Some(true))
        );
        assert_eq!(s.sessions[1].state, SessionState::Encerrada { codigo: 2 });
        // Regravada no formato novo, ida e volta.
        save(&p, &s).unwrap();
        assert_eq!(load(&p).unwrap(), s);
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("\"version\": 2")
        );
    }

    #[test]
    fn lista_guarda_o_nome_do_perfil_e_nao_as_chaves() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sessions.json");
        let mut opts = LaunchOptions::new("shell");
        opts.profile = Some("vm".into());
        let s = Saved {
            version: VERSION,
            next_id: 2,
            sessions: vec![SavedSession {
                id: 1,
                name: "shell:a".into(),
                opts,
                pass_env: vec![],
                state: SessionState::Viva,
            }],
        };
        save(&p, &s).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("\"profile\": \"vm\""), "{text}");
        assert!(
            !text.contains("clipboard\": true") && !text.contains("\"kvm\": true"),
            "{text}"
        );
    }
}
