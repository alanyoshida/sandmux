//! Arquivo de configuração (`config.toml`) e camadas de política.
//!
//! O arquivo guarda só a diferença em relação às camadas embutidas, em três
//! tipos de seção: `[global]`, `[profile.<nome>]` e `[project."<pasta>"]`.
//! [`resolve`] combina baseline, preset, global, perfil nomeado, perfil do
//! projeto e opções do `run` numa política efetiva, guardando a origem de
//! cada valor.

pub mod edit;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use toml_edit::{Document, Item, Value};

use crate::compose::{LayerMode, RunOptions};
use crate::host::Host;
use crate::limits::{self, Limits, MemoryLimit, ProcessLimit};
use crate::netcatalog;
use sandmux_spec::net::{NetPolicy, Pattern, Rule, RuleKind};
use sandmux_spec::window::WindowMode;
use sandmux_spec::{tr, trf};

/// Nome do arquivo dentro da pasta de configuração.
pub const FILE: &str = "config.toml";

/// Chaves de lista, na ordem em que aparecem nas mensagens.
pub const LIST_KEYS: &[&str] = &["ro", "rw", "pass-env"];
/// Chaves de liga e desliga.
pub const BOOL_KEYS: &[&str] = &["repo", "kvm", "gpu", "clipboard", "harness-asks", "portal"];
/// Chaves de teto.
pub const LIMIT_KEYS: &[&str] = &["memory", "processes"];
/// Chaves da rede: o modo, em qualquer seção; a retenção do registro, só em
/// `[global]`.
pub const NET_KEYS: &[&str] = &["network"];
/// Janelas e áudio do desktop: `true`, `false` ou `"direct"`.
pub const WINDOW_KEY: &str = "window";
/// Listas da política de rede: padrões de destino, e os grupos do catálogo
/// comum em `catalog`.
pub const NET_LIST_KEYS: &[&str] = &[
    "deny",
    "allow",
    "allow-off",
    "catalog",
    "catalog-off",
    "decrypt",
];
pub const GLOBAL_ONLY_KEYS: &[&str] = &[
    "net-log-days",
    "net-log-max",
    "language",
    "notify",
    "screen-history",
];
/// Chaves SSH do cofre que a sessão pode usar (`"*"` é todas).
pub const SSH_KEYS: &str = "ssh-keys";
/// Conexões de banco do cofre que a sessão pode usar (`"*"` é todas).
pub const DB_KEY: &str = "db";
/// Portas de entrada publicadas (`8642` ou `8443@0.0.0.0`).
pub const PORTS_KEY: &str = "ports";
/// Portas do host levadas ao `127.0.0.1` do sandbox.
pub const HOST_PORTS_KEY: &str = "host-ports";

/// Listas de servidores MCP (nomes).
pub const MCP_LIST_KEYS: &[&str] = &["mcp-deny", "mcp-allow"];

/// Listas da política de arquivos (padrões de arquivo).
pub const FILE_LIST_KEYS: &[&str] = &[
    "file-deny",
    "file-allow",
    "secret-files",
    "secret-files-off",
];

/// Confere um padrão de arquivo (o `~/` vale como o HOME de quem lança).
pub fn valid_file_pattern(s: &str) -> Result<(), String> {
    sandmux_spec::files::FilePattern::parse(s, Some(std::path::Path::new("/h"))).map(|_| ())
}

/// Um nome de chave SSH aceito em `ssh-keys`.
pub fn valid_ssh_key_name(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Modo de rede do sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    Aberta,
    Inferencia,
    Custom,
    SemRede,
}

impl NetworkMode {
    pub const ALL: [NetworkMode; 4] = [
        NetworkMode::Aberta,
        NetworkMode::Inferencia,
        NetworkMode::Custom,
        NetworkMode::SemRede,
    ];

    pub fn parse(s: &str) -> Result<NetworkMode, String> {
        match s.trim() {
            "aberta" => Ok(NetworkMode::Aberta),
            "inferencia" => Ok(NetworkMode::Inferencia),
            "custom" => Ok(NetworkMode::Custom),
            "sem-rede" => Ok(NetworkMode::SemRede),
            other => Err(trf!(
                "modo de rede inválido: {other:?}; use \"aberta\", \"inferencia\", \"custom\" ou \"sem-rede\"",
                "invalid network mode: {other:?}; use \"aberta\", \"inferencia\", \"custom\" or \"sem-rede\""
            )),
        }
    }

    /// O modo do gateway; `None` sem rede.
    pub fn policy_mode(self) -> Option<sandmux_spec::net::PolicyMode> {
        use sandmux_spec::net::PolicyMode;
        match self {
            NetworkMode::Aberta => Some(PolicyMode::Open),
            NetworkMode::Inferencia => Some(PolicyMode::Inference),
            NetworkMode::Custom => Some(PolicyMode::Custom),
            NetworkMode::SemRede => None,
        }
    }

    /// Descrição curta, para o resumo da política.
    pub fn describe(self) -> &'static str {
        match self {
            NetworkMode::Aberta => tr!("aberta, registrada", "open, logged"),
            NetworkMode::Inferencia => tr!("só inferência, registrada", "inference only, logged"),
            NetworkMode::Custom => tr!(
                "custom (allowlist), registrada",
                "custom (allowlist), logged"
            ),
            NetworkMode::SemRede => tr!("sem rede", "no network"),
        }
    }
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NetworkMode::Aberta => "aberta",
            NetworkMode::Inferencia => "inferencia",
            NetworkMode::Custom => "custom",
            NetworkMode::SemRede => "sem-rede",
        })
    }
}

/// Caminho do arquivo de configuração do host.
pub fn path(host: &Host) -> PathBuf {
    host.config_dir.join(FILE)
}

/// Erro de leitura ou de validação, com a linha quando se sabe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => f.write_str(&trf!(
                "configuração inválida em {}, linha {l}: {}",
                "invalid configuration in {}, line {l}: {}",
                self.path.display(),
                self.message
            )),
            None => f.write_str(&trf!(
                "configuração inválida em {}: {}",
                "invalid configuration in {}: {}",
                self.path.display(),
                self.message
            )),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Uma camada: o que uma seção (ou as opções do `run`) define.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub pass_env: Vec<String>,
    pub repo: Option<bool>,
    pub kvm: Option<bool>,
    pub gpu: Option<bool>,
    pub clipboard: Option<bool>,
    /// As perguntas de permissão do harness (`harness-asks`).
    pub harness_asks: Option<bool>,
    /// O portal do desktop (`portal`).
    pub portal: Option<bool>,
    pub memory: Option<MemoryLimit>,
    pub processes: Option<ProcessLimit>,
    pub network: Option<NetworkMode>,
    pub window: Option<WindowMode>,
    pub deny: Vec<Pattern>,
    pub allow: Vec<Pattern>,
    pub allow_off: Vec<Pattern>,
    pub catalog: Vec<String>,
    pub catalog_off: Vec<Pattern>,
    /// Domínios cujo TLS é decifrado.
    pub decrypt: Vec<Pattern>,
    /// Chaves SSH do cofre que a sessão pode usar.
    pub ssh_keys: Vec<String>,
    /// Conexões de banco do cofre que a sessão pode usar.
    pub db: Vec<String>,
    /// Portas de entrada publicadas.
    pub ports: Vec<sandmux_spec::net::PortRule>,
    /// Portas do host (`host-ports`).
    pub host_ports: Vec<u16>,
    /// Política de arquivos: `file-deny`, `file-allow`, `secret-files` e
    /// `secret-files-off`, como escritos.
    pub file_deny: Vec<String>,
    pub file_allow: Vec<String>,
    pub secret_files: Vec<String>,
    pub secret_files_off: Vec<String>,
    /// Servidores MCP desabilitados e, quando não vazia, os únicos
    /// habilitados.
    pub mcp_deny: Vec<String>,
    pub mcp_allow: Vec<String>,
}

impl Layer {
    /// Uma lista da política de arquivos, pelo nome da chave.
    pub fn file_list_mut(&mut self, key: &str) -> Option<&mut Vec<String>> {
        match key {
            "file-deny" => Some(&mut self.file_deny),
            "file-allow" => Some(&mut self.file_allow),
            "secret-files" => Some(&mut self.secret_files),
            "secret-files-off" => Some(&mut self.secret_files_off),
            _ => None,
        }
    }

    /// Uma lista de padrões da rede, pelo nome da chave.
    pub fn patterns_mut(&mut self, key: &str) -> Option<&mut Vec<Pattern>> {
        match key {
            "deny" => Some(&mut self.deny),
            "decrypt" => Some(&mut self.decrypt),
            "allow" => Some(&mut self.allow),
            "allow-off" => Some(&mut self.allow_off),
            "catalog-off" => Some(&mut self.catalog_off),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Layer::default()
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        match key {
            "repo" => self.repo,
            "kvm" => self.kvm,
            "gpu" => self.gpu,
            "clipboard" => self.clipboard,
            "harness-asks" => self.harness_asks,
            "portal" => self.portal,
            _ => None,
        }
    }

    fn bool_mut(&mut self, key: &str) -> Option<&mut Option<bool>> {
        match key {
            "repo" => Some(&mut self.repo),
            "kvm" => Some(&mut self.kvm),
            "gpu" => Some(&mut self.gpu),
            "clipboard" => Some(&mut self.clipboard),
            "harness-asks" => Some(&mut self.harness_asks),
            "portal" => Some(&mut self.portal),
            _ => None,
        }
    }

    /// As chaves definidas, no formato do TOML, para listagens.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        let list = |v: &[String]| {
            v.iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let paths = |v: &[PathBuf]| {
            v.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        if !self.ro.is_empty() {
            out.push(format!("ro = [{}]", list(&paths(&self.ro))));
        }
        if !self.rw.is_empty() {
            out.push(format!("rw = [{}]", list(&paths(&self.rw))));
        }
        if !self.pass_env.is_empty() {
            out.push(format!("pass-env = [{}]", list(&self.pass_env)));
        }
        for k in BOOL_KEYS {
            if let Some(v) = self.get_bool(k) {
                out.push(format!("{k} = {v}"));
            }
        }
        if let Some(m) = self.memory {
            out.push(format!("memory = \"{m}\""));
        }
        if let Some(n) = self.network {
            out.push(format!("network = \"{n}\""));
        }
        if let Some(w) = self.window {
            out.push(format!("window = {}", w.toml()));
        }
        let pats = |v: &[Pattern]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        for (k, v) in [
            ("deny", &self.deny),
            ("allow", &self.allow),
            ("allow-off", &self.allow_off),
            ("catalog-off", &self.catalog_off),
            ("decrypt", &self.decrypt),
        ] {
            if !v.is_empty() {
                out.push(format!("{k} = [{}]", list(&pats(v))));
            }
        }
        if !self.catalog.is_empty() {
            out.push(format!("catalog = [{}]", list(&self.catalog)));
        }
        if !self.ssh_keys.is_empty() {
            out.push(format!("ssh-keys = [{}]", list(&self.ssh_keys)));
        }
        if !self.db.is_empty() {
            out.push(format!("db = [{}]", list(&self.db)));
        }
        if !self.ports.is_empty() {
            let v: Vec<String> = self.ports.iter().map(|p| p.to_string()).collect();
            out.push(format!("ports = [{}]", list(&v)));
        }
        if !self.host_ports.is_empty() {
            let v: Vec<String> = self.host_ports.iter().map(|p| p.to_string()).collect();
            out.push(format!("host-ports = [{}]", v.join(", ")));
        }
        for (k, v) in [
            ("file-deny", &self.file_deny),
            ("file-allow", &self.file_allow),
            ("secret-files", &self.secret_files),
            ("secret-files-off", &self.secret_files_off),
            ("mcp-deny", &self.mcp_deny),
            ("mcp-allow", &self.mcp_allow),
        ] {
            if !v.is_empty() {
                out.push(format!("{k} = [{}]", list(v)));
            }
        }
        match self.processes {
            Some(ProcessLimit::Count(n)) => out.push(format!("processes = {n}")),
            Some(ProcessLimit::Max) => out.push("processes = \"max\"".into()),
            None => {}
        }
        out
    }
}

/// Ações da TUI com tecla configurável, na ordem da ajuda, com a tecla
/// padrão.
pub const TUI_ACTIONS: &[(&str, char)] = &[
    ("new", 'c'),
    ("next", 'n'),
    ("prev", 'p'),
    ("list", 'w'),
    ("rename", ','),
    ("restart", 'r'),
    ("close", 'x'),
    ("paste-image", 'v'),
    ("detach", 'd'),
    ("help", '?'),
    ("panel", 's'),
    ("network", 'a'),
    ("files", 'f'),
    ("folders", 'o'),
    ("mcp", 'm'),
    ("chat", 'i'),
    ("command", 'g'),
    ("menu", ':'),
    ("settings", 'e'),
    ("doctor", 'D'),
    ("enter", 't'),
    ("group-prev", '('),
    ("group-next", ')'),
    ("groups", 'G'),
    ("split-side", '%'),
    ("split-stack", '"'),
    ("zoom", 'z'),
    ("pane-grow", '>'),
    ("pane-shrink", '<'),
];

/// Uma cor do tema da TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    /// A cor do terminal.
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

const ANSI_NAMES: &[&str] = &[
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright-black",
    "bright-red",
    "bright-green",
    "bright-yellow",
    "bright-blue",
    "bright-magenta",
    "bright-cyan",
    "bright-white",
];

/// Formatos de cor aceitos, para mensagens.
pub fn color_formats() -> &'static str {
    tr!(
        "\"#rrggbb\", um índice de 0 a 255, um dos 16 nomes ANSI (\"red\", \"bright-blue\"…) ou \"default\"",
        "\"#rrggbb\", an index from 0 to 255, one of the 16 ANSI names (\"red\", \"bright-blue\"…) or \"default\""
    )
}

