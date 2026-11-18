//! `sandmux files`: o registro de arquivos, lido do disco.

use anyhow::Result;
use sandmux_spec::FileEntry;
use sandmux_spec::{tr, trf};

/// Pasta do registro de arquivos.
pub fn log_dir() -> std::path::PathBuf {
    sandmux::paths::state_dir().join("files")
}

fn read_day(dir: &std::path::Path, day: &str) -> Vec<FileEntry> {
    std::fs::read_to_string(dir.join(format!("{day}.jsonl")))
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// A sessão de uma etiqueta (`s<id>-<n>`).
fn session_of(tag: &str) -> Option<u32> {
    tag.strip_prefix('s')?.split('-').next()?.parse().ok()
}

/// Linhas da tabela, das mais velhas às mais novas.
pub fn table(lines: &[FileEntry]) -> Vec<String> {
    let rows: Vec<[String; 5]> = lines
        .iter()
        .map(|l| {
            [
                l.ts.split('T').nth(1).unwrap_or(&l.ts).to_string(),
                session_of(&l.tag).map_or_else(
                    || {
                        if l.tag.starts_with("run-") {
                            "run".into()
                        } else if l.tag.starts_with("docker-") {
                            "docker".into()
                        } else {
                            "-".into()
                        }
                    },
                    |s| s.to_string(),
                ),
                l.access.clone(),
                l.path.display().to_string(),
                l.blocked
                    .as_deref()
                    .map(|b| trf!("bloqueado: {b}", "blocked: {b}"))
                    .unwrap_or_default(),
            ]
        })
        .collect();
    let head = [
        tr!("HORA", "TIME"),
        tr!("SESSÃO", "SESSION"),
        tr!("ACESSO", "ACCESS"),
        tr!("CAMINHO", "PATH"),
        "",
    ];
    let mut w = head.map(|h| h.chars().count());
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let fmt = |cells: [&str; 5]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c:<width$}", width = w[i]))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut out = vec![fmt(head)];
    for r in &rows {
        out.push(fmt([&r[0], &r[1], &r[2], &r[3], &r[4]]));
    }
    out
}

/// As linhas de uma sessão: as dela e as do Docker privado da pasta dela
/// (etiqueta `docker-<id>`, com a mesma pasta de trabalho).
pub fn of_session(lines: Vec<FileEntry>, id: u32) -> Vec<FileEntry> {
    let workdirs: std::collections::HashSet<std::path::PathBuf> = lines
        .iter()
        .filter(|l| session_of(&l.tag) == Some(id))
        .map(|l| l.workdir.clone())
        .collect();
    lines
        .into_iter()
        .filter(|l| {
            session_of(&l.tag) == Some(id)
                || (l.tag.starts_with("docker-") && workdirs.contains(&l.workdir))
        })
        .collect()
}

/// Os acessos de uma sessão em `days` dias, um por caminho (o mais
/// recente), dos mais novos aos mais velhos.
pub fn recent(id: u32, days: u32) -> Vec<FileEntry> {
    let dir = log_dir();
    let all: Vec<FileEntry> = of_session(
        (0..days.max(1))
            .rev()
            .flat_map(|d| read_day(&dir, &sandmux_net::log::day_ago(d)))
            .collect(),
        id,
    );
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for l in all.into_iter().rev() {
        if seen.insert(l.path.clone()) {
            out.push(l);
        }
    }
    out
}

pub fn run(id: Option<u32>, days: u32) -> Result<i32> {
    let dir = log_dir();
    let all: Vec<FileEntry> = (0..days.max(1))
        .rev()
        .flat_map(|d| read_day(&dir, &sandmux_net::log::day_ago(d)))
        .collect();
    let mut lines = match id {
        Some(id) => of_session(all, id),
        None => all,
    };
    if lines.is_empty() {
        println!(
            "{}",
            tr!(
                "Nenhum acesso a arquivo registrado.",
                "No file accesses logged."
            )
        );
        return Ok(0);
    }
    // As últimas 200 linhas.
    let skip = lines.len().saturating_sub(200);
    lines.drain(..skip);
    for l in table(&lines) {
        println!("{l}");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(tag: &str, w: &str, p: &str) -> FileEntry {
        FileEntry {
            ts: "2026-11-17T10:00:00".into(),
            tag: tag.into(),
            workdir: w.into(),
            path: p.into(),
            access: "r".into(),
            blocked: None,
            new: false,
        }
    }

    #[test]
    fn sessao_inclui_o_docker_da_pasta() {
        let l = vec![
            e("s3-1", "/w", "/w/a"),
            e("docker-ab12", "/w", "/w/.env"),
            e("docker-cd34", "/outra", "/outra/.env"),
            e("s4-1", "/outra", "/outra/b"),
        ];
        let got: Vec<String> = of_session(l, 3)
            .iter()
            .map(|x| x.path.display().to_string())
            .collect();
        assert_eq!(got, ["/w/a", "/w/.env"]);
        let t = table(&[e("docker-ab12", "/w", "/w/.env")]);
        assert!(t[1].contains("docker"), "{t:?}");
    }
}
