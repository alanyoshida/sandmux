//! Troca de segredos por valores falsos em arquivos de configuração.
//!
//! Duas regras, aplicadas juntas:
//!
//! - **pelo nome da chave**: em JSON, YAML e `.env`, o valor de uma chave com
//!   nome de credencial é trocado, desde que pareça um valor (não um número,
//!   um booleano ou uma referência a variável);
//! - **pelo formato do valor**: em qualquer texto, um valor com formato de
//!   token conhecido (prefixos de provedores, JWT, tokens de bot) é trocado.
//!
//! Na dúvida, é segredo: trocar a mais só exige recadastrar; trocar a menos
//! vaza.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Prefixo de todo valor falso.
pub const FAKE_PREFIX: &str = "sandmux-fake-";

/// Uma troca feita: em qual chave (ou `<token>` quando foi pelo formato) e
/// qual valor falso entrou no lugar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub key: String,
    pub fake: String,
    pub line: usize,
}

/// Formato do arquivo, pela extensão ou pelo nome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Env,
    Text,
}

pub fn format_of(name: &str) -> Format {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".json") || lower.ends_with(".jsonc") {
        Format::Json
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        Format::Yaml
    } else if lower == ".env" || lower.ends_with(".env") || lower.starts_with(".env.") {
        Format::Env
    } else {
        Format::Text
    }
}

static KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        // Nomes exatos usados por arquivos de credencial (o auth.json do opencode
        // guarda `key`, e `access`/`refresh` no login OAuth) e fragmentos comuns.
        r"(?i)(^(key|access|refresh|pass|secret)$|api[_-]?key|apikey|secret|token|passw(?:or)?d|pwd|credential|private[_-]?key|access[_-]?key|client[_-]?secret|auth|bearer|cookie|session[_-]?key|signing[_-]?key|webhook)",
    )
    .unwrap()
});

static KEY_EXCLUDED_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(_|-)?(type|mode|method|provider|strategy|enabled|file|path|dir|limit|count|max|min|ttl|timeout|expires?|header|name|url_base|scheme)$",
    )
    .unwrap()
});

static TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(sk-(?:or-v1-|or-|ant-|proj-)?[A-Za-z0-9_\-]{16,}",
        r"|gh[pousr]_[A-Za-z0-9]{20,}",
        r"|github_pat_[A-Za-z0-9_]{20,}",
        r"|xox[abprs]-[A-Za-z0-9\-]{10,}",
        r"|xapp-[A-Za-z0-9\-]{10,}",
        r"|glpat-[A-Za-z0-9_\-]{16,}",
        r"|hf_[A-Za-z0-9]{20,}",
        r"|AKIA[0-9A-Z]{16}",
        r"|AIza[0-9A-Za-z_\-]{30,}",
        r"|eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        r"|[MN][A-Za-z0-9]{23,}\.[A-Za-z0-9_\-]{6}\.[A-Za-z0-9_\-]{27,}",
        r"|\b[0-9]{6,12}:[A-Za-z0-9_\-]{30,})",
    ))
    .unwrap()
});

static JSON_PAIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""((?:[^"\\]|\\.)*)"(\s*:\s*)"((?:[^"\\]|\\.)*)""#).unwrap());

static YAML_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(\s*(?:-\s+)?)("[^"]+"|'[^']+'|[A-Za-z0-9_.\-]+)(\s*:\s+)(.*?)\s*$"#).unwrap()
});

static ENV_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*(?:#\s*)?(?:export\s+)?)([A-Za-z_][A-Za-z0-9_]*)(=)(.*?)\s*$").unwrap()
});

