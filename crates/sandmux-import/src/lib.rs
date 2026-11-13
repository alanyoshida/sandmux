//! Importação do estado de um harness para o HOME sintético.
//!
//! A cópia é de mão única, guiada por uma tabela de permissão por harness: o
//! que não está na tabela fica de fora. Segredos são trocados por valores
//! falsos ([`secrets`]); um manifesto, fora do HOME sintético, registra cada
//! troca sem o valor real. Rodada de novo, a importação só copia arquivos que
//! ainda não existem e lista os que diferem.

pub mod secrets;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::secrets::{format_of, normalize_fakes, scrub};

/// Pastas que nunca são copiadas, em qualquer nível.
const SKIP_DIRS: &[&str] = &["node_modules", "__pycache__", ".venv", "venv", ".cache"];

/// Um item da tabela, relativo ao HOME.
#[derive(Debug, Clone, Copy)]
enum Item {
    File(&'static str),
    Dir(&'static str),
    /// Arquivos do primeiro nível da pasta cujo nome casa com um dos padrões.
    TopFiles(&'static str, &'static [&'static str]),
}

/// Pastas do estado real de cada harness, para listar o que ficou de fora.
fn state_roots(harness: &str) -> &'static [&'static str] {
    match harness {
        "opencode" => &[".config/opencode", ".local/share/opencode"],
        "hermes" => &[".hermes"],
        _ => &[],
    }
}

fn table(harness: &str) -> Option<&'static [Item]> {
    const OPENCODE: &[Item] = &[
        Item::File(".config/opencode/opencode.json"),
        Item::File(".config/opencode/opencode.jsonc"),
        Item::File(".config/opencode/config.json"),
        Item::File(".config/opencode/tui.json"),
        Item::File(".config/opencode/tui.jsonc"),
        Item::Dir(".config/opencode/plugins"),
        Item::Dir(".config/opencode/plugin"),
        Item::Dir(".config/opencode/skills"),
        Item::Dir(".config/opencode/agent"),
        Item::Dir(".config/opencode/agents"),
        Item::Dir(".config/opencode/command"),
        Item::Dir(".config/opencode/commands"),
        Item::Dir(".config/opencode/themes"),
        Item::TopFiles(
            ".config/opencode",
            &[
                "*.js",
                "*.mjs",
                "*.cjs",
                "*.ts",
                "*.mts",
                "package.json",
                "package-lock.json",
                "bun.lock",
            ],
        ),
        Item::File(".local/share/opencode/auth.json"),
    ];
    const HERMES: &[Item] = &[
        Item::File(".hermes/config.yaml"),
        Item::File(".hermes/SOUL.md"),
        Item::File(".hermes/.env"),
        Item::File(".hermes/auth.json"),
        Item::Dir(".hermes/skills"),
        Item::Dir(".hermes/memories"),
        Item::Dir(".hermes/plugins"),
        Item::Dir(".hermes/hooks"),
    ];
    match harness {
        "opencode" => Some(OPENCODE),
        "hermes" => Some(HERMES),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("o preset {0} não tem importação")]
    NotImportable(String),
    #[error("{0}")]
    Io(String),
}

fn io(e: std::io::Error, p: &Path) -> ImportError {
    ImportError::Io(format!("{}: {e}", p.display()))
}

/// Uma troca registrada no manifesto. Nunca guarda o valor real.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Arquivo, relativo ao HOME.
    pub file: PathBuf,
    /// Chave cujo valor foi trocado, ou `<token>` quando a troca foi pelo formato.
    pub key: String,
    pub line: usize,
    pub fake: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub harness: String,
    pub entries: Vec<ManifestEntry>,
}

/// O que a importação fez.
#[derive(Debug, Default)]
pub struct Report {
    pub copied: Vec<PathBuf>,
    pub overwritten: Vec<PathBuf>,
    pub unchanged: Vec<PathBuf>,
    pub differing: Vec<PathBuf>,
    pub ignored: Vec<PathBuf>,
    pub replaced: Vec<ManifestEntry>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.copied.is_empty()
            && self.overwritten.is_empty()
            && self.unchanged.is_empty()
            && self.differing.is_empty()
    }
}