impl ThemeColor {
    /// O texto que [`ThemeColor::parse`] lê de volta.
    pub fn to_text(&self) -> String {
        match self {
            ThemeColor::Default => "default".into(),
            ThemeColor::Indexed(i) => i.to_string(),
            ThemeColor::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }

    pub fn parse(s: &str) -> Option<ThemeColor> {
        let s = s.trim();
        if s == "default" {
            return Some(ThemeColor::Default);
        }
        if let Some(h) = s.strip_prefix('#') {
            if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
            return Some(ThemeColor::Rgb(v(0)?, v(2)?, v(4)?));
        }
        if let Ok(n) = s.parse::<u8>() {
            return Some(ThemeColor::Indexed(n));
        }
        ANSI_NAMES
            .iter()
            .position(|n| *n == s)
            .map(|i| ThemeColor::Indexed(i as u8))
    }
}

/// Elementos com cor no tema, na ordem da documentação.
pub const THEME_ELEMENTS: &[&str] = &[
    "tab-active-fg",
    "tab-active-bg",
    "tab-fg",
    "tab-bg",
    "bar-bg",
    "status-fg",
    "note-fg",
    "note-bg",
    "modal-border",
    "modal-title",
    "selected-fg",
    "selected-bg",
    "error-fg",
    "selection-fg",
    "selection-bg",
    "tab-index-fg",
    "tab-index-bg",
    "tab-active-index-fg",
    "tab-active-index-bg",
    "chip-fg",
    "chip-harness-bg",
    "chip-folder-bg",
    "chip-group-bg",
    "chip-vault-bg",
    "chip-alert-bg",
    "chip-time-bg",
    "accent",
    "muted-fg",
    "ok-fg",
    "page-bg",
    "page-fg",
    "surface-bg",
];

/// Seções do painel lateral, na ordem padrão (nomes de `panel-order` e
/// `panel-collapsed`).
pub const PANEL_SECTIONS: &[&str] = &["protecao", "recursos", "portas", "rede", "mcp"];

/// Estilo da barra de abas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// Blocos coloridos, sem símbolos especiais: funciona em qualquer fonte.
    #[default]
    Blocos,
    /// Blocos e separadores do Powerline, com ícones do Nerd Font.
    Powerline,
    /// Texto simples, sem símbolos especiais.
    Simples,
}

/// Temas embutidos.
pub const THEMES: &[&str] = &[
    "padrao",
    "escuro",
    "claro",
    "catppuccin",
    "rose-pine",
    "tokyo-night",
    "kanagawa",
    "gruvbox",
    "dracula",
];

/// Paleta base de um tema pronto, de onde saem as cores dos elementos.
struct Palette {
    /// Fundo da barra e da página.
    bg: &'static str,
    /// Fundo das superfícies: nomes das abas, lateral da página.
    surface: &'static str,
    /// Fundo do número das abas de fundo.
    overlay: &'static str,
    /// Texto sobre `overlay`.
    on_overlay: &'static str,
    fg: &'static str,
    muted: &'static str,
    accent: &'static str,
    /// Segundo destaque: o bloco do harness.
    accent2: &'static str,
    red: &'static str,
    green: &'static str,
    yellow: &'static str,
    cyan: &'static str,
}

impl Palette {
    fn pairs(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("bar-bg", self.bg),
            ("tab-active-fg", self.bg),
            ("tab-active-bg", self.accent),
            ("tab-fg", self.muted),
            ("tab-bg", self.surface),
            ("status-fg", self.yellow),
            ("note-fg", self.bg),
            ("note-bg", self.red),
            ("modal-border", self.accent),
            ("modal-title", self.accent),
            ("selected-fg", self.bg),
            ("selected-bg", self.accent),
            ("error-fg", self.red),
            ("selection-fg", self.bg),
            ("selection-bg", self.yellow),
            ("tab-index-fg", self.on_overlay),
            ("tab-index-bg", self.overlay),
            ("tab-active-index-fg", self.bg),
            ("tab-active-index-bg", self.accent),
            ("chip-fg", self.bg),
            ("chip-harness-bg", self.accent2),
            ("chip-folder-bg", self.accent),
            ("chip-group-bg", self.muted),
            ("chip-vault-bg", self.green),
            ("chip-alert-bg", self.yellow),
            ("chip-time-bg", self.cyan),
            ("accent", self.accent),
            ("muted-fg", self.muted),
            ("ok-fg", self.green),
            ("page-bg", self.bg),
            ("page-fg", self.fg),
            ("surface-bg", self.surface),
        ]
    }
}

/// As paletas dos temas prontos, das versões públicas de cada um.
fn palette(name: &str) -> Option<Palette> {
    let p = match name {
        // Catppuccin Mocha com azul.
        "escuro" => Palette {
            bg: "#1e1e2e",
            surface: "#313244",
            overlay: "#45475a",
            on_overlay: "#cdd6f4",
            fg: "#cdd6f4",
            muted: "#a6adc8",
            accent: "#89b4fa",
            accent2: "#f38ba8",
            red: "#f38ba8",
            green: "#a6e3a1",
            yellow: "#f9e2af",
            cyan: "#89dceb",
        },
        // Catppuccin Latte.
        "claro" => Palette {
            bg: "#eff1f5",
            surface: "#ccd0da",
            overlay: "#9ca0b0",
            on_overlay: "#eff1f5",
            fg: "#4c4f69",
            muted: "#5c5f77",
            accent: "#1e66f5",
            accent2: "#d20f39",
            red: "#d20f39",
            green: "#40a02b",
            yellow: "#df8e1d",
            cyan: "#04a5e5",
        },
        // Catppuccin Mocha com mauve.
        "catppuccin" => Palette {
            bg: "#1e1e2e",
            surface: "#313244",
            overlay: "#45475a",
            on_overlay: "#cdd6f4",
            fg: "#cdd6f4",
            muted: "#a6adc8",
            accent: "#cba6f7",
            accent2: "#f5c2e7",
            red: "#f38ba8",
            green: "#a6e3a1",
            yellow: "#f9e2af",
            cyan: "#89dceb",
        },
        // Rosé Pine (principal).
        "rose-pine" => Palette {
            bg: "#191724",
            surface: "#26233a",
            overlay: "#403d52",
            on_overlay: "#e0def4",
            fg: "#e0def4",
            muted: "#908caa",
            accent: "#c4a7e7",
            accent2: "#ebbcba",
            red: "#eb6f92",
            green: "#9ccfd8",
            yellow: "#f6c177",
            cyan: "#31748f",
        },
        // Tokyo Night (Night).
        "tokyo-night" => Palette {
            bg: "#1a1b26",
            surface: "#292e42",
            overlay: "#414868",
            on_overlay: "#c0caf5",
            fg: "#c0caf5",
            muted: "#a9b1d6",
            accent: "#7aa2f7",
            accent2: "#bb9af7",
            red: "#f7768e",
            green: "#9ece6a",
            yellow: "#e0af68",
            cyan: "#7dcfff",
        },
        // Kanagawa (Wave).
        "kanagawa" => Palette {
            bg: "#1f1f28",
            surface: "#2a2a37",
            overlay: "#54546d",
            on_overlay: "#dcd7ba",
            fg: "#dcd7ba",
            muted: "#c8c093",
            accent: "#7e9cd8",
            accent2: "#d27e99",
            red: "#e46876",
            green: "#98bb6c",
            yellow: "#e6c384",
            cyan: "#7fb4ca",
        },
        // Gruvbox (escuro).
        "gruvbox" => Palette {
            bg: "#282828",
            surface: "#3c3836",
            overlay: "#504945",
            on_overlay: "#ebdbb2",
            fg: "#ebdbb2",
            muted: "#a89984",
            accent: "#fe8019",
            accent2: "#d3869b",
            red: "#fb4934",
            green: "#b8bb26",
            yellow: "#fabd2f",
            cyan: "#8ec07c",
        },
        "dracula" => Palette {
            bg: "#282a36",
            surface: "#44475a",
            overlay: "#6272a4",
            on_overlay: "#f8f8f2",
            fg: "#f8f8f2",
            muted: "#bfbfbf",
            accent: "#bd93f9",
            accent2: "#ff79c6",
            red: "#ff5555",
            green: "#50fa7b",
            yellow: "#f1fa8c",
            cyan: "#8be9fd",
        },
        _ => return None,
    };
    Some(p)
}

/// Tema resolvido: cor por elemento (sem cor é o estilo do tema `padrao`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Theme {
    pub name: String,
    pub colors: BTreeMap<String, ThemeColor>,
}

impl Theme {
    pub fn builtin(name: &str) -> Option<Theme> {
        let pairs = match name {
            "padrao" => Vec::new(),
            _ => palette(name)?.pairs(),
        };
        Some(Theme {
            name: name.into(),
            colors: pairs
                .iter()
                .filter_map(|(k, v)| ThemeColor::parse(v).map(|c| (k.to_string(), c)))
                .collect(),
        })
    }

    pub fn get(&self, element: &str) -> Option<ThemeColor> {
        self.colors.get(element).copied()
    }
}

/// Tecla de prefixo, atalhos e tema da TUI (`[tui]`). Não é camada de
/// política.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiConfig {
    /// Letra do prefixo, com Ctrl (`C-b`).
    pub prefix: char,
    pub keys: BTreeMap<String, char>,
    pub theme: Theme,
    /// Painel lateral visível ao abrir.
    pub panel: bool,
    /// Estilo da barra (`style`).
    pub style: BarStyle,
    /// Barra na primeira linha (`bar = "top"`); por padrão, na última.
    pub bar_top: bool,
    /// Ordem das seções do painel (`panel-order`), sempre com todas.
    pub panel_order: Vec<String>,
    /// Seções do painel recolhidas (`panel-collapsed`).
    pub panel_collapsed: Vec<String>,
}

impl Default for TuiConfig {
    fn default() -> Self {
        TuiConfig {
            prefix: 'b',
            keys: TUI_ACTIONS
                .iter()
                .map(|(a, k)| (a.to_string(), *k))
                .collect(),
            theme: Theme::builtin("padrao").unwrap_or_default(),
            panel: true,
            style: BarStyle::Blocos,
            bar_top: false,
            panel_order: PANEL_SECTIONS.iter().map(|s| s.to_string()).collect(),
            panel_collapsed: Vec::new(),
        }
    }
}

impl TuiConfig {
    /// A ação ligada à tecla `c`.
    pub fn action_for(&self, c: char) -> Option<&str> {
        self.keys
            .iter()
            .find(|(_, k)| **k == c)
            .map(|(a, _)| a.as_str())
    }

    pub fn key(&self, action: &str) -> char {
        self.keys.get(action).copied().unwrap_or('\0')
    }
}

/// Define o idioma do processo: `SANDMUX_LANG`, depois `language` do
/// arquivo, depois o sistema. Um arquivo que não abre ou é inválido fica de
/// fora (o erro aparece no idioma do sistema, quando for lido de novo).
pub fn init_language(path: &Path) -> sandmux_spec::lang::Lang {
    use sandmux_spec::lang;
    lang::set(lang::resolve_env(None));
    let configured = Config::load(path).ok().and_then(|c| c.language);
    let l = lang::resolve_env(configured);
    lang::set(l);
    l
}

/// O arquivo lido e validado.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Dias de registro de rede guardados (`net-log-days`, só em `[global]`).
    pub net_log_days: Option<u32>,
    /// Teto do registro de rede em bytes (`net-log-max`, só em `[global]`).
    pub net_log_max: Option<u64>,
    /// Idioma da interface (`language`, só em `[global]`); sem ele, o do
    /// sistema.
    pub language: Option<sandmux_spec::lang::Lang>,
    /// Notificações do desktop (`notify`, só em `[global]`); sem a chave,
    /// ligadas.
    pub notify: Option<bool>,
    /// Histórico de tela cifrado em disco (`screen-history`, só em
    /// `[global]`); sem a chave, desligado.
    pub screen_history: Option<bool>,
    pub global: Layer,
    pub profiles: BTreeMap<String, Layer>,
    pub projects: BTreeMap<PathBuf, Layer>,
    pub tui: TuiConfig,
    /// Seção `[assistant]`: modelos e provedores do assistente.
    pub assistant: crate::assistant::AssistantConfig,
}

