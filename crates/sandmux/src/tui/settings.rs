//! Página de configurações: as opções do `config.toml` em seções, com o
//! valor atual de cada uma e a gravação pelo editor de configuração (o
//! mesmo do `sandmux config`), que valida o arquivo inteiro antes de gravar.

use std::path::{Path, PathBuf};

use sandmux_policy::Host;
use sandmux_policy::config::edit::{self, Op, Target};
use sandmux_spec::{tr, trf};
use toml_edit::DocumentMut;

/// Onde a chave mora no arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    /// Camada (`[global]` ou `[project."…"]`), conforme o alvo.
    Layer,
    Tui,
    Assistant,
    /// Só em `[global]`.
    GlobalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Toggle,
    Choice(&'static [&'static str]),
    /// Texto livre; com `Layer`, um teto (`memory`, `processes`).
    Text,
    List,
}

#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub section: usize,
    pub key: &'static str,
    /// Rótulo em português e em inglês.
    pub labels: [&'static str; 2],
    /// Ajuda em português e em inglês.
    pub helps: [&'static str; 2],
    pub kind: Kind,
    pub at: At,
}

impl Item {
    pub fn label(&self) -> &'static str {
        sandmux_spec::tr!(self.labels[0], self.labels[1])
    }

    pub fn help(&self) -> &'static str {
        sandmux_spec::tr!(self.helps[0], self.helps[1])
    }
}

/// Seções da página, em português e em inglês.
pub const SECTIONS: &[[&str; 2]] = &[
    ["Geral", "General"],
    ["Aparência", "Appearance"],
    ["Atalhos", "Shortcuts"],
    ["Sandbox", "Sandbox"],
    ["Rede", "Network"],
    ["Arquivos", "Files"],
    ["MCP", "MCP"],
    ["Assistente", "Assistant"],
    ["Registro", "Logging"],
    ["Cofre", "Vault"],
    ["Diagnóstico", "Diagnosis"],
    ["Processos", "Processes"],
];

/// A seção do cofre: não tem itens do arquivo, e sim a tela do cofre.
pub const VAULT: usize = 9;

/// A seção do diagnóstico: as verificações do `sandmux doctor`.
pub const DOCTOR: usize = 10;

/// A seção da árvore de processos da aba ativa.
pub const PROCS: usize = 11;

/// O nome de uma seção no idioma atual.
pub fn section_name(i: usize) -> &'static str {
    SECTIONS
        .get(i)
        .map(|s| sandmux_spec::tr!(s[0], s[1]))
        .unwrap_or("")
}

const LANGUAGES: &[&str] = &["auto", "pt", "en"];

const STYLES: &[&str] = &["blocos", "powerline", "simples"];
const BARS: &[&str] = &["bottom", "top"];
const GRAPH_EDGES: &[&str] = &["braille", "blocos"];
const NETWORK: &[&str] = &["aberta", "inferencia", "custom", "sem-rede"];
const WINDOW: &[&str] = &["false", "true", "direct"];

macro_rules! item {
    ($s:expr, $k:expr, $l:expr, $kind:expr, $at:expr, $h:expr) => {
        Item {
            section: $s,
            key: $k,
            labels: $l,
            helps: $h,
            kind: $kind,
            at: $at,
        }
    };
}

