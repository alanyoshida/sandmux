//! Assistente: modelos escritos `<provedor>/<modelo>` e os provedores, todos
//! pela API compatível com a da OpenAI (`<url>/chat/completions`).

use std::collections::BTreeMap;

/// Provedores conhecidos: nome, URL base e a variável da chave (a mesma de
/// [`crate::presets::PROVIDERS`]).
pub const KNOWN: &[(&str, &str, &str)] = &[
    (
        "openrouter",
        "https://openrouter.ai/api/v1",
        "OPENROUTER_API_KEY",
    ),
    (
        "anthropic",
        "https://api.anthropic.com/v1",
        "ANTHROPIC_API_KEY",
    ),
    ("openai", "https://api.openai.com/v1", "OPENAI_API_KEY"),
    (
        "gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "GEMINI_API_KEY",
    ),
    (
        "deepseek",
        "https://api.deepseek.com/v1",
        "DEEPSEEK_API_KEY",
    ),
    ("mistral", "https://api.mistral.ai/v1", "MISTRAL_API_KEY"),
    ("groq", "https://api.groq.com/openai/v1", "GROQ_API_KEY"),
    ("xai", "https://api.x.ai/v1", "XAI_API_KEY"),
];

/// Harness interno das abas de chat.
pub const HARNESS: &str = "chat";
/// Origem das regras de rede dos provedores próprios.
pub const ORIGIN: &str = "assistente";

/// Variável com os provedores, entregue ao `sandmux-chat`.
pub const PROVIDERS_ENV: &str = "SANDMUX_CHAT_PROVIDERS";

/// Seção `[assistant]` da configuração.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssistantConfig {
    pub chat_model: Option<String>,
    /// Modelo do modal de comando; sem ele, vale o do chat.
    pub command_model: Option<String>,
    pub models: Vec<String>,
    /// Provedores próprios: nome e URL base.
    pub providers: BTreeMap<String, String>,
}

/// Um provedor resolvido: nome, URL base e a variável da chave (vazia num
/// provedor próprio, que a recebe do daemon).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Provider {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub env: String,
}

impl Provider {
    /// O host da URL, sem porta.
    pub fn host(&self) -> String {
        url_host(&self.url).unwrap_or_default()
    }

    pub fn is_known(&self) -> bool {
        KNOWN.iter().any(|(n, _, _)| *n == self.name)
    }
}

/// Divide `<provedor>/<modelo>` na primeira barra.
pub fn split_model(model: &str) -> Result<(&str, &str), String> {
    match model.trim().split_once('/') {
        Some((p, m)) if !p.is_empty() && !m.trim().is_empty() => Ok((p, m.trim())),
        _ => Err(format!(
            "modelo inválido: {model:?}; escreva <provedor>/<modelo>, como openrouter/deepseek/deepseek-v4.1-flash-1017"
        )),
    }
}

