//! Modelos de servidores locais no opencode: os provedores da configuração
//! que apontam para o host (LM Studio, Ollama, Unsloth Studio e outros
//! compatíveis com a OpenAI) e o bloco que cadastra os modelos descobertos no
//! `OPENCODE_CONFIG_CONTENT`.

use std::path::Path;

/// Um provedor do opencode servido na máquina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProvider {
    pub id: String,
    /// URL base, com `http://` e o host local.
    pub base_url: String,
    pub port: u16,
    /// Valor do cabeçalho `Authorization` da configuração.
    pub authorization: Option<String>,
    /// `options.apiKey`.
    pub api_key: Option<String>,
}

/// URL base padrão dos provedores locais sem `baseURL`.
fn default_url(id: &str) -> Option<&'static str> {
    match id {
        "lmstudio" => Some("http://127.0.0.1:1234/v1"),
        "ollama" => Some("http://127.0.0.1:11434/v1"),
        _ => None,
    }
}

/// Host e porta de uma URL `http://` local; `None` em outra.
pub fn local_port(url: &str) -> Option<u16> {
    let rest = url.strip_prefix("http://")?;
    let auth = rest.split(['/', '?', '#']).next()?;
    let (host, port) = if let Some(h) = auth.strip_prefix('[') {
        let (h, after) = h.split_once(']')?;
        (h, after.strip_prefix(':').unwrap_or("80"))
    } else {
        match auth.rsplit_once(':') {
            Some((h, p)) => (h, p),
            None => (auth, "80"),
        }
    };
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return None;
    }
    port.parse().ok().filter(|p| *p > 0)
}

fn read(dir: &Path, out: &mut Vec<LocalProvider>) {
    for name in ["opencode.json", "opencode.jsonc"] {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&crate::mcp::strip_jsonc(&text))
        else {
            continue;
        };
        let Some(provs) = v.get("provider").and_then(|p| p.as_object()) else {
            continue;
        };
        for (id, p) in provs {
            let opts = p.get("options");
            let url = opts
                .and_then(|o| o.get("baseURL"))
                .and_then(|u| u.as_str())
                .map(str::to_string)
                .or_else(|| default_url(id).map(str::to_string));
            let Some(url) = url else { continue };
            let Some(port) = local_port(&url) else {
                continue;
            };
            let authorization = opts
                .and_then(|o| o.get("headers"))
                .and_then(|h| h.as_object())
                .and_then(|h| {
                    h.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                        .and_then(|(_, v)| v.as_str())
                })
                .map(str::to_string);
            let api_key = opts
                .and_then(|o| o.get("apiKey"))
                .and_then(|k| k.as_str())
                .map(str::to_string);
            let lp = LocalProvider {
                id: id.clone(),
                base_url: url.trim_end_matches('/').to_string(),
                port,
                authorization,
                api_key,
            };
            // O do projeto vence o global, como no opencode.
            out.retain(|x| x.id != lp.id);
            out.push(lp);
        }
    }
}

/// Os provedores locais da configuração global (HOME sintético) e do projeto.
pub fn local_providers(synthetic_home: &Path, workdir: &Path) -> Vec<LocalProvider> {
    let mut out = Vec::new();
    read(&synthetic_home.join(".config/opencode"), &mut out);
    read(workdir, &mut out);
    out
}

/// Mescla os modelos descobertos num `OPENCODE_CONFIG_CONTENT` (JSON).
pub fn merge_models(current: &str, found: &[(String, Vec<String>)]) -> String {
    let mut v: serde_json::Value =
        serde_json::from_str(current).unwrap_or_else(|_| serde_json::json!({}));
    if !v.is_object() {
        v = serde_json::json!({});
    }
    for (id, models) in found {
        if models.is_empty() {
            continue;
        }
        let root = v.as_object_mut().expect("objeto");
        let prov = root
            .entry("provider")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .map(|p| p.entry(id.clone()).or_insert_with(|| serde_json::json!({})));
        let Some(prov) = prov.and_then(|p| p.as_object_mut()) else {
            continue;
        };
        let ms = prov
            .entry("models")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(ms) = ms.as_object_mut() {
            for m in models {
                ms.entry(m.clone()).or_insert_with(|| serde_json::json!({}));
            }
        }
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_locais() {
        assert_eq!(local_port("http://127.0.0.1:8888/v1"), Some(8888));
        assert_eq!(local_port("http://localhost:1234/v1"), Some(1234));
        assert_eq!(local_port("http://[::1]:11434/v1"), Some(11434));
        assert_eq!(local_port("http://localhost/v1"), Some(80));
        assert_eq!(local_port("https://127.0.0.1:8888/v1"), None);
        assert_eq!(local_port("http://192.168.0.2:8888/v1"), None);
    }

    #[test]
    fn provedores_da_configuracao() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("home");
        let work = d.path().join("work");
        std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            home.join(".config/opencode/opencode.json"),
            r#"{"provider": {
                "unsloth-local": {"npm": "@ai-sdk/openai-compatible",
                    "options": {"baseURL": "http://127.0.0.1:8888/v1",
                        "headers": {"Authorization": "Bearer sandmux-fake-x"}},
                    "models": {"a": {}}},
                "lmstudio": {"models": {}},
                "openrouter": {"options": {"apiKey": "x"}},
                "longe": {"options": {"baseURL": "http://10.0.0.2:8080/v1"}}
            }}"#,
        )
        .unwrap();
        std::fs::write(
            work.join("opencode.jsonc"),
            "{ // projeto\n \"provider\": {\"ollama\": {\"options\": {\"apiKey\": \"k\"}}}, }",
        )
        .unwrap();
        let ps = local_providers(&home, &work);
        let ids: Vec<&str> = ps.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["unsloth-local", "lmstudio", "ollama"]);
        let u = &ps[0];
        assert_eq!(
            (u.base_url.as_str(), u.port),
            ("http://127.0.0.1:8888/v1", 8888)
        );
        assert_eq!(u.authorization.as_deref(), Some("Bearer sandmux-fake-x"));
        assert_eq!(ps[1].port, 1234);
        assert_eq!((ps[2].port, ps[2].api_key.as_deref()), (11434, Some("k")));
    }

    #[test]
    fn mescla_no_config_content() {
        let cur = r#"{"provider":{"openrouter":{"options":{"apiKey":"{env:K}"}}},"mcp":{}}"#;
        let out = merge_models(
            cur,
            &[
                (
                    "unsloth-local".into(),
                    vec!["Bonsai-27B-GGUF".into(), "g".into()],
                ),
                ("vazio".into(), vec![]),
            ],
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["provider"]["openrouter"]["options"]["apiKey"], "{env:K}");
        assert!(v["provider"]["unsloth-local"]["models"]["Bonsai-27B-GGUF"].is_object());
        assert!(v["provider"].get("vazio").is_none());
        assert!(v["mcp"].is_object());
    }
}