impl Config {
    /// Lê o arquivo. Um arquivo que não existe é uma configuração vazia.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(path, &text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError {
                path: path.into(),
                line: None,
                message: trf!(
                    "não foi possível ler o arquivo: {e}",
                    "could not read the file: {e}"
                ),
            }),
        }
    }

    /// Valida o texto de um arquivo (`path` só aparece nas mensagens).
    pub fn parse(path: &Path, text: &str) -> Result<Config, ConfigError> {
        let err = |span: Option<std::ops::Range<usize>>, message: String| ConfigError {
            path: path.into(),
            line: span.map(|s| line_of(text, s.start)),
            message,
        };
        let doc = Document::parse(text)
            .map_err(|e| err(e.span(), e.message().trim().replace('\n', " ").to_string()))?;
        let mut cfg = Config::default();
        for (key, item) in doc.as_table().iter() {
            let kspan = doc.as_table().key(key).and_then(|k| k.span());
            match key {
                "global" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            tr!(
                                "`global` precisa ser uma seção `[global]`",
                                "`global` must be a `[global]` section"
                            )
                            .into(),
                        )
                    })?;
                    cfg.global = layer_in(t, &err, true)?;
                    for k in GLOBAL_ONLY_KEYS {
                        let Some(item) = t.get(k) else { continue };
                        let span = item.span();
                        match *k {
                            "net-log-days" => {
                                let n = item
                                    .as_integer()
                                    .filter(|n| (1..=3650).contains(n))
                                    .ok_or_else(|| {
                                        err(
                                            span,
                                            tr!(
                                                "`net-log-days` precisa ser um número de dias de 1 a 3650",
                                                "`net-log-days` must be a number of days from 1 to 3650"
                                            )
                                            .into(),
                                        )
                                    })?;
                                cfg.net_log_days = Some(n as u32);
                            }
                            "language" => {
                                let l = item
                                    .as_str()
                                    .and_then(sandmux_spec::lang::Lang::parse)
                                    .ok_or_else(|| {
                                        err(
                                            span,
                                            tr!(
                                                "`language` precisa ser \"pt\" ou \"en\"",
                                                "`language` must be \"pt\" or \"en\""
                                            )
                                            .into(),
                                        )
                                    })?;
                                cfg.language = Some(l);
                            }
                            "notify" => {
                                let b = item.as_bool().ok_or_else(|| {
                                    err(
                                        span,
                                        tr!(
                                            "`notify` precisa ser true ou false",
                                            "`notify` must be true or false"
                                        )
                                        .into(),
                                    )
                                })?;
                                cfg.notify = Some(b);
                            }
                            "screen-history" => {
                                let b = item.as_bool().ok_or_else(|| {
                                    err(
                                        span,
                                        tr!(
                                            "`screen-history` precisa ser true ou false",
                                            "`screen-history` must be true or false"
                                        )
                                        .into(),
                                    )
                                })?;
                                cfg.screen_history = Some(b);
                            }
                            _ => {
                                let b = item
                                    .as_str()
                                    .and_then(|s| match MemoryLimit::parse(s) {
                                        Ok(MemoryLimit::Bytes(b)) => Some(b),
                                        _ => None,
                                    })
                                    .ok_or_else(|| {
                                        err(
                                            span,
                                            tr!(
                                                "`net-log-max` precisa ser um tamanho como \"500M\" ou \"2G\" (mínimo 128M)",
                                                "`net-log-max` must be a size like \"500M\" or \"2G\" (minimum 128M)"
                                            )
                                            .into(),
                                        )
                                    })?;
                                cfg.net_log_max = Some(b);
                            }
                        }
                    }
                }
                "profile" | "project" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            trf!(
                                "`{key}` precisa ser uma seção `[{key}.<nome>]`",
                                "`{key}` must be a `[{key}.<name>]` section"
                            ),
                        )
                    })?;
                    for (name, sub) in t.iter() {
                        let nspan = t.key(name).and_then(|k| k.span()).or(kspan.clone());
                        let st = sub.as_table_like().ok_or_else(|| {
                            err(
                                nspan.clone(),
                                trf!(
                                    "`{key}.{name}` precisa ser uma seção",
                                    "`{key}.{name}` must be a section"
                                ),
                            )
                        })?;
                        let l = layer(st, &err)?;
                        if key == "profile" {
                            if !valid_profile_name(name) {
                                return Err(err(
                                    nspan,
                                    trf!(
                                        "nome de perfil inválido: {name:?}. Use letras minúsculas, números, `-` e `_`, começando por letra ou número",
                                        "invalid profile name: {name:?}. Use lowercase letters, digits, `-` and `_`, starting with a letter or digit"
                                    ),
                                ));
                            }
                            cfg.profiles.insert(name.to_string(), l);
                        } else {
                            let p = PathBuf::from(name);
                            if !p.is_absolute() {
                                return Err(err(
                                    nspan,
                                    trf!(
                                        "a pasta de um projeto precisa ser um caminho absoluto: {name:?}",
                                        "a project folder must be an absolute path: {name:?}"
                                    ),
                                ));
                            }
                            cfg.projects.insert(normalize(&p), l);
                        }
                    }
                }
                "tui" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            tr!(
                                "`tui` precisa ser uma seção `[tui]`",
                                "`tui` must be a `[tui]` section"
                            )
                            .into(),
                        )
                    })?;
                    cfg.tui = tui_section(t, &err)?;
                }
                "assistant" => {
                    let t = item.as_table_like().ok_or_else(|| {
                        err(
                            kspan.clone(),
                            tr!(
                                "`assistant` precisa ser uma seção `[assistant]`",
                                "`assistant` must be an `[assistant]` section"
                            )
                            .into(),
                        )
                    })?;
                    cfg.assistant = assistant_section(t, &err)?;
                }
                other => {
                    return Err(err(
                        kspan,
                        trf!(
                            "seção desconhecida: `{other}`. Seções aceitas: `[global]`, `[profile.<nome>]`, `[project.\"<pasta>\"]`, `[tui]` e `[assistant]`",
                            "unknown section: `{other}`. Accepted sections: `[global]`, `[profile.<name>]`, `[project.\"<folder>\"]`, `[tui]` and `[assistant]`"
                        ),
                    ));
                }
            }
        }
        Ok(cfg)
    }

    /// Nomes dos perfis, para mensagens.
    pub fn profile_names(&self) -> String {
        if self.profiles.is_empty() {
            tr!("nenhum perfil definido", "no profile defined").into()
        } else {
            self.profiles.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

type ErrFn<'a> = dyn Fn(Option<std::ops::Range<usize>>, String) -> ConfigError + 'a;

fn layer(t: &dyn toml_edit::TableLike, err: &ErrFn<'_>) -> Result<Layer, ConfigError> {
    layer_in(t, err, false)
}

fn layer_in(
    t: &dyn toml_edit::TableLike,
    err: &ErrFn<'_>,
    global: bool,
) -> Result<Layer, ConfigError> {
    let mut l = Layer::default();
    for (key, item) in t.iter() {
        let kspan = t.key(key).and_then(|k| k.span());
        let vspan = item.span().or(kspan.clone());
        match key {
            "ro" | "rw" => {
                let items = strings(item, key, vspan.clone(), err)?;
                let mut out = Vec::new();
                for (s, span) in items {
                    if !valid_path(&s) {
                        return Err(err(
                            span,
                            trf!(
                                "`{key}` aceita só caminhos absolutos ou iniciados por `~/`: {s:?}",
                                "`{key}` only accepts absolute paths or paths starting with `~/`: {s:?}"
                            ),
                        ));
                    }
                    out.push(PathBuf::from(s));
                }
                if key == "ro" {
                    l.ro = out;
                } else {
                    l.rw = out;
                }
            }
            "pass-env" => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !valid_var_name(&s) {
                        return Err(err(
                            span,
                            trf!(
                                "`pass-env` aceita só nomes de variáveis, nunca valores: {s:?}",
                                "`pass-env` only accepts variable names, never values: {s:?}"
                            ),
                        ));
                    }
                    l.pass_env.push(s);
                }
            }
            "repo" | "kvm" | "gpu" | "clipboard" | "harness-asks" | "portal" => {
                let v = item.as_bool().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        trf!(
                            "`{key}` precisa ser true ou false",
                            "`{key}` must be true or false"
                        ),
                    )
                })?;
                *l.bool_mut(key).expect("chave de liga e desliga") = Some(v);
            }
            "memory" => {
                let s = item.as_str().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        trf!(
                            "`memory` precisa ser um texto: {}",
                            "`memory` must be a string: {}",
                            limits::memory_formats()
                        ),
                    )
                })?;
                l.memory = Some(MemoryLimit::parse(s).map_err(|m| err(vspan.clone(), m))?);
            }
            "processes" => {
                let v = if let Some(n) = item.as_integer() {
                    u32::try_from(n)
                        .map_err(|_| {
                            trf!(
                                "teto de processos inválido: {n}",
                                "invalid process cap: {n}"
                            )
                        })
                        .and_then(ProcessLimit::count)
                } else if let Some(s) = item.as_str() {
                    ProcessLimit::parse(s)
                } else {
                    Err(trf!(
                        "`processes` precisa ser {}",
                        "`processes` must be {}",
                        limits::process_formats()
                    ))
                };
                l.processes = Some(v.map_err(|m| err(vspan.clone(), m))?);
            }
            "network" => {
                let s = item.as_str().unwrap_or("");
                l.network = Some(NetworkMode::parse(s).map_err(|m| err(vspan.clone(), m))?);
            }
            "window" => {
                let s = match (item.as_bool(), item.as_str()) {
                    (Some(b), _) => b.to_string(),
                    (None, Some(s)) => s.to_string(),
                    _ => String::new(),
                };
                l.window = Some(WindowMode::parse(&s).map_err(|m| err(vspan.clone(), m))?);
            }
            "deny" | "allow" | "allow-off" | "catalog-off" | "decrypt" => {
                let mut out = Vec::new();
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    out.push(
                        s.parse::<Pattern>()
                            .map_err(|m| err(span, format!("`{key}`: {m}")))?,
                    );
                }
                *l.patterns_mut(key).expect("lista de padrões") = out;
            }
            "catalog" => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !netcatalog::is_group(&s) {
                        return Err(err(
                            span,
                            trf!(
                                "`catalog`: grupo desconhecido {s:?}. Grupos: {}",
                                "`catalog`: unknown group {s:?}. Groups: {}",
                                netcatalog::group_names()
                            ),
                        ));
                    }
                    l.catalog.push(s);
                }
            }
            k if MCP_LIST_KEYS.contains(&k) => {
                let mut out = Vec::new();
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !valid_ssh_key_name(&s) || s == "*" {
                        return Err(err(
                            span,
                            trf!(
                                "`{key}` aceita nomes de servidores MCP: {s:?}",
                                "`{key}` accepts MCP server names: {s:?}"
                            ),
                        ));
                    }
                    out.push(s);
                }
                if key == "mcp-deny" {
                    l.mcp_deny = out;
                } else {
                    l.mcp_allow = out;
                }
            }
            k if FILE_LIST_KEYS.contains(&k) => {
                let mut out = Vec::new();
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    valid_file_pattern(&s).map_err(|m| err(span, format!("`{key}`: {m}")))?;
                    out.push(s);
                }
                *l.file_list_mut(key).expect("lista de arquivos") = out;
            }
            HOST_PORTS_KEY => {
                let arr = item.as_array().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        tr!(
                            "`host-ports` precisa ser uma lista de números de porta, como [8888, 9000]",
                            "`host-ports` must be a list of port numbers, like [8888, 9000]"
                        )
                        .into(),
                    )
                })?;
                for v in arr.iter() {
                    let p = v
                        .as_integer()
                        .and_then(|n| u16::try_from(n).ok())
                        .filter(|n| *n > 0)
                        .ok_or_else(|| {
                            err(
                                v.span().or(vspan.clone()),
                                tr!(
                                    "`host-ports` aceita números de porta de 1 a 65535",
                                    "`host-ports` accepts port numbers from 1 to 65535"
                                )
                                .into(),
                            )
                        })?;
                    if !l.host_ports.contains(&p) {
                        l.host_ports.push(p);
                    }
                }
            }
            PORTS_KEY => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    l.ports.push(
                        s.parse::<sandmux_spec::net::PortRule>()
                            .map_err(|m| err(span, format!("`ports`: {m}")))?,
                    );
                }
            }
            DB_KEY => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !valid_ssh_key_name(&s) {
                        return Err(err(
                            span,
                            trf!(
                                "`db` aceita nomes de conexões do cofre, ou \"*\": {s:?}",
                                "`db` accepts vault connection names, or \"*\": {s:?}"
                            ),
                        ));
                    }
                    l.db.push(s);
                }
            }
            SSH_KEYS => {
                for (s, span) in strings(item, key, vspan.clone(), err)? {
                    if !valid_ssh_key_name(&s) {
                        return Err(err(
                            span,
                            trf!(
                                "`ssh-keys` aceita nomes de chaves do cofre, ou \"*\": {s:?}",
                                "`ssh-keys` accepts vault key names, or \"*\": {s:?}"
                            ),
                        ));
                    }
                    l.ssh_keys.push(s);
                }
            }
            other if GLOBAL_ONLY_KEYS.contains(&other) => {
                // Lidas por quem chama, e só em `[global]`.
                if !global {
                    return Err(err(
                        kspan,
                        trf!(
                            "`{other}` só é aceita em `[global]`",
                            "`{other}` is only accepted in `[global]`"
                        ),
                    ));
                }
            }
            other => {
                return Err(err(
                    kspan,
                    trf!(
                        "chave desconhecida: `{other}`. Chaves aceitas: {}",
                        "unknown key: `{other}`. Accepted keys: {}",
                        LIST_KEYS
                            .iter()
                            .chain(BOOL_KEYS)
                            .chain(LIMIT_KEYS)
                            .chain(NET_KEYS)
                            .chain([&WINDOW_KEY])
                            .chain(NET_LIST_KEYS)
                            .chain([&SSH_KEYS, &DB_KEY, &PORTS_KEY, &HOST_PORTS_KEY])
                            .chain(FILE_LIST_KEYS)
                            .chain(MCP_LIST_KEYS)
                            .map(|k| format!("`{k}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
    }
    let rw: Vec<PathBuf> = l.rw.iter().map(|p| normalize(p)).collect();
    if let Some(p) = l.ro.iter().find(|p| rw.contains(&normalize(p))) {
        let span = t.key("rw").and_then(|k| k.span());
        return Err(err(
            span,
            trf!(
                "{} aparece em `ro` e em `rw` na mesma seção",
                "{} appears in both `ro` and `rw` in the same section",
                p.display()
            ),
        ));
    }
    Ok(l)
}

/// Lê a seção `[assistant]`.
fn assistant_section(
    t: &dyn toml_edit::TableLike,
    err: &ErrFn<'_>,
) -> Result<crate::assistant::AssistantConfig, ConfigError> {
    use crate::assistant::{AssistantConfig, KNOWN, url_host, valid_provider_name};
    let mut cfg = AssistantConfig::default();
    // Os provedores primeiro: os modelos são conferidos contra eles.
    if let Some(item) = t.get("provider") {
        let kspan = t.key("provider").and_then(|k| k.span());
        let pt = item.as_table_like().ok_or_else(|| {
            err(
                kspan.clone(),
                tr!(
                    "`provider` precisa ser uma seção `[assistant.provider.<nome>]`",
                    "`provider` must be an `[assistant.provider.<name>]` section"
                )
                .into(),
            )
        })?;
        for (name, sub) in pt.iter() {
            let nspan = pt.key(name).and_then(|k| k.span()).or(kspan.clone());
            if !valid_provider_name(name) {
                return Err(err(
                    nspan,
                    trf!(
                        "nome de provedor inválido: {name:?}. Use letras minúsculas, números, `-` e `_`",
                        "invalid provider name: {name:?}. Use lowercase letters, digits, `-` and `_`"
                    ),
                ));
            }
            if KNOWN.iter().any(|(n, _, _)| *n == name) {
                return Err(err(
                    nspan,
                    trf!(
                        "`{name}` já é um provedor conhecido; escolha outro nome",
                        "`{name}` is already a known provider; choose another name"
                    ),
                ));
            }
            let st = sub.as_table_like().ok_or_else(|| {
                err(
                    nspan.clone(),
                    trf!(
                        "`provider.{name}` precisa ser uma seção `[assistant.provider.{name}]`",
                        "`provider.{name}` must be an `[assistant.provider.{name}]` section"
                    ),
                )
            })?;
            let mut url = None;
            for (k, v) in st.iter() {
                let ks = st.key(k).and_then(|x| x.span());
                match k {
                    "url" => {
                        let u = v.as_str().unwrap_or("");
                        if url_host(u).is_none() {
                            return Err(err(
                                v.span().or(ks),
                                trf!(
                                    "`url` precisa começar por http:// ou https:// e ter um host: {u:?}",
                                    "`url` must start with http:// or https:// and have a host: {u:?}"
                                ),
                            ));
                        }
                        url = Some(u.trim_end_matches('/').to_string());
                    }
                    other => {
                        return Err(err(
                            ks,
                            trf!(
                                "chave desconhecida em `[assistant.provider.{name}]`: `{other}`. Chave aceita: `url`",
                                "unknown key in `[assistant.provider.{name}]`: `{other}`. Accepted key: `url`"
                            ),
                        ));
                    }
                }
            }
            let url = url.ok_or_else(|| {
                err(
                    nspan.clone(),
                    trf!(
                        "falta `url` em `[assistant.provider.{name}]`",
                        "missing `url` in `[assistant.provider.{name}]`"
                    ),
                )
            })?;
            cfg.providers.insert(name.to_string(), url);
        }
    }
    let check = |cfg: &AssistantConfig, m: &str, span| -> Result<(), ConfigError> {
        cfg.resolve(m).map(|_| ()).map_err(|e| err(span, e))
    };
    for (key, item) in t.iter() {
        let kspan = t.key(key).and_then(|k| k.span());
        let vspan = item.span().or(kspan.clone());
        match key {
            "provider" => {}
            "chat-model" | "command-model" => {
                let m = item.as_str().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        trf!(
                            "`{key}` precisa ser um texto como \"openrouter/deepseek/deepseek-v4.1-flash-1017\"",
                            "`{key}` must be a string like \"openrouter/deepseek/deepseek-v4.1-flash-1017\""
                        ),
                    )
                })?;
                check(&cfg, m, vspan.clone())?;
                if key == "chat-model" {
                    cfg.chat_model = Some(m.trim().to_string());
                } else {
                    cfg.command_model = Some(m.trim().to_string());
                }
            }
            "models" => {
                let a = item.as_array().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        tr!(
                            "`models` precisa ser uma lista de modelos",
                            "`models` must be a list of models"
                        )
                        .into(),
                    )
                })?;
                for v in a.iter() {
                    let m = v.as_str().ok_or_else(|| {
                        err(
                            v.span().or(vspan.clone()),
                            tr!(
                                "`models` precisa ser uma lista de textos",
                                "`models` must be a list of strings"
                            )
                            .into(),
                        )
                    })?;
                    check(&cfg, m, v.span().or(vspan.clone()))?;
                    cfg.models.push(m.trim().to_string());
                }
            }
            other => {
                return Err(err(
                    kspan,
                    trf!(
                        "chave desconhecida em `[assistant]`: `{other}`. Chaves aceitas: `chat-model`, `command-model`, `models`, `provider`",
                        "unknown key in `[assistant]`: `{other}`. Accepted keys: `chat-model`, `command-model`, `models`, `provider`"
                    ),
                ));
            }
        }
    }
    Ok(cfg)
}