fn random_hex() -> String {
    let mut b = [0u8; 8];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Gera um valor falso novo, diferente a cada chamada.
pub fn fake_value() -> String {
    format!("{FAKE_PREFIX}{}", random_hex())
}

pub fn is_credential_key(key: &str) -> bool {
    let k = key.trim_matches(|c| c == '"' || c == '\'');
    KEY_RE.is_match(k) && !KEY_EXCLUDED_SUFFIX.is_match(k)
}

/// O valor parece um segredo (e não um número, booleano ou referência).
fn qualifies(v: &str) -> bool {
    let t = v.trim();
    if t.len() < 4 || t.starts_with(FAKE_PREFIX) {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "null" | "none" | "yes" | "no" | "on" | "off" | "~"
    ) {
        return false;
    }
    if t.parse::<f64>().is_ok() {
        return false;
    }
    if t.starts_with("${")
        || t.starts_with("{env:")
        || t.starts_with("{file:")
        || t.starts_with("$(")
    {
        return false;
    }
    if t.starts_with('$')
        && t[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c == '{')
    {
        return false;
    }
    if matches!(t.chars().next(), Some('{' | '[' | '|' | '>' | '&' | '*')) {
        return false;
    }
    true
}

/// Troca o valor preservando um esquema de autorização (`Bearer x`).
fn replace_value(v: &str, fake: &str) -> String {
    for scheme in ["Bearer ", "bearer ", "Basic ", "Token ", "token "] {
        if let Some(rest) = v.strip_prefix(scheme)
            && !rest.trim().is_empty()
        {
            return format!("{scheme}{fake}");
        }
    }
    fake.to_string()
}

fn strip_quotes(v: &str) -> (&str, &str, &str) {
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return (&v[..1], &v[1..v.len() - 1], &v[v.len() - 1..]);
        }
    }
    ("", v, "")
}

/// Aplica as duas regras ao texto. Devolve o texto novo e as trocas.
pub fn scrub(text: &str, format: Format) -> (String, Vec<Replacement>) {
    let mut out_lines = Vec::new();
    let mut reps = Vec::new();
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let (body, nl) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        let mut l = body.to_string();
        match format {
            Format::Json => {
                l = JSON_PAIR
                    .replace_all(&l, |c: &Captures| {
                        let (k, sep, v) = (&c[1], &c[2], &c[3]);
                        if is_credential_key(k) && qualifies(v) {
                            let fake = fake_value();
                            reps.push(Replacement {
                                key: k.into(),
                                fake: fake.clone(),
                                line: i + 1,
                            });
                            format!("\"{k}\"{sep}\"{}\"", replace_value(v, &fake))
                        } else {
                            c[0].to_string()
                        }
                    })
                    .into_owned();
            }
            Format::Yaml | Format::Env => {
                let re = if format == Format::Yaml {
                    &*YAML_LINE
                } else {
                    &*ENV_LINE
                };
                if let Some(c) = re.captures(&l) {
                    let (pre, k, sep, rest) = (&c[1], &c[2], &c[3], &c[4]);
                    let (value, comment) = match rest.find(" #") {
                        Some(pos) if !rest.starts_with('"') && !rest.starts_with('\'') => {
                            (&rest[..pos], &rest[pos..])
                        }
                        _ => (rest, ""),
                    };
                    let (q1, inner, q2) = strip_quotes(value.trim_end());
                    if is_credential_key(k) && qualifies(inner) {
                        let fake = fake_value();
                        reps.push(Replacement {
                            key: k.trim_matches(|c| c == '"' || c == '\'').into(),
                            fake: fake.clone(),
                            line: i + 1,
                        });
                        l = format!(
                            "{pre}{k}{sep}{q1}{}{q2}{comment}",
                            replace_value(inner, &fake)
                        );
                    }
                }
            }
            Format::Text => {}
        }
        l = TOKEN_RE
            .replace_all(&l, |c: &Captures| {
                let m = &c[0];
                if m.starts_with(FAKE_PREFIX) {
                    return m.to_string();
                }
                let fake = fake_value();
                reps.push(Replacement {
                    key: "<token>".into(),
                    fake: fake.clone(),
                    line: i + 1,
                });
                fake
            })
            .into_owned();
        out_lines.push(format!("{l}{nl}"));
    }
    (out_lines.concat(), reps)
}