pub const ITEMS: &[Item] = &[
    item!(
        0,
        "language",
        ["Idioma", "Language"],
        Kind::Choice(LANGUAGES),
        At::GlobalOnly,
        [
            "Idioma do sandmux inteiro: TUI, linha de comando e mensagens. auto segue o sistema (LANG).",
            "Language of the whole sandmux: TUI, command line and messages. auto follows the system (LANG)."
        ]
    ),
    item!(
        0,
        "screen-history",
        ["Histórico em disco", "History on disk"],
        Kind::Toggle,
        At::GlobalOnly,
        [
            "Guarda a saída das abas em disco, cifrada com uma chave do cofre, para ela voltar depois de reiniciar o daemon. Sem cofre, nada é gravado.",
            "Keeps the tabs' output on disk, encrypted with a key from the vault, so it comes back after restarting the daemon. Without a vault, nothing is saved."
        ]
    ),
    item!(
        0,
        "notify",
        ["Notificações do desktop", "Desktop notifications"],
        Kind::Toggle,
        At::GlobalOnly,
        [
            "Avisa no desktop bloqueios de rede, chaves reais e abas encerradas, quando a TUI não está em foco naquela aba.",
            "Notifies on the desktop of network blocks, real keys and ended tabs, when the TUI is not focused on that tab."
        ]
    ),
    item!(
        1,
        "theme",
        ["Tema", "Theme"],
        Kind::Choice(sandmux_policy::config::THEMES),
        At::Tui,
        [
            "Cores da barra, dos modais, do painel e desta página. Vale na hora.",
            "Colors of the bar, dialogs, panel and this page. Applies right away."
        ]
    ),
    item!(
        1,
        "style",
        ["Estilo da barra", "Bar style"],
        Kind::Choice(STYLES),
        At::Tui,
        [
            "blocos funciona em qualquer fonte; powerline precisa de uma Nerd Font; simples é só texto.",
            "blocks (blocos in the file) works with any font; powerline needs a Nerd Font; plain (simples) is just text."
        ]
    ),
    item!(
        1,
        "bar",
        ["Posição da barra", "Bar position"],
        Kind::Choice(BARS),
        At::Tui,
        [
            "bottom põe a barra na última linha; top, na primeira.",
            "bottom puts the bar on the last line; top, on the first."
        ]
    ),
    item!(
        1,
        "panel",
        ["Painel lateral", "Side panel"],
        Kind::Toggle,
        At::Tui,
        [
            "Mostra o painel de proteção, recursos e rede ao abrir a TUI.",
            "Shows the protection, resources and network panel when the TUI opens."
        ]
    ),
    item!(
        1,
        "graph-edges",
        ["Arestas do grafo", "Graph edges"],
        Kind::Choice(GRAPH_EDGES),
        At::Tui,
        [
            "Como o grafo do cofre desenha as arestas: braille é mais fino; blocos usa meio-bloco, para fontes sem Braille.",
            "How the vault graph draws its edges: braille is finer; blocks (blocos in the file) uses half blocks, for fonts without Braille."
        ]
    ),
    item!(
        2,
        "prefix",
        ["Tecla de prefixo", "Prefix key"],
        Kind::Text,
        At::Tui,
        [
            "Ctrl com uma letra minúscula, como C-b. As teclas de cada ação ficam em [tui.keys] (docs/tui.md).",
            "Ctrl plus a lowercase letter, such as C-b. Each action's key lives in [tui.keys] (docs/tui.md)."
        ]
    ),
    item!(
        3,
        "repo",
        ["Repositório", "Repository"],
        Kind::Toggle,
        At::Layer,
        [
            "Monta o repositório git da pasta de trabalho inteiro, não só a pasta.",
            "Mounts the whole git repository of the work folder, not just the folder."
        ]
    ),
    item!(
        3,
        "kvm",
        ["KVM", "KVM"],
        Kind::Toggle,
        At::Layer,
        [
            "Dá acesso a /dev/kvm (máquinas virtuais e emuladores).",
            "Gives access to /dev/kvm (virtual machines and emulators)."
        ]
    ),
    item!(
        3,
        "gpu",
        ["GPU", "GPU"],
        Kind::Toggle,
        At::Layer,
        [
            "Dá acesso à placa de vídeo (/dev/dri e NVIDIA).",
            "Gives access to the graphics card (/dev/dri and NVIDIA)."
        ]
    ),
    item!(
        3,
        "window",
        ["Janela", "Window"],
        Kind::Choice(WINDOW),
        At::Layer,
        [
            "Janelas e áudio do desktop: true pelo socket restrito do compositor (security-context); direct pelo socket comum, que também dá o clipboard. O agente pode gravar o microfone.",
            "Desktop windows and audio: true through the compositor's restricted socket (security-context); direct through the plain socket, which also gives the clipboard. The agent can record the microphone."
        ]
    ),
    item!(
        3,
        "portal",
        ["Portal", "Portal"],
        Kind::Toggle,
        At::Layer,
        [
            "Portal do desktop: tema claro ou escuro, notificações e abrir link no seu navegador, só para domínios que a rede da aba permite. Um D-Bus próprio do sandmux, não o seu.",
            "Desktop portal: light or dark theme, notifications and opening links in your browser, only for domains the tab's network allows. A D-Bus of sandmux's own, not yours."
        ]
    ),
    item!(
        3,
        "clipboard",
        ["Clipboard", "Clipboard"],
        Kind::Toggle,
        At::Layer,
        [
            "Deixa o harness copiar para o clipboard do sistema.",
            "Lets the harness copy to the system clipboard."
        ]
    ),
    item!(
        3,
        "memory",
        ["Memória", "Memory"],
        Kind::Text,
        At::Layer,
        [
            "Teto de memória: um tamanho como 8G, uma porcentagem como 50% ou max. Vazio volta ao padrão.",
            "Memory cap: a size such as 8G, a percentage such as 50%, or max. Empty goes back to the default."
        ]
    ),
    item!(
        3,
        "processes",
        ["Processos", "Processes"],
        Kind::Text,
        At::Layer,
        [
            "Teto de processos: um número ou max. Vazio volta ao padrão.",
            "Process cap: a number or max. Empty goes back to the default."
        ]
    ),
    item!(
        3,
        "harness-asks",
        ["Perguntas do harness", "Harness prompts"],
        Kind::Toggle,
        At::Layer,
        [
            "Deixa o opencode, o Claude Code, o hermes, o oh-my-pi e o Antigravity CLI pedirem confirmação antes de agir. Desligado, eles rodam sem perguntar, e o sandbox é a barreira.",
            "Lets opencode, Claude Code, hermes, oh-my-pi and the Antigravity CLI ask for confirmation before acting. Off, they run without asking, and the sandbox is the barrier."
        ]
    ),
    item!(
        3,
        "ro",
        ["Pastas só leitura", "Read-only folders"],
        Kind::List,
        At::Layer,
        [
            "Pastas extras montadas só para leitura (caminhos absolutos ou com ~/).",
            "Extra folders mounted read-only (absolute paths or starting with ~/)."
        ]
    ),
    item!(
        3,
        "rw",
        ["Pastas com escrita", "Writable folders"],
        Kind::List,
        At::Layer,
        [
            "Pastas extras montadas com escrita. Pastas perigosas são recusadas.",
            "Extra folders mounted writable. Dangerous folders are refused."
        ]
    ),
    item!(
        3,
        "pass-env",
        ["Variáveis passadas", "Passed variables"],
        Kind::List,
        At::Layer,
        [
            "Nomes de variáveis de ambiente passadas ao harness (só nomes, nunca valores).",
            "Names of environment variables passed to the harness (names only, never values)."
        ]
    ),
    item!(
        3,
        "ssh-keys",
        ["Chaves SSH", "SSH keys"],
        Kind::List,
        At::Layer,
        [
            "Chaves SSH do cofre que a sessão pode usar, ou * para todas.",
            "SSH keys from the vault the session may use, or * for all."
        ]
    ),
    item!(
        3,
        "db",
        ["Bancos", "Databases"],
        Kind::List,
        At::Layer,
        [
            "Conexões de banco do cofre que a sessão pode usar, ou * para todas.",
            "Database connections from the vault the session may use, or * for all."
        ]
    ),
    item!(
        4,
        "network",
        ["Modo de rede", "Network mode"],
        Kind::Choice(NETWORK),
        At::Layer,
        [
            "aberta registra tudo; inferencia só deixa os provedores de modelo; custom usa a allowlist; sem-rede corta a rede.",
            "open (aberta in the file) logs everything; inference (inferencia) only allows model providers; custom uses the allowlist; no network (sem-rede) cuts the network."
        ]
    ),
    item!(
        4,
        "deny",
        ["Denylist", "Denylist"],
        Kind::List,
        At::Layer,
        [
            "Destinos sempre bloqueados, como *.exemplo.com ou 10.0.0.0/8.",
            "Destinations always blocked, such as *.example.com or 10.0.0.0/8."
        ]
    ),
    item!(
        4,
        "allow",
        ["Allowlist", "Allowlist"],
        Kind::List,
        At::Layer,
        [
            "Destinos liberados no modo custom (e somados aos provedores no modo inferencia).",
            "Destinations allowed in custom mode (and added to the providers in inferencia mode)."
        ]
    ),
    item!(
        4,
        "allow-off",
        ["Allowlist desligada", "Allowlist off"],
        Kind::List,
        At::Layer,
        [
            "Destinos da allowlist de outras camadas desligados aqui.",
            "Allowlist destinations from other layers turned off here."
        ]
    ),
    item!(
        4,
        "catalog",
        ["Catálogos", "Catalogs"],
        Kind::List,
        At::Layer,
        [
            "Grupos do catálogo comum liberados (pacotes, git, documentação…).",
            "Groups of the common catalog allowed (packages, git, documentation…)."
        ]
    ),
    item!(
        4,
        "catalog-off",
        ["Catálogos desligados", "Catalogs off"],
        Kind::List,
        At::Layer,
        [
            "Grupos do catálogo desligados aqui.",
            "Catalog groups turned off here."
        ]
    ),
    item!(
        4,
        "decrypt",
        ["Decifração", "Decryption"],
        Kind::List,
        At::Layer,
        [
            "Domínios em que o gateway abre o HTTPS para registrar o método e o caminho.",
            "Domains where the gateway opens HTTPS to log the method and path."
        ]
    ),
    item!(
        4,
        "ports",
        ["Portas de entrada", "Inbound ports"],
        Kind::List,
        At::Layer,
        [
            "Portas do sandbox publicadas no host, como 8642 ou 8443@0.0.0.0.",
            "Sandbox ports published on the host, such as 8642 or 8443@0.0.0.0."
        ]
    ),
    item!(
        4,
        "host-ports",
        ["Portas do host", "Host ports"],
        Kind::List,
        At::Layer,
        [
            "Portas da sua máquina levadas ao 127.0.0.1 do sandbox (servidores de modelo locais).",
            "Ports of your machine brought to the sandbox 127.0.0.1 (local model servers)."
        ]
    ),
    item!(
        4,
        "http2",
        ["HTTP/2 decifrado", "Decrypted HTTP/2"],
        Kind::Toggle,
        At::Layer,
        [
            "HTTP/2 e gRPC nas conexões decifradas, quando o cliente e o destino falam h2. Desligado, o gateway fala só HTTP/1.1 nas conexões seguintes.",
            "HTTP/2 and gRPC on decrypted connections, when the client and the destination speak h2. Off, the gateway speaks only HTTP/1.1 on the following connections."
        ]
    ),
    item!(
        5,
        "file-deny",
        ["Arquivos negados", "Denied files"],
        Kind::List,
        At::Layer,
        [
            "Padrões de arquivo que o harness não pode abrir.",
            "File patterns the harness cannot open."
        ]
    ),
    item!(
        5,
        "file-allow",
        ["Arquivos liberados", "Allowed files"],
        Kind::List,
        At::Layer,
        [
            "Padrões liberados mesmo dentro de um padrão negado.",
            "Patterns allowed even inside a denied pattern."
        ]
    ),
    item!(
        5,
        "secret-files",
        ["Arquivos de segredo", "Secret files"],
        Kind::List,
        At::Layer,
        [
            "Padrões tratados como segredo (além dos padrões embutidos).",
            "Patterns treated as secrets (besides the built-in ones)."
        ]
    ),
    item!(
        5,
        "secret-files-off",
        ["Segredos desligados", "Secrets off"],
        Kind::List,
        At::Layer,
        [
            "Padrões embutidos de segredo desligados aqui.",
            "Built-in secret patterns turned off here."
        ]
    ),
    item!(
        6,
        "mcp-deny",
        ["MCP negados", "Denied MCP"],
        Kind::List,
        At::Layer,
        [
            "Servidores MCP desabilitados nas sessões.",
            "MCP servers disabled in the sessions."
        ]
    ),
    item!(
        6,
        "mcp-allow",
        ["MCP liberados", "Allowed MCP"],
        Kind::List,
        At::Layer,
        [
            "Servidores MCP habilitados mesmo negados em outra camada.",
            "MCP servers enabled even if denied in another layer."
        ]
    ),
    item!(
        7,
        "chat-model",
        ["Modelo do chat", "Chat model"],
        Kind::Text,
        At::Assistant,
        [
            "provedor/modelo usado nas abas de chat. Enter lista os modelos dos provedores; e edita o texto.",
            "provider/model used in chat tabs. Enter lists the providers' models; e edits the text."
        ]
    ),
    item!(
        7,
        "command-model",
        ["Modelo de comandos", "Command model"],
        Kind::Text,
        At::Assistant,
        [
            "provedor/modelo que gera os comandos (Ctrl+b g); vazio usa o do chat. Enter lista os modelos; e edita o texto.",
            "provider/model that generates commands (Ctrl+b g); empty uses the chat one. Enter lists the models; e edits the text."
        ]
    ),
    item!(
        7,
        "models",
        ["Modelos sugeridos", "Suggested models"],
        Kind::List,
        At::Assistant,
        [
            "Modelos oferecidos com ↑ ↓ no chat e no modal de comando; a acrescenta da lista dos provedores.",
            "Models offered with ↑ ↓ in the chat and command dialogs; a adds from the providers' list."
        ]
    ),
    item!(
        8,
        "net-log-days",
        ["Dias do registro de rede", "Network log days"],
        Kind::Text,
        At::GlobalOnly,
        [
            "Quantos dias de registro de rede guardar (1 a 3650).",
            "How many days of network log to keep (1 to 3650)."
        ]
    ),
    item!(
        8,
        "net-log-max",
        ["Teto do registro de rede", "Network log cap"],
        Kind::Text,
        At::GlobalOnly,
        [
            "Tamanho máximo do registro de rede, como 500M ou 2G (mínimo 128M).",
            "Maximum size of the network log, such as 500M or 2G (minimum 128M)."
        ]
    ),
];

/// Os índices dos itens de uma seção.
pub fn items_of(section: usize) -> Vec<usize> {
    (0..ITEMS.len())
        .filter(|i| ITEMS[*i].section == section)
        .collect()
}

/// Itens de modelo do assistente, escolhidos pelo seletor.
pub fn is_model_item(key: &str) -> bool {
    matches!(key, "chat-model" | "command-model")
}

/// Valor padrão de uma chave de liga e desliga.
pub fn default_bool(key: &str) -> bool {
    matches!(key, "gpu" | "panel" | "notify" | "portal" | "http2")
}

/// De onde vem o valor mostrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Gravado no alvo.
    Here,
    /// Herdado de `[global]` (com o alvo num projeto).
    Global,
    Default,
}

/// O valor atual de um item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    /// Texto do valor (para listas, os itens separados por vírgula).
    pub value: Option<String>,
    pub list: Vec<String>,
    pub bool: Option<bool>,
    pub from: Source,
}

fn table<'d>(doc: &'d DocumentMut, path: &[String]) -> Option<&'d dyn toml_edit::TableLike> {
    let mut t: &dyn toml_edit::TableLike = doc.as_table();
    for p in path {
        t = t.get(p)?.as_table_like()?;
    }
    Some(t)
}

fn show(v: &toml_edit::Value) -> String {
    match v {
        toml_edit::Value::String(s) => s.value().clone(),
        other => other.to_string().trim().to_string(),
    }
}

/// O alvo efetivo de um item: as chaves fora das camadas valem só no global.
pub fn target_of(item: &Item, project: Option<&Path>) -> Target {
    match (item.at, project) {
        (At::Layer, Some(p)) => Target::Project(p.to_path_buf()),
        _ => Target::Global,
    }
}

fn section_path(item: &Item, target: &Target) -> Vec<String> {
    match item.at {
        At::Tui => vec!["tui".into()],
        At::Assistant => vec!["assistant".into()],
        At::GlobalOnly => vec!["global".into()],
        At::Layer => match target {
            Target::Global => vec!["global".into()],
            Target::Profile(n) => vec!["profile".into(), n.clone()],
            Target::Project(p) => vec!["project".into(), p.display().to_string()],
        },
    }
}

fn read_at(doc: &DocumentMut, path: &[String], key: &str) -> Option<toml_edit::Value> {
    table(doc, path)?.get(key)?.as_value().cloned()
}

