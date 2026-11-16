//! O login do Google do Antigravity CLI (`agy`): o arquivo do token, com os
//! tokens reais trocados pelos falsos do cofre (`agy:access` e
//! `agy:refresh`). Usado pela importação do login do host e pelo vigia de
//! chaves reais do daemon, para um login real gravado no HOME sintético.

use std::path::Path;

use serde_json::Value;

/// O arquivo do token, relativo ao HOME.
pub const TOKEN_FILE: &str = ".gemini/antigravity-cli/antigravity-oauth-token";

/// Os prefixos dos falsos do cofre.
const FAKE_PREFIXES: &[&str] = &["ya29.sandmux", "1//sandmux"];

/// O valor é um falso do cofre?
pub fn is_fake(v: &str) -> bool {
    FAKE_PREFIXES.iter().any(|p| v.starts_with(p))
}

/// O arquivo do token lido. Sem `Debug`: tem os valores reais.
pub struct Login {
    /// O texto lido, para conferir que o arquivo não mudou.
    pub original: Vec<u8>,
    value: Value,
}

/// Os campos dos tokens no JSON e o papel de cada um no cofre.
const FIELDS: &[(&str, &str)] = &[("access_token", "access"), ("refresh_token", "refresh")];

impl Login {
    /// Lê o arquivo; `None` sem arquivo, fora do formato ou sem nenhum token.
    pub fn read(path: &Path) -> Option<Login> {
        let original = std::fs::read(path).ok()?;
        let value: Value = serde_json::from_slice(&original).ok()?;
        let l = Login { original, value };
        (!l.tokens().is_empty()).then_some(l)
    }

    /// Os tokens do arquivo (papel e valor), falsos ou não.
    pub fn tokens(&self) -> Vec<(String, String)> {
        FIELDS
            .iter()
            .filter_map(|(field, role)| {
                self.value
                    .get("token")?
                    .get(*field)?
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| (role.to_string(), s.to_string()))
            })
            .collect()
    }

    /// Os tokens reais (papel e valor): os que não são falsos do cofre.
    pub fn reals(&self) -> Vec<(String, String)> {
        self.tokens()
            .into_iter()
            .filter(|(_, v)| !is_fake(v))
            .collect()
    }

    /// O JSON com cada token do papel trocado pelo falso dele; os demais
    /// campos ficam como estão.
    pub fn with_fakes(&self, fakes: &[(String, String)]) -> Vec<u8> {
        let mut v = self.value.clone();
        if let Some(t) = v.get_mut("token").and_then(Value::as_object_mut) {
            for (field, role) in FIELDS {
                if let Some((_, fake)) = fakes.iter().find(|(r, _)| r == role)
                    && t.contains_key(*field)
                {
                    t.insert(field.to_string(), Value::String(fake.clone()));
                }
            }
        }
        serde_json::to_vec(&v).unwrap_or_default()
    }
}

/// Grava o arquivo do token com os falsos, só para o dono.
pub fn write(path: &Path, data: &[u8]) -> Result<(), crate::ImportError> {
    crate::write_private(path, data, 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = r#"{"token":{"access_token":"ya29.a0REAL","token_type":"Bearer","refresh_token":"1//0gREAL","expiry":"2026-11-16T10:00:00Z","expires_in":3599},"auth_method":"google","id_token_email":"ana@exemplo.com"}"#;

    #[test]
    fn troca_so_os_tokens() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t");
        std::fs::write(&p, HOST).unwrap();
        let l = Login::read(&p).unwrap();
        assert_eq!(l.reals().len(), 2);
        let out = l.with_fakes(&[
            ("access".into(), "ya29.sandmuxA".into()),
            ("refresh".into(), "1//sandmuxR".into()),
        ]);
        let s = String::from_utf8(out).unwrap();
        assert!(!s.contains("REAL"), "{s}");
        assert!(
            s.contains("ya29.sandmuxA") && s.contains("1//sandmuxR"),
            "{s}"
        );
        assert!(s.contains("ana@exemplo.com") && s.contains("2026-11-16T10:00:00Z"));
        std::fs::write(&p, &s).unwrap();
        let l = Login::read(&p).unwrap();
        assert!(l.reals().is_empty());
        assert_eq!(l.tokens().len(), 2);
    }

    #[test]
    fn fora_do_formato() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t");
        assert!(Login::read(&p).is_none());
        std::fs::write(&p, "{}").unwrap();
        assert!(Login::read(&p).is_none());
        std::fs::write(&p, "não é json").unwrap();
        assert!(Login::read(&p).is_none());
    }
}
