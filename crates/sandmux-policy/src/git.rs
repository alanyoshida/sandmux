//! Detecção do repositório git da pasta de trabalho e montagens que protegem
//! os pontos do `.git` que o git do host executa ou obedece.

use std::path::{Path, PathBuf};

use sandmux_spec::Mount;

/// Como a pasta de trabalho se relaciona com um repositório.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitLayout {
    /// Sem repositório.
    None,
    /// A pasta de trabalho é a raiz de um repositório.
    Repo { git_dir: PathBuf },
    /// A pasta de trabalho é um worktree; `common_dir` é o `.git` principal.
    Worktree {
        git_file: PathBuf,
        common_dir: PathBuf,
    },
    /// O `.git` fica numa pasta acima da pasta de trabalho.
    Subfolder {
        repo_root: PathBuf,
        git_dir: PathBuf,
    },
}

pub fn detect(workdir: &Path) -> GitLayout {
    let dotgit = workdir.join(".git");
    if dotgit.is_dir() && !dotgit.is_symlink() {
        return GitLayout::Repo { git_dir: dotgit };
    }
    if dotgit.is_file() {
        if let Some(common) = worktree_common_dir(&dotgit) {
            return GitLayout::Worktree {
                git_file: dotgit,
                common_dir: common,
            };
        }
        return GitLayout::None;
    }
    let mut cur = workdir.parent();
    while let Some(dir) = cur {
        let g = dir.join(".git");
        if g.is_dir() || g.is_file() {
            return GitLayout::Subfolder {
                repo_root: dir.to_path_buf(),
                git_dir: g,
            };
        }
        cur = dir.parent();
    }
    GitLayout::None
}

/// Lê `gitdir: <caminho>` do arquivo `.git` de um worktree e, dali, o
/// `commondir` que aponta para o `.git` principal.
fn worktree_common_dir(git_file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git_file).ok()?;
    let rel = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    let base = git_file.parent()?;
    let gitdir = base.join(rel).canonicalize().ok()?;
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
    gitdir.join(common.trim()).canonicalize().ok()
}

/// Montagens que protegem um diretório git: o próprio diretório vira ponto de
/// montagem (não pode ser renomeado nem substituído), `hooks` e `config` ficam
/// em somente leitura, e o mesmo vale para cada submódulo em `modules`.
///
/// Devolve também as pastas `hooks` que faltam e precisam ser criadas antes.
pub fn protect(git_dir: &Path) -> (Vec<Mount>, Vec<PathBuf>) {
    let mut mounts = Vec::new();
    let mut create = Vec::new();
    let mut dirs = vec![git_dir.to_path_buf()];
    collect_modules(&git_dir.join("modules"), &mut dirs);
    for d in dirs {
        mounts.push(Mount::Bind {
            source: d.clone(),
            dest: d.clone(),
            writable: true,
        });
        let hooks = d.join("hooks");
        if !hooks.exists() {
            create.push(hooks.clone());
        }
        mounts.push(Mount::Bind {
            source: hooks.clone(),
            dest: hooks,
            writable: false,
        });
        for f in ["config", "config.worktree"] {
            let p = d.join(f);
            if p.is_file() {
                mounts.push(Mount::Bind {
                    source: p.clone(),
                    dest: p,
                    writable: false,
                });
            }
        }
    }
    (mounts, create)
}

/// Diretórios git de submódulos (com `HEAD` e `config`), recursivamente.
fn collect_modules(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() || p.is_symlink() {
            continue;
        }
        if p.join("HEAD").is_file() && p.join("config").is_file() {
            out.push(p.clone());
            collect_modules(&p.join("modules"), out);
        } else {
            // Submódulos com barra no nome ficam em pastas aninhadas.
            collect_modules(&p, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn detecta_repo_worktree_e_subpasta() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ],
        );
        assert_eq!(
            detect(&repo),
            GitLayout::Repo {
                git_dir: repo.join(".git")
            }
        );

        std::fs::create_dir(repo.join("sub")).unwrap();
        assert_eq!(
            detect(&repo.join("sub")),
            GitLayout::Subfolder {
                repo_root: repo.clone(),
                git_dir: repo.join(".git")
            }
        );

        git(&repo, &["worktree", "add", "-q", "../wt"]);
        match detect(&root.join("wt")) {
            GitLayout::Worktree { common_dir, .. } => assert_eq!(common_dir, repo.join(".git")),
            other => panic!("{other:?}"),
        }
        assert_eq!(detect(&root), GitLayout::None);
    }

    #[test]
    fn protege_hooks_config_e_submodulos() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path().join(".git");
        for p in ["hooks", "modules/a/hooks", "modules/b/c"] {
            std::fs::create_dir_all(g.join(p)).unwrap();
        }
        for p in [
            "config",
            "modules/a/config",
            "modules/a/HEAD",
            "modules/b/c/config",
            "modules/b/c/HEAD",
        ] {
            std::fs::write(g.join(p), "").unwrap();
        }
        let (mounts, create) = protect(&g);
        let ro: Vec<PathBuf> = mounts
            .iter()
            .filter_map(|m| match m {
                Mount::Bind {
                    dest,
                    writable: false,
                    ..
                } => Some(dest.clone()),
                _ => None,
            })
            .collect();
        for p in [
            "hooks",
            "config",
            "modules/a/hooks",
            "modules/a/config",
            "modules/b/c/config",
            "modules/b/c/hooks",
        ] {
            assert!(ro.contains(&g.join(p)), "{p} sem proteção");
        }
        assert_eq!(create, vec![g.join("modules/b/c/hooks")]);
    }
}