/// O preset tem importação.
pub fn importable(harness: &str) -> bool {
    table(harness).is_some()
}

/// Existe estado real do harness para importar.
pub fn has_state(harness: &str, real_home: &Path) -> bool {
    table(harness).is_some_and(|t| !files_to_copy(t, real_home).is_empty())
}

fn matches_pattern(name: &str, pat: &str) -> bool {
    match pat.strip_prefix('*') {
        Some(suffix) => name.ends_with(suffix),
        None => name == pat,
    }
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()) {
                continue;
            }
            walk(&p, base, out);
        } else if ft.is_file()
            && let Ok(rel) = p.strip_prefix(base)
        {
            out.push(rel.to_path_buf());
        }
    }
}

/// Arquivos a copiar, relativos ao HOME, em ordem estável.
fn files_to_copy(items: &[Item], home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for item in items {
        match *item {
            Item::File(rel) => {
                let p = home.join(rel);
                if p.is_file() && !p.is_symlink() {
                    out.push(PathBuf::from(rel));
                }
            }
            Item::Dir(rel) => {
                let p = home.join(rel);
                if p.is_dir() && !p.is_symlink() {
                    walk(&p, home, &mut out);
                }
            }
            Item::TopFiles(rel, pats) => {
                let Ok(rd) = fs::read_dir(home.join(rel)) else {
                    continue;
                };
                let mut names: Vec<_> = rd
                    .flatten()
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| pats.iter().any(|p| matches_pattern(n, p)))
                    .collect();
                names.sort();
                out.extend(names.into_iter().map(|n| Path::new(rel).join(n)));
            }
        }
    }
    let mut seen = BTreeSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// Entradas do primeiro nível das pastas de estado que a tabela não cobre.
fn ignored_entries(harness: &str, home: &Path, copied: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in state_roots(harness) {
        let Ok(rd) = fs::read_dir(home.join(root)) else {
            continue;
        };
        let mut names: Vec<PathBuf> = rd
            .flatten()
            .map(|e| Path::new(root).join(e.file_name()))
            .collect();
        names.sort();
        for n in names {
            if !copied.iter().any(|c| c.starts_with(&n)) {
                out.push(n);
            }
        }
    }
    out
}

/// Lê um arquivo do HOME real e devolve o conteúdo já com os segredos
/// trocados.
fn processed(real: &Path, rel: &Path) -> Result<(Vec<u8>, Vec<secrets::Replacement>), ImportError> {
    let bytes = fs::read(real).map_err(|e| io(e, real))?;
    match String::from_utf8(bytes) {
        Ok(text) => {
            let name = rel
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (out, reps) = scrub(&text, format_of(&name));
            Ok((out.into_bytes(), reps))
        }
        Err(e) => Ok((e.into_bytes(), Vec::new())),
    }
}

fn write_private(path: &Path, data: &[u8], mode: u32) -> Result<(), ImportError> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
    }
    let tmp = path.with_extension("sandmux-import.tmp");
    fs::write(&tmp, data).map_err(|e| io(e, &tmp))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode & 0o777)).map_err(|e| io(e, &tmp))?;
    fs::rename(&tmp, path).map_err(|e| io(e, path))
}

