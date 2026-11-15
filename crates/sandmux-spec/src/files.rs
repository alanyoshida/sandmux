//! Política de arquivos: padrões de caminho e de nome, e a decisão de uma
//! abertura pelo caminho na visão do agente.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Lista padrão de arquivos com cara de segredo (decisão do usuário,
/// 2026-11-14).
pub const DEFAULT_SECRETS: &[&str] = &[
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "*.kdbx",
    "id_rsa*",
    "id_ecdsa*",
    "id_ed25519*",
    ".npmrc",
    ".pypirc",
    ".netrc",
    "credentials*.json",
    "*.tfstate",
    ".git-credentials",
];

/// Exceções fixas da lista padrão.
pub const SECRET_EXCEPTIONS: &[&str] = &[".env.example", ".env.sample", ".env.template", "*.pub"];

/// Um padrão de arquivo: caminho absoluto (com `*`, `**` e `?`) ou nome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum FilePattern {
    /// Caminho na visão do agente, por componentes.
    Path(String),
    /// Nome de arquivo ou de pasta, em qualquer lugar.
    Name(String),
}

impl std::fmt::Display for FilePattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilePattern::Path(p) | FilePattern::Name(p) => f.write_str(p),
        }
    }
}

impl From<FilePattern> for String {
    fn from(p: FilePattern) -> String {
        p.to_string()
    }
}

impl TryFrom<String> for FilePattern {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        Self::parse(&s, None)
    }
}

impl FilePattern {
    /// Lê um padrão. `~/` vira `home` (sem `home`, é recusado).
    pub fn parse(s: &str, home: Option<&Path>) -> Result<FilePattern, String> {
        let s = s.trim();
        if s.is_empty() || s.contains('\0') {
            return Err(crate::tr!("padrão de arquivo vazio", "empty file pattern").into());
        }
        if let Some(rest) = s.strip_prefix("~/") {
            let h = home.ok_or(crate::tr!("`~/` sem HOME", "`~/` without HOME"))?;
            return Ok(FilePattern::Path(format!(
                "{}/{}",
                h.display().to_string().trim_end_matches('/'),
                rest.trim_end_matches('/')
            )));
        }
        if s.starts_with('/') {
            let t = s.trim_end_matches('/');
            return Ok(FilePattern::Path(if t.is_empty() {
                "/".into()
            } else {
                t.into()
            }));
        }
        if s.contains('/') {
            return Err(crate::trf!(
                "padrão de arquivo inválido: {s:?}; use um caminho que começa com `/` ou `~/`, ou um nome sem `/`, como `*.sql`",
                "invalid file pattern: {s:?}; use a path starting with `/` or `~/`, or a name without `/`, such as `*.sql`"
            ));
        }
        Ok(FilePattern::Name(s.into()))
    }

    /// O caminho casa (ele ou uma pasta acima dele)?
    pub fn matches(&self, path: &Path) -> bool {
        let comps: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        match self {
            FilePattern::Name(n) => comps.iter().any(|c| glob(n, c)),
            FilePattern::Path(p) => {
                let pat: Vec<&str> = p.split('/').filter(|c| !c.is_empty()).collect();
                // Pasta vale para o conteúdo: casa com algum prefixo.
                (0..=comps.len()).any(|k| path_glob(&pat, &comps[..k]))
            }
        }
    }
}

/// `*` e `?` dentro de um componente.
pub fn glob(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    fn go(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    go(&p, &s)
}

fn path_glob(p: &[&str], s: &[&str]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(&"**") => (0..=s.len()).any(|k| path_glob(&p[1..], &s[k..])),
        Some(c) => !s.is_empty() && glob(c, s[0]) && path_glob(&p[1..], &s[1..]),
    }
}

/// Política de arquivos compilada de um lançamento.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilePolicy {
    pub deny: Vec<FilePattern>,
    pub allow: Vec<FilePattern>,
    /// Padrões de segredo (a lista padrão mais `secret-files`).
    pub secrets: Vec<FilePattern>,
    /// Exceções aos segredos (as fixas mais `secret-files-off`).
    pub secret_exceptions: Vec<FilePattern>,
    /// Pasta de trabalho: só nela os segredos ficam escondidos.
    pub workdir: PathBuf,
}

/// Decisão sobre uma abertura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVerdict {
    Pass,
    /// Negado por `file-deny` (o padrão).
    Deny(String),
    /// Segredo escondido.
    Secret,
}

