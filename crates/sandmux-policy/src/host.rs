//! Fatos do host que a política usa: usuário, HOME, pasta atual, ambiente.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::PolicyError;

/// O que a composição precisa saber do host.
#[derive(Debug, Clone)]
pub struct Host {
    /// HOME real, canônico.
    pub home: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub user: String,
    pub group: String,
    /// Shell do usuário (`$SHELL` ou o do passwd).
    pub user_shell: PathBuf,
    /// Diretório de dados do sandmux.
    pub data_dir: PathBuf,
    /// Pasta de configuração do sandmux (`$XDG_CONFIG_HOME/sandmux`).
    pub config_dir: PathBuf,
    /// Pasta de onde o comando foi executado, canônica.
    pub cwd: PathBuf,
    /// Ambiente do usuário.
    pub env: BTreeMap<String, String>,
    /// Raiz do sistema para consultas de existência (`/`; outro valor nos testes).
    pub sys_root: PathBuf,
}

impl Host {
    /// Lê os fatos do processo atual.
    pub fn detect() -> Result<Host, PolicyError> {
        let env: BTreeMap<String, String> = std::env::vars().collect();
        // SAFETY: consultas sem efeito colateral.
        let (uid, gid) = unsafe { (libc_getuid(), libc_getgid()) };
        let pw = passwd_entry(uid);
        let home_raw = env
            .get("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| pw.as_ref().map(|p| p.home.clone()))
            .ok_or_else(|| PolicyError::Io("HOME não definido".into()))?;
        let home = home_raw
            .canonicalize()
            .map_err(|e| PolicyError::Io(format!("HOME {}: {e}", home_raw.display())))?;
        let user = pw
            .as_ref()
            .map(|p| p.name.clone())
            .or_else(|| env.get("USER").cloned())
            .unwrap_or_else(|| format!("u{uid}"));
        let group = group_name(gid).unwrap_or_else(|| user.clone());
        let user_shell = env
            .get("SHELL")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| pw.as_ref().map(|p| p.shell.clone()))
            .unwrap_or_else(|| PathBuf::from("/bin/bash"));
        let data_dir = match env.get("XDG_DATA_HOME").map(PathBuf::from) {
            Some(p) if p.is_absolute() => p.join("sandmux"),
            _ => home.join(".local/share/sandmux"),
        };
        let config_dir = config_dir_from(&env, &home);
        let cwd = std::env::current_dir()
            .and_then(|p| p.canonicalize())
            .map_err(|e| PolicyError::Io(format!("pasta atual: {e}")))?;
        Ok(Host {
            home,
            uid,
            gid,
            user,
            group,
            user_shell,
            data_dir,
            config_dir,
            cwd,
            env,
            sys_root: PathBuf::from("/"),
        })
    }

    /// Caminho de sistema visto pela raiz de consulta.
    pub fn sys(&self, p: impl AsRef<Path>) -> PathBuf {
        let p = p.as_ref();
        self.sys_root.join(p.strip_prefix("/").unwrap_or(p))
    }

    /// Caminho relativo ao HOME real.
    pub fn h(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }
}

/// `$XDG_CONFIG_HOME/sandmux`, ou `~/.config/sandmux` quando a variável não
/// existe ou não é absoluta.
pub fn config_dir_from(env: &BTreeMap<String, String>, home: &Path) -> PathBuf {
    let p = match env.get("XDG_CONFIG_HOME").map(PathBuf::from) {
        Some(p) if p.is_absolute() => p.join("sandmux"),
        _ => home.join(".config/sandmux"),
    };
    // Canônica quando existe, para comparar com caminhos canônicos.
    p.canonicalize()
        .unwrap_or_else(|_| crate::config::normalize(&p))
}

struct Passwd {
    name: String,
    home: PathBuf,
    shell: PathBuf,
}

unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
    #[link_name = "getgid"]
    fn libc_getgid() -> u32;
}

/// Lê a entrada do usuário em `/etc/passwd` (sem NSS: suficiente para o
/// nome, o HOME e o shell de uma conta local).
fn passwd_entry(uid: u32) -> Option<Passwd> {
    let text = std::fs::read_to_string("/etc/passwd").ok()?;
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() >= 7 && f[2].parse::<u32>().ok() == Some(uid)).then(|| Passwd {
            name: f[0].to_string(),
            home: PathBuf::from(f[5]),
            shell: PathBuf::from(f[6]),
        })
    })
}

fn group_name(gid: u32) -> Option<String> {
    let text = std::fs::read_to_string("/etc/group").ok()?;
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.len() >= 3 && f[2].parse::<u32>().ok() == Some(gid)).then(|| f[0].to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasta_de_configuracao() {
        let home = Path::new("/home/ana");
        let mut env = BTreeMap::new();
        assert_eq!(
            config_dir_from(&env, home),
            PathBuf::from("/home/ana/.config/sandmux")
        );
        env.insert("XDG_CONFIG_HOME".into(), "/tmp/cfg".into());
        assert_eq!(
            config_dir_from(&env, home),
            PathBuf::from("/tmp/cfg/sandmux")
        );
        env.insert("XDG_CONFIG_HOME".into(), "/tmp/a/../cfg".into());
        assert_eq!(
            config_dir_from(&env, home),
            PathBuf::from("/tmp/cfg/sandmux")
        );
        env.insert("XDG_CONFIG_HOME".into(), "relativo".into());
        assert_eq!(
            config_dir_from(&env, home),
            PathBuf::from("/home/ana/.config/sandmux")
        );
    }
}