/// Importa o estado do harness. `overwrite` substitui os arquivos que já
/// existem no HOME sintético.
pub fn import(
    harness: &str,
    real_home: &Path,
    synthetic_home: &Path,
    manifest_path: &Path,
    overwrite: bool,
) -> Result<Report, ImportError> {
    use std::os::unix::fs::PermissionsExt;
    let items = table(harness).ok_or_else(|| ImportError::NotImportable(harness.into()))?;
    let files = files_to_copy(items, real_home);
    let mut report = Report::default();
    let mut touched: BTreeSet<PathBuf> = BTreeSet::new();

    for rel in &files {
        let real = real_home.join(rel);
        let dest = synthetic_home.join(rel);
        let (data, reps) = processed(&real, rel)?;
        let mode = fs::metadata(&real)
            .map(|m| m.permissions().mode())
            .unwrap_or(0o600);
        let exists = dest.symlink_metadata().is_ok();
        if exists && !overwrite {
            let current = fs::read(&dest).unwrap_or_default();
            let same = match (std::str::from_utf8(&current), std::str::from_utf8(&data)) {
                (Ok(a), Ok(b)) => normalize_fakes(a) == normalize_fakes(b),
                _ => current == data,
            };
            if same {
                report.unchanged.push(rel.clone());
            } else {
                report.differing.push(rel.clone());
            }
            continue;
        }
        write_private(&dest, &data, mode)?;
        touched.insert(rel.clone());
        if exists {
            report.overwritten.push(rel.clone());
        } else {
            report.copied.push(rel.clone());
        }
        report
            .replaced
            .extend(reps.into_iter().map(|r| ManifestEntry {
                file: rel.clone(),
                key: r.key,
                line: r.line,
                fake: r.fake,
            }));
    }
    report.ignored = ignored_entries(harness, real_home, &files);

    let mut manifest: Manifest = fs::read(manifest_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| Manifest {
            harness: harness.into(),
            entries: vec![],
        });
    manifest.entries.retain(|e| !touched.contains(&e.file));
    manifest.entries.extend(report.replaced.iter().cloned());
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| ImportError::Io(e.to_string()))?;
    write_private(manifest_path, &json, 0o600)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_KEY: &str = "sk-or-v1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        let synth = d.path().join("synth");
        let manifest = d.path().join("data/manifests/opencode.json");
        let w = |rel: &str, s: &str| {
            let p = real.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, s).unwrap();
        };
        w(
            ".config/opencode/opencode.json",
            r#"{"mcp": {"x": {"type": "local", "environment": {"X_TOKEN": "segredo-do-mcp"}}}}"#,
        );
        w(".config/opencode/skills/revisao/SKILL.md", "# revisão\n");
        w(".config/opencode/plugins/node_modules/dep/index.js", "x");
        w(
            ".config/opencode/herdr-tui-session.js",
            "module.exports = 1\n",
        );
        w(
            ".local/share/opencode/auth.json",
            &format!(r#"{{"openrouter": {{"type": "api", "key": "{REAL_KEY}"}}}}"#),
        );
        w(".local/share/opencode/opencode.db", "SQLite format 3");
        w(".local/share/opencode/log/x.log", "log");
        w(".local/share/opencode/tool-output/a", "saida");
        (d, real, synth, manifest)
    }

    fn all_text(dir: &Path) -> String {
        let mut files = Vec::new();
        walk(dir, dir, &mut files);
        files
            .iter()
            .map(|f| fs::read_to_string(dir.join(f)).unwrap_or_default())
            .collect()
    }

    #[test]
    fn importa_so_a_tabela_e_troca_segredos() {
        let (_d, real, synth, manifest) = fixture();
        let before = all_text(&real);
        let r = import("opencode", &real, &synth, &manifest, false).unwrap();
        assert!(
            synth
                .join(".config/opencode/skills/revisao/SKILL.md")
                .exists()
        );
        assert!(synth.join(".config/opencode/herdr-tui-session.js").exists());
        assert!(!synth.join(".config/opencode/plugins/node_modules").exists());
        assert!(!synth.join(".local/share/opencode/opencode.db").exists());
        assert!(!synth.join(".local/share/opencode/log").exists());
        let text = all_text(&synth);
        assert!(!text.contains(REAL_KEY) && !text.contains("segredo-do-mcp"));
        assert_eq!(r.replaced.len(), 2);
        assert!(
            r.ignored
                .contains(&PathBuf::from(".local/share/opencode/opencode.db"))
        );
        // O HOME real fica idêntico.
        assert_eq!(before, all_text(&real));
        // O manifesto não tem valores reais.
        let m = fs::read_to_string(&manifest).unwrap();
        assert!(!m.contains(REAL_KEY) && !m.contains("segredo-do-mcp"));
        assert!(m.contains("sandmux-fake-"));
    }

    #[test]
    fn reimportacao_copia_so_o_novo_e_lista_diferencas() {
        let (_d, real, synth, manifest) = fixture();
        import("opencode", &real, &synth, &manifest, false).unwrap();
        fs::write(real.join(".config/opencode/skills/nova.md"), "nova").unwrap();
        fs::write(
            real.join(".config/opencode/opencode.json"),
            r#"{"model": "outro"}"#,
        )
        .unwrap();
        fs::write(
            synth.join(".config/opencode/skills/revisao/SKILL.md"),
            "ajuste local",
        )
        .unwrap();
        let r = import("opencode", &real, &synth, &manifest, false).unwrap();
        assert_eq!(
            r.copied,
            vec![PathBuf::from(".config/opencode/skills/nova.md")]
        );
        assert!(
            r.differing
                .contains(&PathBuf::from(".config/opencode/opencode.json"))
        );
        assert!(
            r.differing
                .contains(&PathBuf::from(".config/opencode/skills/revisao/SKILL.md"))
        );
        assert!(
            r.unchanged
                .contains(&PathBuf::from(".local/share/opencode/auth.json"))
        );
        assert_eq!(
            fs::read_to_string(synth.join(".config/opencode/skills/revisao/SKILL.md")).unwrap(),
            "ajuste local"
        );

        let r = import("opencode", &real, &synth, &manifest, true).unwrap();
        assert!(
            r.overwritten
                .contains(&PathBuf::from(".config/opencode/opencode.json"))
        );
        assert_eq!(
            fs::read_to_string(synth.join(".config/opencode/opencode.json")).unwrap(),
            r#"{"model": "outro"}"#
        );
        let m: Manifest = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        assert_eq!(
            m.entries
                .iter()
                .filter(|e| e.file.ends_with("auth.json"))
                .count(),
            1
        );
    }

    #[test]
    fn hermes_importa_conhecimento_e_deixa_historico_de_fora() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        let synth = d.path().join("synth");
        let w = |rel: &str, s: &str| {
            let p = real.join(".hermes").join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, s).unwrap();
        };
        w(
            "config.yaml",
            "model:\n  default: x\nupdates:\n  check: true\n",
        );
        w("SOUL.md", "alma");
        w(
            ".env",
            &format!("OPENROUTER_API_KEY={REAL_KEY}\nLOG_LEVEL=info\n"),
        );
        w(
            "auth.json",
            r#"{"providers": {"nous": {"access_token": "token-real-do-nous-123"}}}"#,
        );
        w("memories/MEMORY.md", "lembra disso");
        w("skills/x/SKILL.md", "skill");
        w("sessions/s1.json", "conversa");
        w("logs/a.log", "log");
        w("cache/x", "c");
        w("gateway.pid", "1");
        w("state.db", "db");
        w("pairing/telegram.json", "pareamento");
        w("cron/jobs.json", "tarefas");
        let r = import("hermes", &real, &synth, &d.path().join("m.json"), false).unwrap();
        for p in [
            "config.yaml",
            "SOUL.md",
            ".env",
            "auth.json",
            "memories/MEMORY.md",
            "skills/x/SKILL.md",
        ] {
            assert!(synth.join(".hermes").join(p).exists(), "{p} não copiado");
        }
        for p in [
            "sessions",
            "logs",
            "cache",
            "gateway.pid",
            "state.db",
            "pairing",
            "cron",
        ] {
            assert!(!synth.join(".hermes").join(p).exists(), "{p} copiado");
            assert!(
                r.ignored.contains(&PathBuf::from(".hermes").join(p)),
                "{p} não listado como ignorado"
            );
        }
        let text = all_text(&synth);
        assert!(!text.contains(REAL_KEY) && !text.contains("token-real-do-nous-123"));
        assert!(text.contains("LOG_LEVEL=info"));
    }

    #[test]
    fn preset_shell_nao_tem_importacao() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(
            import("shell", d.path(), d.path(), &d.path().join("m"), false),
            Err(ImportError::NotImportable(_))
        ));
    }
}
