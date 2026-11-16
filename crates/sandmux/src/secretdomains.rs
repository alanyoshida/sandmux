//! Domínios donos das chaves reais achadas nos HOMEs sintéticos: pelo nome
//! da chave, pelo provedor em volta ou pelo `baseURL` da configuração. Usado
//! pelo `sandmux doctor --fix`, pelo `sandmux vault import` e pelo vigia de
//! chaves do daemon.

/// O texto de um arquivo do HOME sintético de um harness.
pub fn synth_text(
    host: &sandmux_policy::Host,
    harness: &str,
    file: &std::path::Path,
) -> Option<String> {
    let path = sandmux_policy::Layout::new(&host.data_dir)
        .home(harness)
        .join(file);
    std::fs::read_to_string(path).ok()
}

/// O provedor em volta da linha `line` (a partir de 1): o nome do objeto
/// JSON (ou da seção) que a contém, como `"openrouter": {` no `auth.json` do
/// opencode. `headers` e `options` são pulados.
pub fn provider_of(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let upto = line.checked_sub(1)?.min(lines.len());
    lines[..upto].iter().rev().find_map(|l| {
        let t = l.trim();
        let name = t
            .strip_suffix('{')
            .map(str::trim_end)
            .and_then(|t| t.strip_suffix(':'))
            .map(|n| n.trim().trim_matches('"').to_string())?;
        (!name.is_empty() && !matches!(name.as_str(), "headers" | "options")).then_some(name)
    })
}

/// O nome que o sandbox usa para a máquina.
pub const HOST_DOMAIN: &str = "host.sandmux.internal";

/// `localhost`, `127.0.0.1` e `::1` viram o nome da máquina no sandbox; o
/// resto fica como está.
pub fn normalize_domain(d: &str) -> String {
    let t = d.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    if matches!(t.as_str(), "localhost" | "127.0.0.1" | "::1") {
        HOST_DOMAIN.to_string()
    } else {
        d.trim().to_string()
    }
}

/// O domínio pelo `baseURL` do provedor na configuração do opencode (JSON ou
/// JSONC): o host da URL, ou o nome da máquina para um endereço local.
pub fn base_url_domain(text: &str, provider: &str) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_str(&sandmux_policy::mcp::strip_jsonc(text)).ok()?;
    let url = v
        .get("provider")?
        .get(provider)?
        .get("options")?
        .get("baseURL")?
        .as_str()?;
    sandmux_policy::assistant::url_host(url).map(|h| normalize_domain(&h))
}

/// Os domínios donos de uma chave: pelo nome dela, pelo provedor em volta, ou
/// pelo `baseURL` do provedor no mesmo arquivo. Vazio quando nada indica.
pub fn infer_domains(key: &str, provider: Option<&str>, text: Option<&str>) -> Vec<String> {
    let d = known_domains(key);
    if !d.is_empty() {
        return d;
    }
    let Some(p) = provider else {
        return Vec::new();
    };
    let d = known_domains(p);
    if !d.is_empty() {
        return d;
    }
    text.and_then(|t| base_url_domain(t, p))
        .into_iter()
        .collect()
}

/// Como [`infer_domains`], e, no opencode, também pelo `baseURL` do
/// provedor na configuração do HOME sintético (a chave do `auth.json` fica
/// longe da URL dela, no `opencode.json`).
pub fn infer_domains_for(
    host: &sandmux_policy::Host,
    harness: &str,
    key: &str,
    provider: Option<&str>,
    text: Option<&str>,
) -> Vec<String> {
    let d = infer_domains(key, provider, text);
    if !d.is_empty() || harness != "opencode" {
        return d;
    }
    let Some(p) = provider else { return d };
    for f in [
        ".config/opencode/opencode.json",
        ".config/opencode/opencode.jsonc",
    ] {
        if let Some(cfg) = synth_text(host, harness, std::path::Path::new(f))
            && let Some(dom) = base_url_domain(&cfg, p)
        {
            return vec![dom];
        }
    }
    d
}

/// Domínios conhecidos pelo nome de uma chave da importação ou de um
/// segredo guardado sem `--domain`.
pub fn known_domains(key: &str) -> Vec<String> {
    let k = key.to_ascii_lowercase();
    // O segredo do webhook do Telegram é conferido pelo próprio hermes nos
    // pedidos que chegam: fica fora da troca.
    if k.contains("webhook") {
        return Vec::new();
    }
    let table: &[(&str, &[&str])] = &[
        ("openrouter", &["openrouter.ai"]),
        ("anthropic", &["api.anthropic.com"]),
        ("openai", &["api.openai.com"]),
        ("deepseek", &["api.deepseek.com"]),
        ("mistral", &["api.mistral.ai"]),
        ("groq", &["api.groq.com"]),
        ("gemini", &["generativelanguage.googleapis.com"]),
        ("google", &["generativelanguage.googleapis.com"]),
        ("xai", &["api.x.ai"]),
        ("telegram", &["api.telegram.org"]),
        ("discord", &["discord.com", "gateway.discord.gg"]),
        ("slack", &["slack.com"]),
    ];
    table
        .iter()
        .find(|(n, _)| k.contains(n))
        .map(|(_, d)| d.iter().map(|x| x.to_string()).collect())
        .unwrap_or_default()
}

