//! Servidores MCP dos harnesses: o inventário, o estado pela configuração e
//! a aplicação (opencode pelo `OPENCODE_CONFIG_CONTENT`, hermes por um
//! `config.yaml` só da execução).

use std::path::Path;

/// Um servidor MCP de um harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    /// `local` ou `remote`.
    pub kind: String,
    /// O comando (local) ou a URL (remoto).
    pub target: String,
    /// `global` (HOME sintético) ou `projeto` (pasta de trabalho).
    pub origin: String,
}

/// Tira comentários (`//`, `/* */`) e vírgulas sobrando de um JSONC.
pub fn strip_jsonc(text: &str) -> String {
    let b: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_str) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match (c, b.get(i + 1)) {
            ('"', _) => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            ('/', Some('/')) => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            (',', _) => {
                // Vírgula antes de `}` ou `]` (ignorando espaços): sobra.
                let mut j = i + 1;
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                if !matches!(b.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn opencode_file(dir: &Path, origin: &str, out: &mut Vec<McpServer>) {
    for name in ["opencode.json", "opencode.jsonc"] {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(&text)) else {
            continue;
        };
        let Some(mcp) = v.get("mcp").and_then(|m| m.as_object()) else {
            continue;
        };
        for (k, s) in mcp {
            let kind = s.get("type").and_then(|t| t.as_str()).unwrap_or("local");
            let target = if kind == "remote" {
                s.get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                match s.get("command") {
                    Some(serde_json::Value::Array(a)) => a
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    Some(serde_json::Value::String(c)) => c.clone(),
                    _ => String::new(),
                }
            };
            out.retain(|e: &McpServer| e.name != *k);
            out.push(McpServer {
                name: k.clone(),
                kind: kind.into(),
                target,
                origin: origin.into(),
            });
        }
    }
}

/// Os servidores do opencode: o global do HOME sintético e o da pasta de
/// trabalho (que vence num nome repetido).
pub fn opencode_servers(synthetic_home: &Path, workdir: &Path) -> Vec<McpServer> {
    let mut out = Vec::new();
    opencode_file(&synthetic_home.join(".config/opencode"), "global", &mut out);
    opencode_file(workdir, "projeto", &mut out);
    out
}

fn indent(l: &str) -> usize {
    l.len() - l.trim_start().len()
}

fn yaml_key(l: &str) -> Option<String> {
    let t = l.trim();
    if t.starts_with('#') || t.starts_with('-') {
        return None;
    }
    let (k, _) = t.split_once(':')?;
    Some(k.trim().trim_matches(['"', '\'']).to_string())
}

/// Os blocos do `mcp_servers`: (nome, primeira linha, linha depois do fim,
/// linhas do bloco).
fn hermes_blocks(text: &str) -> Option<Vec<(String, usize, usize)>> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| indent(l) == 0 && yaml_key(l).as_deref() == Some("mcp_servers"))?;
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(start + 1) {
        if !l.trim().is_empty() && !l.trim_start().starts_with('#') && indent(l) == 0 {
            end = i;
            break;
        }
    }
    let child = lines[start + 1..end]
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| indent(l))
        .min()?;
    let mut blocks: Vec<(String, usize, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate().take(end).skip(start + 1) {
        if indent(l) == child && !l.trim().is_empty() && !l.trim_start().starts_with('#') {
            let k = yaml_key(l)?;
            if let Some(last) = blocks.last_mut() {
                last.2 = i;
            }
            blocks.push((k, i, end));
        }
    }
    Some(blocks)
}

/// Os servidores do hermes, pelo `mcp_servers` do `config.yaml`.
pub fn hermes_servers(text: &str) -> Vec<McpServer> {
    let lines: Vec<&str> = text.lines().collect();
    hermes_blocks(text)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, a, b)| {
            let field = |key: &str| {
                lines[a + 1..b].iter().find_map(|l| {
                    (yaml_key(l).as_deref() == Some(key))
                        .then(|| {
                            l.split_once(':')
                                .map(|x| x.1.trim().trim_matches(['"', '\'']).to_string())
                        })
                        .flatten()
                })
            };
            let url = field("url").filter(|u| !u.is_empty());
            McpServer {
                name,
                kind: if url.is_some() {
                    "remote".into()
                } else {
                    "local".into()
                },
                target: url.or_else(|| field("command")).unwrap_or_default(),
                origin: "global".into(),
            }
        })
        .collect()
}