fn tui_section(t: &dyn toml_edit::TableLike, err: &ErrFn<'_>) -> Result<TuiConfig, ConfigError> {
    let mut cfg = TuiConfig::default();
    // Ações com tecla escolhida pelo usuário: vencem a tecla padrão de outra.
    let mut explicit: Vec<String> = Vec::new();
    // O tema vem antes das cores, que ficam por cima dele.
    if let Some(item) = t.get("theme") {
        let span = item
            .span()
            .or_else(|| t.key("theme").and_then(|k| k.span()));
        let name = item.as_str().unwrap_or("");
        cfg.theme = Theme::builtin(name).ok_or_else(|| {
            err(
                span,
                trf!(
                    "tema desconhecido: {name:?}. Temas: {}",
                    "unknown theme: {name:?}. Themes: {}",
                    THEMES.join(", ")
                ),
            )
        })?;
    }
    for (key, item) in t.iter() {
        let kspan = t.key(key).and_then(|k| k.span());
        let vspan = item.span().or(kspan.clone());
        match key {
            "prefix" => {
                let s = item.as_str().unwrap_or("");
                let letter = s
                    .strip_prefix("C-")
                    .filter(|r| r.len() == 1)
                    .and_then(|r| r.chars().next())
                    .filter(|c| c.is_ascii_lowercase());
                cfg.prefix = letter.ok_or_else(|| {
                    err(
                        vspan.clone(),
                        trf!(
                            "`prefix` precisa ser Ctrl com uma letra minúscula, como \"C-b\": {s:?}",
                            "`prefix` must be Ctrl with a lowercase letter, like \"C-b\": {s:?}"
                        ),
                    )
                })?;
            }
            "keys" => {
                let k = item.as_table_like().ok_or_else(|| {
                    err(
                        kspan.clone(),
                        tr!(
                            "`keys` precisa ser uma seção `[tui.keys]`",
                            "`keys` must be a `[tui.keys]` section"
                        )
                        .into(),
                    )
                })?;
                for (action, v) in k.iter() {
                    let aspan = k.key(action).and_then(|x| x.span());
                    if !TUI_ACTIONS.iter().any(|(a, _)| *a == action) {
                        return Err(err(
                            aspan,
                            trf!(
                                "ação desconhecida em `[tui.keys]`: `{action}`. Ações: {}",
                                "unknown action in `[tui.keys]`: `{action}`. Actions: {}",
                                TUI_ACTIONS
                                    .iter()
                                    .map(|(a, _)| *a)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                    let vs = v.span().or(aspan.clone());
                    let s = v.as_str().unwrap_or("");
                    let mut chars = s.chars();
                    let c = match (chars.next(), chars.next()) {
                        (Some(c), None) if !c.is_control() && !c.is_ascii_digit() && c != ' ' => c,
                        _ => {
                            return Err(err(
                                vs,
                                trf!(
                                    "a tecla de `{action}` precisa ser um caractere visível que não seja dígito: {s:?}",
                                    "the key for `{action}` must be a visible character that is not a digit: {s:?}"
                                ),
                            ));
                        }
                    };
                    cfg.keys.insert(action.to_string(), c);
                    explicit.push(action.to_string());
                }
            }
            "theme" => {}
            "panel" => {
                cfg.panel = item.as_bool().ok_or_else(|| {
                    err(
                        vspan.clone(),
                        tr!(
                            "`panel` precisa ser true ou false",
                            "`panel` must be true or false"
                        )
                        .into(),
                    )
                })?;
            }
            "style" => {
                cfg.style = match item.as_str() {
                    Some("blocos") => BarStyle::Blocos,
                    Some("powerline") => BarStyle::Powerline,
                    Some("simples") => BarStyle::Simples,
                    _ => {
                        return Err(err(
                            vspan.clone(),
                            tr!(
                                "`style` precisa ser \"blocos\", \"powerline\" (precisa de uma fonte Nerd Font) ou \"simples\"",
                                "`style` must be \"blocos\", \"powerline\" (needs a Nerd Font) or \"simples\""
                            )
                            .into(),
                        ));
                    }
                };
            }
            "bar" => {
                cfg.bar_top = match item.as_str() {
                    Some("bottom") => false,
                    Some("top") => true,
                    _ => {
                        return Err(err(
                            vspan.clone(),
                            tr!(
                                "`bar` precisa ser \"bottom\" ou \"top\"",
                                "`bar` must be \"bottom\" or \"top\""
                            )
                            .into(),
                        ));
                    }
                };
            }
            "panel-order" | "panel-collapsed" => {
                let msg = || {
                    trf!(
                        "`{key}` precisa ser uma lista de seções do painel, sem repetir: {}",
                        "`{key}` must be a list of panel sections, without repeats: {}",
                        PANEL_SECTIONS
                            .iter()
                            .map(|s| format!("\"{s}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let a = item.as_array().ok_or_else(|| err(vspan.clone(), msg()))?;
                let mut names: Vec<String> = Vec::new();
                for v in a.iter() {
                    let s = v
                        .as_str()
                        .filter(|s| PANEL_SECTIONS.contains(s) && !names.iter().any(|n| n == s))
                        .ok_or_else(|| err(v.span().or(vspan.clone()), msg()))?;
                    names.push(s.to_string());
                }
                if key == "panel-order" {
                    // As que faltam vão para o fim, na ordem padrão.
                    for s in PANEL_SECTIONS {
                        if !names.iter().any(|n| n == s) {
                            names.push(s.to_string());
                        }
                    }
                    cfg.panel_order = names;
                } else {
                    cfg.panel_collapsed = names;
                }
            }
            "colors" => {
                let c = item.as_table_like().ok_or_else(|| {
                    err(
                        kspan.clone(),
                        tr!(
                            "`colors` precisa ser uma seção `[tui.colors]`",
                            "`colors` must be a `[tui.colors]` section"
                        )
                        .into(),
                    )
                })?;
                for (el, v) in c.iter() {
                    let espan = c.key(el).and_then(|x| x.span());
                    if !THEME_ELEMENTS.contains(&el) {
                        return Err(err(
                            espan,
                            trf!(
                                "elemento desconhecido em `[tui.colors]`: `{el}`. Elementos: {}",
                                "unknown element in `[tui.colors]`: `{el}`. Elements: {}",
                                THEME_ELEMENTS.join(", ")
                            ),
                        ));
                    }
                    let vs = v.span().or(espan);
                    let parsed = v.as_str().and_then(ThemeColor::parse).or_else(|| {
                        v.as_integer()
                            .and_then(|n| u8::try_from(n).ok())
                            .map(ThemeColor::Indexed)
                    });
                    let color = parsed.ok_or_else(|| {
                        err(
                            vs,
                            trf!(
                                "cor inválida em `{el}`; use {}",
                                "invalid color in `{el}`; use {}",
                                color_formats()
                            ),
                        )
                    })?;
                    cfg.theme.colors.insert(el.to_string(), color);
                }
            }
            other => {
                return Err(err(
                    kspan,
                    trf!(
                        "chave desconhecida em `[tui]`: `{other}`. Chaves aceitas: `prefix`, `theme`, `panel`, `panel-order`, `panel-collapsed`, `style`, `bar`, `keys`, `colors`",
                        "unknown key in `[tui]`: `{other}`. Accepted keys: `prefix`, `theme`, `panel`, `panel-order`, `panel-collapsed`, `style`, `bar`, `keys`, `colors`"
                    ),
                ));
            }
        }
    }
    // Uma tecla escolhida pelo usuário tira a tecla padrão de outra ação
    // (uma ação nova numa versão nova não invalida o arquivo); duas
    // escolhidas iguais são erro.
    let taken: Vec<char> = explicit
        .iter()
        .filter_map(|a| cfg.keys.get(a).copied())
        .collect();
    cfg.keys
        .retain(|a, c| explicit.contains(a) || !taken.contains(c));
    let mut seen: BTreeMap<char, &str> = BTreeMap::new();
    for (a, c) in &cfg.keys {
        if let Some(other) = seen.insert(*c, a) {
            return Err(err(
                t.key("keys").and_then(|k| k.span()),
                trf!(
                    "as ações `{other}` e `{a}` usam a mesma tecla {c:?}",
                    "the actions `{other}` and `{a}` use the same key {c:?}"
                ),
            ));
        }
    }
    Ok(cfg)
}

type Spanned = (String, Option<std::ops::Range<usize>>);

fn strings(
    item: &Item,
    key: &str,
    span: Option<std::ops::Range<usize>>,
    err: &ErrFn<'_>,
) -> Result<Vec<Spanned>, ConfigError> {
    let arr = item.as_array().ok_or_else(|| {
        err(
            span.clone(),
            trf!(
                "`{key}` precisa ser uma lista de textos",
                "`{key}` must be a list of strings"
            ),
        )
    })?;
    arr.iter()
        .map(|v: &Value| {
            v.as_str()
                .map(|s| (s.to_string(), v.span().or(span.clone())))
                .ok_or_else(|| {
                    err(
                        v.span().or(span.clone()),
                        trf!(
                            "`{key}` precisa ser uma lista de textos",
                            "`{key}` must be a list of strings"
                        ),
                    )
                })
        })
        .collect()
}

/// Caminho aceito no arquivo: absoluto ou iniciado por `~/`.
pub fn valid_path(s: &str) -> bool {
    (s.starts_with('/') || (s.starts_with("~/") && s.len() > 2)) && !s.contains('\0')
}

/// Nome de variável de ambiente: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn valid_var_name(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Nome de perfil: `[a-z0-9][a-z0-9_-]*`.
pub fn valid_profile_name(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

/// Normalização léxica: tira `.`, resolve `..` sem tocar no disco e a barra
/// final.
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Expande `~/` com o HOME e normaliza.
pub fn expand(p: &Path, home: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => normalize(&home.join(rest)),
        Err(_) => normalize(p),
    }
}

/// De onde veio um valor da política efetiva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Baseline ou preset.
    Default,
    Global,
    Profile(String),
    Project(PathBuf),
    Run,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Origin::Default => f.write_str(tr!("padrão", "default")),
            Origin::Global => f.write_str(tr!("modificada (global)", "modified (global)")),
            Origin::Profile(n) => {
                f.write_str(&trf!("modificada (perfil {n})", "modified (profile {n})"))
            }
            Origin::Project(p) => f.write_str(&trf!(
                "modificada (projeto {})",
                "modified (project {})",
                p.display()
            )),
            Origin::Run => f.write_str(tr!("modificada (opção do run)", "modified (run option)")),
        }
    }
}

/// Um valor de liga e desliga com a origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub value: bool,
    pub origin: Origin,
}

/// Uma pasta extra da política efetiva.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraPath {
    pub path: PathBuf,
    pub writable: bool,
    pub origin: Origin,
}

/// O que o lançamento pede.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub harness: &'a str,
    /// Pasta de trabalho, absoluta.
    pub workdir: &'a Path,
    pub profile: Option<&'a str>,
    /// Opções dadas no `run` (caminhos já absolutos).
    pub run: &'a Layer,
}

/// A política resolvida, com a origem de cada valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub extras: Vec<ExtraPath>,
    pub pass_env: Vec<(String, Origin)>,
    pub repo: Setting,
    pub kvm: Setting,
    pub gpu: Setting,
    pub clipboard: Setting,
    pub harness_asks: Setting,
    pub portal: Setting,
    pub memory: LimitSetting<MemoryLimit>,
    pub processes: LimitSetting<ProcessLimit>,
    pub network: LimitSetting<NetworkMode>,
    pub window: LimitSetting<WindowMode>,
    /// Todas as regras de rede, com a origem e o estado, inclusive as que o
    /// modo atual não usa (para `sandmux net rules`).
    pub net_rules: Vec<Rule>,
    /// Domínios decifrados, somados de todas as camadas.
    pub decrypt: Vec<Pattern>,
    /// Chaves SSH escolhidas, somadas de todas as camadas.
    pub ssh_keys: Vec<String>,
    /// Conexões de banco escolhidas, somadas de todas as camadas.
    pub db: Vec<String>,
    /// Portas de entrada, somadas de todas as camadas.
    pub ports: Vec<sandmux_spec::net::PortRule>,
    /// Portas do host (`host-ports`).
    pub host_ports: Vec<u16>,
    /// Listas da política de arquivos, somadas de todas as camadas.
    pub file_deny: Vec<String>,
    pub file_allow: Vec<String>,
    pub secret_files: Vec<String>,
    pub secret_files_off: Vec<String>,
    /// Servidores MCP, somados de todas as camadas.
    pub mcp_deny: Vec<String>,
    pub mcp_allow: Vec<String>,
    pub profile: Option<String>,
    /// Pasta do perfil de projeto aplicado.
    pub project: Option<PathBuf>,
}

impl Effective {
    /// Confinamento da aba de chat: só inferência (mais os hosts dos
    /// provedores próprios), sem pastas extras, `pass-env`, repositório,
    /// chaves SSH, bancos nem portas. As regras de bloqueio continuam.
    pub fn confine_chat(&mut self, a: &crate::assistant::AssistantConfig) {
        use sandmux_spec::net::{Pattern, Rule, RuleKind};
        self.extras.clear();
        self.pass_env.clear();
        self.repo.value = false;
        self.kvm.value = false;
        self.gpu.value = false;
        self.clipboard.value = false;
        self.ssh_keys.clear();
        self.db.clear();
        self.ports.clear();
        self.host_ports.clear();
        self.network = LimitSetting {
            value: NetworkMode::Inferencia,
            origin: Origin::Default,
        };
        self.window = LimitSetting {
            value: WindowMode::Off,
            origin: Origin::Default,
        };
        for p in a.all_providers().iter().filter(|p| !p.is_known()) {
            if let Ok(pattern) = p.host().parse::<Pattern>() {
                self.net_rules.push(Rule {
                    pattern,
                    kind: RuleKind::Allow,
                    origin: crate::assistant::ORIGIN.into(),
                    enabled: true,
                });
            }
        }
    }
}

impl Effective {
    /// A política de arquivos compilada: `~/` vira `home`.
    pub fn file_policy(&self, home: &Path, workdir: &Path) -> sandmux_spec::files::FilePolicy {
        use sandmux_spec::files::{FilePattern, default_secrets};
        let pats = |v: &[String]| -> Vec<FilePattern> {
            v.iter()
                .filter_map(|s| FilePattern::parse(s, Some(home)).ok())
                .collect()
        };
        let (mut secrets, mut secret_exceptions) = default_secrets();
        secrets.extend(pats(&self.secret_files));
        secret_exceptions.extend(pats(&self.secret_files_off));
        sandmux_spec::files::FilePolicy {
            deny: pats(&self.file_deny),
            allow: pats(&self.file_allow),
            secrets,
            secret_exceptions,
            workdir: workdir.to_path_buf(),
        }
    }
}

/// Um teto com a origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitSetting<T> {
    pub value: T,
    pub origin: Origin,
}

