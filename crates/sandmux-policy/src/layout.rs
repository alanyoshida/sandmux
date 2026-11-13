//! Organização do diretório de dados do sandmux.
//!
//! ```text
//! <dados>/
//!   homes/<harness>/              HOME sintético
//!   layers/<harness>/<chave>/     camada de escrita: upper/ e work/
//!   layers/<harness>.lock         trava da camada
//!   fallback/<harness>/<chave>/   caches próprios no modo de recuo
//!   inbox/                        pasta de entrada
//!   manifests/<harness>.json      segredos trocados pela importação
//! ```

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn home(&self, harness: &str) -> PathBuf {
        self.root.join("homes").join(harness)
    }

    pub fn layer(&self, harness: &str, key: &str) -> (PathBuf, PathBuf) {
        let d = self.root.join("layers").join(harness).join(key);
        (d.join("upper"), d.join("work"))
    }

    pub fn layer_lock(&self, harness: &str) -> PathBuf {
        self.root.join("layers").join(format!("{harness}.lock"))
    }

    pub fn fallback_cache(&self, harness: &str, key: &str) -> PathBuf {
        self.root.join("fallback").join(harness).join(key)
    }

    pub fn inbox(&self) -> PathBuf {
        self.root.join("inbox")
    }

    pub fn manifest(&self, harness: &str) -> PathBuf {
        self.root.join("manifests").join(format!("{harness}.json"))
    }
}

/// Chave de uma pasta na camada: o caminho relativo ao HOME (ou absoluto),
/// com as barras trocadas.
pub fn layer_key(home: &Path, path: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => format!("home__{}", rel.to_string_lossy().replace('/', "__")),
        Err(_) => format!(
            "abs__{}",
            path.strip_prefix("/")
                .unwrap_or(path)
                .to_string_lossy()
                .replace('/', "__")
        ),
    }
}