/// O `config.yaml` sem os blocos de `disabled`. `None` quando não há o que
/// tirar ou o arquivo não é entendido.
pub fn hermes_filtered(text: &str, disabled: &[String]) -> Option<String> {
    let blocks = hermes_blocks(text)?;
    let cut: Vec<&(String, usize, usize)> = blocks
        .iter()
        .filter(|(n, _, _)| disabled.contains(n))
        .collect();
    if cut.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (i, l) in text.lines().enumerate() {
        if cut.iter().any(|(_, a, b)| i >= *a && i < *b) {
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    // Sem nenhum servidor, o mapa fica vazio e válido.
    if cut.len() == blocks.len() {
        out = out.replacen("mcp_servers:\n", "mcp_servers: {}\n", 1);
    }
    Some(out)
}

/// O `config.yaml` com mais um servidor em `mcp_servers` (criado no fim
/// quando não existe, ou quando é o mapa vazio `{}`).
pub fn hermes_with_server(text: &str, name: &str, command: &str) -> String {
    let block = |indent: usize| {
        let p = " ".repeat(indent);
        format!("{p}{name}:\n{p}{p}command: {command}\n")
    };
    let lines: Vec<&str> = text.lines().collect();
    if let Some(i) = lines
        .iter()
        .position(|l| indent(l) == 0 && yaml_key(l).as_deref() == Some("mcp_servers"))
    {
        let empty_map = lines[i].trim_end().ends_with("{}");
        let child = hermes_blocks(text)
            .and_then(|b| b.first().map(|(_, a, _)| indent(lines[*a])))
            .unwrap_or(2);
        let mut out = String::new();
        for (j, l) in lines.iter().enumerate() {
            if j == i {
                out.push_str("mcp_servers:\n");
                out.push_str(&block(child));
                let _ = empty_map;
                continue;
            }
            out.push_str(l);
            out.push('\n');
        }
        return out;
    }
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("mcp_servers:\n");
    out.push_str(&block(2));
    out
}

/// O estado de um servidor pelas listas da configuração.
pub fn enabled(name: &str, deny: &[String], allow: &[String]) -> bool {
    !deny.iter().any(|d| d == name) && (allow.is_empty() || allow.iter().any(|a| a == name))
}

/// O trecho `"mcp"` do `OPENCODE_CONFIG_CONTENT` com os desabilitados.
pub fn opencode_disabled_json(disabled: &[String]) -> Option<String> {
    if disabled.is_empty() {
        return None;
    }
    let mut m = serde_json::Map::new();
    for d in disabled {
        m.insert(d.clone(), serde_json::json!({ "enabled": false }));
    }
    Some(serde_json::Value::Object(m).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventario_do_opencode() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("h");
        let work = d.path().join("w");
        std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            home.join(".config/opencode/opencode.jsonc"),
            "{ // global\n \"mcp\": { \"github\": {\"type\": \"remote\", \"url\": \"https://api.github.com/mcp\"}, /* x */\n \"fs\": {\"type\": \"local\", \"command\": [\"npx\", \"fs-mcp\"],}, } }",
        )
        .unwrap();
        std::fs::write(
            work.join("opencode.json"),
            r#"{"mcp": {"fs": {"type": "local", "command": ["./meu-fs"]}, "db": {"type": "local", "command": "db-mcp"}}}"#,
        )
        .unwrap();
        let s = opencode_servers(&home, &work);
        let by = |n: &str| s.iter().find(|x| x.name == n).unwrap().clone();
        assert_eq!(by("github").kind, "remote");
        assert_eq!(by("github").target, "https://api.github.com/mcp");
        assert_eq!(by("fs").target, "./meu-fs");
        assert_eq!(by("fs").origin, "projeto");
        assert_eq!(by("db").target, "db-mcp");
        assert_eq!(s.len(), 3);
        assert_eq!(
            strip_jsonc(r#"{"a": "http://x//y", }"#),
            r#"{"a": "http://x//y" }"#
        );
    }

    #[test]
    fn hermes_e_filtro() {
        let text = "model: x\nmcp_servers:\n  fs:\n    command: npx\n    args: [\"fs\"]\n  # comentário\n  web:\n    url: https://web.test/mcp\nterminal:\n  backend: local\n";
        let s = hermes_servers(text);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].name.as_str(), s[0].kind.as_str()), ("fs", "local"));
        assert_eq!(
            (s[1].name.as_str(), s[1].target.as_str()),
            ("web", "https://web.test/mcp")
        );
        let f = hermes_filtered(text, &["web".into()]).unwrap();
        assert!(
            f.contains("  fs:") && !f.contains("web") && f.contains("terminal:\n  backend: local"),
            "{f}"
        );
        let all = hermes_filtered(text, &["web".into(), "fs".into()]).unwrap();
        assert!(
            all.contains("mcp_servers: {}") && all.contains("terminal:"),
            "{all}"
        );
        assert_eq!(hermes_filtered(text, &["outro".into()]), None);
        assert_eq!(hermes_filtered("model: x\n", &["fs".into()]), None);
        assert!(enabled("fs", &[], &[]));
        assert!(!enabled("fs", &["fs".into()], &[]));
        assert!(!enabled("web", &[], &["fs".into()]));
        let added = hermes_with_server(text, "sandmux-db", "/x/db");
        assert!(
            added.contains("mcp_servers:\n  sandmux-db:\n    command: /x/db\n  fs:"),
            "{added}"
        );
        assert_eq!(hermes_servers(&added).len(), 3);
        let novo = hermes_with_server("model: x\n", "sandmux-db", "/x/db");
        assert_eq!(hermes_servers(&novo)[0].name, "sandmux-db");
        let vazio = hermes_with_server("mcp_servers: {}\nterminal: 1\n", "sandmux-db", "/x/db");
        assert_eq!(hermes_servers(&vazio).len(), 1, "{vazio}");
        assert_eq!(
            opencode_disabled_json(&["github".into()]).as_deref(),
            Some(r#"{"github":{"enabled":false}}"#)
        );
    }
}