/// O valor atual do item no alvo; sem valor nele, o herdado do global ou o
/// padrão.
pub fn current(doc: &DocumentMut, item: &Item, project: Option<&Path>) -> Current {
    let target = target_of(item, project);
    let here = read_at(doc, &section_path(item, &target), item.key);
    let (v, from) = match here {
        Some(v) => (Some(v), Source::Here),
        None if matches!(target, Target::Project(_)) => {
            match read_at(doc, &["global".to_string()], item.key) {
                Some(v) => (Some(v), Source::Global),
                None => (None, Source::Default),
            }
        }
        None => (None, Source::Default),
    };
    let list: Vec<String> = v
        .as_ref()
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(show).collect())
        .unwrap_or_default();
    let bool = match item.kind {
        Kind::Toggle => Some(
            v.as_ref()
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| default_bool(item.key)),
        ),
        _ => None,
    };
    let value = match (&v, item.kind) {
        (Some(_), Kind::List) => Some(list.join(", ")),
        (Some(v), _) => Some(show(v)),
        (None, Kind::Choice(vals)) => match item.key {
            "theme" => Some("padrao".into()),
            _ => vals.first().map(|s| s.to_string()),
        },
        (None, _) => None,
    };
    Current {
        value,
        list,
        bool,
        from,
    }
}

/// A mudança pedida na página.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Bool(bool),
    /// Uma escolha ou um texto; vazio apaga a chave.
    Text(String),
    Add(String),
    Remove(String),
}

/// Lê o arquivo de configuração para a página (sem arquivo, vazio).
pub fn load(path: &Path) -> Result<DocumentMut, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(trf!("ler {}: {e}", "read {}: {e}", path.display())),
    };
    text.parse::<DocumentMut>().map_err(|e| e.to_string())
}

/// Aplica a mudança no arquivo; devolve o aviso a mostrar.
pub fn apply(
    path: &Path,
    host: &Host,
    item: &Item,
    project: Option<&Path>,
    change: Change,
) -> Result<String, String> {
    let target = target_of(item, project);
    let r = match item.at {
        At::Layer => {
            let op = match (&change, item.kind) {
                (Change::Bool(b), _) => Op::Set(item.key.into(), *b),
                (Change::Text(t), _) if t.trim().is_empty() => Op::Unset(item.key.into()),
                (Change::Text(t), _) => Op::SetLimit(item.key.into(), t.trim().into()),
                (Change::Add(v), _) => Op::Add(item.key.into(), v.trim().into()),
                (Change::Remove(v), _) => Op::Remove(item.key.into(), v.clone()),
            };
            edit::apply(path, host, &target, &op)
        }
        _ => {
            let sec = section_path(item, &target);
            let sec: Vec<&str> = sec.iter().map(String::as_str).collect();
            let value: Option<toml_edit::Value> = match (&change, item.kind) {
                (Change::Bool(b), _) => Some((*b).into()),
                (Change::Text(t), _) if t.trim().is_empty() => None,
                (Change::Text(t), _) if item.key == "language" && t.trim() == "auto" => None,
                (Change::Text(t), _) if item.key == "net-log-days" => Some(
                    t.trim()
                        .parse::<i64>()
                        .map_err(|_| {
                            tr!(
                                "`net-log-days` precisa ser um número de dias de 1 a 3650",
                                "`net-log-days` must be a number of days from 1 to 3650"
                            )
                            .to_string()
                        })?
                        .into(),
                ),
                (Change::Text(t), _) => Some(t.trim().into()),
                (Change::Add(v), _) | (Change::Remove(v), _) => {
                    let doc = load(path)?;
                    let mut list = current(&doc, item, project).list;
                    if let Change::Add(_) = change {
                        let v = v.trim().to_string();
                        if list.contains(&v) {
                            return Ok(trf!("{v} já está na lista", "{v} is already in the list"));
                        }
                        list.push(v);
                    } else {
                        list.retain(|x| x != v);
                    }
                    (!list.is_empty()).then(|| toml_edit::Array::from_iter(list).into())
                }
            };
            edit::set_value(path, &sec, item.key, value)
        }
    };
    match r {
        Ok(o) if !o.changed => Ok(o
            .notice
            .unwrap_or_else(|| tr!("nada mudou", "nothing changed").into())),
        Ok(_) => Ok(match item.at {
            At::Tui => trf!(
                "{} gravado em {}; vale na hora",
                "{} saved in {}; applies right away",
                item.key,
                where_(&target, item)
            ),
            At::Assistant => trf!(
                "{} gravado em [assistant]; vale no próximo pedido",
                "{} saved in [assistant]; applies on the next request",
                item.key
            ),
            _ => trf!(
                "{} gravado em {}; vale nas abas lançadas a partir de agora",
                "{} saved in {}; applies to tabs launched from now on",
                item.key,
                where_(&target, item)
            ),
        }),
        Err(e) => Err(e.to_string()),
    }
}

fn where_(target: &Target, item: &Item) -> String {
    match item.at {
        At::Tui => "[tui]".into(),
        At::Assistant => "[assistant]".into(),
        _ => target.to_string(),
    }
}

/// Edição de uma lista na página.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListEdit {
    pub sel: usize,
    /// Texto do item sendo acrescentado.
    pub input: Option<String>,
}

/// Estado da página.
pub struct Settings {
    pub section: usize,
    /// Posição do item dentro da seção.
    pub item: usize,
    /// O foco está nos itens (senão, nas seções).
    pub on_items: bool,
    /// Pasta do projeto da aba ativa, quando há aba.
    pub project_dir: Option<PathBuf>,
    /// O alvo é o projeto (senão, o global).
    pub on_project: bool,
    /// Texto sendo editado num item de texto.
    pub edit: Option<String>,
    pub list: Option<ListEdit>,
    /// Último aviso, e se é erro.
    pub note: Option<(String, bool)>,
    pub doc: DocumentMut,
    pub path: PathBuf,
    /// A tela do cofre.
    pub vault: VaultView,
    /// A tela do diagnóstico.
    pub doctor: DoctorView,
    /// A árvore de processos da aba ativa.
    pub procs: super::procs::ProcsView,
}

/// A tela do diagnóstico: o relatório, a linha escolhida e o `f` em
/// andamento.
#[derive(Default)]
pub struct DoctorView {
    pub report: Option<crate::doctor::Report>,
    /// A hora local da última verificação (`HH:MM:SS`).
    pub at: String,
    /// A linha escolhida, contando as verificações de todos os grupos.
    pub sel: usize,
    pub fix: Option<FixFlow>,
    /// A senha do cofre, pedida antes de mover as chaves.
    pub unlock: Option<VaultForm>,
}

impl DoctorView {
    /// A verificação escolhida.
    pub fn selected(&self) -> Option<&crate::doctor::Check> {
        self.report
            .as_ref()?
            .groups
            .iter()
            .flat_map(|g| &g.checks)
            .nth(self.sel)
    }

    fn count(&self) -> usize {
        self.report
            .as_ref()
            .map_or(0, |r| r.groups.iter().map(|g| g.checks.len()).sum())
    }
}

/// O `doctor --fix` pela TUI: os arquivos com chaves reais, a chave atual e
/// os domínios já confirmados das chaves do arquivo. Os valores reais nunca
/// aparecem na tela.
pub struct FixFlow {
    files: Vec<(&'static str, sandmux_import::RealSecrets)>,
    file: usize,
    key: usize,
    answers: Vec<Vec<String>>,
    /// O campo de domínios da chave atual.
    pub input: String,
    agy: Option<(PathBuf, sandmux_import::agy_login::Login)>,
    /// Esperando a confirmação do login do `agy`.
    pub on_agy: bool,
    moved: usize,
    harnesses: Vec<&'static str>,
    refused: Vec<String>,
}

impl FixFlow {
    /// O nome no cofre da chave atual.
    pub fn name(&self) -> String {
        let (h, r) = &self.files[self.file];
        crate::doctor::key_name(h, r, &r.found[self.key])
    }

    /// Onde está a chave atual e quantas faltam no arquivo.
    pub fn context(&self) -> String {
        let (h, r) = &self.files[self.file];
        trf!(
            "{h}: {}, linha {}, chave {} de {} do arquivo",
            "{h}: {}, line {}, key {} of {} in the file",
            r.file.display(),
            r.found[self.key].line,
            self.key + 1,
            r.found.len()
        )
    }
}

/// A tela do cofre na página: o estado, a lista e o formulário aberto.
#[derive(Default)]
pub struct VaultView {
    /// Existe, está aberto e quantos segredos tem; `None` antes de ler.
    pub info: Option<(bool, bool, usize)>,
    pub entries: Vec<sandmux_proto::msg::VaultEntry>,
    pub sel: usize,
    pub form: Option<VaultForm>,
    /// Nome do segredo esperando a confirmação para apagar.
    pub confirm: Option<String>,
    /// Keyring ligado e o problema dele, quando há (protocolo 28).
    pub keyring: Option<(bool, Option<String>)>,
    /// A tela do grafo, aberta por `g`: ocupa a página inteira.
    pub graph: Option<super::vaultgraph::GraphView>,
}

/// O que o formulário do cofre faz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    Init,
    Unlock,
    Passwd,
    Add,
    AddSsh,
    /// Trocar os domínios do segredo com este nome.
    Domains(String),
    /// Trocar o valor do segredo com este nome, mantendo o falso.
    Value(String),
    /// Guardar a senha do cofre no keyring.
    Keyring,
}

