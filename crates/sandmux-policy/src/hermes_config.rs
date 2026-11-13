//! Edição pontual do `config.yaml` do hermes no HOME sintético: garante
//! `updates.check: false` sem reescrever o resto do arquivo.

use std::path::Path;

use crate::PolicyError;

/// Garante a chave em `<home>/.hermes/config.yaml`, criando o arquivo se
/// preciso.
pub fn ensure_update_check_off(home: &Path) -> Result<(), PolicyError> {
    let path = home.join(".hermes/config.yaml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let new = set_update_check_off(&text);
    if new != text {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PolicyError::Io(format!("{}: {e}", parent.display())))?;
        }
        std::fs::write(&path, new)
            .map_err(|e| PolicyError::Io(format!("{}: {e}", path.display())))?;
    }
    Ok(())
}

/// Conteúdo do `~/.hermes/.env` de uma execução com `--pass-env`: as linhas
/// de `existing`, sem as que definem uma das variáveis passadas, mais uma
/// linha `NOME="valor"` para cada uma. O hermes carrega esse arquivo por cima
/// do ambiente e lê as credenciais direto dele, então o valor real precisa
/// estar no arquivo.
///
/// Recusa valores com quebra de linha ou `${`: o hermes expande `${...}`
/// mesmo entre aspas, e uma quebra de linha mudaria as linhas do arquivo.
pub fn env_with_values(existing: &str, vars: &[(String, String)]) -> Result<String, PolicyError> {
    for (name, value) in vars {
        if value.contains(['\n', '\r']) || value.contains("${") {
            return Err(PolicyError::UnrepresentableEnv(name.clone()));
        }
    }
    let mut out = String::new();
    for line in existing.lines() {
        let defined = dotenv_key(line);
        if defined.is_some_and(|k| vars.iter().any(|(n, _)| n == k)) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    for (name, value) in vars {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!("{name}=\"{escaped}\"\n"));
    }
    Ok(out)
}

/// Nome definido por uma linha de `.env` (`NOME=...` ou `export NOME=...`).
fn dotenv_key(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let t = t.strip_prefix("export ").map(str::trim_start).unwrap_or(t);
    let (key, _) = t.split_once('=')?;
    let key = key.trim_end();
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
    valid.then_some(key)
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn is_content(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// Devolve o texto com `updates.check: false`, mexendo só no necessário.
pub fn set_update_check_off(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let top = lines.iter().position(|l| {
        indent_of(l) == 0 && {
            let t = l.trim_end();
            t == "updates:"
                || t.starts_with("updates:") && t["updates:".len()..].trim_start().starts_with('#')
        }
    });
    let Some(top) = top else {
        // Sem bloco `updates`; um `updates:` com valor na mesma linha é trocado.
        if let Some(i) = lines
            .iter()
            .position(|l| indent_of(l) == 0 && l.starts_with("updates:"))
        {
            lines[i] = "updates:\n  check: false".into();
            return join(lines, text);
        }
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push("updates:".into());
        lines.push("  check: false".into());
        return join(lines, text);
    };
    let mut child_indent = None;
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(top + 1) {
        if !is_content(line) {
            continue;
        }
        let ind = indent_of(line);
        if ind == 0 {
            end = i;
            break;
        }
        child_indent.get_or_insert(ind);
        if Some(ind) == child_indent && line.trim_start().starts_with("check:") {
            let comment = line
                .split_once(" #")
                .map(|(_, c)| format!(" #{c}"))
                .unwrap_or_default();
            let new = format!("{}check: false{comment}", " ".repeat(ind));
            if lines[i] != new {
                lines[i] = new;
            }
            return join(lines, text);
        }
    }
    let ind = child_indent.unwrap_or(2);
    let _ = end;
    lines.insert(top + 1, format!("{}check: false", " ".repeat(ind)));
    join(lines, text)
}

fn join(lines: Vec<String>, original: &str) -> String {
    let mut s = lines.join("\n");
    if original.is_empty() || original.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivo_vazio() {
        assert_eq!(set_update_check_off(""), "updates:\n  check: false\n");
    }

    #[test]
    fn sem_bloco_updates() {
        assert_eq!(
            set_update_check_off("model:\n  default: x\n"),
            "model:\n  default: x\n\nupdates:\n  check: false\n"
        );
    }

    #[test]
    fn check_ligado_vira_false_mantendo_o_resto() {
        let t = "a: 1\nupdates:\n    channel: stable\n    check: true # comentário\nb: 2\n";
        assert_eq!(
            set_update_check_off(t),
            "a: 1\nupdates:\n    channel: stable\n    check: false # comentário\nb: 2\n"
        );
    }

    #[test]
    fn bloco_sem_check_recebe_com_a_mesma_indentacao() {
        let t = "updates:\n    channel: stable\nb: 2\n";
        assert_eq!(
            set_update_check_off(t),
            "updates:\n    check: false\n    channel: stable\nb: 2\n"
        );
    }

    #[test]
    fn ja_desligado_nao_muda() {
        let t = "updates:\n  check: false\n";
        assert_eq!(set_update_check_off(t), t);
    }

    #[test]
    fn check_de_outro_bloco_nao_e_confundido() {
        let t = "plugins:\n  check: true\nupdates:\n  channel: x\n";
        assert_eq!(
            set_update_check_off(t),
            "plugins:\n  check: true\nupdates:\n  check: false\n  channel: x\n"
        );
    }

    fn vars(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn env_troca_o_falso_e_mantem_o_resto() {
        let existing = "# chaves\nOPENROUTER_API_KEY=sandmux-fake-0123456789abcdef\nexport OPENAI_API_KEY = sandmux-fake-1\nOUTRA=1\n";
        let out = env_with_values(
            existing,
            &vars(&[("OPENROUTER_API_KEY", "sk-real"), ("OPENAI_API_KEY", "sk2")]),
        )
        .unwrap();
        assert_eq!(
            out,
            "# chaves\nOUTRA=1\nOPENROUTER_API_KEY=\"sk-real\"\nOPENAI_API_KEY=\"sk2\"\n"
        );
        assert!(!out.contains("sandmux-fake"));
    }

    #[test]
    fn env_acrescenta_quando_nao_existe_e_escapa() {
        let out = env_with_values("", &vars(&[("K", r#"a"b\c $X #y"#)])).unwrap();
        assert_eq!(out, "K=\"a\\\"b\\\\c $X #y\"\n");
    }

    #[test]
    fn env_recusa_o_que_nao_cabe_sem_mostrar_o_valor() {
        for v in ["a\nb", "a\rb", "x${HOME}y"] {
            let err = env_with_values("", &vars(&[("K", v)]))
                .unwrap_err()
                .to_string();
            assert!(err.contains("K") && !err.contains(v), "{err}");
        }
    }
}
