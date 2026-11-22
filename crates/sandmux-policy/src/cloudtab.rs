//! As identidades de cloud numa aba. No GCP, a pasta do `gcloud` só desta
//! execução, com uma configuração por identidade (o token falso em arquivo)
//! e o ADC com o refresh falso da padrão. No Kubernetes, a pasta `~/.kube`
//! com um kubeconfig só com falsos. O real fica no gateway.

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
    write_files(dir, gcloud_files(ids, home))
}

/// Recria `dir` com os arquivos (pastas 700, arquivos 600). Sem arquivos,
/// não cria nada e devolve `false`.
fn write_files(dir: &Path, files: Vec<(PathBuf, String)>) -> std::io::Result<bool> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    if files.is_empty() {
        return Ok(false);
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
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

/// Uma identidade de Kubernetes, como a aba a vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeTab {
    /// O nome sem o prefixo (`kind-dev` de `kube:kind-dev`).
    pub name: String,
    /// O nome original do contexto no host.
    pub context: String,
    pub namespace: String,
    pub fake: String,
    pub default: bool,
}

/// Onde a pasta do `kubectl` fica no HOME da aba.
pub const KUBE_DIR: &str = ".kube";
/// A CA do lançamento, dentro da pasta do `kubectl` da aba.
pub const KUBE_CA_FILE: &str = "sandmux-ca.crt";

/// O kubeconfig da aba (JSON, que o `kubectl` lê como YAML): um contexto
/// por identidade, com o nome original, o servidor sintético, a CA do
/// lançamento e o token falso. `home` é o HOME dentro do sandbox.
pub fn kubeconfig(ids: &[KubeTab], home: &Path) -> Option<String> {
    let default = ids.iter().find(|i| i.default).or(ids.first())?;
    let ca = home.join(KUBE_DIR).join(KUBE_CA_FILE).display().to_string();
    let user = |i: &KubeTab| format!("sandmux-{}", i.name);
    let clusters: Vec<_> = ids
        .iter()
        .map(|i| {
            serde_json::json!({
                "name": i.name,
                "cluster": {
                    "server": format!("https://{}", sandmux_spec::net::kube_domain(&i.name)),
                    "certificate-authority": ca,
                }
            })
        })
        .collect();
    let users: Vec<_> = ids
        .iter()
        .map(|i| serde_json::json!({ "name": user(i), "user": { "token": i.fake } }))
        .collect();
    let contexts: Vec<_> = ids
        .iter()
        .map(|i| {
            let mut c = serde_json::json!({ "cluster": i.name, "user": user(i) });
            if !i.namespace.is_empty() {
                c["namespace"] = i.namespace.clone().into();
            }
            serde_json::json!({ "name": i.context, "context": c })
        })
        .collect();
    let v = serde_json::json!({
        "apiVersion": "v1",
        "kind": "Config",
        "preferences": {},
        "clusters": clusters,
        "users": users,
        "contexts": contexts,
        "current-context": default.context,
    });
    Some(format!("{v:#}\n"))
}

/// Grava a pasta do `kubectl` da aba em `dir`. Sem identidade, não cria
/// nada e devolve `false`.
pub fn write_kube_dir(dir: &Path, ids: &[KubeTab], home: &Path) -> std::io::Result<bool> {
    let files = kubeconfig(ids, home)
        .map(|c| vec![(PathBuf::from("config"), c)])
        .unwrap_or_default();
    write_files(dir, files)
}

/// Uma identidade da AWS, como a aba a vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsTab {
    /// O nome original do perfil no host.
    pub profile: String,
    pub region: String,
    pub fake_id: String,
    pub fake_secret: String,
    pub default: bool,
}

/// Onde a pasta do `aws` fica no HOME da aba.
pub const AWS_DIR: &str = ".aws";

