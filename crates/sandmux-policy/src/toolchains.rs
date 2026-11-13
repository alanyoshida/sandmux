//! Toolchains do HOME real que entram no sandbox, por lista conhecida.

use std::path::{Path, PathBuf};

use crate::host::Host;

/// Uma toolchain detectada no host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    pub name: &'static str,
    /// Pastas que recebem a camada de escrita.
    pub dirs: Vec<PathBuf>,
    /// Arquivos avulsos, em somente leitura.
    pub files: Vec<PathBuf>,
    /// Pastas acrescentadas ao `PATH`, em ordem.
    pub path: Vec<PathBuf>,
    /// Arquivos ou pastas de credencial cobertos, quando existem.
    pub covers: Vec<PathBuf>,
}

/// Detecta as toolchains da lista que estão instaladas. As ausentes são
/// ignoradas.
pub fn detect(host: &Host) -> Vec<Toolchain> {
    let h = |rel: &str| host.h(rel);
    let mut out = Vec::new();

    if h(".nvm").is_dir() {
        let mut path = Vec::new();
        if let Some(bin) = nvm_default_bin(&h(".nvm")) {
            path.push(bin);
        }
        out.push(Toolchain {
            name: "nvm",
            dirs: vec![h(".nvm")],
            files: vec![],
            path,
            covers: vec![],
        });
    }
    if h(".bun").is_dir() {
        out.push(Toolchain {
            name: "bun",
            dirs: vec![h(".bun")],
            files: vec![],
            path: vec![h(".bun/bin")],
            covers: vec![],
        });
    }
    if h(".cargo").is_dir() {
        out.push(Toolchain {
            name: "cargo",
            dirs: vec![h(".cargo")],
            files: vec![],
            path: vec![h(".cargo/bin")],
            covers: vec![h(".cargo/credentials.toml"), h(".cargo/credentials")],
        });
    }
    if h(".rustup").is_dir() {
        out.push(Toolchain {
            name: "rustup",
            dirs: vec![h(".rustup")],
            files: vec![],
            path: vec![],
            covers: vec![],
        });
    }
    let uv_files: Vec<PathBuf> = ["uv", "uvx"]
        .iter()
        .map(|f| h(&format!(".local/bin/{f}")))
        .filter(|p| p.is_file())
        .collect();
    let uv_data = h(".local/share/uv");
    if !uv_files.is_empty() || uv_data.is_dir() {
        out.push(Toolchain {
            name: "uv",
            dirs: if uv_data.is_dir() {
                vec![uv_data]
            } else {
                vec![]
            },
            files: uv_files,
            path: vec![],
            covers: vec![h(".local/share/uv/credentials")],
        });
    }
    if h(".pyenv").is_dir() {
        out.push(Toolchain {
            name: "pyenv",
            dirs: vec![h(".pyenv")],
            files: vec![],
            path: vec![h(".pyenv/shims"), h(".pyenv/bin")],
            covers: vec![],
        });
    }
    if h("go").is_dir() {
        let mut path = vec![h("go/bin")];
        if host.sys("/usr/local/go/bin").is_dir() {
            path.push(PathBuf::from("/usr/local/go/bin"));
        }
        out.push(Toolchain {
            name: "go",
            dirs: vec![h("go")],
            files: vec![],
            path,
            covers: vec![],
        });
    }
    for brew in [host.sys("/home/linuxbrew/.linuxbrew"), h(".linuxbrew")] {
        if brew.is_dir() {
            let real = if brew.starts_with(&host.sys_root) && host.sys_root != Path::new("/") {
                Path::new("/").join(brew.strip_prefix(&host.sys_root).unwrap())
            } else {
                brew.clone()
            };
            out.push(Toolchain {
                name: "linuxbrew",
                dirs: vec![real.clone()],
                files: vec![],
                path: vec![real.join("bin"), real.join("sbin")],
                covers: vec![],
            });
            break;
        }
    }
    out
}

/// Pasta `bin` da versão padrão do Node no nvm: segue os aliases (`default`,
/// `lts/*`, `lts/<nome>`) até uma versão instalada; sem alias resolvível, usa
/// a maior versão instalada.
pub fn nvm_default_bin(nvm: &Path) -> Option<PathBuf> {
    let versions = nvm.join("versions/node");
    let mut installed: Vec<(Vec<u64>, String)> = std::fs::read_dir(&versions)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let nums = parse_version(&name)?;
            Some((nums, name))
        })
        .collect();
    installed.sort();
    let resolve = |want: &str| -> Option<String> {
        let w = want.trim().trim_start_matches('v');
        let prefix: Vec<u64> = w.split('.').filter_map(|p| p.parse().ok()).collect();
        if prefix.is_empty() {
            return None;
        }
        installed
            .iter()
            .rev()
            .find(|(nums, _)| nums.starts_with(&prefix))
            .map(|(_, n)| n.clone())
    };
    let mut alias = std::fs::read_to_string(nvm.join("alias/default")).ok();
    for _ in 0..5 {
        let Some(a) = alias.take() else { break };
        let a = a.trim().to_string();
        if let Some(v) = resolve(&a) {
            return Some(versions.join(v).join("bin"));
        }
        let next = if a == "lts/*" || a == "node" || a == "stable" {
            if a == "lts/*" {
                std::fs::read_to_string(nvm.join("alias/lts/*")).ok()
            } else {
                None
            }
        } else {
            std::fs::read_to_string(nvm.join("alias").join(&a)).ok()
        };
        alias = next;
    }
    installed.last().map(|(_, n)| versions.join(n).join("bin"))
}

fn parse_version(name: &str) -> Option<Vec<u64>> {
    let v = name.strip_prefix('v')?;
    let nums: Option<Vec<u64>> = v.split('.').map(|p| p.parse().ok()).collect();
    nums.filter(|n| n.len() == 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nvm_fixture(alias: Option<&str>) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for v in ["v18.20.1", "v22.3.0", "v24.21.0"] {
            std::fs::create_dir_all(d.path().join("versions/node").join(v).join("bin")).unwrap();
        }
        std::fs::create_dir_all(d.path().join("alias/lts")).unwrap();
        if let Some(a) = alias {
            std::fs::write(d.path().join("alias/default"), a).unwrap();
        }
        d
    }

    #[test]
    fn nvm_versao_explicita() {
        let d = nvm_fixture(Some("22"));
        assert!(nvm_default_bin(d.path()).unwrap().ends_with("v22.3.0/bin"));
    }

    #[test]
    fn nvm_lts_resolve_pelo_alias() {
        let d = nvm_fixture(Some("lts/*"));
        std::fs::write(d.path().join("alias/lts/*"), "lts/iron").unwrap();
        std::fs::write(d.path().join("alias/lts/iron"), "v18.20.1").unwrap();
        assert!(nvm_default_bin(d.path()).unwrap().ends_with("v18.20.1/bin"));
    }

    #[test]
    fn nvm_sem_alias_usa_a_maior() {
        let d = nvm_fixture(None);
        assert!(nvm_default_bin(d.path()).unwrap().ends_with("v24.21.0/bin"));
    }
}