impl FormKind {
    /// Os campos: rótulo em português e em inglês, e se é segredo (`•`).
    pub fn fields(&self) -> &'static [([&'static str; 2], bool)] {
        match self {
            FormKind::Init => &[
                (["Senha", "Password"], true),
                (["Repita a senha", "Repeat the password"], true),
            ],
            FormKind::Unlock | FormKind::Keyring => &[(["Senha", "Password"], true)],
            FormKind::Passwd => &[
                (["Senha atual", "Current password"], true),
                (["Senha nova", "New password"], true),
                (["Repita a senha nova", "Repeat the new password"], true),
            ],
            FormKind::Add => &[
                (["Nome", "Name"], false),
                (["Valor", "Value"], true),
                (["Domínios (vírgula)", "Domains (comma)"], false),
            ],
            FormKind::AddSsh => &[
                (["Nome", "Name"], false),
                (["Arquivo da chave", "Key file"], false),
                (
                    [
                        "Passphrase (vazia se não tem)",
                        "Passphrase (empty if none)",
                    ],
                    true,
                ),
                (["Servidores (vírgula)", "Servers (comma)"], false),
            ],
            FormKind::Domains(_) => &[(["Domínios (vírgula)", "Domains (comma)"], false)],
            FormKind::Value(_) => &[(["Valor novo", "New value"], true)],
        }
    }

    pub fn title(&self) -> String {
        match self {
            FormKind::Init => sandmux_spec::tr!("Criar o cofre", "Create the vault").into(),
            FormKind::Unlock => sandmux_spec::tr!("Desbloquear", "Unlock").into(),
            FormKind::Passwd => sandmux_spec::tr!("Trocar a senha", "Change the password").into(),
            FormKind::Add => sandmux_spec::tr!("Novo segredo", "New secret").into(),
            FormKind::AddSsh => sandmux_spec::tr!("Nova chave SSH", "New SSH key").into(),
            FormKind::Domains(n) => sandmux_spec::trf!("Domínios de {n}", "Domains of {n}"),
            FormKind::Value(n) => sandmux_spec::trf!("Valor novo de {n}", "New value of {n}"),
            FormKind::Keyring => sandmux_spec::tr!(
                "Guardar a senha no keyring",
                "Store the password in the keyring"
            )
            .into(),
        }
    }
}

/// Um formulário do cofre: o campo atual e o texto de cada um (apagado da
/// memória ao sair).
pub struct VaultForm {
    pub kind: FormKind,
    pub step: usize,
    pub fields: Vec<zeroize::Zeroizing<String>>,
}

impl VaultForm {
    pub fn new(kind: FormKind, first: String) -> VaultForm {
        let n = kind.fields().len();
        let mut fields: Vec<zeroize::Zeroizing<String>> = (0..n)
            .map(|_| zeroize::Zeroizing::new(String::new()))
            .collect();
        *fields[0] = first;
        VaultForm {
            kind,
            step: 0,
            fields,
        }
    }

    /// Um campo que já vem preenchido (o domínio escolhido no grafo).
    pub fn prefill(mut self, field: usize, text: String) -> VaultForm {
        if let Some(f) = self.fields.get_mut(field) {
            **f = text;
        }
        self
    }
}

impl Settings {
    pub fn new(path: PathBuf, project_dir: Option<PathBuf>) -> Settings {
        let (doc, note) = match load(&path) {
            Ok(d) => (d, None),
            Err(e) => (DocumentMut::new(), Some((e, true))),
        };
        Settings {
            section: 0,
            item: 0,
            on_items: false,
            project_dir,
            on_project: false,
            edit: None,
            list: None,
            note,
            doc,
            path,
            vault: VaultView::default(),
            doctor: DoctorView::default(),
            procs: Default::default(),
        }
    }

    pub fn project(&self) -> Option<&Path> {
        self.on_project
            .then_some(self.project_dir.as_deref())
            .flatten()
    }

    /// O item escolhido.
    pub fn selected(&self) -> Option<&'static Item> {
        items_of(self.section).get(self.item).map(|i| &ITEMS[*i])
    }

    pub fn reload(&mut self) {
        if let Ok(d) = load(&self.path) {
            self.doc = d;
        }
    }
}

// ---- teclas e mouse da página ----

use super::input::Key;
use super::{App, Hit, Modal};
use sandmux_proto::Msg;

impl App {
    pub(super) fn open_settings(&mut self) {
        let path = sandmux_policy::config::path(&self.host);
        let project = self.tabs.get(self.active).map(|t| {
            t.workdir
                .canonicalize()
                .unwrap_or_else(|_| t.workdir.clone())
        });
        let mut s = Box::new(Settings::new(path, project));
        // A contagem de falhas aparece no nome da seção já na abertura.
        self.doctor_refresh(&mut s);
        self.modal = Some(Modal::Settings(s));
    }

    /// A página aberta na seção do diagnóstico.
    pub(super) fn open_doctor(&mut self) {
        self.open_settings();
        if let Some(Modal::Settings(s)) = self.modal.as_mut() {
            s.section = DOCTOR;
            s.on_items = true;
        }
    }

    /// Grava a mudança no item escolhido, relê o arquivo e aplica na hora o
    /// que é da TUI (tema, barra) e do assistente.
    fn settings_change(&mut self, s: &mut Settings, ch: Change) {
        let Some(item) = s.selected() else {
            return;
        };
        let r = apply(&s.path, &self.host, item, s.project(), ch);
        // O que vale no lançamento não chega às abas vivas.
        let live = self
            .tabs
            .iter()
            .filter(|t| t.state == sandmux_proto::SessionState::Viva)
            .count();
        let saved =
            matches!(&r, Ok(n) if n.contains(sandmux_spec::tr!(" gravado em ", " saved in ")));
        s.note = Some(match r {
            Ok(_) if saved && item.at == At::Layer && live > 0 => (
                sandmux_spec::trf!(
                    "{} gravado em {}; {live} aba(s) viva(s) com o valor antigo: Ctrl+b r reinicia",
                    "{} saved in {}; {live} live tab(s) with the old value: Ctrl+b r restarts",
                    item.key,
                    where_(&target_of(item, s.project()), item)
                ),
                false,
            ),
            Ok(n) => (n, false),
            Err(e) => (e, true),
        });
        s.reload();
        if item.key == "language" {
            sandmux_policy::config::init_language(&s.path);
            // A conexão de controle reabre com o idioma novo na saudação.
            self.control = None;
            // O aviso sai no idioma novo.
            if let Some((_, false)) = &s.note {
                s.note = Some((
                    sandmux_spec::tr!(
                        "idioma gravado em [global]; vale na hora",
                        "language saved in [global]; applies right away"
                    )
                    .to_string(),
                    false,
                ));
            }
        }
        if let Ok(c) = sandmux_policy::config::Config::load(&s.path) {
            if item.key == "panel" && self.panel != c.tui.panel {
                self.panel = c.tui.panel;
                self.resize_all();
                self.refresh_panel();
            }
            self.cfg = c.tui;
            self.assistant = c.assistant;
        }
    }

    /// Passa uma escolha para a vizinha (`step` 1 ou -1).
    fn settings_step(&mut self, s: &mut Settings, step: isize) {
        let Some(item) = s.selected() else {
            return;
        };
        let Kind::Choice(vals) = item.kind else {
            return;
        };
        let cur = current(&s.doc, item, s.project()).value.unwrap_or_default();
        let i = vals.iter().position(|v| *v == cur).unwrap_or(0) as isize;
        let n = vals.len() as isize;
        let next = vals[((i + step) % n + n) as usize % vals.len()];
        self.settings_change(s, Change::Text(next.into()));
    }

    /// Enter ou Espaço num item: alterna, avança, edita ou abre a lista.
    fn settings_activate(&mut self, s: &mut Settings) {
        let Some(item) = s.selected() else {
            return;
        };
        let cur = current(&s.doc, item, s.project());
        match item.kind {
            Kind::Toggle => {
                let now = cur.bool.unwrap_or(false);
                self.settings_change(s, Change::Bool(!now));
            }
            Kind::Choice(_) => self.settings_step(s, 1),
            Kind::Text if is_model_item(item.key) => {
                self.open_picker(super::picker::PickFor::Setting)
            }
            Kind::Text => {
                s.edit = Some(match cur.from {
                    Source::Here => cur.value.unwrap_or_default(),
                    _ => String::new(),
                })
            }
            Kind::List => s.list = Some(ListEdit::default()),
        }
    }

    /// O modelo escolhido no seletor: grava o item, ou acrescenta à lista.
    pub(super) fn settings_pick(&mut self, s: &mut Settings, add: bool, model: String) {
        self.settings_change(
            s,
            if add {
                Change::Add(model)
            } else {
                Change::Text(model)
            },
        );
    }

    pub(super) fn settings_key(&mut self, mut s: Box<Settings>, k: Key) -> Option<Box<Settings>> {
        if let Some(text) = s.edit.as_mut() {
            match k {
                Key::Esc => s.edit = None,
                Key::Backspace => {
                    text.pop();
                }
                Key::Char(c) => text.push(c),
                Key::Enter => {
                    let t = s.edit.take().unwrap_or_default();
                    self.settings_change(&mut s, Change::Text(t));
                }
                _ => {}
            }
            return Some(s);
        }
        if s.list.is_some() {
            let item = s.selected()?;
            let list = current(&s.doc, item, s.project()).list;
            let l = s.list.as_mut()?;
            if let Some(input) = l.input.as_mut() {
                match k {
                    Key::Esc => l.input = None,
                    Key::Backspace => {
                        input.pop();
                    }
                    Key::Char(c) => input.push(c),
                    Key::Enter => {
                        let v = l.input.take().unwrap_or_default();
                        if !v.trim().is_empty() {
                            self.settings_change(&mut s, Change::Add(v));
                        }
                    }
                    _ => {}
                }
                return Some(s);
            }
            match k {
                Key::Esc | Key::Left | Key::Char('q') => s.list = None,
                Key::Up => l.sel = l.sel.saturating_sub(1),
                Key::Down => l.sel = (l.sel + 1).min(list.len().saturating_sub(1)),
                Key::Char('a') | Key::Enter if item.key == "models" => {
                    self.open_picker(super::picker::PickFor::SettingAdd)
                }
                Key::Char('a') | Key::Enter => l.input = Some(String::new()),
                Key::Char('x') | Key::Char('d') => {
                    if let Some(v) = list.get(l.sel).cloned() {
                        l.sel = l.sel.min(list.len().saturating_sub(2));
                        self.settings_change(&mut s, Change::Remove(v));
                    }
                }
                _ => {}
            }
            return Some(s);
        }
        if s.section == VAULT
            && (s.on_items
                || s.vault.form.is_some()
                || s.vault.confirm.is_some()
                || s.vault.graph.is_some())
            && self.vault_key(&mut s, &k)
        {
            return Some(s);
        }
        // Na seção do cofre o alvo (global ou projeto) não vale nada: `g`
        // abre o grafo.
        if s.section == VAULT && k == Key::Char('g') {
            self.graph_open(&mut s);
            return Some(s);
        }
        if s.section == DOCTOR
            && (s.on_items || s.doctor.fix.is_some() || s.doctor.unlock.is_some())
            && self.doctor_key(&mut s, &k)
        {
            return Some(s);
        }
        if s.section == PROCS && s.on_items && self.procs_key(&mut s, &k) {
            return Some(s);
        }
        let before = s.section;
        let r = self.settings_nav(&mut s, k);
        if s.section == VAULT && before != VAULT {
            self.vault_refresh(&mut s);
        }
        if s.section == DOCTOR && before != DOCTOR {
            self.doctor_refresh(&mut s);
        }
        if s.section == PROCS && before != PROCS {
            self.procs_refresh(&mut s);
        }
        if r { Some(s) } else { None }
    }