/// O `config` e o `credentials` da aba: um perfil por identidade, com o
/// nome original, a região e o par falso; o `[default]` é a identidade
/// padrão. Vazio sem identidade.
pub fn aws_files(ids: &[AwsTab]) -> Vec<(PathBuf, String)> {
    let Some(default) = ids.iter().find(|i| i.default).or(ids.first()) else {
        return Vec::new();
    };
    let mut config = String::new();
    let mut creds = String::new();
    let mut section = |name: &str, i: &AwsTab| {
        let head = if name == "default" {
            "[default]".to_string()
        } else {
            format!("[profile {name}]")
        };
        config.push_str(&head);
        config.push('\n');
        if !i.region.is_empty() {
            config.push_str(&format!("region = {}\n", i.region));
        }
        config.push('\n');
        creds.push_str(&format!(
            "[{name}]\naws_access_key_id = {}\naws_secret_access_key = {}\n\n",
            i.fake_id, i.fake_secret
        ));
    };
    section("default", default);
    for i in ids.iter().filter(|i| i.profile != "default") {
        section(&i.profile, i);
    }
    vec![
        (PathBuf::from("config"), config),
        (PathBuf::from("credentials"), creds),
    ]
}

/// Grava a pasta do `aws` da aba em `dir`. Sem identidade, não cria nada e
/// devolve `false`.
pub fn write_aws_dir(dir: &Path, ids: &[AwsTab]) -> std::io::Result<bool> {
    write_files(dir, aws_files(ids))
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

    fn kube(name: &str, default: bool) -> KubeTab {
        KubeTab {
            name: name.into(),
            context: format!("ctx:{name}"),
            namespace: if name == "dev" {
                "app".into()
            } else {
                String::new()
            },
            fake: format!("sandmux-fake-kube-{name}"),
            default,
        }
    }

    #[test]
    fn kubeconfig_da_aba() {
        assert!(kubeconfig(&[], Path::new("/home/ana")).is_none());
        let text = kubeconfig(
            &[kube("dev", false), kube("prod", true)],
            Path::new("/home/ana"),
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["current-context"], "ctx:prod");
        assert_eq!(
            v["clusters"][0]["cluster"]["server"],
            "https://dev.kube.sandmux.internal"
        );
        assert_eq!(
            v["clusters"][0]["cluster"]["certificate-authority"],
            "/home/ana/.kube/sandmux-ca.crt"
        );
        assert_eq!(v["users"][1]["user"]["token"], "sandmux-fake-kube-prod");
        assert_eq!(v["contexts"][0]["name"], "ctx:dev");
        assert_eq!(v["contexts"][0]["context"]["namespace"], "app");
        assert!(v["contexts"][1]["context"].get("namespace").is_none());
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("s1-1");
        assert!(!write_kube_dir(&dir, &[], Path::new("/h")).unwrap());
        assert!(!dir.exists());
        assert!(write_kube_dir(&dir, &[kube("dev", true)], Path::new("/h")).unwrap());
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(dir.join("config")).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn pasta_do_aws_da_aba() {
        assert!(aws_files(&[]).is_empty());
        let t = |p: &str, d: bool| AwsTab {
            profile: p.into(),
            region: if p == "Dev_SSO" {
                "sa-east-1".into()
            } else {
                String::new()
            },
            fake_id: format!("AKIASANDMUX{}", p.len()),
            fake_secret: format!("segredo-{p}"),
            default: d,
        };
        let f = aws_files(&[t("Dev_SSO", true), t("ci", false)]);
        let config = &f[0].1;
        let creds = &f[1].1;
        assert!(
            config.starts_with("[default]\nregion = sa-east-1\n"),
            "{config}"
        );
        assert!(
            config.contains("[profile Dev_SSO]\nregion = sa-east-1"),
            "{config}"
        );
        assert!(config.contains("[profile ci]\n"), "{config}");
        assert!(
            creds.contains("[default]\naws_access_key_id = AKIASANDMUX7\n"),
            "{creds}"
        );
        assert!(
            creds.contains(
                "[ci]\naws_access_key_id = AKIASANDMUX2\naws_secret_access_key = segredo-ci"
            ),
            "{creds}"
        );
        // O perfil `default` do host não se repete.
        let f = aws_files(&[t("default", true), t("ci", false)]);
        assert_eq!(f[0].1.matches("[default]").count(), 1);
        assert!(!f[0].1.contains("[profile default]"));
    }
}