impl FilePolicy {
    /// Decide pelo caminho na visão do agente. `created` diz se o agente
    /// criou o arquivo.
    pub fn decide(&self, path: &Path, created: impl Fn(&Path) -> bool) -> FileVerdict {
        if self.allow.iter().any(|p| p.matches(path)) {
            return FileVerdict::Pass;
        }
        if let Some(p) = self.deny.iter().find(|p| p.matches(path)) {
            return FileVerdict::Deny(p.to_string());
        }
        if self.is_secret(path) && !created(path) {
            return FileVerdict::Secret;
        }
        FileVerdict::Pass
    }

    /// Um arquivo da pasta de trabalho com cara de segredo (pelo nome, ou
    /// por um padrão de caminho).
    pub fn is_secret(&self, path: &Path) -> bool {
        if self.workdir.as_os_str().is_empty() || !path.starts_with(&self.workdir) {
            return false;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let hit = |p: &FilePattern| match p {
            FilePattern::Name(n) => glob(n, name),
            FilePattern::Path(_) => p.matches(path),
        };
        self.secrets.iter().any(hit) && !self.secret_exceptions.iter().any(hit)
    }
}

/// Os padrões da lista padrão e das exceções fixas.
pub fn default_secrets() -> (Vec<FilePattern>, Vec<FilePattern>) {
    let p = |v: &[&str]| v.iter().map(|s| FilePattern::Name((*s).into())).collect();
    (p(DEFAULT_SECRETS), p(SECRET_EXCEPTIONS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(s: &str) -> FilePattern {
        FilePattern::parse(s, Some(Path::new("/home/ana"))).unwrap()
    }

    #[test]
    fn padroes() {
        assert!(pat("*.sql").matches(Path::new("/w/db/dados.sql")));
        assert!(pat("node_modules").matches(Path::new("/w/node_modules/x/y.js")));
        assert!(pat("/w/privado").matches(Path::new("/w/privado/a/b")));
        assert!(!pat("/w/privado").matches(Path::new("/w/privados/a")));
        assert!(pat("~/proj/*.sql").matches(Path::new("/home/ana/proj/a.sql")));
        assert!(!pat("~/proj/*.sql").matches(Path::new("/home/ana/proj/sub/a.sql")));
        assert!(pat("~/proj/**/*.sql").matches(Path::new("/home/ana/proj/sub/x/a.sql")));
        assert!(pat("/w/a?c").matches(Path::new("/w/abc")));
        assert!(FilePattern::parse("", None).is_err());
        assert!(FilePattern::parse("a/b", None).is_err());
        assert!(FilePattern::parse("~/x", None).is_err());
        let j = serde_json::to_string(&pat("*.sql")).unwrap();
        assert_eq!(
            serde_json::from_str::<FilePattern>(&j).unwrap(),
            pat("*.sql")
        );
    }

    #[test]
    fn decisao() {
        let (secrets, secret_exceptions) = default_secrets();
        let p = FilePolicy {
            deny: vec![pat("*.sql"), pat("/w/privado")],
            allow: vec![pat("/w/ok.sql"), pat("/w/.env.liberado")],
            secrets,
            secret_exceptions,
            workdir: "/w".into(),
        };
        let none = |_: &Path| false;
        assert_eq!(
            p.decide(Path::new("/w/a.sql"), none),
            FileVerdict::Deny("*.sql".into())
        );
        assert_eq!(p.decide(Path::new("/w/ok.sql"), none), FileVerdict::Pass);
        assert_eq!(
            p.decide(Path::new("/w/privado/x"), none),
            FileVerdict::Deny("/w/privado".into())
        );
        assert_eq!(p.decide(Path::new("/w/.env"), none), FileVerdict::Secret);
        assert_eq!(
            p.decide(Path::new("/w/sub/.env.prod"), none),
            FileVerdict::Secret
        );
        assert_eq!(
            p.decide(Path::new("/w/.env.example"), none),
            FileVerdict::Pass
        );
        assert_eq!(
            p.decide(Path::new("/w/.env.liberado"), none),
            FileVerdict::Pass
        );
        assert_eq!(
            p.decide(Path::new("/w/.ssh/id_ed25519.pub"), none),
            FileVerdict::Pass
        );
        assert_eq!(
            p.decide(Path::new("/w/k/id_ed25519"), none),
            FileVerdict::Secret
        );
        assert_eq!(p.decide(Path::new("/w/x.pem"), |_| true), FileVerdict::Pass);
        // Fora da pasta de trabalho, segredo não se esconde.
        assert_eq!(
            p.decide(Path::new("/home/ana/.env"), none),
            FileVerdict::Pass
        );
        assert_eq!(p.decide(Path::new("/w/README.md"), none), FileVerdict::Pass);
    }
}
