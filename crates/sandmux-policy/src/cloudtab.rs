//! As identidades de GCP numa aba: a pasta do `gcloud` só desta execução,
//! com uma configuração por identidade (o token falso em arquivo) e o ADC
//! com o refresh falso da padrão. Só falsos: o real fica no gateway.

use std::path::{Path, PathBuf};

/// Uma identidade de GCP, como a aba a vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcpTab {
    /// O nome sem o prefixo (`trabalho` de `gcp:trabalho`).
    pub name: String,
    pub account: String,
    pub project: String,
    pub access_fake: String,
    pub refresh_fake: String,
    pub default: bool,
}

/// Onde a pasta do `gcloud` fica no HOME da aba.
pub const GCLOUD_DIR: &str = ".config/gcloud";

/// Os arquivos da pasta do `gcloud` da aba: caminho relativo à pasta e
/// conteúdo. `home` é o HOME dentro do sandbox (para o caminho absoluto do
/// arquivo de token). Vazio sem identidade.
pub fn gcloud_files(ids: &[GcpTab], home: &Path) -> Vec<(PathBuf, String)> {
    if ids.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in ids {
        let token = PathBuf::from("sandmux").join(format!("{}.token", i.name));
        let mut conf = String::from("[core]\n");
        if !i.account.is_empty() {
            conf.push_str(&format!("account = {}\n", i.account));
        }
        if !i.project.is_empty() {
            conf.push_str(&format!("project = {}\n", i.project));
        }
        conf.push_str(&format!(
            "\n[auth]\naccess_token_file = {}\n",
            home.join(GCLOUD_DIR).join(&token).display()
        ));
        out.push((
            PathBuf::from("configurations").join(format!("config_{}", i.name)),
            conf,
        ));
        out.push((token, format!("{}\n", i.access_fake)));
    }
    let default = ids.iter().find(|i| i.default).unwrap_or(&ids[0]);
    out.push((PathBuf::from("active_config"), default.name.clone()));
    let adc = serde_json::json!({
        "type": "authorized_user",
        "client_id": "sandmux.apps.googleusercontent.com",
        "client_secret": "sandmux",
        "refresh_token": default.refresh_fake,
        "universe_domain": "googleapis.com",
    });
    out.push((
        PathBuf::from("application_default_credentials.json"),
        format!("{adc:#}\n"),
    ));
    out
}

/// Grava os arquivos em `dir` (pasta 700, arquivos 600). Sem identidade,
/// não cria nada e devolve `false`.
pub fn write_gcloud_dir(dir: &Path, ids: &[GcpTab], home: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let files = gcloud_files(ids, home);
    if files.is_empty() {
        return Ok(false);
    }
    let _ = std::fs::remove_dir_all(dir);
    for (rel, contents) in files {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&p)?
            .write_all(contents.as_bytes())?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str, default: bool) -> GcpTab {
        GcpTab {
            name: name.into(),
            account: format!("{name}@exemplo.com"),
            project: if name == "trabalho" {
                "app-dev".into()
            } else {
                String::new()
            },
            access_fake: format!("ya29.sandmux-fake-{name}"),
            refresh_fake: format!("1//sandmux-fake-{name}"),
            default,
        }
    }

    #[test]
    fn pasta_do_gcloud_da_aba() {
        assert!(gcloud_files(&[], Path::new("/home/ana")).is_empty());
        let f = gcloud_files(
            &[id("trabalho", false), id("deploy", true)],
            Path::new("/home/ana"),
        );
        let get = |p: &str| {
            f.iter()
                .find(|(r, _)| r == Path::new(p))
                .map(|(_, c)| c.clone())
                .unwrap_or_else(|| panic!("sem {p}: {f:?}"))
        };
        let conf = get("configurations/config_trabalho");
        assert!(conf.contains("account = trabalho@exemplo.com"), "{conf}");
        assert!(conf.contains("project = app-dev"), "{conf}");
        assert!(
            conf.contains("access_token_file = /home/ana/.config/gcloud/sandmux/trabalho.token"),
            "{conf}"
        );
        assert!(!get("configurations/config_deploy").contains("project"));
        assert_eq!(get("sandmux/deploy.token"), "ya29.sandmux-fake-deploy\n");
        assert_eq!(get("active_config"), "deploy");
        let adc: serde_json::Value =
            serde_json::from_str(&get("application_default_credentials.json")).unwrap();
        assert_eq!(adc["type"], "authorized_user");
        assert_eq!(adc["refresh_token"], "1//sandmux-fake-deploy");
        // Sem padrão marcada, a primeira.
        let f = gcloud_files(&[id("a", false), id("b", false)], Path::new("/h"));
        assert!(
            f.iter()
                .any(|(r, c)| r == Path::new("active_config") && c == "a")
        );
    }

    #[test]
    fn grava_a_pasta() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("s1-1");
        assert!(!write_gcloud_dir(&dir, &[], Path::new("/h")).unwrap());
        assert!(!dir.exists());
        assert!(write_gcloud_dir(&dir, &[id("trabalho", true)], Path::new("/h")).unwrap());
        let m = std::fs::metadata(dir.join("sandmux/trabalho.token")).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::read_to_string(dir.join("active_config")).unwrap(),
            "trabalho"
        );
    }
}