/// Troca todo valor falso por um marcador fixo, para comparar arquivos
/// importados em momentos diferentes.
pub fn normalize_fakes(text: &str) -> String {
    static FAKE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"sandmux-fake-[0-9a-f]{16}").unwrap());
    FAKE_RE.replace_all(text, "sandmux-fake-*").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_OR: &str = "sk-or-v1-0123456789abcdef0123456789abcdef0123456789abcdef";
    const REAL_GH: &str = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcd012345";

    fn assert_clean(out: &str, secrets: &[&str]) {
        for s in secrets {
            assert!(!out.contains(s), "segredo sobrou: {s}\n{out}");
        }
    }

    #[test]
    fn auth_json_do_opencode() {
        let t = format!(
            "{{\n  \"openrouter\": {{\n    \"type\": \"api\",\n    \"key\": \"{REAL_OR}\"\n  }}\n}}\n"
        );
        let (out, reps) = scrub(&t, Format::Json);
        assert_clean(&out, &[REAL_OR]);
        assert!(out.contains("\"type\": \"api\""));
        assert_eq!(reps.len(), 1);
        assert!(out.contains(FAKE_PREFIX));
        assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok());
    }

    #[test]
    fn oauth_do_opencode() {
        let t = r#"{"anthropic": {"type": "oauth", "refresh": "rt-real-0123456789", "access": "at-real-0123456789", "expires": 1760000000}}"#;
        let (out, reps) = scrub(t, Format::Json);
        assert_clean(&out, &["rt-real-0123456789", "at-real-0123456789"]);
        assert!(out.contains("\"type\": \"oauth\"") && out.contains("1760000000"));
        assert_eq!(reps.len(), 2);
    }

    #[test]
    fn token_de_mcp_em_header_e_ambiente() {
        let t = format!(
            r#"{{"mcp": {{"gh": {{"type": "remote", "headers": {{"Authorization": "Bearer {REAL_GH}"}}, "environment": {{"GITHUB_TOKEN": "outro-segredo-123", "LOG_LEVEL": "debug"}}}}}}}}"#
        );
        let (out, reps) = scrub(&t, Format::Json);
        assert_clean(&out, &[REAL_GH, "outro-segredo-123"]);
        assert!(out.contains("\"Authorization\": \"Bearer sandmux-fake-"));
        assert!(out.contains("\"LOG_LEVEL\": \"debug\""));
        assert!(out.contains("\"type\": \"remote\""));
        assert_eq!(reps.len(), 2);
    }

    #[test]
    fn env_do_hermes() {
        let t = format!(
            "OPENROUTER_API_KEY={REAL_OR}\nTELEGRAM_BOT_TOKEN=\"1234567890:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"\nLOG_LEVEL=info\nMAX_TOKENS=4096\n# SLACK_TOKEN=xoxb-1111-2222-abcdefghij\nexport DB_PASSWORD='hunter22'\n"
        );
        let (out, reps) = scrub(&t, Format::Env);
        assert_clean(
            &out,
            &[
                REAL_OR,
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "xoxb-1111",
                "hunter22",
            ],
        );
        assert!(out.contains("LOG_LEVEL=info"));
        assert!(out.contains("MAX_TOKENS=4096"));
        assert!(out.contains("TELEGRAM_BOT_TOKEN=\"sandmux-fake-"));
        assert_eq!(reps.len(), 4);
    }

    #[test]
    fn yaml_com_comentario_e_tipo() {
        let t = "providers:\n  openrouter:\n    api_key: abc-def-segredo # chave\n    auth_type: oauth\n    max_tokens: 1000\n    base_url: https://openrouter.ai/api/v1\n    api_key_env: ${OPENROUTER_API_KEY}\n";
        let (out, reps) = scrub(t, Format::Yaml);
        assert_clean(&out, &["abc-def-segredo"]);
        assert!(out.contains("    api_key: sandmux-fake-"));
        assert!(out.contains(" # chave"));
        assert!(out.contains("auth_type: oauth"));
        assert!(out.contains("max_tokens: 1000"));
        assert!(out.contains("${OPENROUTER_API_KEY}"));
        assert_eq!(reps.len(), 1);
    }

    #[test]
    fn token_em_texto_livre_e_trocado() {
        let t = format!("use a chave {REAL_GH} no script\nconst k = \"{REAL_OR}\";\n");
        let (out, reps) = scrub(&t, Format::Text);
        assert_clean(&out, &[REAL_GH, REAL_OR]);
        assert_eq!(reps.len(), 2);
    }

    #[test]
    fn valores_falsos_sao_unicos_e_nao_derivam_do_real() {
        let t = format!("A_TOKEN={REAL_OR}\nB_TOKEN={REAL_OR}\n");
        let (out, reps) = scrub(&t, Format::Env);
        assert_ne!(reps[0].fake, reps[1].fake);
        assert!(!out.contains(&REAL_OR[10..30]));
    }

    #[test]
    fn segunda_passagem_nao_troca_de_novo() {
        let (once, _) = scrub(&format!("API_KEY={REAL_OR}\n"), Format::Env);
        let (twice, reps) = scrub(&once, Format::Env);
        assert_eq!(once, twice);
        assert!(reps.is_empty());
    }
}