impl Effective {
    pub fn ro(&self) -> Vec<PathBuf> {
        self.extras
            .iter()
            .filter(|e| !e.writable)
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn rw(&self) -> Vec<PathBuf> {
        self.extras
            .iter()
            .filter(|e| e.writable)
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn pass_env_names(&self) -> Vec<String> {
        self.pass_env.iter().map(|(n, _)| n.clone()).collect()
    }

    /// Os tetos em bytes e em número, para uma máquina com `mem_total` bytes
    /// de RAM.
    pub fn limits(&self, mem_total: u64) -> Limits {
        Limits {
            memory: self.memory.value.bytes(mem_total),
            processes: self.processes.value.value(),
        }
    }

    /// As opções de composição desta política.
    pub fn run_options(
        &self,
        harness: &str,
        workdir: &Path,
        args: &[String],
        layer: LayerMode,
    ) -> RunOptions {
        RunOptions {
            harness: harness.into(),
            workdir: Some(workdir.into()),
            ro: self.ro(),
            rw: self.rw(),
            pass_env: self.pass_env_names(),
            repo: self.repo.value,
            kvm: self.kvm.value,
            no_gpu: !self.gpu.value,
            harness_asks: self.harness_asks.value,
            args: args.to_vec(),
            layer,
            docker_socket: None,
            network: self.network.value,
            net_policy: self.net_policy(),
            fake_env: Vec::new(),
            tool_secrets: Vec::new(),
            ssh_agent_socket: None,
            ssh_servers: Vec::new(),
            window: None,
            portal: None,
            files_log: None,
            file_policy: Default::default(),
            file_policy_file: None,
            folders: Vec::new(),
            control_socket: None,
            mcp_deny: self.mcp_deny.clone(),
            mcp_allow: self.mcp_allow.clone(),
            db_socket: None,
            db_mcp: None,
            chat_bin: None,
            chat_env: Vec::new(),
            opencode_models: Vec::new(),
            net: None,
        }
    }

    /// A política de rede do gateway, com só as regras que o modo usa;
    /// `None` no modo sem rede.
    pub fn net_policy(&self) -> Option<NetPolicy> {
        use sandmux_spec::net::PolicyMode;
        let mode = self.network.value.policy_mode()?;
        let rules = self
            .net_rules
            .iter()
            .filter(|r| match (r.kind, mode) {
                (RuleKind::Deny, _) => true,
                (RuleKind::Allow, PolicyMode::Open) => false,
                (RuleKind::Allow, PolicyMode::Inference) => {
                    r.origin == netcatalog::INFERENCE_ORIGIN || r.origin == crate::assistant::ORIGIN
                }
                (RuleKind::Allow, PolicyMode::Custom) => r.origin != netcatalog::INFERENCE_ORIGIN,
            })
            .cloned()
            .collect();
        Some(NetPolicy {
            mode,
            rules,
            decrypt: self.decrypt.clone(),
            // Os servidores das chaves SSH vêm do cofre, pelo daemon.
            ssh: Vec::new(),
            ports: self.ports.clone(),
            host_ports: self.host_ports.clone(),
        })
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        match key {
            "repo" => Some(&self.repo),
            "kvm" => Some(&self.kvm),
            "gpu" => Some(&self.gpu),
            "clipboard" => Some(&self.clipboard),
            "harness-asks" => Some(&self.harness_asks),
            "portal" => Some(&self.portal),
            _ => None,
        }
    }
}

/// Erros da resolução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownProfile { name: String, available: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownProfile { name, available } => f.write_str(&trf!(
                "o perfil {name:?} não existe no arquivo de configuração. Perfis: {available}",
                "profile {name:?} does not exist in the configuration file. Profiles: {available}"
            )),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Canoniza quando o caminho existe; senão, só normaliza.
fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| normalize(p))
}

/// O perfil de projeto que vale para `workdir`: o da pasta mais próxima.
pub fn project_for<'c>(cfg: &'c Config, workdir: &Path) -> Option<(&'c PathBuf, &'c Layer)> {
    let wd = canon(workdir);
    cfg.projects
        .iter()
        .filter(|(p, _)| wd.starts_with(canon(p)))
        .max_by_key(|(p, _)| canon(p).components().count())
}

/// Nome curto da origem de uma regra de rede.
pub fn rule_origin(o: &Origin) -> String {
    match o {
        Origin::Default => "padrão".into(),
        Origin::Global => "global".into(),
        Origin::Profile(n) => format!("perfil {n}"),
        Origin::Project(p) => format!("projeto {}", p.display()),
        Origin::Run => "aba".into(),
    }
}

/// O nome de exibição de um valor do arquivo (o modo de rede, o estilo da
/// barra, o tema): em inglês, a tradução; o arquivo continua com o valor.
/// O histórico de tela em disco está ligado no arquivo (`[global]
/// screen-history`; desligado sem a chave ou com o arquivo ilegível).
pub fn screen_history_enabled(path: &Path) -> bool {
    Config::load(path)
        .ok()
        .and_then(|c| c.screen_history)
        .unwrap_or(false)
}

/// As notificações do desktop estão ligadas no arquivo (`[global] notify`,
/// ligadas sem a chave ou com o arquivo ilegível).
pub fn notify_enabled(path: &Path) -> bool {
    Config::load(path)
        .ok()
        .and_then(|c| c.notify)
        .unwrap_or(true)
}

pub fn value_label(key: &str, v: &str) -> String {
    if !sandmux_spec::lang::is_en() {
        return v.to_string();
    }
    let en = match (key, v) {
        ("network", "aberta") => "open",
        ("network", "inferencia") => "inference",
        ("network", "sem-rede") => "no network",
        ("window", "direct") => "direct",
        ("style", "blocos") => "blocks",
        ("style", "simples") => "plain",
        ("theme", "padrao") => "default",
        ("theme", "escuro") => "dark",
        ("theme", "claro") => "light",
        _ => v,
    };
    en.to_string()
}

/// A origem de uma regra para exibição: o valor guardado (em português, e
/// comparado pela TUI) traduzido para o idioma atual.
pub fn origin_text(origin: &str) -> String {
    if !sandmux_spec::lang::is_en() {
        return origin.to_string();
    }
    let map = [
        ("catálogo de exfiltração", "exfiltration catalog"),
        ("catálogo de inferência", "inference catalog"),
        ("padrão", "default"),
        ("aba", "tab"),
        ("assistente", "assistant"),
    ];
    if let Some((_, en)) = map.iter().find(|(pt, _)| *pt == origin) {
        return en.to_string();
    }
    for (pt, en) in [
        ("catálogo comum (", "common catalog ("),
        ("perfil ", "profile "),
        ("projeto ", "project "),
    ] {
        if let Some(rest) = origin.strip_prefix(pt) {
            return format!("{en}{rest}");
        }
    }
    origin.to_string()
}

