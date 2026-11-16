//! Importação do estado de um harness para o HOME sintético.
//!
//! A cópia é de mão única, guiada por uma tabela de permissão por harness: o
//! que não está na tabela fica de fora. Segredos são trocados por valores
//! falsos ([`secrets`]); um manifesto, fora do HOME sintético, registra cada
//! troca sem o valor real. Rodada de novo, a importação só copia arquivos que
//! ainda não existem e lista os que diferem.

pub mod agy_login;
pub mod secrets;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::secrets::{format_of, normalize_fakes, scrub};

/// Pastas que nunca são copiadas, em qualquer nível.
const SKIP_DIRS: &[&str] = &["node_modules", "__pycache__", ".venv", "venv", ".cache"];

/// Pastas que ficam de fora, em qualquer nível, das pastas de framework.
const FRAMEWORK_SKIP: &[&str] = &[".git", "cache", "log"];

/// Arquivos que nunca são copiados. O `fish_variables` guarda as variáveis
/// universais do fish, inclusive as exportadas.
const SKIP_FILES: &[&str] = &["fish_variables"];

/// Um item da tabela, relativo ao HOME.
#[derive(Debug, Clone, Copy)]
enum Item {
    File(&'static str),
    Dir(&'static str),
    /// Arquivos do primeiro nível da pasta cujo nome casa com um dos padrões.
    TopFiles(&'static str, &'static [&'static str]),
    /// Pasta de framework (oh-my-zsh, oh-my-bash): copiada sem as pastas de
    /// [`FRAMEWORK_SKIP`].
    Framework(&'static str),
}

/// Nome do shell que o preset `shell` usa, pela regra do preset: o `SHELL`
/// do usuário quando está nas pastas de sistema; senão, `bash`.
pub fn shell_name() -> String {
    let s = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .unwrap_or_default();
    let in_system = ["/usr/", "/bin/"].iter().any(|p| s.starts_with(p));
    if in_system && s.is_file() {
        s.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bash".into())
    } else {
        "bash".into()
    }
}

/// O arquivo principal de inicialização do shell, que recebe a marca no
/// prompt.
fn shell_main_rc(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(".bashrc"),
        "zsh" => Some(".zshrc"),
        "fish" => Some(".config/fish/config.fish"),
        _ => None,
    }
}

/// Trecho no fim do arquivo principal importado: `[sandmux]` na frente do
/// prompt do usuário.
fn prompt_mark(shell: &str) -> &'static str {
    match shell {
        "zsh" => "\n# sandmux: marca o prompt do shell confinado.\nPROMPT=\"[sandmux] $PROMPT\"\n",
        "fish" => {
            "\n# sandmux: marca o prompt do shell confinado.\nif functions -q fish_prompt\n    functions -c fish_prompt __sandmux_user_prompt\n    function fish_prompt\n        echo -n '[sandmux] '\n        __sandmux_user_prompt\n    end\nend\n"
        }
        _ => "\n# sandmux: marca o prompt do shell confinado.\nPS1=\"[sandmux] $PS1\"\n",
    }
}

fn shell_table(shell: &str) -> &'static [Item] {
    const BASH: &[Item] = &[
        Item::File(".bashrc"),
        Item::File(".bash_aliases"),
        Item::File(".bash_profile"),
        Item::File(".bash_login"),
        Item::File(".bash_logout"),
        Item::File(".profile"),
        Item::File(".inputrc"),
        Item::Framework(".oh-my-bash"),
        Item::Framework(".bash_it"),
    ];
    const ZSH: &[Item] = &[
        Item::File(".zshrc"),
        Item::File(".zshenv"),
        Item::File(".zprofile"),
        Item::File(".zlogin"),
        Item::File(".zlogout"),
        Item::File(".p10k.zsh"),
        Item::Framework(".oh-my-zsh"),
    ];
    const FISH: &[Item] = &[
        Item::Framework(".config/fish"),
        Item::Framework(".config/omf"),
        Item::Framework(".local/share/omf"),
    ];
    const OTHER: &[Item] = &[Item::File(".profile")];
    match shell {
        "bash" => BASH,
        "zsh" => ZSH,
        "fish" => FISH,
        _ => OTHER,
    }
}

/// Pastas do estado real de cada harness, para listar o que ficou de fora.
fn state_roots(harness: &str) -> &'static [&'static str] {
    match harness {
        "opencode" => &[".config/opencode", ".local/share/opencode"],
        "hermes" => &[".hermes"],
        "claude" => &[".claude", ".claude.json"],
        "omp" => &[".omp"],
        "agy" => &[".gemini/antigravity-cli", ".gemini/config"],
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
    // Sem `.credentials.json` (o login é feito no sandmux, e o token fica no
    // cofre), sem conversas, histórico e caches.
    const CLAUDE: &[Item] = &[
        Item::File(".claude.json"),
        Item::File(".claude/settings.json"),
        Item::File(".claude/CLAUDE.md"),
        Item::Dir(".claude/agents"),
        Item::Dir(".claude/commands"),
        Item::Dir(".claude/skills"),
        Item::Dir(".claude/hooks"),
        Item::Dir(".claude/plugins"),
        Item::Dir(".claude/output-styles"),
    ];
    // Sem o `agent.db` (credenciais) nem os outros bancos, sessões e caches.
    const OMP: &[Item] = &[
        Item::File(".omp/agent/config.yml"),
        Item::Dir(".omp/agent/extensions"),
        Item::Dir(".omp/agent/agents"),
        Item::Dir(".omp/agent/rules"),
        Item::Dir(".omp/agent/managed-skills"),
        Item::Dir(".omp/agent/skills"),
        Item::Dir(".omp/agent/commands"),
        Item::Dir(".omp/agent/prompts"),
    ];
    // Antigravity CLI: sem o token do login do Google, os segredos de plugins
    // (`~/.config/antigravity`), os projetos (caminhos do host), as conversas,
    // os registros, os caches e os bancos.
    const AGY: &[Item] = &[
        Item::File(".gemini/antigravity-cli/settings.json"),
        Item::File(".gemini/antigravity-cli/keybindings.json"),
        Item::File(".gemini/config/config.json"),
        Item::File(".gemini/config/mcp_config.json"),
        Item::File(".gemini/config/hooks.json"),
        Item::File(".gemini/config/AGENTS.md"),
        Item::File(".gemini/config/GEMINI.md"),
        Item::File(".gemini/config/skills.json"),
        Item::File(".gemini/config/rules.json"),
        Item::File(".gemini/config/plugins.json"),
        Item::File(".gemini/config/agents.json"),
        Item::Dir(".gemini/config/hooks"),
        Item::Dir(".gemini/config/plugins"),
        Item::Dir(".gemini/config/skills"),
        Item::Dir(".gemini/config/agents"),
        Item::Dir(".gemini/config/rules"),
        Item::Dir(".gemini/config/workflows"),
        Item::File(".gemini/GEMINI.md"),
        Item::File(".gemini/AGENTS.md"),
    ];
    match harness {
        "opencode" => Some(OPENCODE),
        "omp" => Some(OMP),
        "agy" => Some(AGY),
        "hermes" => Some(HERMES),
        "claude" => Some(CLAUDE),
        "shell" => Some(shell_table(&shell_name())),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    NotImportable(String),
    Io(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotImportable(h) => f.write_str(&sandmux_spec::trf!(
                "o preset {h} não tem importação",
                "the preset {h} has no import"
            )),
            ImportError::Io(m) => f.write_str(m),
        }
    }
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
    /// Portas de serviços locais (`127.0.0.1`, `localhost`, `[::1]`) citadas
    /// na configuração do harness: no sandbox, o `127.0.0.1` é o dele.
    pub local_ports: Vec<u16>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.copied.is_empty()
            && self.overwritten.is_empty()
            && self.unchanged.is_empty()
            && self.differing.is_empty()
    }
}

/// Arquivos de configuração em que URLs locais importam.
const CONFIG_FILES: &[&str] = &[
    "opencode.json",
    "opencode.jsonc",
    "config.yaml",
    "config.yml",
];

/// As portas citadas com `127.0.0.1:`, `localhost:` ou `[::1]:`.
pub fn local_ports(text: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for host in ["127.0.0.1:", "localhost:", "[::1]:"] {
        let mut rest = text;
        while let Some(i) = rest.find(host) {
            rest = &rest[i + host.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = digits.parse::<u16>()
                && p > 0
                && !out.contains(&p)
            {
                out.push(p);
            }
        }
    }
    out.sort_unstable();
    out
}

/// Um arquivo do HOME sintético com credenciais reais. Sem `Debug`: o texto
/// original tem os valores reais.
pub struct RealSecrets {
    /// Caminho relativo ao HOME sintético.
    pub file: PathBuf,
    /// O texto original e o texto com os falsos no lugar.
    pub original: String,
    pub scrubbed: String,
    pub found: Vec<secrets::Replacement>,
}

/// As credenciais reais nos arquivos que a importação do harness pega,
/// procuradas dentro do HOME sintético (gravadas pelo próprio harness, como
/// o "Connect provider" do opencode). Nada é gravado.
pub fn find_real_secrets(harness: &str, synthetic_home: &Path) -> Vec<RealSecrets> {
    let Some(items) = table(harness) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rel in files_to_copy(items, synthetic_home) {
        let Ok(text) = fs::read_to_string(synthetic_home.join(&rel)) else {
            continue;
        };
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Só arquivos de configuração: os textos (skills, memórias em
        // Markdown) trazem tokens de exemplo, que não são credenciais.
        let format = format_of(&name);
        if format == secrets::Format::Text {
            continue;
        }
        let (scrubbed, found) = scrub(&text, format);
        if !found.is_empty() {
            out.push(RealSecrets {
                file: rel,
                original: text,
                scrubbed,
                found,
            });
        }
    }
    out
}

/// Harnesses cujos HOMEs sintéticos o `sandmux doctor` e o vigia de chaves
/// do daemon varrem atrás de chaves reais.
pub const SCANNED: &[&str] = &["opencode", "hermes"];

/// As pastas do HOME sintético em que um arquivo que [`find_real_secrets`]
/// lê pode ser gravado, para o vigia de chaves observar. Inclui as que ainda
/// não existem: o vigia observa a mais funda que existe.
pub fn watch_dirs(harness: &str, synthetic_home: &Path) -> Vec<PathBuf> {
    let Some(items) = table(harness) else {
        return Vec::new();
    };
    let mut dirs = BTreeSet::new();
    for item in items {
        match *item {
            Item::File(rel) => {
                if let Some(parent) = Path::new(rel).parent() {
                    dirs.insert(synthetic_home.join(parent));
                }
            }
            Item::TopFiles(rel, _) => {
                dirs.insert(synthetic_home.join(rel));
            }
            Item::Dir(_) | Item::Framework(_) => {}
        }
    }
    for rel in files_to_copy(items, synthetic_home) {
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if format_of(&name) != secrets::Format::Text
            && let Some(parent) = rel.parent()
        {
            dirs.insert(synthetic_home.join(parent));
        }
    }
    dirs.into_iter().collect()
}

/// Grava o arquivo com os falsos (mesmo modo) e anota as trocas no
/// manifesto do harness.
pub fn apply_fakes(
    harness: &str,
    synthetic_home: &Path,
    manifest_path: &Path,
    r: &RealSecrets,
) -> Result<(), ImportError> {
    use std::os::unix::fs::PermissionsExt;
    let path = synthetic_home.join(&r.file);
    let mode = fs::metadata(&path)
        .map(|m| m.permissions().mode())
        .unwrap_or(0o600);
    write_private(&path, r.scrubbed.as_bytes(), mode)?;
    let mut manifest: Manifest = fs::read(manifest_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| Manifest {
            harness: harness.into(),
            entries: vec![],
        });
    manifest
        .entries
        .extend(r.found.iter().map(|x| ManifestEntry {
            file: r.file.clone(),
            key: x.key.clone(),
            line: x.line,
            fake: x.fake.clone(),
        }));
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| ImportError::Io(e.to_string()))?;
    write_private(manifest_path, &json, 0o600)
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

fn walk(dir: &Path, base: &Path, skip: &[&str], out: &mut Vec<PathBuf>) {
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
            let name = e.file_name();
            let name = name.to_string_lossy();
            if SKIP_DIRS.contains(&name.as_ref()) || skip.contains(&name.as_ref()) {
                continue;
            }
            walk(&p, base, skip, out);
        } else if ft.is_file()
            && !SKIP_FILES.contains(&e.file_name().to_string_lossy().as_ref())
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
            Item::Dir(rel) | Item::Framework(rel) => {
                let p = home.join(rel);
                let skip = match *item {
                    Item::Framework(_) => FRAMEWORK_SKIP,
                    _ => &[],
                };
                if p.is_dir() && !p.is_symlink() {
                    walk(&p, home, skip, &mut out);
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

pub(crate) fn write_private(path: &Path, data: &[u8], mode: u32) -> Result<(), ImportError> {
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
    let shell = (harness == "shell").then(shell_name);
    let main_rc = shell.as_deref().and_then(shell_main_rc);
    let mut report = Report::default();
    let mut touched: BTreeSet<PathBuf> = BTreeSet::new();

    for rel in &files {
        let real = real_home.join(rel);
        let dest = synthetic_home.join(rel);
        let (mut data, reps) = processed(&real, rel)?;
        if let (Some(sh), Some(main)) = (shell.as_deref(), main_rc)
            && rel == Path::new(main)
        {
            data.extend_from_slice(prompt_mark(sh).as_bytes());
        }
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
    for rel in &files {
        let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !CONFIG_FILES.contains(&name) {
            continue;
        }
        if let Ok(t) = fs::read_to_string(real_home.join(rel)) {
            for p in local_ports(&t) {
                if !report.local_ports.contains(&p) {
                    report.local_ports.push(p);
                }
            }
        }
    }
    report.local_ports.sort_unstable();

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
        walk(dir, dir, &[], &mut files);
        files
            .iter()
            .map(|f| fs::read_to_string(dir.join(f)).unwrap_or_default())
            .collect()
    }

    #[test]
    fn chaves_reais_no_home_sintetico() {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("home");
        let auth = home.join(".local/share/opencode/auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        fs::write(
            &auth,
            "{\n  \"lmstudio\": {\"type\": \"api\", \"key\": \"sandmux-fake-0123456789abcdef\"},\n  \"openrouter\": {\n    \"type\": \"api\",\n    \"key\": \"sk-or-v1-realkey123456\"\n  }\n}\n",
        )
        .unwrap();
        let found = find_real_secrets("opencode", &home);
        assert_eq!(found.len(), 1);
        let r = &found[0];
        assert_eq!(r.file, PathBuf::from(".local/share/opencode/auth.json"));
        assert_eq!(r.found.len(), 1);
        assert_eq!(r.found[0].real, "sk-or-v1-realkey123456");
        assert_eq!(r.found[0].line, 5);
        assert!(!format!("{:?}", r.found).contains("realkey"));
        assert!(!r.scrubbed.contains("realkey"));
        let m = d.path().join("manifest.json");
        apply_fakes("opencode", &home, &m, r).unwrap();
        let after = fs::read_to_string(&auth).unwrap();
        assert!(after.contains(&r.found[0].fake) && !after.contains("realkey"));
        let manifest = fs::read_to_string(&m).unwrap();
        assert!(manifest.contains(&r.found[0].fake) && !manifest.contains("realkey"));
        assert!(find_real_secrets("opencode", &home).is_empty());
        // Texto de skill com token de exemplo: fora da busca.
        let skill = home.join(".config/opencode/skills/x/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(
            &skill,
            "use ghp_1234567890abcdefghijABCDEFGHIJ123456 como exemplo\n",
        )
        .unwrap();
        assert!(find_real_secrets("opencode", &home).is_empty());
    }

    #[test]
    fn portas_locais_citadas() {
        let t = r#"{"provider":{"lmstudio":{"options":{"baseURL":"http://127.0.0.1:8888/v1"}}},
            "mcp":{"x":{"url":"http://localhost:9000/mcp"},"y":{"url":"http://[::1]:7000"}},
            "z":"127.0.0.1:abc", "w":"https://openrouter.ai:443"}"#;
        assert_eq!(local_ports(t), vec![7000, 8888, 9000]);
        assert!(local_ports("nada local").is_empty());
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
    fn preset_sem_importacao() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(
            import("chat", d.path(), d.path(), &d.path().join("m"), false),
            Err(ImportError::NotImportable(_))
        ));
        assert!(importable("shell"));
    }

    fn write(home: &Path, rel: &str, s: &str) {
        let p = home.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    fn rels(v: Vec<PathBuf>) -> Vec<String> {
        v.into_iter().map(|p| p.display().to_string()).collect()
    }

    #[test]
    fn tabela_do_shell_so_com_o_shell_em_uso() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        write(h, ".bashrc", "alias ll='ls -l'\n");
        write(h, ".profile", "PATH=$HOME/bin:$PATH\n");
        write(h, ".zshrc", "alias zz=1\n");
        write(h, ".config/starship.toml", "format = '$all'\n");
        write(h, ".oh-my-bash/oh-my-bash.sh", "# omb\n");
        write(h, ".oh-my-bash/themes/font/font.theme.sh", "# tema\n");
        write(h, ".oh-my-bash/.git/HEAD", "ref\n");
        write(h, ".oh-my-bash/cache/x", "x\n");
        write(h, ".oh-my-bash/log/update.log", "x\n");
        let bash = rels(files_to_copy(shell_table("bash"), h));
        assert_eq!(
            bash,
            [
                ".bashrc",
                ".profile",
                ".oh-my-bash/oh-my-bash.sh",
                ".oh-my-bash/themes/font/font.theme.sh",
            ]
        );
        write(h, ".oh-my-zsh/oh-my-zsh.sh", "# omz\n");
        write(h, ".oh-my-zsh/.git/HEAD", "ref\n");
        let zsh = rels(files_to_copy(shell_table("zsh"), h));
        assert_eq!(zsh, [".zshrc", ".oh-my-zsh/oh-my-zsh.sh"]);
        write(h, ".config/fish/config.fish", "alias g git\n");
        write(h, ".config/fish/functions/x.fish", "function x; end\n");
        write(h, ".config/fish/fish_variables", "SETUVAR --export K:v\n");
        let fish = rels(files_to_copy(shell_table("fish"), h));
        assert_eq!(
            fish,
            [".config/fish/config.fish", ".config/fish/functions/x.fish"]
        );
        assert_eq!(rels(files_to_copy(shell_table("dash"), h)), [".profile"]);
    }

    #[test]
    fn rc_do_shell_tem_segredos_trocados_e_a_marca() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path();
        write(
            h,
            ".bashrc",
            "export GITHUB_TOKEN=segredo-do-bashrc-123\nAPI_KEY='outro-segredo-456'\nexport EDITOR=vim\nexport T=$(pass show x)\n",
        );
        write(
            h,
            ".config/fish/config.fish",
            "set -gx OPENAI_API_KEY segredo-do-fish-789\nset -gx EDITOR vim\n",
        );
        let (out, reps) = processed(&h.join(".bashrc"), Path::new(".bashrc")).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(reps.len(), 2, "{reps:?}");
        assert!(!out.contains("segredo-do-bashrc-123") && !out.contains("outro-segredo-456"));
        assert!(out.contains("export GITHUB_TOKEN=sandmux-fake-"), "{out}");
        assert!(out.contains("API_KEY='sandmux-fake-"), "{out}");
        assert!(
            out.contains("export EDITOR=vim\nexport T=$(pass show x)\n"),
            "{out}"
        );
        let rel = Path::new(".config/fish/config.fish");
        let (out, reps) = processed(&h.join(rel), rel).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(reps.len(), 1, "{reps:?}");
        assert!(
            out.starts_with("set -gx OPENAI_API_KEY sandmux-fake-"),
            "{out}"
        );
        assert!(out.contains("set -gx EDITOR vim"), "{out}");
        assert_eq!(shell_main_rc("bash"), Some(".bashrc"));
        assert!(prompt_mark("bash").contains("PS1=\"[sandmux] $PS1\""));
        assert!(prompt_mark("zsh").contains("PROMPT=\"[sandmux] $PROMPT\""));
        assert!(prompt_mark("fish").contains("functions -c fish_prompt"));
    }
}