/// O host da URL `http(s)://` mais próxima da linha `line` (a partir de 1),
/// até 10 linhas antes ou depois, normalizado. É só uma sugestão para uma
/// chave sem domínio conhecido, que o usuário confirma na TUI; o vigia do
/// daemon não a usa, porque uma URL próxima pode ser de outro serviço.
pub fn nearby_url_domain(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let at = line.checked_sub(1)?;
    (0..=10).find_map(|d| {
        [at.checked_sub(d), Some(at + d)]
            .into_iter()
            .flatten()
            .filter_map(|i| lines.get(i))
            .find_map(|l| url_in(l))
    })
}

/// O host da primeira URL `http(s)://` da linha.
fn url_in(l: &str) -> Option<String> {
    let i = l.find("https://").or_else(|| l.find("http://"))?;
    let rest = &l[i..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '`' | '>' | ')'))
        .unwrap_or(rest.len());
    sandmux_policy::assistant::url_host(&rest[..end]).map(|h| normalize_domain(&h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sugestao_pela_url_proxima() {
        let t = "llm:\n  api_key: sk-REAL\n  base_url: https://api.exemplo.com/v1\n";
        assert_eq!(nearby_url_domain(t, 2).as_deref(), Some("api.exemplo.com"));
        // A mais próxima ganha.
        let t = "a: https://longe.com\nb\nc\nkey: x\nd: \"http://perto.com:8080/v1\"\n";
        assert_eq!(nearby_url_domain(t, 4).as_deref(), Some("perto.com"));
        // Endereço local vira o nome da máquina.
        let t = "key: x\nurl: http://localhost:1234/v1\n";
        assert_eq!(nearby_url_domain(t, 1).as_deref(), Some(HOST_DOMAIN));
        // Nada além de 10 linhas.
        let t = format!("key: x\n{}url: https://api.longe.com\n", "\n".repeat(11));
        assert_eq!(nearby_url_domain(&t, 1), None);
        assert_eq!(nearby_url_domain("key: x\n", 1), None);
    }

    #[test]
    fn dominio_pelo_provedor_e_pela_url() {
        let cfg = "{\n  \"provider\": {\n    \"unsloth-local\": {\n      \"options\": {\n        \"baseURL\": \"http://127.0.0.1:8888/v1\",\n        \"headers\": {\n          \"Authorization\": \"Bearer sandmux-fake-x\"\n        }\n      }\n    },\n    \"corp\": {\"options\": {\"baseURL\": \"https://llm.empresa.com/v1\"}}\n  }\n}\n";
        assert_eq!(provider_of(cfg, 7).as_deref(), Some("unsloth-local"));
        assert_eq!(
            infer_domains("Authorization", Some("unsloth-local"), Some(cfg)),
            vec!["host.sandmux.internal"]
        );
        assert_eq!(
            base_url_domain(cfg, "corp").as_deref(),
            Some("llm.empresa.com")
        );
        assert_eq!(
            infer_domains("key", Some("openrouter"), None),
            vec!["openrouter.ai"]
        );
        assert!(infer_domains("key", Some("x"), None).is_empty());
        for d in ["localhost", "127.0.0.1", "::1", "[::1]", " LOCALHOST "] {
            assert_eq!(normalize_domain(d), "host.sandmux.internal", "{d}");
        }
        assert_eq!(normalize_domain("api.openai.com"), "api.openai.com");
    }

    #[test]
    fn dominios_pelo_nome_da_chave() {
        assert_eq!(known_domains("OPENROUTER_API_KEY"), vec!["openrouter.ai"]);
        assert_eq!(known_domains("openrouter.key"), vec!["openrouter.ai"]);
        assert!(known_domains("<token>").is_empty());
        assert_eq!(
            known_domains("discord"),
            vec!["discord.com", "gateway.discord.gg"]
        );
        assert_eq!(known_domains("SLACK_APP_TOKEN"), vec!["slack.com"]);
        assert_eq!(
            known_domains("TELEGRAM_BOT_TOKEN"),
            vec!["api.telegram.org"]
        );
        assert!(known_domains("TELEGRAM_WEBHOOK_SECRET").is_empty());
    }
}