/// Compila as regras de rede de todas as camadas, da menos para a mais
/// específica: catálogos, `deny`, `allow` e os itens desligados.
fn net_rules(layers: &[(&Layer, Origin)]) -> Vec<Rule> {
    let off: Vec<&Pattern> = layers.iter().flat_map(|(l, _)| &l.catalog_off).collect();
    let allow_off: Vec<&Pattern> = layers.iter().flat_map(|(l, _)| &l.allow_off).collect();
    let groups: Vec<&String> = layers.iter().flat_map(|(l, _)| &l.catalog).collect();
    let mut rules: Vec<Rule> = Vec::new();
    let push = |rules: &mut Vec<Rule>, r: Rule| {
        // Um mesmo padrão e tipo aparece uma vez, com a origem mais específica.
        rules.retain(|x| !(x.pattern == r.pattern && x.kind == r.kind));
        rules.push(r);
    };
    let pat = |s: &str| s.parse::<Pattern>().expect("catálogo válido");
    for (_, items) in netcatalog::EXFIL {
        for i in *items {
            let p = pat(i);
            let enabled = !off.contains(&&p);
            push(
                &mut rules,
                Rule {
                    pattern: p,
                    kind: RuleKind::Deny,
                    origin: netcatalog::EXFIL_ORIGIN.into(),
                    enabled,
                },
            );
        }
    }
    for i in netcatalog::INFERENCE {
        let p = pat(i);
        let enabled = !off.contains(&&p);
        push(
            &mut rules,
            Rule {
                pattern: p,
                kind: RuleKind::Allow,
                origin: netcatalog::INFERENCE_ORIGIN.into(),
                enabled,
            },
        );
    }
    for (g, items) in netcatalog::COMMON {
        let enabled = groups.iter().any(|x| x.as_str() == *g);
        for i in *items {
            push(
                &mut rules,
                Rule {
                    pattern: pat(i),
                    kind: RuleKind::Allow,
                    origin: netcatalog::common_origin(g),
                    enabled,
                },
            );
        }
    }
    for (l, o) in layers {
        for p in &l.deny {
            push(
                &mut rules,
                Rule {
                    pattern: p.clone(),
                    kind: RuleKind::Deny,
                    origin: rule_origin(o),
                    enabled: true,
                },
            );
        }
        for p in &l.allow {
            push(
                &mut rules,
                Rule {
                    pattern: p.clone(),
                    kind: RuleKind::Allow,
                    origin: rule_origin(o),
                    enabled: !allow_off.contains(&p),
                },
            );
        }
    }
    // Cadastrado e desligado sem `allow` em camada nenhuma: aparece desligado.
    for (l, o) in layers {
        for p in &l.allow_off {
            if !rules
                .iter()
                .any(|r| r.kind == RuleKind::Allow && &r.pattern == p)
            {
                rules.push(Rule {
                    pattern: p.clone(),
                    kind: RuleKind::Allow,
                    origin: rule_origin(o),
                    enabled: false,
                });
            }
        }
    }
    rules
}

