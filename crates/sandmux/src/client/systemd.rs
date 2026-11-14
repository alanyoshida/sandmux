//! Unidade systemd de usuário do daemon.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub const UNIT: &str = "sandmuxd.service";

/// Pasta das unidades de usuário: `$XDG_CONFIG_HOME/systemd/user` ou
/// `~/.config/systemd/user`.
pub fn unit_dir() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".config")
        });
    config.join("systemd/user")
}

pub fn unit_path() -> PathBuf {
    unit_dir().join(UNIT)
}

/// `XDG_DATA_HOME` e `XDG_CONFIG_HOME` do cliente, quando definidos.
#[derive(Debug, Clone, Copy, Default)]
pub struct Homes<'a> {
    pub data: Option<&'a Path>,
    pub config: Option<&'a Path>,
}

/// Texto da unidade.
pub fn unit_text(exec: &Path, homes: Homes<'_>) -> String {
    let mut env = String::from("Environment=SANDMUX_START_MODE=systemd\n");
    if let Some(d) = homes.data {
        env.push_str(&format!("Environment=XDG_DATA_HOME={}\n", d.display()));
    }
    if let Some(c) = homes.config {
        env.push_str(&format!("Environment=XDG_CONFIG_HOME={}\n", c.display()));
    }
    format!(
        "# Gerado pelo sandmux. `sandmux daemon disable` remove esta unidade.\n\
         [Unit]\n\
         Description=sandmux: daemon de sessões de agentes confinados\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         {env}\
         Restart=on-failure\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exec.display()
    )
}

/// O systemd de usuário responde?
pub fn available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn systemctl(args: &[&str]) -> Result<()> {
    let out = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("executar systemctl")?;
    if !out.status.success() {
        bail!(
            "systemctl --user {} falhou: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Grava a unidade quando ela não existe ou mudou. Devolve se gravou.
pub fn write_unit(exec: &Path, homes: Homes<'_>) -> Result<bool> {
    write_unit_in(&unit_dir(), exec, homes)
}

/// Como [`write_unit`], numa pasta dada.
pub fn write_unit_in(dir: &Path, exec: &Path, homes: Homes<'_>) -> Result<bool> {
    let path = dir.join(UNIT);
    let text = unit_text(exec, homes);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, text).with_context(|| format!("gravar {}", path.display()))?;
    Ok(true)
}

/// Instala (se preciso), habilita no login e inicia o daemon.
pub fn install_and_start(exec: &Path, homes: Homes<'_>) -> Result<()> {
    let existed = unit_path().exists();
    let written = write_unit(exec, homes)?;
    if written {
        if existed {
            eprintln!(
                "sandmux: atualizei a unidade systemd de usuário {} (o binário ou as pastas do sandmux mudaram)",
                unit_path().display()
            );
        } else {
            eprintln!(
                "sandmux: instalei a unidade systemd de usuário {}, habilitada no login, para as sessões sobreviverem ao terminal e voltarem depois de um reboot. `sandmux daemon disable` remove.",
                unit_path().display()
            );
        }
        systemctl(&["daemon-reload"])?;
    }
    systemctl(&["enable", "--now", UNIT])
}

/// Para, desabilita e remove a unidade. `call_systemctl` falso só remove o
/// arquivo (testes e máquinas sem systemd).
pub fn disable(call_systemctl: bool) -> Result<bool> {
    let path = unit_path();
    if call_systemctl && path.exists() {
        systemctl(&["disable", "--now", UNIT])?;
    }
    let removed = match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e).with_context(|| format!("remover {}", path.display())),
    };
    if call_systemctl && removed {
        systemctl(&["daemon-reload"])?;
    }
    Ok(removed)
}

/// A unidade está habilitada?
pub fn enabled() -> Option<bool> {
    let out = Command::new("systemctl")
        .args(["--user", "is-enabled", UNIT])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim() == "enabled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grava_so_quando_muda() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("systemd/user");
        let a = Path::new("/a/sandmuxd");
        assert!(write_unit_in(&dir, a, Homes::default()).unwrap());
        assert!(!write_unit_in(&dir, a, Homes::default()).unwrap());
        assert!(write_unit_in(&dir, Path::new("/b/sandmuxd"), Homes::default()).unwrap());
        let text = std::fs::read_to_string(dir.join(UNIT)).unwrap();
        assert!(text.contains("ExecStart=/b/sandmuxd\n"));
    }

    #[test]
    fn texto_da_unidade() {
        let t = unit_text(
            Path::new("/opt/sandmux/sandmuxd"),
            Homes {
                data: Some(Path::new("/dados")),
                config: Some(Path::new("/home/ana/cfg")),
            },
        );
        assert!(t.contains("Environment=XDG_CONFIG_HOME=/home/ana/cfg\n"));
        assert!(t.contains("ExecStart=/opt/sandmux/sandmuxd\n"));
        assert!(t.contains("Restart=on-failure\n"));
        assert!(t.contains("WantedBy=default.target\n"));
        assert!(t.contains("Environment=XDG_DATA_HOME=/dados\n"));
        assert!(t.contains("Environment=SANDMUX_START_MODE=systemd\n"));
        let t = unit_text(Path::new("/x/sandmuxd"), Homes::default());
        assert!(!t.contains("XDG_DATA_HOME") && !t.contains("XDG_CONFIG_HOME"));
    }
}
