//! O valor falso dos segredos do cofre na configuração de cada ferramenta,
//! só durante a execução: variáveis de ambiente e arquivos montados por cima
//! dos guardados no HOME sintético.

use std::path::{Path, PathBuf};

/// Um segredo para uma ferramenta: o domínio exato e o valor falso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSecret {
    pub tool: String,
    pub domain: String,
    pub fake: String,
}

/// O que a composição acrescenta.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ToolConfig {
    pub env: Vec<(String, String)>,
    /// Arquivos só desta execução: caminho relativo ao HOME e conteúdo.
    pub files: Vec<(PathBuf, String)>,
}

fn b64(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// Monta as variáveis e os arquivos das ferramentas. `home` é o HOME
/// sintético no host, de onde vêm os arquivos guardados.
pub fn configure(list: &[ToolSecret], home: &Path) -> ToolConfig {
    ToolConfig {
        env: env_for(list),
        files: files_for(list, home),
    }
}

/// As variáveis das ferramentas.
pub fn env_for(list: &[ToolSecret]) -> Vec<(String, String)> {
    let mut c = ToolConfig::default();
    let of = |t: &'static str| list.iter().filter(move |s| s.tool == t);

    if let Some(s) = of("gh").next() {
        c.env.push(("GH_TOKEN".into(), s.fake.clone()));
    }

    let git: Vec<&ToolSecret> = of("git").collect();
    if !git.is_empty() {
        c.env
            .push(("GIT_CONFIG_COUNT".into(), git.len().to_string()));
        for (i, s) in git.iter().enumerate() {
            c.env.push((
                format!("GIT_CONFIG_KEY_{i}"),
                format!("credential.https://{}.helper", s.domain),
            ));
            c.env.push((
                format!("GIT_CONFIG_VALUE_{i}"),
                format!(
                    "!f() {{ echo username=sandmux; echo password={}; }}; f",
                    s.fake
                ),
            ));
        }
    }

    let pip: Vec<String> = of("pip")
        .filter(|s| s.domain != "pypi.org")
        .map(|s| format!("https://sandmux:{}@{}/simple", s.fake, s.domain))
        .collect();
    if !pip.is_empty() {
        c.env.push(("PIP_EXTRA_INDEX_URL".into(), pip.join(" ")));
    }

    if let Some(s) = of("cargo").find(|s| s.domain == "crates.io") {
        c.env.push(("CARGO_REGISTRY_TOKEN".into(), s.fake.clone()));
    }

    // Canais do hermes: as variáveis que o gateway dele lê.
    for (tool, var) in CHANNEL_ENV {
        if let Some(s) = of(tool).next() {
            c.env.push((var.to_string(), s.fake.clone()));
        }
    }

    c.env
}

/// As variáveis dos canais do hermes, por ferramenta.
pub const CHANNEL_ENV: &[(&str, &str)] = &[
    ("telegram", "TELEGRAM_BOT_TOKEN"),
    ("discord", "DISCORD_BOT_TOKEN"),
    ("slack", "SLACK_BOT_TOKEN"),
    ("slack-app", "SLACK_APP_TOKEN"),
];

/// As variáveis dos canais, para o `~/.hermes/.env` da execução (o hermes o
/// lê por cima do ambiente).
pub fn channel_env(list: &[ToolSecret]) -> Vec<(String, String)> {
    env_for(list)
        .into_iter()
        .filter(|(k, _)| CHANNEL_ENV.iter().any(|(_, v)| v == k))
        .collect()
}

/// O valor falso no formato do token do canal, a partir de um falso comum.
pub fn channel_fake(tools: &[String], fake: &str) -> Option<String> {
    let has = |t: &str| tools.iter().any(|x| x == t);
    if has("telegram") {
        Some(format!("0000000000:{fake}"))
    } else if has("slack-app") {
        Some(format!("xapp-{fake}"))
    } else if has("slack") {
        Some(format!("xoxb-{fake}"))
    } else {
        None
    }
}

/// Os arquivos só desta execução: caminho relativo ao HOME e conteúdo.
pub fn files_for(list: &[ToolSecret], home: &Path) -> Vec<(PathBuf, String)> {
    let mut c = ToolConfig::default();
    let of = |t: &'static str| list.iter().filter(move |s| s.tool == t);

    let npm: Vec<&ToolSecret> = of("npm").collect();
    if !npm.is_empty() {
        let mut text = std::fs::read_to_string(home.join(".npmrc")).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for s in npm {
            text.push_str(&format!("//{}/:_authToken={}\n", s.domain, s.fake));
        }
        c.files.push((PathBuf::from(".npmrc"), text));
    }

    let docker: Vec<&ToolSecret> = of("docker").collect();
    if !docker.is_empty() {
        let saved = std::fs::read_to_string(home.join(".docker/config.json")).unwrap_or_default();
        let mut v: serde_json::Value =
            serde_json::from_str(&saved).unwrap_or_else(|_| serde_json::json!({}));
        if !v.is_object() {
            v = serde_json::json!({});
        }
        let auths = v
            .as_object_mut()
            .expect("objeto")
            .entry("auths")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(a) = auths.as_object_mut() {
            for s in docker {
                a.insert(
                    s.domain.clone(),
                    serde_json::json!({ "auth": b64(&format!("sandmux:{}", s.fake)) }),
                );
            }
        }
        c.files.push((
            PathBuf::from(".docker/config.json"),
            serde_json::to_string_pretty(&v).unwrap_or_default(),
        ));
    }
    c.files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(tool: &str, domain: &str, fake: &str) -> ToolSecret {
        ToolSecret {
            tool: tool.into(),
            domain: domain.into(),
            fake: fake.into(),
        }
    }

    #[test]
    fn cada_ferramenta() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".npmrc"), "save-exact=true").unwrap();
        let c = configure(
            &[
                ts("gh", "github.com", "F1"),
                ts("git", "github.com", "F1"),
                ts("git", "gitlab.empresa.com", "F2"),
                ts("npm", "npm.empresa.com", "F3"),
                ts("pip", "pypi.empresa.com", "F4"),
                ts("pip", "pypi.org", "F5"),
                ts("cargo", "crates.io", "F6"),
                ts("docker", "ghcr.io", "F7"),
            ],
            d.path(),
        );
        let env = |k: &str| c.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(env("GH_TOKEN").as_deref(), Some("F1"));
        assert_eq!(env("GIT_CONFIG_COUNT").as_deref(), Some("2"));
        assert_eq!(
            env("GIT_CONFIG_KEY_1").as_deref(),
            Some("credential.https://gitlab.empresa.com.helper")
        );
        assert!(env("GIT_CONFIG_VALUE_0").unwrap().contains("password=F1"));
        assert_eq!(
            env("PIP_EXTRA_INDEX_URL").as_deref(),
            Some("https://sandmux:F4@pypi.empresa.com/simple")
        );
        assert_eq!(env("CARGO_REGISTRY_TOKEN").as_deref(), Some("F6"));
        let npmrc = &c
            .files
            .iter()
            .find(|f| f.0 == Path::new(".npmrc"))
            .unwrap()
            .1;
        assert_eq!(npmrc, "save-exact=true\n//npm.empresa.com/:_authToken=F3\n");
        let dockercfg = &c
            .files
            .iter()
            .find(|f| f.0 == Path::new(".docker/config.json"))
            .unwrap()
            .1;
        let v: serde_json::Value = serde_json::from_str(dockercfg).unwrap();
        assert_eq!(v["auths"]["ghcr.io"]["auth"], b64("sandmux:F7"));
        assert_eq!(b64("sandmux:F7"), "c2FuZG11eDpGNw==");
        assert_eq!(configure(&[], d.path()), ToolConfig::default());
    }

    #[test]
    fn canais_do_hermes() {
        let ts = |tool: &str, domain: &str, fake: &str| ToolSecret {
            tool: tool.into(),
            domain: domain.into(),
            fake: fake.into(),
        };
        let list = vec![
            ts("telegram", "api.telegram.org", "0000000000:f1"),
            ts("discord", "discord.com", "f2"),
            ts("discord", "gateway.discord.gg", "f2"),
            ts("slack", "slack.com", "xoxb-f3"),
            ts("slack-app", "slack.com", "xapp-f4"),
            ts("gh", "github.com", "f5"),
        ];
        let env = channel_env(&list);
        assert_eq!(
            env,
            vec![
                (
                    "TELEGRAM_BOT_TOKEN".to_string(),
                    "0000000000:f1".to_string()
                ),
                ("DISCORD_BOT_TOKEN".into(), "f2".into()),
                ("SLACK_BOT_TOKEN".into(), "xoxb-f3".into()),
                ("SLACK_APP_TOKEN".into(), "xapp-f4".into()),
            ]
        );
        assert!(env_for(&list).iter().any(|(k, _)| k == "GH_TOKEN"));
        let t = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            channel_fake(&t(&["telegram"]), "sandmux-fake-x").as_deref(),
            Some("0000000000:sandmux-fake-x")
        );
        assert_eq!(
            channel_fake(&t(&["slack-app"]), "f").as_deref(),
            Some("xapp-f")
        );
        assert_eq!(channel_fake(&t(&["slack"]), "f").as_deref(), Some("xoxb-f"));
        assert_eq!(channel_fake(&t(&["discord"]), "f"), None);
        // O formato do Telegram que o hermes confere.
        let f = channel_fake(
            &t(&["telegram"]),
            "sandmux-fake-0123456789abcdef0123456789abcdef",
        )
        .unwrap();
        let (id, rest) = f.split_once(':').unwrap();
        assert!(id.chars().all(|c| c.is_ascii_digit()) && rest.len() >= 30);
    }
}
