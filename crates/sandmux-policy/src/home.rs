//! Criação do HOME sintético de um harness, fora do sandbox.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::PolicyError;
use crate::host::Host;
use crate::layout::Layout;
use crate::presets;

/// Resultado da criação.
#[derive(Debug, Default)]
pub struct HomeSetup {
    pub created: bool,
    pub warnings: Vec<String>,
}

fn io(e: std::io::Error, what: &Path) -> PolicyError {
    PolicyError::Io(format!("{}: {e}", what.display()))
}

/// Cria uma pasta com permissão restrita ao usuário, com os pais.
pub fn private_dir(p: &Path) -> Result<(), PolicyError> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
        .map_err(|e| io(e, p))
}

/// Garante o HOME sintético do harness. Na criação, grava a identidade do git
/// e o arquivo de configuração do preset `shell`. Um HOME existente não é
/// alterado.
pub fn ensure(host: &Host, layout: &Layout, harness: &str) -> Result<HomeSetup, PolicyError> {
    let home = layout.home(harness);
    let mut out = HomeSetup::default();
    if home.is_dir() {
        return Ok(out);
    }
    private_dir(&home)?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(|e| io(e, &home))?;
    out.created = true;
    out.warnings.extend(write_git_identity(host, &home));
    if harness == "shell" {
        write_shell_rc(&presets::system_shell(host), &home)?;
    }
    // A chave do hermes (`updates.check: false`) não é gravada aqui: um
    // config.yaml criado antes da importação impediria importar o real. Ela é
    // garantida depois da importação e a cada `sandmux run`.
    Ok(out)
}

/// Grava `.gitconfig` só com `user.name` e `user.email` do git do usuário.
pub fn write_git_identity(host: &Host, synthetic_home: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let get = |key: &str| -> Option<String> {
        let out = Command::new("git")
            .args(["config", "--global", "--get", key])
            .env("HOME", &host.home)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let target = synthetic_home.join(".gitconfig");
    let mut missing = Vec::new();
    for key in ["user.name", "user.email"] {
        match get(key) {
            Some(v) => {
                let ok = Command::new("git")
                    .arg("config")
                    .arg("--file")
                    .arg(&target)
                    .args([key, &v])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !ok {
                    missing.push(key);
                }
            }
            None => missing.push(key),
        }
    }
    if !missing.is_empty() {
        warnings.push(format!(
            "a configuração do git não define {}; um commit dentro do sandbox vai pedir a identidade",
            missing.join(" e ")
        ));
    }
    warnings
}

/// Arquivo de configuração mínimo do shell confinado.
pub fn write_shell_rc(shell: &Path, home: &Path) -> Result<(), PolicyError> {
    let name = shell
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let header = "# Gerado pelo sandmux ao criar o HOME sintético do preset shell.\n# O .bashrc e o .zshrc reais não são copiados. Este arquivo pode ser editado.\n";
    let (path, body): (PathBuf, String) = match name.as_str() {
        "zsh" => (
            home.join(".zshrc"),
            format!("{header}PROMPT='[sandmux] %n@%m:%~%# '\n"),
        ),
        "fish" => (
            home.join(".config/fish/config.fish"),
            format!(
                "{header}function fish_prompt\n    echo -n '[sandmux] '(prompt_pwd)'> '\nend\n"
            ),
        ),
        _ => (
            home.join(".bashrc"),
            format!("{header}PS1='[sandmux] \\u@\\h:\\w\\$ '\n"),
        ),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    fs::write(&path, body).map_err(|e| io(e, &path))
}