    /// As teclas de navegação da página; `false` fecha.
    fn settings_nav(&mut self, s: &mut Settings, k: Key) -> bool {
        let n_items = items_of(s.section).len();
        match k {
            Key::Esc | Key::Char('q') => return false,
            Key::Tab => s.on_items = !s.on_items,
            Key::Char('g') => {
                if s.project_dir.is_some() {
                    s.on_project = !s.on_project;
                } else {
                    s.note = Some((
                        tr!("sem aba aberta: só o global", "no open tab: global only").into(),
                        true,
                    ));
                }
            }
            Key::Up if s.on_items => s.item = s.item.saturating_sub(1),
            Key::Down if s.on_items => s.item = (s.item + 1).min(n_items.saturating_sub(1)),
            Key::Up => {
                s.section = s.section.saturating_sub(1);
                s.item = 0;
            }
            Key::Down => {
                s.section = (s.section + 1).min(SECTIONS.len() - 1);
                s.item = 0;
            }
            Key::Right | Key::Enter if !s.on_items => s.on_items = true,
            Key::Left if s.on_items => {
                if matches!(s.selected().map(|i| i.kind), Some(Kind::Choice(_))) {
                    self.settings_step(s, -1);
                } else {
                    s.on_items = false;
                }
            }
            Key::Right if matches!(s.selected().map(|i| i.kind), Some(Kind::Choice(_))) => {
                self.settings_step(s, 1)
            }
            Key::Enter | Key::Char(' ') if s.on_items => self.settings_activate(s),
            Key::Char('e') if s.on_items && s.selected().is_some_and(|i| is_model_item(i.key)) => {
                let cur = s
                    .selected()
                    .map(|i| current(&s.doc, i, s.project()))
                    .filter(|c| c.from == Source::Here)
                    .and_then(|c| c.value)
                    .unwrap_or_default();
                s.edit = Some(cur);
            }
            Key::Char('x')
                if s.on_items && !matches!(s.selected().map(|i| i.kind), Some(Kind::List)) =>
            {
                self.settings_change(s, Change::Text(String::new()));
            }
            _ => {}
        }
        true
    }

    /// Clique na página: escolhe a seção ou o item; no toggle já escolhido,
    /// alterna.
    pub(super) fn settings_click(&mut self, mut s: Box<Settings>, x: u16, y: u16) -> Box<Settings> {
        let hit = self
            .settings_hits
            .borrow()
            .iter()
            .find(|(row, a, b, _)| *row == y && x >= *a && x < *b)
            .map(|h| h.3);
        match hit {
            Some(Hit::Section(i)) => {
                s.section = i;
                s.item = 0;
                s.on_items = false;
                s.list = None;
                s.edit = None;
                if i == VAULT {
                    self.vault_refresh(&mut s);
                }
                if i == DOCTOR {
                    self.doctor_refresh(&mut s);
                }
                if i == PROCS {
                    self.procs_refresh(&mut s);
                }
            }
            Some(Hit::Item(i)) if s.section == PROCS => {
                s.on_items = true;
                if let Some(r) = s.procs.rows().get(i) {
                    s.procs.sel = Some(r.pid);
                }
            }
            Some(Hit::Crumb(i)) => {
                s.on_items = true;
                s.procs.crumb(i);
            }
            Some(Hit::Item(i)) if s.section == VAULT => {
                s.on_items = true;
                s.vault.sel = i;
            }
            Some(Hit::Item(i)) if s.section == DOCTOR => {
                if s.doctor.fix.is_none() && s.doctor.unlock.is_none() {
                    s.on_items = true;
                    s.doctor.sel = i;
                }
            }
            Some(Hit::GraphRow(_) | Hit::GraphNode(_)) => {}
            Some(Hit::Item(i)) => {
                let again = s.on_items && s.item == i && s.list.is_none() && s.edit.is_none();
                s.item = i;
                s.on_items = true;
                if again || matches!(s.selected().map(|i| i.kind), Some(Kind::Toggle)) {
                    self.settings_activate(&mut s);
                }
            }
            None => {}
        }
        s
    }
}

// ---- processos ----

impl App {
    /// Pede ao daemon a árvore de processos da aba ativa.
    pub(super) fn procs_refresh(&mut self, s: &mut Settings) {
        // A versão do auxiliar é conferida ao abrir a seção; `r` confere de
        // novo.
        if !s.procs.loaded {
            s.procs.old_helper = sandmux::helper::outdated();
        }
        let Some(id) = self.tabs.get(self.active).map(|t| t.id) else {
            s.procs.set(
                Vec::new(),
                Some(tr!("nenhuma aba aberta", "no open tab").into()),
            );
            return;
        };
        match self.call(&Msg::ProcessTree { id }) {
            Ok(Msg::ProcessTreeOf {
                nodes, unavailable, ..
            }) => s.procs.set(nodes, unavailable),
            Ok(Msg::Error { message }) => s.procs.set(Vec::new(), Some(message)),
            Ok(_) => {}
            Err(e) => s.procs.set(Vec::new(), Some(format!("{e:#}"))),
        }
    }

    /// A árvore da página aberta na seção de processos, a cada segundo.
    pub(super) fn refresh_procs(&mut self) {
        if !matches!(&self.modal, Some(Modal::Settings(s)) if s.section == PROCS) {
            return;
        }
        if let Some(Modal::Settings(mut s)) = self.modal.take() {
            self.procs_refresh(&mut s);
            self.modal = Some(Modal::Settings(s));
        }
    }

    /// As teclas da árvore de processos; `true` quando a tecla foi usada.
    fn procs_key(&mut self, s: &mut Settings, k: &Key) -> bool {
        let v = &mut s.procs;
        match k {
            Key::Up => v.step(-1),
            Key::Down => v.step(1),
            Key::Right => {
                v.unfold();
            }
            Key::Left => {
                if !v.fold() {
                    s.on_items = false;
                }
            }
            Key::Char(' ') => v.toggle(),
            Key::Enter => v.enter(),
            Key::Backspace => {
                v.back();
            }
            Key::Esc => {
                if !v.back() {
                    s.on_items = false;
                }
            }
            Key::Char('+') | Key::Char('=') => v.zoom_in(),
            Key::Char('-') => v.zoom_out(),
            Key::Char('r') => {
                s.procs.old_helper = sandmux::helper::outdated();
                self.procs_refresh(s)
            }
            _ => return false,
        }
        true
    }
}

// ---- cofre ----