/// Combina as camadas: baseline e preset (sem chaves hoje), global, perfil
/// nomeado, projeto e `run`.
pub fn resolve(cfg: &Config, host: &Host, req: &Request<'_>) -> Result<Effective, ResolveError> {
    let mut layers: Vec<(&Layer, Origin)> = vec![(&cfg.global, Origin::Global)];
    if let Some(name) = req.profile {
        let l = cfg
            .profiles
            .get(name)
            .ok_or_else(|| ResolveError::UnknownProfile {
                name: name.into(),
                available: cfg.profile_names(),
            })?;
        layers.push((l, Origin::Profile(name.into())));
    }
    let project = project_for(cfg, req.workdir);
    if let Some((p, l)) = project {
        layers.push((l, Origin::Project(p.clone())));
    }
    layers.push((req.run, Origin::Run));

    let default = |value| Setting {
        value,
        origin: Origin::Default,
    };
    let mut eff = Effective {
        extras: Vec::new(),
        pass_env: Vec::new(),
        repo: default(false),
        kvm: default(false),
        gpu: default(true),
        clipboard: default(false),
        harness_asks: default(false),
        portal: default(true),
        memory: LimitSetting {
            value: limits::DEFAULT_MEMORY,
            origin: Origin::Default,
        },
        processes: LimitSetting {
            value: limits::DEFAULT_PROCESSES,
            origin: Origin::Default,
        },
        network: LimitSetting {
            value: NetworkMode::Aberta,
            origin: Origin::Default,
        },
        window: LimitSetting {
            value: WindowMode::Off,
            origin: Origin::Default,
        },
        net_rules: Vec::new(),
        decrypt: Vec::new(),
        ssh_keys: Vec::new(),
        db: Vec::new(),
        ports: Vec::new(),
        host_ports: Vec::new(),
        file_deny: Vec::new(),
        file_allow: Vec::new(),
        secret_files: Vec::new(),
        secret_files_off: Vec::new(),
        mcp_deny: Vec::new(),
        mcp_allow: Vec::new(),
        profile: req.profile.map(String::from),
        project: project.map(|(p, _)| p.clone()),
    };
    eff.net_rules = net_rules(&layers);
    // As abas dos harnesses com login capturado decifram os hosts do login
    // (o Claude Code os da Anthropic, o `agy` os do Google): o login e a
    // renovação passam pela captura dos tokens.
    for h in sandmux_spec::net::capture_hosts(req.harness) {
        if let Ok(p) = h.parse::<Pattern>()
            && !eff.decrypt.contains(&p)
        {
            eff.decrypt.push(p);
        }
    }
    for (l, _) in &layers {
        for p in &l.decrypt {
            if !eff.decrypt.contains(p) {
                eff.decrypt.push(p.clone());
            }
        }
        for k in &l.ssh_keys {
            if !eff.ssh_keys.contains(k) {
                eff.ssh_keys.push(k.clone());
            }
        }
        for k in &l.db {
            if !eff.db.contains(k) {
                eff.db.push(k.clone());
            }
        }
        for p in &l.host_ports {
            if !eff.host_ports.contains(p) {
                eff.host_ports.push(*p);
            }
        }
        for r in &l.ports {
            // A mesma porta em duas camadas: vale a mais específica (a última).
            eff.ports.retain(|x| x.port != r.port);
            eff.ports.push(*r);
        }
        for (from, to) in [
            (&l.file_deny, &mut eff.file_deny),
            (&l.file_allow, &mut eff.file_allow),
            (&l.secret_files, &mut eff.secret_files),
            (&l.secret_files_off, &mut eff.secret_files_off),
            (&l.mcp_deny, &mut eff.mcp_deny),
            (&l.mcp_allow, &mut eff.mcp_allow),
        ] {
            for k in from {
                if !to.contains(k) {
                    to.push(k.clone());
                }
            }
        }
    }
    for (l, origin) in layers {
        for key in BOOL_KEYS {
            if let Some(v) = l.get_bool(key) {
                let s = match *key {
                    "repo" => &mut eff.repo,
                    "kvm" => &mut eff.kvm,
                    "gpu" => &mut eff.gpu,
                    "harness-asks" => &mut eff.harness_asks,
                    "portal" => &mut eff.portal,
                    _ => &mut eff.clipboard,
                };
                *s = Setting {
                    value: v,
                    origin: origin.clone(),
                };
            }
        }
        if let Some(m) = l.memory {
            eff.memory = LimitSetting {
                value: m,
                origin: origin.clone(),
            };
        }
        if let Some(p) = l.processes {
            eff.processes = LimitSetting {
                value: p,
                origin: origin.clone(),
            };
        }
        if let Some(n) = l.network {
            eff.network = LimitSetting {
                value: n,
                origin: origin.clone(),
            };
        }
        if let Some(w) = l.window {
            eff.window = LimitSetting {
                value: w,
                origin: origin.clone(),
            };
        }
        for (list, writable) in [(&l.ro, false), (&l.rw, true)] {
            for p in list {
                let path = expand(p, &host.home);
                // A camada mais específica decide; dentro de uma camada, `rw`
                // vem depois de `ro`.
                eff.extras.retain(|e| e.path != path);
                eff.extras.push(ExtraPath {
                    path,
                    writable,
                    origin: origin.clone(),
                });
            }
        }
        for n in &l.pass_env {
            if !eff.pass_env.iter().any(|(e, _)| e == n) {
                eff.pass_env.push((n.clone(), origin.clone()));
            }
        }
    }
    let _ = req.harness;
    if req.harness == crate::assistant::HARNESS {
        eff.confine_chat(&cfg.assistant);
    }
    Ok(eff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(home: &Path) -> Host {
        Host {
            home: home.into(),
            uid: 1000,
            gid: 1000,
            user: "ana".into(),
            group: "ana".into(),
            user_shell: "/bin/bash".into(),
            data_dir: home.join(".local/share/sandmux"),
            config_dir: home.join(".config/sandmux"),
            cwd: home.into(),
            env: Default::default(),
            sys_root: "/".into(),
        }
    }

    fn parse(text: &str) -> Result<Config, ConfigError> {
        Config::parse(Path::new("/c/config.toml"), text)
    }

    fn line(text: &str) -> usize {
        parse(text).unwrap_err().line.expect("linha")
    }

    #[test]
    fn as_tres_secoes_e_as_chaves() {
        let c = parse(
            r#"
# comentário
[global]
ro = ["~/docs", "/opt/ref"]
gpu = false

[profile.web]
clipboard = true
pass-env = ["OPENROUTER_API_KEY"]

[project."/home/ana/proj"]
rw = ["~/saida"]
kvm = true
repo = true
"#,
        )
        .unwrap();
        assert_eq!(
            c.global.ro,
            vec![PathBuf::from("~/docs"), "/opt/ref".into()]
        );
        assert_eq!(c.global.gpu, Some(false));
        assert_eq!(c.profiles["web"].clipboard, Some(true));
        assert_eq!(c.profiles["web"].pass_env, vec!["OPENROUTER_API_KEY"]);
        let p = &c.projects[Path::new("/home/ana/proj")];
        assert_eq!(p.kvm, Some(true));
        assert_eq!(p.repo, Some(true));
        assert_eq!(p.rw, vec![PathBuf::from("~/saida")]);
    }

    #[test]
    fn arquivo_ausente_e_vazio() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::load(&d.path().join("nada.toml")).unwrap(),
            Config::default()
        );
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn toml_malformado_com_linha() {
        let e = parse("[global]\nro = []\n\n[profile.x\nkvm = true\n").unwrap_err();
        assert_eq!(e.line, Some(4), "{e}");
        assert!(e.to_string().contains("/c/config.toml"), "{e}");
    }

    #[test]
    fn secao_desconhecida() {
        let t = "[global]\n\n[rede]\nmodo = 1\n";
        assert_eq!(line(t), 3);
        assert!(parse(t).unwrap_err().message.contains("rede"));
        assert!(parse("solto = true\n").is_err());
    }

    #[test]
    fn chave_desconhecida() {
        let t = "[global]\nkvm = true\nrede = \"aberta\"\n";
        let e = parse(t).unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("`rede`"), "{e}");
    }

    #[test]
    fn tipo_errado() {
        let t = "[global]\n\nkvm = \"sim\"\n";
        let e = parse(t).unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("`kvm` precisa ser true ou false"), "{e}");
        assert!(parse("[global]\nro = \"~/docs\"\n").is_err());
        assert!(parse("[global]\nro = [1]\n").is_err());
    }

    #[test]
    fn caminho_relativo() {
        let e = parse("[global]\nrw = [\n  \"/ok\",\n  \"saida\",\n]\n").unwrap_err();
        assert_eq!(e.line, Some(4), "{e}");
        assert!(parse("[global]\nro = [\"~\"]\n").is_err());
        assert!(parse("[global]\nro = [\"~ana/x\"]\n").is_err());
    }

    #[test]
    fn projeto_com_caminho_relativo() {
        assert_eq!(line("[global]\n[project.\"proj\"]\nkvm = true\n"), 2);
    }

    #[test]
    fn nome_de_variavel() {
        let e = parse("[global]\npass-env = [\"OPENROUTER_API_KEY=sk-123\"]\n").unwrap_err();
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("nunca valores"), "{e}");
        // O valor nunca aparece inteiro? Aparece o item recusado, que é o que
        // o usuário escreveu: a mensagem precisa apontar qual é.
        assert!(parse("[global]\npass-env = [\"1X\"]\n").is_err());
        assert!(parse("[global]\npass-env = [\"_X1\"]\n").is_ok());
    }

    #[test]
    fn nome_de_perfil() {
        assert_eq!(line("[global]\n\n[profile.Web]\n"), 3);
        assert!(parse("[profile.web-2_x]\n").is_ok());
    }

    #[test]
    fn mesmo_caminho_em_ro_e_rw() {
        let e = parse("[global]\nro = [\"~/docs\"]\nrw = [\"~/docs/\"]\n").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("ro") && e.message.contains("rw"), "{e}");
    }

    fn req<'a>(workdir: &'a Path, profile: Option<&'a str>, run: &'a Layer) -> Request<'a> {
        Request {
            harness: "shell",
            workdir,
            profile,
            run,
        }
    }

    #[test]
    fn abas_com_login_decifram_os_hosts_do_login() {
        let h = host(Path::new("/home/ana"));
        let c = parse("[global]\n").unwrap();
        let run = Layer::default();
        let decrypt = |harness: &'static str| {
            let mut r = req(Path::new("/w"), None, &run);
            r.harness = harness;
            resolve(&c, &h, &r)
                .unwrap()
                .decrypt
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
        };
        assert!(decrypt("agy").contains(&"*.googleapis.com".to_string()));
        assert!(decrypt("claude").contains(&"platform.claude.com".to_string()));
        assert!(!decrypt("shell").iter().any(|d| d.contains("googleapis")));
    }

    #[test]
    fn aba_de_chat_so_inferencia() {
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\nnetwork = \"aberta\"\nrepo = true\nro = [\"/opt\"]\npass-env = [\"X\"]\ndeny = [\"openai.com\"]\n[assistant.provider.local]\nurl = \"https://llm.local:8443/v1\"\n",
        )
        .unwrap();
        let run = Layer::default();
        let mut r = req(Path::new("/w"), None, &run);
        r.harness = crate::assistant::HARNESS;
        let e = resolve(&c, &h, &r).unwrap();
        assert_eq!(e.network.value, NetworkMode::Inferencia);
        assert!(e.extras.is_empty() && e.pass_env.is_empty() && !e.repo.value);
        let p = e.net_policy().unwrap();
        let has = |s: &str, k: RuleKind| {
            p.rules
                .iter()
                .any(|x| x.pattern.to_string() == s && x.kind == k)
        };
        assert!(has("openrouter.ai", RuleKind::Allow));
        assert!(has("llm.local", RuleKind::Allow));
        assert!(has("openai.com", RuleKind::Deny));
        // Outra aba continua com a política dela.
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(e.network.value, NetworkMode::Aberta);
        assert!(
            !e.net_rules
                .iter()
                .any(|x| x.origin == crate::assistant::ORIGIN)
        );
    }

    #[test]
    fn camadas_e_padroes() {
        let home = Path::new("/home/ana");
        let h = host(home);
        let run = Layer::default();
        let e = resolve(&Config::default(), &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert!(e.gpu.value && !e.kvm.value && !e.clipboard.value && !e.repo.value);
        assert_eq!(e.gpu.origin, Origin::Default);
        assert!(e.extras.is_empty());

        let c = parse("[global]\ngpu = false\nkvm = true\n").unwrap();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert!(!e.gpu.value);
        assert_eq!(e.gpu.origin, Origin::Global);
        assert!(e.kvm.value);
    }

    #[test]
    fn precedencia_liga_desliga() {
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\ngpu = false\n[profile.vm]\nkvm = true\n[project.\"/w\"]\ngpu = true\nkvm = true\n",
        )
        .unwrap();
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert!(e.gpu.value);
        assert_eq!(e.gpu.origin, Origin::Project("/w".into()));
        // O run vence o projeto.
        let run = Layer {
            kvm: Some(false),
            ..Default::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), Some("vm"), &run)).unwrap();
        assert!(!e.kvm.value);
        assert_eq!(e.kvm.origin, Origin::Run);
        // O projeto vence o perfil nomeado.
        let c2 = parse("[profile.vm]\nkvm = true\n[project.\"/w\"]\nkvm = false\n").unwrap();
        let e = resolve(&c2, &h, &req(Path::new("/w"), Some("vm"), &none)).unwrap();
        assert!(!e.kvm.value);
    }

    #[test]
    fn listas_acumuladas() {
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\nro = [\"~/docs\"]\nrw = [\"~/saida\"]\npass-env = [\"A\"]\n[project.\"/w\"]\nro = [\"~/ref\", \"/home/ana/saida\"]\npass-env = [\"A\", \"B\"]\n",
        )
        .unwrap();
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(
            e.ro(),
            vec![
                PathBuf::from("/home/ana/docs"),
                "/home/ana/ref".into(),
                "/home/ana/saida".into()
            ]
        );
        assert!(e.rw().is_empty());
        assert_eq!(e.pass_env_names(), vec!["A", "B"]);
        assert_eq!(e.pass_env[1].1, Origin::Project("/w".into()));
        // Fora do projeto, `saida` continua com escrita.
        let e = resolve(&c, &h, &req(Path::new("/x"), None, &none)).unwrap();
        assert_eq!(e.rw(), vec![PathBuf::from("/home/ana/saida")]);
    }

    #[test]
    fn perfil_de_projeto_pela_pasta() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        for p in ["proj/api/src", "proj2"] {
            std::fs::create_dir_all(base.join(p)).unwrap();
        }
        std::os::unix::fs::symlink(base.join("proj"), base.join("atalho")).unwrap();
        let text = format!(
            "[project.\"{b}/proj\"]\nkvm = true\n[project.\"{b}/proj/api\"]\nclipboard = true\n",
            b = base.display()
        );
        let c = parse(&text).unwrap();
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let at = |w: &str| resolve(&c, &h, &req(&base.join(w), None, &none)).unwrap();
        // Subpasta.
        let e = at("proj/api/../..//proj");
        assert_eq!(e.project, Some(base.join("proj")));
        // Aninhados: só o mais próximo.
        let e = at("proj/api/src");
        assert_eq!(e.project, Some(base.join("proj/api")));
        assert!(e.clipboard.value && !e.kvm.value);
        // /proj não casa com /proj2.
        assert_eq!(at("proj2").project, None);
        // Link para a pasta.
        assert_eq!(at("atalho").project, Some(base.join("proj")));
    }

    #[test]
    fn perfil_inexistente() {
        let c = parse("[profile.vm]\n[profile.web]\n").unwrap();
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), Some("nada"), &none)).unwrap_err();
        let s = e.to_string();
        assert!(s.contains("nada") && s.contains("vm, web"), "{s}");
    }

    /// HOME de teste com `docs` e a pasta de configuração.
    fn compose_with(text: &str, workdir: &str) -> Result<crate::Composed, crate::PolicyError> {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().canonicalize().unwrap();
        for p in ["docs", ".config/sandmux", "proj", ".ssh"] {
            std::fs::create_dir_all(home.join(p)).unwrap();
        }
        std::fs::write(home.join(".config/sandmux/config.toml"), "").unwrap();
        let mut h = host(&home);
        h.cwd = home.join("proj");
        let c = parse(text).unwrap();
        let wd = home.join(workdir);
        let none = Layer::default();
        let e = resolve(&c, &h, &req(&wd, None, &none)).unwrap();
        let r = crate::compose(&h, &e.run_options("shell", &wd, &[], LayerMode::Temporary));
        drop(d);
        r
    }

    #[test]
    fn caminhos_da_configuracao_passam_pela_composicao() {
        let c = compose_with("[global]\nro = [\"~/docs\"]\n", "proj").unwrap();
        let ro = c.spec.mounts.iter().any(|m| {
            matches!(m,
            sandmux_spec::Mount::Bind { source, writable: false, .. } if source.ends_with("docs"))
        });
        assert!(ro, "{:?}", c.spec.mounts);
        assert!(matches!(
            compose_with("[global]\nro = [\"~/nada\"]\n", "proj"),
            Err(crate::PolicyError::MissingPath(_))
        ));
        // Aviso de credencial também para caminho vindo da configuração.
        let c = compose_with("[global]\nro = [\"~/.ssh\"]\n", "proj").unwrap();
        assert!(
            c.warnings.iter().any(|w| w.contains("credenciais reais")),
            "{:?}",
            c.warnings
        );
    }

    #[test]
    fn pasta_de_configuracao_sem_escrita() {
        let e = compose_with("[global]\nrw = [\"~/.config\"]\n", "proj").unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
        assert!(compose_with("[global]\nro = [\"~/.config\"]\n", "proj").is_ok());
        let e = compose_with("", ".config/sandmux").unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
        let e = compose_with(
            "[global]\nrw = [\"~/.config/sandmux/config.toml\"]\n",
            "proj",
        )
        .unwrap_err();
        assert!(e.to_string().contains("configuração do sandmux"), "{e}");
    }

    #[test]
    fn tetos_no_arquivo() {
        let c = parse("[global]\nmemory = \"8G\"\nprocesses = 512\n[profile.x]\nprocesses = \"max\"\nmemory = \"50%\"\n").unwrap();
        assert_eq!(c.global.memory, Some(MemoryLimit::Bytes(8 << 30)));
        assert_eq!(c.global.processes, Some(ProcessLimit::Count(512)));
        assert_eq!(c.profiles["x"].processes, Some(ProcessLimit::Max));
        assert_eq!(c.profiles["x"].memory, Some(MemoryLimit::Percent(50)));
        let e = parse("[global]\n\nmemory = \"muito\"\n").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(
            e.message.contains("\"50%\"") && e.message.contains("max"),
            "{e}"
        );
        let e = parse("[global]\nprocesses = 0\n").unwrap_err();
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("mínimo"), "{e}");
        assert!(parse("[global]\nmemory = 8\n").is_err());
        assert!(parse("[global]\nprocesses = true\n").is_err());
        assert!(parse("[global]\nprocesses = -3\n").is_err());
    }

    #[test]
    fn tetos_nas_camadas() {
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let e = resolve(&Config::default(), &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.memory.value, MemoryLimit::Percent(75));
        assert_eq!(e.processes.value, ProcessLimit::Count(8192));
        assert_eq!(e.memory.origin, Origin::Default);
        let total = 46 * (1u64 << 30);
        assert_eq!(
            e.limits(total),
            Limits {
                memory: Some(total / 100 * 75),
                processes: Some(8192)
            }
        );
        // O projeto vence o global.
        let c = parse("[global]\nmemory = \"8G\"\n[project.\"/w\"]\nmemory = \"max\"\n").unwrap();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.memory.value, MemoryLimit::Max);
        assert_eq!(e.memory.origin, Origin::Project("/w".into()));
        assert_eq!(e.limits(total).memory, None);
        // O run vence o projeto.
        let run = Layer {
            memory: Some(MemoryLimit::Bytes(1 << 30)),
            processes: Some(ProcessLimit::Count(64)),
            ..Default::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(
            e.limits(total),
            Limits {
                memory: Some(1 << 30),
                processes: Some(64)
            }
        );
        assert_eq!(e.processes.origin, Origin::Run);
    }

    #[test]
    fn secao_do_assistente() {
        let c = parse(
            "[assistant]\nchat-model = \"openrouter/deepseek/x\"\nmodels = [\"local/llama3\"]\n[assistant.provider.local]\nurl = \"http://host.sandmux.internal:11434/v1/\"\n",
        )
        .unwrap();
        assert_eq!(
            c.assistant.chat_model.as_deref(),
            Some("openrouter/deepseek/x")
        );
        assert_eq!(c.assistant.models, vec!["local/llama3"]);
        assert_eq!(
            c.assistant.providers["local"],
            "http://host.sandmux.internal:11434/v1"
        );
        // Não é camada de política.
        assert_eq!(c.global, Layer::default());
        let e = parse("[assistant]\nchat-model = \"x/y\"\ncor = 1\n").unwrap_err();
        assert!(e.message.contains("provedor desconhecido"), "{}", e.message);
        let e = parse("[assistant]\ncor = 1\n").unwrap_err();
        assert!(e.message.contains("`cor`") && e.line == Some(2), "{e:?}");
        assert!(parse("[assistant]\nchat-model = \"semprovedor\"\n").is_err());
        assert!(parse("[assistant.provider.openrouter]\nurl = \"https://x\"\n").is_err());
        assert!(parse("[assistant.provider.l]\nurl = \"ftp://x\"\n").is_err());
        assert!(parse("[assistant.provider.l]\n").is_err());
        // A tecla escolhida vence a padrão de outra ação, que fica sem tecla.
        let c = parse("[tui.keys]\nnew = \"t\"\n").unwrap();
        assert_eq!(c.tui.action_for('t'), Some("new"));
        assert_eq!(c.tui.key("enter"), '\0');
        assert!(parse("[tui.keys]\nnew = \"t\"\nclose = \"t\"\n").is_err());
        let c = parse("[tui.keys]\nchat = \"y\"\n").unwrap();
        assert_eq!(c.tui.action_for('y'), Some("chat"));
        assert_eq!(TuiConfig::default().key("chat"), 'i');
        assert_eq!(TuiConfig::default().key("command"), 'g');
        let d = TuiConfig::default();
        assert_eq!((d.style, d.bar_top), (BarStyle::Blocos, false));
        let c = parse("[tui]\nstyle = \"simples\"\nbar = \"top\"\n").unwrap();
        assert_eq!((c.tui.style, c.tui.bar_top), (BarStyle::Simples, true));
        assert!(parse("[tui]\nstyle = \"bonito\"\n").is_err());
        assert!(parse("[tui]\nbar = \"lado\"\n").is_err());
        let c =
            parse("[tui]\npanel-order = [\"rede\", \"mcp\"]\npanel-collapsed = [\"protecao\"]\n")
                .unwrap();
        assert_eq!(
            c.tui.panel_order,
            vec!["rede", "mcp", "protecao", "recursos", "portas"]
        );
        assert_eq!(c.tui.panel_collapsed, vec!["protecao"]);
        assert!(parse("[tui]\npanel-order = [\"rede\", \"rede\"]\n").is_err());
        assert!(parse("[tui]\npanel-collapsed = [\"outra\"]\n").is_err());
        assert!(parse("[tui.colors]\nchip-time-bg = \"#89dceb\"\n").is_ok());
        let c = parse("[assistant]\ncommand-model = \"openai/x\"\n").unwrap();
        assert_eq!(c.assistant.command_model.as_deref(), Some("openai/x"));
        assert_eq!(
            c.assistant.default_command_model().as_deref(),
            Some("openai/x")
        );
        assert!(parse("[assistant]\ncommand-model = \"nada/x\"\n").is_err());
    }

    #[test]
    fn secao_da_tui() {
        let c = parse("[tui]\nprefix = \"C-a\"\n[tui.keys]\nnew = \"t\"\n").unwrap();
        assert_eq!(c.tui.prefix, 'a');
        assert_eq!(c.tui.action_for('t'), Some("new"));
        assert_eq!(c.tui.action_for('c'), None);
        assert_eq!(c.tui.key("next"), 'n');
        assert_eq!(Config::default().tui.action_for('x'), Some("close"));
        // A seção não muda a política.
        assert_eq!(c.global, Layer::default());
        let e = parse("[tui]\nprefix = \"C-a\"\ncor = 1\n").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("`cor`"), "{e}");
        assert!(parse("[tui]\nprefix = \"b\"\n").is_err());
        assert!(parse("[tui]\nprefix = \"C-B\"\n").is_err());
        assert!(parse("[tui.keys]\nvoar = \"z\"\n").is_err());
        assert!(parse("[tui.keys]\nnew = \"1\"\n").is_err());
        assert!(parse("[tui.keys]\nnew = \"ab\"\n").is_err());
        // Duas teclas escolhidas iguais são erro; a padrão de outra ação cede.
        let e = parse("[tui.keys]\nnext = \"x\"\nclose = \"x\"\n").unwrap_err();
        assert!(
            e.message.contains("close") && e.message.contains("next"),
            "{e}"
        );
        let c = parse("[tui.keys]\nnext = \"x\"\n").unwrap();
        assert_eq!(
            (c.tui.action_for('x'), c.tui.key("close")),
            (Some("next"), '\0')
        );
    }

    #[test]
    fn tema_e_cores() {
        assert_eq!(Config::default().tui.theme.name, "padrao");
        assert!(Config::default().tui.theme.colors.is_empty());
        let c = parse("[tui]\ntheme = \"escuro\"\n[tui.colors]\ntab-active-bg = \"#aa0000\"\ntab-fg = 3\nbar-bg = \"bright-blue\"\nerror-fg = \"default\"\n").unwrap();
        assert_eq!(c.tui.theme.name, "escuro");
        assert_eq!(
            c.tui.theme.get("tab-active-bg"),
            Some(ThemeColor::Rgb(0xaa, 0, 0))
        );
        assert_eq!(c.tui.theme.get("tab-fg"), Some(ThemeColor::Indexed(3)));
        assert_eq!(c.tui.theme.get("bar-bg"), Some(ThemeColor::Indexed(12)));
        assert_eq!(c.tui.theme.get("error-fg"), Some(ThemeColor::Default));
        assert_eq!(c.tui.theme.get("status-fg"), ThemeColor::parse("#f9e2af"));
        let e = parse("[tui]\n\ntheme = \"neon\"\n").unwrap_err();
        assert_eq!(e.line, Some(3));
        assert!(e.message.contains("escuro"), "{e}");
        let e = parse("[tui.colors]\ntab-bg = \"roxo-claro\"\n").unwrap_err();
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("#rrggbb"), "{e}");
        assert!(parse("[tui.colors]\nborda = \"red\"\n").is_err());
        assert!(parse("[tui.colors]\ntab-bg = \"#12345\"\n").is_err());
        assert!(parse("[tui.colors]\ntab-bg = 300\n").is_err());
        for t in THEMES {
            assert!(Theme::builtin(t).is_some());
        }
        assert_eq!(
            Theme::builtin("escuro").unwrap().colors.len(),
            THEME_ELEMENTS.len()
        );
        assert!(Config::default().tui.panel);
        assert!(!parse("[tui]\npanel = false\n").unwrap().tui.panel);
        assert!(parse("[tui]\npanel = \"sim\"\n").is_err());
        assert_eq!(Config::default().tui.action_for('s'), Some("panel"));
    }

    #[test]
    fn rede_nas_camadas_e_retencao() {
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let e = resolve(&Config::default(), &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.network.value, NetworkMode::Aberta);
        let c = parse("[global]\nnet-log-days = 7\nnet-log-max = \"1G\"\n[project.\"/w\"]\nnetwork = \"sem-rede\"\n").unwrap();
        assert_eq!((c.net_log_days, c.net_log_max), (Some(7), Some(1 << 30)));
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.network.value, NetworkMode::SemRede);
        assert_eq!(e.network.origin, Origin::Project("/w".into()));
        let run = Layer {
            network: Some(NetworkMode::Aberta),
            ..Default::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(e.network.value, NetworkMode::Aberta);
        assert!(parse("[global]\nnetwork = \"fechada\"\n").is_err());
        let e = parse("[profile.x]\nnet-log-days = 3\n").unwrap_err();
        assert!(e.message.contains("[global]"), "{e}");
        assert!(parse("[global]\nnet-log-days = 0\n").is_err());
        assert!(parse("[global]\nnet-log-max = \"10M\"\n").is_err());
    }

    #[test]
    fn politica_de_arquivos() {
        use sandmux_spec::files::FileVerdict;
        let h = host(Path::new("/home/ana"));
        let c = parse("[global]\nfile-deny = [\"*.sql\"]\nsecret-files = [\"*.cred\"]\n[project.\"/w\"]\nfile-allow = [\"/w/ok.sql\"]\nsecret-files-off = [\".npmrc\"]\nfile-deny = [\"~/privado\"]\n")
            .unwrap();
        let run = Layer {
            file_deny: vec!["dump".into()],
            ..Layer::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(e.file_deny, vec!["*.sql", "~/privado", "dump"]);
        let p = e.file_policy(Path::new("/home/ana"), Path::new("/w"));
        let none = |_: &Path| false;
        assert!(matches!(
            p.decide(Path::new("/w/a.sql"), none),
            FileVerdict::Deny(_)
        ));
        assert_eq!(p.decide(Path::new("/w/ok.sql"), none), FileVerdict::Pass);
        assert!(matches!(
            p.decide(Path::new("/home/ana/privado/x"), none),
            FileVerdict::Deny(_)
        ));
        assert_eq!(p.decide(Path::new("/w/a.cred"), none), FileVerdict::Secret);
        assert_eq!(p.decide(Path::new("/w/.npmrc"), none), FileVerdict::Pass);
        assert_eq!(p.decide(Path::new("/w/.env"), none), FileVerdict::Secret);
        assert!(parse("[global]\nfile-deny = [\"\"]\n").is_err());
        assert!(parse("[global]\nfile-allow = [\"a/b\"]\n").is_err());
        assert!(
            c.global
                .describe()
                .contains(&"file-deny = [\"*.sql\"]".to_string())
        );
    }

    #[test]
    fn portas_de_entrada() {
        let h = host(Path::new("/home/ana"));
        let c = parse("[global]\nports = [\"8642\"]\n[project.\"/w\"]\nports = [\"8642@0.0.0.0\", \"3000\"]\n").unwrap();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &Layer::default())).unwrap();
        let v: Vec<String> = e.ports.iter().map(|p| p.to_string()).collect();
        assert_eq!(v, vec!["8642@0.0.0.0", "3000"]);
        assert_eq!(e.net_policy().unwrap().ports.len(), 2);
        assert!(parse("[global]\nports = [\"abc\"]\n").is_err());
    }

    #[test]
    fn chaves_ssh_somadas() {
        let h = host(Path::new("/home/ana"));
        let c = parse("[global]\nssh-keys = [\"github\"]\n[project.\"/w\"]\nssh-keys = [\"trabalho\", \"github\"]\n")
            .unwrap();
        assert_eq!(
            c.global.describe(),
            vec!["ssh-keys = [\"github\"]".to_string()]
        );
        let run = Layer {
            ssh_keys: vec!["*".into()],
            ..Layer::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(e.ssh_keys, vec!["github", "trabalho", "*"]);
        let e = resolve(&c, &h, &req(Path::new("/x"), None, &Layer::default())).unwrap();
        assert_eq!(e.ssh_keys, vec!["github"]);
        assert!(parse("[global]\nssh-keys = [\"com espaço\"]\n").is_err());
        assert!(parse("[global]\nssh-keys = \"github\"\n").is_err());
    }

    #[test]
    fn chaves_da_politica_de_rede() {
        use sandmux_spec::net::{PolicyMode, Verdict};
        let h = host(Path::new("/home/ana"));
        let c = parse(
            "[global]\ndeny = [\"A.test\"]\ncatalog-off = [\"pastebin.com\", \"api.openai.com\"]\nallow-off = [\"off.test\"]\n[project.\"/w\"]\nnetwork = \"custom\"\nallow = [\"*.exemplo.com\", \"off.test\"]\ncatalog = [\"npm\"]\ndeny = [\"api.exemplo.com\"]\n",
        )
        .unwrap();
        assert_eq!(c.global.deny, vec!["a.test".parse().unwrap()]);
        let d =
            parse("[global]\ndecrypt = [\"a.test\"]\n[project.\"/w\"]\ndecrypt = [\"*.b.test\"]\n")
                .unwrap();
        let run = Layer {
            decrypt: vec!["c.test".parse().unwrap()],
            ..Layer::default()
        };
        let e = resolve(&d, &h, &req(Path::new("/w"), None, &run)).unwrap();
        let pol = e.net_policy().unwrap();
        assert!(pol.decrypts("a.test") && pol.decrypts("x.b.test") && pol.decrypts("c.test"));
        assert!(!pol.decrypts("b.test"));
        assert!(parse("[global]\ndecrypt = [\"https://x\"]\n").is_err());
        let run = Layer {
            deny: vec!["b.test".parse().unwrap()],
            ..Layer::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &run)).unwrap();
        assert_eq!(e.network.value, NetworkMode::Custom);
        let find = |p: &str, k: RuleKind| {
            e.net_rules
                .iter()
                .find(|r| r.pattern.to_string() == p && r.kind == k)
                .unwrap_or_else(|| panic!("{p}"))
                .clone()
        };
        assert_eq!(find("a.test", RuleKind::Deny).origin, "global");
        assert_eq!(find("b.test", RuleKind::Deny).origin, "aba");
        assert_eq!(find("api.exemplo.com", RuleKind::Deny).origin, "projeto /w");
        assert!(!find("pastebin.com", RuleKind::Deny).enabled);
        assert!(find("paste.ee", RuleKind::Deny).enabled);
        assert!(!find("off.test", RuleKind::Allow).enabled);
        assert!(find("registry.npmjs.org", RuleKind::Allow).enabled);
        assert!(!find("pypi.org", RuleKind::Allow).enabled);
        assert!(!find("api.openai.com", RuleKind::Allow).enabled);
        // Custom: allowlist e catálogo comum; sem o catálogo de inferência.
        let pol = e.net_policy().unwrap();
        assert_eq!(pol.mode, PolicyMode::Custom);
        assert_eq!(pol.decide_name("www.exemplo.com"), Verdict::Pass);
        assert!(matches!(
            pol.decide_name("api.exemplo.com"),
            Verdict::Block(_)
        ));
        assert_eq!(pol.decide_name("registry.npmjs.org"), Verdict::Pass);
        assert!(matches!(
            pol.decide_name("openrouter.ai"),
            Verdict::Block(_)
        ));
        assert!(matches!(pol.decide_name("off.test"), Verdict::Block(_)));
        // Aberta: só a denylist.
        let open = Layer {
            network: Some(NetworkMode::Aberta),
            ..Layer::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &open)).unwrap();
        let pol = e.net_policy().unwrap();
        assert!(pol.rules.iter().all(|r| r.kind == RuleKind::Deny));
        assert_eq!(pol.decide_name("pastebin.com"), Verdict::Pass);
        assert!(
            matches!(pol.decide_name("paste.ee"), Verdict::Block(m) if m.contains("exfiltração"))
        );
        assert_eq!(pol.decide_name("qualquer.test"), Verdict::Pass);
        // Inferência: só o catálogo de provedores, menos os desligados.
        let inf = Layer {
            network: Some(NetworkMode::Inferencia),
            ..Layer::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &inf)).unwrap();
        let pol = e.net_policy().unwrap();
        assert_eq!(pol.decide_name("openrouter.ai"), Verdict::Pass);
        assert!(matches!(
            pol.decide_name("api.openai.com"),
            Verdict::Block(_)
        ));
        assert!(matches!(
            pol.decide_name("registry.npmjs.org"),
            Verdict::Block(_)
        ));
        // Sem rede: sem política de gateway.
        let none = Layer {
            network: Some(NetworkMode::SemRede),
            ..Layer::default()
        };
        assert!(
            resolve(&c, &h, &req(Path::new("/w"), None, &none))
                .unwrap()
                .net_policy()
                .is_none()
        );
        // Erros.
        for bad in [
            "[global]\ndeny = [\"http://x.test/a\"]\n",
            "[global]\ncatalog = [\"nada\"]\n",
            "[global]\nnetwork = \"fechada\"\n",
            "[global]\nallow = \"x.test\"\n",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let e = parse("[global]\nnetwork = \"fechada\"\n").unwrap_err();
        assert!(
            e.message.contains("inferencia") && e.message.contains("custom"),
            "{e}"
        );
        assert!(
            c.projects
                .values()
                .next()
                .unwrap()
                .describe()
                .iter()
                .any(|l| l == "catalog = [\"npm\"]")
        );
    }

    #[test]
    fn origem_mostrada() {
        assert_eq!(Origin::Default.to_string(), "padrão");
        assert_eq!(
            Origin::Project("/home/ana/proj".into()).to_string(),
            "modificada (projeto /home/ana/proj)"
        );
    }

    #[test]
    fn janela_em_tres_estados_e_precedencia() {
        let c = parse("[global]\nwindow = true\n[project.\"/p\"]\nwindow = \"direct\"\n").unwrap();
        assert_eq!(c.global.window, Some(WindowMode::On));
        assert!(c.global.describe().contains(&"window = true".to_string()));
        let e = parse("[global]\nwindow = \"sim\"\n").unwrap_err();
        assert!(e.message.contains("`window` precisa ser"), "{e}");
        let e = parse("[global]\nwindow = 1\n").unwrap_err();
        assert!(e.message.contains("`window` precisa ser"), "{e}");
        let mut l = Layer {
            window: Some(WindowMode::Direct),
            ..Default::default()
        };
        assert!(l.describe().contains(&"window = \"direct\"".to_string()));
        l.window = Some(WindowMode::Off);
        assert!(l.describe().contains(&"window = false".to_string()));

        // O projeto vence o global, e o `run` vence os dois; a aba de chat
        // nunca tem janela.
        let h = host(Path::new("/home/ana"));
        let none = Layer::default();
        let e = resolve(&c, &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.window.value, WindowMode::On);
        let e = resolve(&c, &h, &req(Path::new("/p"), None, &none)).unwrap();
        assert_eq!(e.window.value, WindowMode::Direct);
        let off = Layer {
            window: Some(WindowMode::Off),
            ..Default::default()
        };
        let e = resolve(&c, &h, &req(Path::new("/p"), None, &off)).unwrap();
        assert_eq!(e.window.value, WindowMode::Off);
        let mut r = req(Path::new("/w"), None, &none);
        r.harness = crate::assistant::HARNESS;
        assert_eq!(resolve(&c, &h, &r).unwrap().window.value, WindowMode::Off);
        let e = resolve(&Config::default(), &h, &req(Path::new("/w"), None, &none)).unwrap();
        assert_eq!(e.window.value, WindowMode::Off);
    }
}