/// Host de uma URL `http://` ou `https://`, sem porta nem caminho.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let auth = rest.split(['/', '?', '#']).next()?;
    if auth.contains('@') || auth.is_empty() {
        return None;
    }
    let host = if let Some(h) = auth.strip_prefix('[') {
        h.split(']').next()?.to_string()
    } else {
        auth.rsplit_once(':')
            .map(|(h, p)| {
                if p.chars().all(|c| c.is_ascii_digit()) {
                    h
                } else {
                    auth
                }
            })
            .unwrap_or(auth)
            .to_string()
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

impl AssistantConfig {
    /// Todos os provedores: os conhecidos e os próprios.
    pub fn all_providers(&self) -> Vec<Provider> {
        KNOWN
            .iter()
            .map(|(n, u, e)| Provider {
                name: n.to_string(),
                url: u.to_string(),
                env: e.to_string(),
            })
            .chain(self.providers.iter().map(|(n, u)| Provider {
                name: n.clone(),
                url: u.trim_end_matches('/').to_string(),
                env: String::new(),
            }))
            .collect()
    }

    pub fn provider(&self, name: &str) -> Option<Provider> {
        self.all_providers().into_iter().find(|p| p.name == name)
    }

    /// Nomes dos provedores, para as mensagens.
    pub fn provider_names(&self) -> String {
        self.all_providers()
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Confere o modelo e devolve o provedor e o nome enviado a ele.
    pub fn resolve(&self, model: &str) -> Result<(Provider, String), String> {
        let (p, m) = split_model(model)?;
        let prov = self.provider(p).ok_or_else(|| {
            format!(
                "provedor desconhecido: {p:?}. Provedores: {}",
                self.provider_names()
            )
        })?;
        Ok((prov, m.to_string()))
    }

    /// O modelo do modal de comando: `command-model` ou, sem ele, o do chat.
    pub fn default_command_model(&self) -> Option<String> {
        self.command_model
            .clone()
            .or_else(|| self.chat_model.clone())
    }

    /// Os modelos oferecidos nos modais: os padrões e os da lista, sem
    /// repetição.
    pub fn model_choices(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self
            .chat_model
            .iter()
            .chain(self.command_model.iter())
            .chain(self.models.iter())
        {
            if !out.contains(m) {
                out.push(m.clone());
            }
        }
        out
    }
}

/// Texto de sistema do modal de comando: só a pasta e o shell, e o pedido de
/// um único comando.
pub fn command_prompt(folder: &str, shell: &str) -> String {
    format!(
        "Você gera comandos de terminal. Responda só com um único comando para o shell {shell}, \
         que será executado na pasta {folder}. Não explique, não use cercas de código nem \
         prompt. Use várias linhas só quando não houver outro jeito."
    )
}

/// Limpa a resposta do modelo: o conteúdo do primeiro bloco de código, sem
/// prompt (`$ ` ou `% `) no começo das linhas e sem linhas vazias nas pontas.
pub fn clean_command(text: &str) -> String {
    let t = text.trim();
    let body = match t.find("```") {
        Some(i) => {
            let after = &t[i + 3..];
            // A primeira linha do bloco é a linguagem (pode ser vazia).
            let after = after.split_once('\n').map(|x| x.1).unwrap_or("");
            after.split("```").next().unwrap_or("")
        }
        None => t,
    };
    let lines: Vec<&str> = body
        .lines()
        .map(|l| {
            l.strip_prefix("$ ")
                .or_else(|| l.strip_prefix("% "))
                .unwrap_or(l)
                .trim_end()
        })
        .collect();
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

/// Nome válido para um provedor próprio.
pub fn valid_provider_name(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_lowercase() || f.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modelo_dividido_na_primeira_barra() {
        assert_eq!(
            split_model("openrouter/deepseek/deepseek-v4.1").unwrap(),
            ("openrouter", "deepseek/deepseek-v4.1")
        );
        assert!(split_model("semprovedor").is_err());
        assert!(split_model("/x").is_err());
        assert!(split_model("p/ ").is_err());
    }

    #[test]
    fn host_da_url() {
        assert_eq!(
            url_host("https://openrouter.ai/api/v1").as_deref(),
            Some("openrouter.ai")
        );
        assert_eq!(
            url_host("http://host.sandmux.internal:11434/v1").as_deref(),
            Some("host.sandmux.internal")
        );
        assert_eq!(url_host("http://[::1]:80/").as_deref(), Some("::1"));
        assert_eq!(url_host("ftp://x"), None);
        assert_eq!(url_host("https://u:p@x/"), None);
    }

    #[test]
    fn provedores_conhecidos_e_proprios() {
        let mut c = AssistantConfig::default();
        c.providers.insert(
            "local".into(),
            "http://host.sandmux.internal:11434/v1/".into(),
        );
        let (p, m) = c.resolve("openrouter/anthropic/claude-sonnet-5").unwrap();
        assert_eq!(p.url, "https://openrouter.ai/api/v1");
        assert_eq!(p.env, "OPENROUTER_API_KEY");
        assert_eq!(m, "anthropic/claude-sonnet-5");
        let (p, m) = c.resolve("local/llama3").unwrap();
        assert_eq!(p.url, "http://host.sandmux.internal:11434/v1");
        assert_eq!(
            (p.host().as_str(), m.as_str()),
            ("host.sandmux.internal", "llama3")
        );
        assert!(!p.is_known());
        let e = c.resolve("nada/x").unwrap_err();
        assert!(e.contains("openrouter") && e.contains("local"), "{e}");
        // Os conhecidos usam as variáveis dos provedores de inferência.
        for (_, url, env) in KNOWN {
            let host = url_host(url).unwrap();
            assert!(
                crate::presets::PROVIDERS.contains(&(env, host.as_str())),
                "{env} {host}"
            );
        }
    }

    #[test]
    fn comando_limpo() {
        assert_eq!(clean_command("```bash\nls -la\n```"), "ls -la");
        assert_eq!(clean_command("  $ ls -la  \n"), "ls -la");
        assert_eq!(
            clean_command("Use:\n```\nfind . -mtime 0\n```\nPronto."),
            "find . -mtime 0"
        );
        assert_eq!(clean_command("```sh\n\n% a\nb\n\n```"), "a\nb");
        assert_eq!(clean_command("echo oi"), "echo oi");
        let p = command_prompt("/home/ana/app", "bash");
        assert!(p.contains("/home/ana/app") && p.contains("bash"), "{p}");
    }

    #[test]
    fn escolhas_do_modal() {
        let c = AssistantConfig {
            chat_model: Some("a/1".into()),
            command_model: None,
            models: vec!["b/2".into(), "a/1".into()],
            providers: BTreeMap::new(),
        };
        assert_eq!(c.model_choices(), vec!["a/1", "b/2"]);
    }
}