impl App {
    /// Relê o estado e a lista do cofre do daemon.
    pub(super) fn vault_refresh(&mut self, s: &mut Settings) {
        match self.call(&Msg::VaultStatus) {
            Ok(Msg::VaultInfo {
                exists,
                unlocked,
                secrets,
            }) => {
                s.vault.info = Some((exists, unlocked, secrets));
                s.vault.entries = if unlocked {
                    match self.call(&Msg::VaultList) {
                        Ok(Msg::VaultEntries { entries }) => entries,
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                };
                s.vault.sel = s.vault.sel.min(s.vault.entries.len().saturating_sub(1));
            }
            Ok(_) => {}
            Err(e) => s.note = Some((format!("{e:#}"), true)),
        }
        s.vault.keyring = match s.vault.info {
            Some((true, _, _)) => match self.call(&Msg::KeyringStatus) {
                Ok(Msg::KeyringInfo { enabled, error, .. }) => Some((enabled, error)),
                _ => None,
            },
            _ => None,
        };
        if s.vault.graph.is_some() {
            let dbs = match s.vault.info {
                Some((_, true, _)) => match self.call(&Msg::DbList) {
                    Ok(Msg::DbEntries { list }) => list,
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            let v = &mut s.vault;
            if let Some(g) = v.graph.as_mut() {
                g.set(&v.entries, dbs);
            }
        }
        self.refresh_vault();
    }

    /// As teclas da tela do cofre; `true` quando a tecla foi usada.
    fn vault_key(&mut self, s: &mut Settings, k: &Key) -> bool {
        if let Some(f) = s.vault.form.as_mut() {
            match k {
                Key::Esc => s.vault.form = None,
                Key::Backspace => {
                    f.fields[f.step].pop();
                }
                Key::Char(c) => f.fields[f.step].push(*c),
                Key::Enter | Key::Tab => {
                    if f.step + 1 < f.fields.len() {
                        f.step += 1;
                    } else if let Some(f) = s.vault.form.take() {
                        let r = self.vault_submit(&f);
                        s.note = Some(match r {
                            Ok(n) => (n, false),
                            Err(e) => (e, true),
                        });
                        self.vault_refresh(s);
                    }
                }
                _ => {}
            }
            return true;
        }
        if let Some(name) = s.vault.confirm.take() {
            if matches!(k, Key::Char('s' | 'S' | 'y' | 'Y')) {
                let r = self.call(&Msg::VaultRemove { name: name.clone() });
                s.note = Some(match r {
                    Ok(_) => (
                        sandmux_spec::trf!(
                            "{name} apagado do cofre",
                            "{name} removed from the vault"
                        ),
                        false,
                    ),
                    Err(e) => (format!("{e:#}"), true),
                });
                self.vault_refresh(s);
            }
            return true;
        }
        if s.vault.graph.is_some() {
            self.graph_key(s, k);
            return true;
        }
        let (exists, unlocked) = s
            .vault
            .info
            .map(|(e, u, _)| (e, u))
            .unwrap_or((false, false));
        let need = |s: &mut Settings, ok: bool, pt: &str, en: &str| {
            if !ok {
                s.note = Some((sandmux_spec::tr!(pt, en).to_string(), true));
            }
            ok
        };
        let sel = s.vault.entries.get(s.vault.sel).cloned();
        match k {
            Key::Up => s.vault.sel = s.vault.sel.saturating_sub(1),
            Key::Down => {
                s.vault.sel = (s.vault.sel + 1).min(s.vault.entries.len().saturating_sub(1))
            }
            Key::Esc | Key::Left => s.on_items = false,
            Key::Char('r') => self.vault_refresh(s),
            Key::Char('n') => {
                if need(s, !exists, "o cofre já existe", "the vault already exists") {
                    s.vault.form = Some(VaultForm::new(FormKind::Init, String::new()));
                }
            }
            Key::Char('u') => {
                if need(
                    s,
                    exists,
                    "não há cofre: n cria",
                    "there is no vault: n creates one",
                ) && need(
                    s,
                    !unlocked,
                    "o cofre já está aberto",
                    "the vault is already open",
                ) {
                    s.vault.form = Some(VaultForm::new(FormKind::Unlock, String::new()));
                }
            }
            Key::Char('l') => {
                if need(
                    s,
                    unlocked,
                    "o cofre já está trancado",
                    "the vault is already locked",
                ) {
                    let r = self.call(&Msg::VaultLock);
                    s.note = Some(match r {
                        Ok(_) => (
                            sandmux_spec::tr!("cofre trancado", "vault locked").into(),
                            false,
                        ),
                        Err(e) => (format!("{e:#}"), true),
                    });
                    self.vault_refresh(s);
                }
            }
            Key::Char('k') => {
                if !need(
                    s,
                    exists,
                    "não há cofre: n cria",
                    "there is no vault: n creates one",
                ) {
                    return true;
                }
                if s.vault.keyring.as_ref().is_some_and(|(on, _)| *on) {
                    let r = self.call(&Msg::KeyringOff);
                    s.note = Some(match r {
                        Ok(_) => (
                            sandmux_spec::tr!(
                                "senha do cofre apagada do keyring",
                                "vault password deleted from the keyring"
                            )
                            .into(),
                            false,
                        ),
                        Err(e) => (format!("{e:#}"), true),
                    });
                    self.vault_refresh(s);
                } else {
                    s.vault.form = Some(VaultForm::new(FormKind::Keyring, String::new()));
                }
            }
            Key::Char('p') => {
                if need(
                    s,
                    exists,
                    "não há cofre: n cria",
                    "there is no vault: n creates one",
                ) {
                    s.vault.form = Some(VaultForm::new(FormKind::Passwd, String::new()));
                }
            }
            Key::Char('a') | Key::Char('s') | Key::Char('d') | Key::Char('x') | Key::Char('v') => {
                if !need(
                    s,
                    unlocked,
                    "desbloqueie o cofre antes (u)",
                    "unlock the vault first (u)",
                ) {
                    return true;
                }
                match (k, sel) {
                    (Key::Char('a'), _) => {
                        s.vault.form = Some(VaultForm::new(FormKind::Add, String::new()))
                    }
                    (Key::Char('s'), _) => {
                        s.vault.form = Some(VaultForm::new(FormKind::AddSsh, String::new()))
                    }
                    (Key::Char('d' | 'v'), Some(e)) if e.kind == "cloud" => {
                        s.note = Some((
                            sandmux_spec::tr!(
                                "uma identidade de cloud não muda: apague (x) e guarde de novo com `sandmux vault add-cloud`",
                                "a cloud identity does not change: remove it (x) and store it again with `sandmux vault add-cloud`"
                            )
                            .into(),
                            true,
                        ))
                    }
                    (Key::Char('d'), Some(e)) => {
                        s.vault.form = Some(VaultForm::new(
                            FormKind::Domains(e.name.clone()),
                            e.domains.join(", "),
                        ))
                    }
                    (Key::Char('x'), Some(e)) => s.vault.confirm = Some(e.name.clone()),
                    (Key::Char('v'), Some(e)) if e.kind.is_empty() => {
                        s.vault.form = Some(VaultForm::new(
                            FormKind::Value(e.name.clone()),
                            String::new(),
                        ))
                    }
                    (Key::Char('v'), Some(_)) => {
                        s.note = Some((
                            sandmux_spec::tr!(
                                "uma chave SSH se troca gravando a chave nova (s)",
                                "an SSH key is replaced by storing the new key (s)"
                            )
                            .into(),
                            true,
                        ))
                    }
                    _ => {
                        s.note = Some((
                            sandmux_spec::tr!(
                                "escolha um segredo da lista",
                                "pick a secret from the list"
                            )
                            .into(),
                            true,
                        ))
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// `g` na seção do cofre: abre a tela do grafo. Sem cofre, só avisa.
    fn graph_open(&mut self, s: &mut Settings) {
        s.vault.graph = Some(Default::default());
        self.vault_refresh(s);
        if !matches!(s.vault.info, Some((true, _, _))) {
            s.vault.graph = None;
            s.note = Some((
                tr!("não há cofre: n cria", "there is no vault: n creates one").into(),
                true,
            ));
            return;
        }
        s.on_items = true;
    }

    /// As teclas da tela do grafo. Todas ficam nela: nenhuma chega à página.
    fn graph_key(&mut self, s: &mut Settings, k: &Key) {
        let unlocked = matches!(s.vault.info, Some((true, true, _)));
        let Some(g) = s.vault.graph.as_mut() else {
            return;
        };
        if g.typing {
            match k {
                Key::Esc => {
                    g.typing = false;
                    g.filter.clear();
                }
                Key::Enter => g.typing = false,
                Key::Backspace => {
                    g.filter.pop();
                }
                Key::Char(c) => g.filter.push(*c),
                _ => {}
            }
            g.fix_sel();
            return;
        }
        if !unlocked {
            match k {
                Key::Esc | Key::Char('q') => s.vault.graph = None,
                Key::Char('u') => {
                    s.vault.form = Some(VaultForm::new(FormKind::Unlock, String::new()))
                }
                Key::Char('r') => self.vault_refresh(s),
                _ => {}
            }
            return;
        }
        match k {
            Key::Esc => {
                if g.detail {
                    g.detail = false;
                } else if g.focus.is_some() {
                    g.focus = None;
                } else if !g.filter.is_empty() {
                    g.filter.clear();
                    g.fix_sel();
                } else {
                    s.vault.graph = None;
                }
            }
            Key::Up if g.focus.is_some() => g.step_focus(-1),
            Key::Down if g.focus.is_some() => g.step_focus(1),
            Key::Up => g.step_row(-1),
            Key::Down => g.step_row(1),
            Key::Tab => g.step_focus(1),
            Key::Right if g.focus.is_none() => g.step_focus(1),
            Key::Left => {
                g.focus = None;
                g.detail = false;
            }
            Key::Enter if g.focus.is_some() => g.detail = !g.detail,
            Key::Enter => g.step_focus(1),
            Key::Char('o') => {
                if g.focus.is_some() && !g.next_owner() {
                    s.note = Some((
                        tr!(
                            "este segredo não tem outro domínio dono",
                            "this secret has no other owner domain"
                        )
                        .into(),
                        true,
                    ));
                }
            }
            Key::Char('+') | Key::Char('=') => g.win.zoom_in(),
            Key::Char('-') => g.win.zoom_out(),
            // Mover o grafo para um lado é levar a janela para o outro.
            Key::Char('h') => g.pan(1.0, 0.0),
            Key::Char('l') => g.pan(-1.0, 0.0),
            Key::Char('k') => g.pan(0.0, 1.0),
            Key::Char('j') => g.pan(0.0, -1.0),
            Key::Char('0') => g.reset(),
            Key::Char('/') => g.typing = true,
            Key::Char('r') => self.vault_refresh(s),
            Key::Char('a' | 's' | 'd' | 'v' | 'x') => self.graph_edit(s, k),
            _ => {}
        }
    }

    /// As teclas de edição na tela do grafo: os formulários da seção, com o
    /// domínio escolhido já preenchido num segredo novo. Uma conexão de
    /// banco não se muda por aqui.
    fn graph_edit(&mut self, s: &mut Settings, k: &Key) {
        use super::vaultgraph::{Group, Kind};
        let Some(g) = s.vault.graph.as_ref() else {
            return;
        };
        let member = g.member().cloned();
        if member.as_ref().is_some_and(|m| m.kind == Kind::Db) {
            s.note = Some((
                tr!(
                    "as conexões de banco se gerenciam por `sandmux db`",
                    "database connections are managed with `sandmux db`"
                )
                .into(),
                true,
            ));
            return;
        }
        let domain = match g.row().map(|r| &r.group) {
            Some(Group::Domain(d)) => d.clone(),
            _ => String::new(),
        };
        let entry = member.and_then(|m| s.vault.entries.iter().find(|e| e.name == m.name).cloned());
        match (k, entry) {
            (Key::Char('a'), _) => {
                s.vault.form = Some(VaultForm::new(FormKind::Add, String::new()).prefill(2, domain))
            }
            (Key::Char('s'), _) => {
                s.vault.form =
                    Some(VaultForm::new(FormKind::AddSsh, String::new()).prefill(3, domain))
            }
            (Key::Char('d' | 'v'), Some(e)) if e.kind == "cloud" => {
                s.note = Some((
                    tr!(
                        "uma identidade de cloud não muda: apague (x) e guarde de novo com `sandmux vault add-cloud`",
                        "a cloud identity does not change: remove it (x) and store it again with `sandmux vault add-cloud`"
                    )
                    .into(),
                    true,
                ))
            }
            (Key::Char('d'), Some(e)) => {
                s.vault.form = Some(VaultForm::new(
                    FormKind::Domains(e.name.clone()),
                    e.domains.join(", "),
                ))
            }
            (Key::Char('x'), Some(e)) => s.vault.confirm = Some(e.name.clone()),
            (Key::Char('v'), Some(e)) if e.kind.is_empty() => {
                s.vault.form = Some(VaultForm::new(
                    FormKind::Value(e.name.clone()),
                    String::new(),
                ))
            }
            (Key::Char('v'), Some(_)) => {
                s.note = Some((
                    tr!(
                        "uma chave SSH se troca gravando a chave nova (s)",
                        "an SSH key is replaced by storing the new key (s)"
                    )
                    .into(),
                    true,
                ))
            }
            _ => {
                s.note = Some((
                    tr!(
                        "ponha o foco num segredo (Tab)",
                        "put the focus on a secret (Tab)"
                    )
                    .into(),
                    true,
                ))
            }
        }
    }

    /// O mouse na tela do grafo: a roda muda o zoom, arrastar move, e um
    /// clique escolhe o domínio da lista ou põe o foco num nó.
    pub(super) fn graph_mouse(
        &mut self,
        mut s: Box<Settings>,
        m: super::modes::Mouse,
    ) -> Box<Settings> {
        if s.vault.form.is_some() || s.vault.confirm.is_some() {
            return s;
        }
        let Some(g) = s.vault.graph.as_mut() else {
            return s;
        };
        let (x, y) = (m.x.saturating_sub(1), m.y.saturating_sub(1));
        let (ax, ay, aw, ah) = self.graph_rect.get();
        let inside = x >= ax && x < ax + aw && y >= ay && y < ay + ah;
        if m.is_wheel() {
            if inside && m.wheel_up() {
                g.win.zoom_in();
            } else if inside {
                g.win.zoom_out();
            }
            return s;
        }
        if m.release {
            g.drag = None;
            return s;
        }
        if m.is_motion() {
            if m.button() == 0
                && let Some((px, py)) = g.drag
            {
                g.drag_by(x as i32 - px as i32, y as i32 - py as i32);
                g.drag = Some((x, y));
            }
            return s;
        }
        if m.button() != 0 {
            return s;
        }
        let hit = self
            .settings_hits
            .borrow()
            .iter()
            .find(|(row, a, b, _)| *row == y && x >= *a && x < *b)
            .map(|h| h.3);
        match hit {
            Some(Hit::GraphRow(i)) => g.select(i),
            Some(Hit::GraphNode(i)) => g.focus = Some(i),
            _ => {}
        }
        if inside {
            g.drag = Some((x, y));
        }
        s
    }

    /// Manda o formulário ao daemon; os textos são apagados da memória ao
    /// sair (o formulário e a mensagem).
    fn vault_submit(&mut self, f: &VaultForm) -> Result<String, String> {
        let v = |i: usize| f.fields[i].as_str();
        let list = |i: usize, normalize: bool| -> Vec<String> {
            v(i).split(',')
                .map(|d| d.trim())
                .filter(|d| !d.is_empty())
                .map(|d| {
                    if normalize {
                        crate::vaultcmd::normalize_domain(d)
                    } else {
                        d.to_string()
                    }
                })
                .collect()
        };
        let mut msg = match &f.kind {
            FormKind::Init => {
                if v(0) != v(1) {
                    return Err(tr!("as senhas não conferem", "the passwords do not match").into());
                }
                Msg::VaultInit {
                    password: v(0).to_string(),
                }
            }
            FormKind::Unlock => Msg::VaultUnlock {
                password: v(0).to_string(),
            },
            FormKind::Keyring => Msg::KeyringOn {
                password: v(0).to_string(),
            },
            FormKind::Passwd => {
                if v(1) != v(2) {
                    return Err(tr!("as senhas não conferem", "the passwords do not match").into());
                }
                Msg::VaultPasswd {
                    old: v(0).to_string(),
                    new: v(1).to_string(),
                }
            }
            FormKind::Add => {
                let domains = list(2, true);
                if domains.is_empty() {
                    return Err(tr!(
                        "falta o domínio: a chave só é trocada no tráfego para os domínios dela",
                        "the domain is missing: the key is only swapped in traffic to its domains"
                    )
                    .into());
                }
                Msg::VaultAdd {
                    name: v(0).trim().to_string(),
                    value: v(1).to_string(),
                    domains,
                    fake: None,
                    tools: Vec::new(),
                }
            }
            FormKind::AddSsh => {
                let path = sandmux_policy::config::expand(Path::new(v(1).trim()), &self.host.home);
                let text = zeroize::Zeroizing::new(
                    std::fs::read_to_string(&path)
                        .map_err(|e| trf!("ler {}: {e}", "read {}: {e}", path.display()))?,
                );
                let pass = (!v(2).is_empty()).then(|| v(2));
                if sandmux::sshagent::is_encrypted(&text)? && pass.is_none() {
                    return Err(tr!(
                        "a chave tem passphrase: preencha o campo da passphrase",
                        "the key has a passphrase: fill in the passphrase field"
                    )
                    .into());
                }
                let key = zeroize::Zeroizing::new(sandmux::sshagent::import_key(&text, pass)?);
                Msg::VaultAddSsh {
                    name: v(0).trim().to_string(),
                    key: key.to_string(),
                    servers: list(3, false),
                }
            }
            FormKind::Value(name) => {
                if v(0).is_empty() {
                    return Err(
                        tr!("o valor não pode ser vazio", "the value cannot be empty").into(),
                    );
                }
                Msg::VaultSetValue {
                    name: name.clone(),
                    value: v(0).to_string(),
                }
            }
            FormKind::Domains(name) => Msg::VaultSetDomains {
                name: name.clone(),
                domains: list(0, true),
            },
        };
        let r = self.call(&msg).map_err(|e| format!("{e:#}"));
        // Apaga as senhas e os valores da mensagem.
        match &mut msg {
            Msg::VaultInit { password }
            | Msg::VaultUnlock { password }
            | Msg::KeyringOn { password } => zeroize::Zeroize::zeroize(password),
            Msg::VaultPasswd { old, new } => {
                zeroize::Zeroize::zeroize(old);
                zeroize::Zeroize::zeroize(new);
            }
            Msg::VaultAdd { value, .. } | Msg::VaultSetValue { value, .. } => {
                zeroize::Zeroize::zeroize(value)
            }
            Msg::VaultAddSsh { key, .. } => zeroize::Zeroize::zeroize(key),
            _ => {}
        }
        r?;
        Ok(match &f.kind {
            FormKind::Init => tr!("cofre criado e aberto", "vault created and open").into(),
            FormKind::Unlock => tr!("cofre desbloqueado", "vault unlocked").into(),
            FormKind::Passwd => tr!("senha trocada", "password changed").into(),
            FormKind::Add | FormKind::AddSsh => {
                trf!(
                    "{} guardado no cofre",
                    "{} stored in the vault",
                    v(0).trim()
                )
            }
            FormKind::Domains(n) => trf!("domínios de {n} trocados", "domains of {n} changed"),
            FormKind::Value(n) => trf!(
                "valor de {n} trocado; o falso e os domínios continuam os mesmos",
                "value of {n} replaced; the fake and the domains stay the same"
            ),
            FormKind::Keyring => tr!(
                "senha guardada no keyring: o daemon abre o cofre sozinho",
                "password stored in the keyring: the daemon opens the vault by itself"
            )
            .into(),
        })
    }
}

// ---- diagnóstico ----

impl App {
    /// Roda as verificações do `sandmux doctor` de novo.
    pub(super) fn doctor_refresh(&mut self, s: &mut Settings) {
        let d = &mut s.doctor;
        d.report = Some(crate::doctor::report(true));
        d.at = super::paste::clock();
        d.sel = d.sel.min(d.count().saturating_sub(1));
    }

    /// As teclas da tela do diagnóstico; `true` quando a tecla foi usada.
    fn doctor_key(&mut self, s: &mut Settings, k: &Key) -> bool {
        if let Some(f) = s.doctor.unlock.as_mut() {
            match k {
                Key::Esc => s.doctor.unlock = None,
                Key::Backspace => {
                    f.fields[0].pop();
                }
                Key::Char(c) => f.fields[0].push(*c),
                Key::Enter => {
                    if let Some(f) = s.doctor.unlock.take() {
                        match self.vault_submit(&f) {
                            Ok(_) => self.doctor_fix_start(s),
                            Err(e) => s.note = Some((e, true)),
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        if let Some(fix) = s.doctor.fix.as_mut() {
            if fix.on_agy {
                fix.on_agy = false;
                if matches!(k, Key::Char('s' | 'S' | 'y' | 'Y')) {
                    self.doctor_move_agy(s);
                } else if let Some(fix) = s.doctor.fix.as_mut() {
                    fix.agy = None;
                }
                self.doctor_fix_next(s);
                return true;
            }
            match k {
                Key::Esc => {
                    // Pula o arquivo inteiro: nada dele vai ao cofre.
                    fix.file += 1;
                    fix.key = 0;
                    fix.answers.clear();
                    self.doctor_fix_next(s);
                }
                Key::Backspace => {
                    fix.input.pop();
                }
                Key::Char(c) => fix.input.push(*c),
                Key::Enter => {
                    let domains = fix
                        .input
                        .split(',')
                        .map(str::trim)
                        .filter(|d| !d.is_empty())
                        .map(crate::vaultcmd::normalize_domain)
                        .collect();
                    fix.answers.push(domains);
                    fix.key += 1;
                    if fix.key >= fix.files[fix.file].1.found.len() {
                        self.doctor_commit_file(s);
                    }
                    self.doctor_fix_next(s);
                }
                _ => {}
            }
            return true;
        }
        let n = s.doctor.count();
        // O aviso da ação anterior não fica preso na tela.
        if matches!(k, Key::Up | Key::Down | Key::Esc | Key::Left) {
            s.note = None;
        }
        match k {
            Key::Up => s.doctor.sel = s.doctor.sel.saturating_sub(1),
            Key::Down => s.doctor.sel = (s.doctor.sel + 1).min(n.saturating_sub(1)),
            Key::Esc | Key::Left => s.on_items = false,
            Key::Char('r') => {
                self.doctor_refresh(s);
                let fails = s.doctor.report.as_ref().map_or(0, |r| r.failures());
                let at = &s.doctor.at;
                s.note = Some((
                    match fails {
                        0 => trf!(
                            "verificado de novo às {at}: nenhuma falha",
                            "checked again at {at}: no failures"
                        ),
                        1 => trf!(
                            "verificado de novo às {at}: 1 falha",
                            "checked again at {at}: 1 failure"
                        ),
                        n => trf!(
                            "verificado de novo às {at}: {n} falhas",
                            "checked again at {at}: {n} failures"
                        ),
                    },
                    false,
                ));
            }
            Key::Char('f') => self.doctor_fix_start(s),
            _ => return false,
        }
        true
    }

    /// Começa o `f`: lê as chaves reais e o login do `agy`, confere o cofre
    /// e pede a senha quando ele está trancado.
    fn doctor_fix_start(&mut self, s: &mut Settings) {
        let files = crate::doctor::real_secrets(&self.host);
        let agy = crate::doctor::agy_login(&self.host);
        if files.is_empty() && agy.is_none() {
            s.note = Some((
                tr!(
                    "nenhuma chave real dentro do sandbox: nada a mover",
                    "no real keys inside the sandbox: nothing to move"
                )
                .into(),
                false,
            ));
            return;
        }
        match self.call(&Msg::VaultStatus) {
            Ok(Msg::VaultInfo { exists: false, .. }) => {
                s.note = Some((
                    tr!(
                        "não há cofre: crie um na seção Cofre (n) e aperte f de novo",
                        "there is no vault: create one in the Vault section (n) and press f again"
                    )
                    .into(),
                    true,
                ));
                return;
            }
            Ok(Msg::VaultInfo {
                unlocked: false, ..
            }) => {
                s.doctor.unlock = Some(VaultForm::new(FormKind::Unlock, String::new()));
                return;
            }
            Ok(_) => {}
            Err(e) => {
                s.note = Some((format!("{e:#}"), true));
                return;
            }
        }
        s.note = None;
        s.doctor.fix = Some(FixFlow {
            files,
            file: 0,
            key: 0,
            answers: Vec::new(),
            input: String::new(),
            agy,
            on_agy: false,
            moved: 0,
            harnesses: Vec::new(),
            refused: Vec::new(),
        });
        self.doctor_fix_next(s);
    }

    /// Prepara o próximo passo: o campo da próxima chave, a confirmação do
    /// login do `agy` ou o fim.
    fn doctor_fix_next(&mut self, s: &mut Settings) {
        let Some(fix) = s.doctor.fix.as_mut() else {
            return;
        };
        while fix
            .files
            .get(fix.file)
            .is_some_and(|(_, r)| fix.key >= r.found.len())
        {
            fix.file += 1;
            fix.key = 0;
        }
        if let Some((h, r)) = fix.files.get(fix.file) {
            let x = &r.found[fix.key];
            fix.input = crate::doctor::suggest_domains(&self.host, h, r, x).join(", ");
            return;
        }
        if fix.agy.is_some() {
            fix.on_agy = true;
            return;
        }
        let Some(fix) = s.doctor.fix.take() else {
            return;
        };
        s.note = Some(if let Some(e) = fix.refused.first() {
            (e.clone(), true)
        } else if fix.moved == 0 {
            (
                tr!(
                    "nenhuma chave levada ao cofre",
                    "no keys moved to the vault"
                )
                .into(),
                false,
            )
        } else {
            (
                trf!(
                    "{} chave(s) levadas ao cofre; reinicie as abas de {} (Ctrl+b r)",
                    "{} key(s) moved to the vault; restart the {} tabs (Ctrl+b r)",
                    fix.moved,
                    fix.harnesses.join(", ")
                ),
                false,
            )
        });
        self.doctor_refresh(s);
        self.refresh_vault();
    }

    /// Manda ao cofre as chaves do arquivo atual, com os domínios
    /// confirmados, e só depois grava os falsos no arquivo.
    fn doctor_commit_file(&mut self, s: &mut Settings) {
        let Some(fix) = s.doctor.fix.as_mut() else {
            return;
        };
        let answers = std::mem::take(&mut fix.answers);
        let (h, r) = &fix.files[fix.file];
        let h = *h;
        let mut ok = true;
        for (x, domains) in r.found.iter().zip(answers) {
            let name = crate::doctor::key_name(h, r, x);
            let mut msg = Msg::VaultAdd {
                name: name.clone(),
                value: x.real.clone(),
                domains,
                fake: Some(x.fake.clone()),
                tools: Vec::new(),
            };
            let res = self.call(&msg);
            if let Msg::VaultAdd { value, .. } = &mut msg {
                zeroize::Zeroize::zeroize(value);
            }
            if let Err(e) = res {
                fix.refused.push(trf!(
                    "{name}: o cofre recusou ({e:#}); {} fica como estava",
                    "{name}: the vault refused ({e:#}); {} stays as it was",
                    r.file.display()
                ));
                ok = false;
                break;
            }
        }
        if ok {
            let layout = sandmux_policy::Layout::new(&self.host.data_dir);
            match sandmux_import::apply_fakes(h, &layout.home(h), &layout.manifest(h), r) {
                Ok(()) => {
                    fix.moved += r.found.len();
                    if !fix.harnesses.contains(&h) {
                        fix.harnesses.push(h);
                    }
                }
                Err(e) => fix.refused.push(format!("{}: {e}", r.file.display())),
            }
        }
        fix.file += 1;
        fix.key = 0;
    }

    /// Leva o login do Google do `agy` ao cofre.
    fn doctor_move_agy(&mut self, s: &mut Settings) {
        let Some(fix) = s.doctor.fix.as_mut() else {
            return;
        };
        let Some((path, l)) = fix.agy.take() else {
            return;
        };
        match crate::vaultcmd::store_agy_login(&l, &path) {
            Ok(()) => {
                fix.moved += l.reals().len();
                if !fix.harnesses.contains(&"agy") {
                    fix.harnesses.push("agy");
                }
            }
            Err(e) => fix.refused.push(trf!(
                "agy: o cofre recusou ({e:#}); o arquivo fica como estava",
                "agy: the vault refused ({e:#}); the file stays as it was"
            )),
        }
    }
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

    fn item(key: &str) -> &'static Item {
        ITEMS.iter().find(|i| i.key == key).unwrap()
    }

    #[test]
    fn todas_as_secoes_tem_itens() {
        for (s, name) in SECTIONS.iter().enumerate() {
            if s == VAULT || s == DOCTOR || s == PROCS {
                continue;
            }
            assert!(!items_of(s).is_empty(), "{}", name[0]);
        }
        // As chaves de camada são as que o editor aceita.
        use sandmux_policy::config as c;
        for i in ITEMS.iter().filter(|i| i.at == At::Layer) {
            let k = i.key;
            assert!(
                c::BOOL_KEYS.contains(&k)
                    || c::LIMIT_KEYS.contains(&k)
                    || c::LIST_KEYS.contains(&k)
                    || c::NET_LIST_KEYS.contains(&k)
                    || c::FILE_LIST_KEYS.contains(&k)
                    || c::MCP_LIST_KEYS.contains(&k)
                    || [
                        c::SSH_KEYS,
                        c::DB_KEY,
                        c::PORTS_KEY,
                        c::HOST_PORTS_KEY,
                        c::WINDOW_KEY,
                        "network"
                    ]
                    .contains(&k),
                "{k}"
            );
        }
        // O HTTP/2 do gateway fica na seção de rede, ligado por padrão.
        let h = ITEMS.iter().find(|i| i.key == "http2").unwrap();
        assert!(items_of(4).iter().any(|&i| ITEMS[i].key == "http2"));
        assert!(matches!(h.kind, Kind::Toggle) && default_bool("http2"));
    }

    #[test]
    fn grava_le_e_recusa() {
        let d = tempfile::tempdir().unwrap();
        let h = host(d.path());
        let p = d.path().join("config.toml");
        let proj = d.path().join("w");
        std::fs::create_dir_all(&proj).unwrap();
        // Toggle no global.
        apply(&p, &h, item("kvm"), None, Change::Bool(true)).unwrap();
        let doc = load(&p).unwrap();
        let c = current(&doc, item("kvm"), None);
        assert_eq!((c.bool, c.from.clone()), (Some(true), Source::Here));
        // No projeto, herdado do global.
        let c = current(&doc, item("kvm"), Some(&proj));
        assert_eq!((c.bool, c.from), (Some(true), Source::Global));
        // GPU sem valor: o padrão, ligado.
        let c = current(&doc, item("gpu"), None);
        assert_eq!((c.bool, c.from), (Some(true), Source::Default));
        // Valor recusado: nada muda.
        let before = std::fs::read_to_string(&p).unwrap();
        let e = apply(&p, &h, item("memory"), None, Change::Text("abc".into())).unwrap_err();
        assert!(!e.is_empty());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before);
        // Lista.
        apply(&p, &h, item("host-ports"), None, Change::Add("8888".into())).unwrap();
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("host-ports = [8888]")
        );
        let doc = load(&p).unwrap();
        assert_eq!(current(&doc, item("host-ports"), None).list, vec!["8888"]);
        apply(
            &p,
            &h,
            item("host-ports"),
            None,
            Change::Remove("8888".into()),
        )
        .unwrap();
        // Tema, modelo e retenção fora das camadas, mesmo com o projeto.
        let n = apply(
            &p,
            &h,
            item("theme"),
            Some(&proj),
            Change::Text("tokyo-night".into()),
        )
        .unwrap();
        assert!(n.contains("vale na hora"), "{n}");
        apply(&p, &h, item("models"), None, Change::Add("openai/x".into())).unwrap();
        apply(
            &p,
            &h,
            item("net-log-days"),
            None,
            Change::Text("30".into()),
        )
        .unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        let cfg = sandmux_policy::config::Config::parse(&p, &text).unwrap();
        assert_eq!(cfg.tui.theme.name, "tokyo-night");
        assert_eq!(cfg.assistant.models, vec!["openai/x"]);
        assert_eq!(cfg.net_log_days, Some(30));
        // Modo de rede do projeto.
        apply(
            &p,
            &h,
            item("network"),
            Some(&proj),
            Change::Text("sem-rede".into()),
        )
        .unwrap();
        let doc = load(&p).unwrap();
        let c = current(&doc, item("network"), Some(&proj));
        assert_eq!(
            (c.value.as_deref(), c.from),
            (Some("sem-rede"), Source::Here)
        );
        assert_eq!(
            current(&doc, item("network"), None).value.as_deref(),
            Some("aberta")
        );
    }
}
