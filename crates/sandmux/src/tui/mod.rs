//! Interface de terminal: abas, uma por sessão do daemon.
//!
//! Threads: uma lê a entrada crua, uma por aba lê a conexão anexada da
//! sessão, uma manda um tique por segundo (lista de sessões) e uma espera os
//! sinais. Todas mandam eventos ao laço principal, que guarda o estado e
//! desenha.

pub mod input;
pub mod modes;
pub mod netscreen;
pub mod paste;
pub mod picker;
pub mod render;
pub mod settings;

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use nix::sys::signal::{SigSet, Signal};
use sandmux::client::{self, Conn};
use sandmux_policy::config::{Config, TuiConfig};
use sandmux_policy::{Host, Layout, home, presets};
use sandmux_proto::msg::NetLine;
use sandmux_proto::term::ModeTracker;
use sandmux_proto::{
    Frame, LaunchOptions, Msg, SessionInfo, SessionState, Size, read_frame, write_control,
    write_data,
};

use input::{Input, Key, Parser};
use modes::Mouse;
use sandmux_proto::pane::{Dir, Pane, Rect as PRect, Sep, Toward};
use sandmux_spec::{tr, trf};

/// Ações da ajuda, na ordem, com a descrição em português e em inglês.
pub const HELP: &[(&str, &str, &str)] = &[
    ("new", "nova aba", "new tab"),
    ("next", "próxima aba", "next tab"),
    ("prev", "aba anterior", "previous tab"),
    ("list", "lista de abas", "tab list"),
    (
        "split-side",
        "dividir o painel lado a lado",
        "split the pane side by side",
    ),
    (
        "split-stack",
        "dividir o painel em cima e embaixo",
        "split the pane top and bottom",
    ),
    (
        "zoom",
        "ampliar o painel ou voltar ao layout",
        "zoom the pane or go back to the layout",
    ),
    ("pane-grow", "aumentar o painel", "grow the pane"),
    ("pane-shrink", "diminuir o painel", "shrink the pane"),
    ("group-prev", "grupo de abas anterior", "previous tab group"),
    ("group-next", "próximo grupo de abas", "next tab group"),
    (
        "groups",
        "grupos de abas: trocar, criar, renomear, levar a aba",
        "tab groups: switch, create, rename, move the tab",
    ),
    ("rename", "renomear a aba", "rename the tab"),
    (
        "restart",
        "reiniciar a aba encerrada ou parada",
        "restart the ended or stopped tab",
    ),
    ("close", "fechar a aba", "close the tab"),
    (
        "paste-image",
        "colar imagem do clipboard",
        "paste an image from the clipboard",
    ),
    (
        "detach",
        "sair (as sessões continuam)",
        "quit (sessions keep running)",
    ),
    ("help", "esta ajuda", "this help"),
    (
        "panel",
        "mostrar ou esconder o painel lateral",
        "show or hide the side panel",
    ),
    (
        "network",
        "rede da aba: acessos e regras",
        "tab network: accesses and rules",
    ),
    ("files", "acessos a arquivos da aba", "tab file accesses"),
    (
        "folders",
        "pastas montadas na aba",
        "folders mounted in the tab",
    ),
    ("mcp", "servidores MCP da aba", "tab MCP servers"),
    (
        "chat",
        "conversar com um modelo numa aba de chat",
        "talk to a model in a chat tab",
    ),
    (
        "command",
        "gerar um comando e escrevê-lo na aba",
        "generate a command and type it into the tab",
    ),
    ("settings", "configurações", "settings"),
    (
        "enter",
        "shell dentro do sandbox da aba (o que o agente vê)",
        "shell inside the tab's sandbox (what the agent sees)",
    ),
    ("menu", "menu de ações", "action menu"),
];

/// A descrição de uma ação da ajuda no idioma atual.
pub fn help_label(item: &(&'static str, &'static str, &'static str)) -> &'static str {
    tr!(item.1, item.2)
}

/// Linhas da tela da aba mandadas com a pergunta do modal de comando.
pub const SCREEN_LINES: usize = 40;

/// A pergunta com as últimas `max` linhas da tela (sem as vazias do fim),
/// como contexto para o modelo.
pub fn with_screen(question: &str, screen: &str, max: usize) -> String {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    let start = end.saturating_sub(max);
    let shown = lines[start..end].join("\n");
    if shown.trim().is_empty() {
        return question.to_string();
    }
    format!("{question}\n\nÚltimas linhas do terminal da aba, como contexto:\n```\n{shown}\n```")
}

/// As ações do menu: as da ajuda, sem o próprio menu, com a descrição no
/// idioma atual.
pub fn menu_items() -> Vec<(&'static str, &'static str)> {
    HELP.iter()
        .filter(|(a, _, _)| *a != "menu")
        .map(|h| (h.0, help_label(h)))
        .collect()
}

const HISTORY: usize = 10_000;

/// Resultado da criação: a conexão anexada, o identificador e os avisos, ou
/// a pergunta de importação a fazer antes.
type Created = Result<(UnixStream, u32, Vec<String>), String>;

enum Ev {
    Stdin(Vec<u8>),
    StdinClosed,
    Frame(u64, Frame),
    Closed(u64),
    Tick,
    Winch,
    Quit,
    /// Resposta do modelo ao modal de comando, com o número do pedido.
    Answer(u64, std::result::Result<String, String>),
    /// A lista de modelos dos provedores, para o seletor.
    Models(std::result::Result<(Vec<String>, Vec<String>), String>),
}

pub struct Tab {
    pub id: u32,
    pub name: String,
    pub state: SessionState,
    pub workdir: PathBuf,
    pub opts: LaunchOptions,
    pub parser: vt100::Parser<paste::Clip>,
    pub tracker: ModeTracker,
    /// Conexão anexada (escrita) e o número dela.
    conn: Option<(u64, UnixStream)>,
    pub scroll: usize,
    /// A sessão acessou um domínio novo enquanto a aba estava inativa.
    pub new_domain: bool,
    /// A sessão teve um bloqueio de rede enquanto a aba estava inativa.
    pub blocked: bool,
    /// O harness terminou com a TUI aberta: perguntar se reinicia.
    pub ask_restart: bool,
    /// Começo do lançamento atual (segundos Unix), para o tempo na barra.
    pub started_at: Option<u64>,
    /// Chaves reais achadas pelo vigia do daemon (protocolo 27).
    pub keys: sandmux_proto::msg::KeyNote,
    /// Janela do último lançamento (protocolo 34).
    pub window: sandmux_spec::window::WindowMode,
}

/// Seleção de texto, em coordenadas da tela da aba.
#[derive(Debug, Clone, Copy)]
pub struct Sel {
    start: (u16, u16),
    end: (u16, u16),
}

impl Sel {
    fn ordered(&self) -> ((u16, u16), (u16, u16)) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn contains(&self, row: u16, col: u16) -> bool {
        let (a, b) = self.ordered();
        (row, col) >= a && (row, col) <= b
    }
}

pub struct NewTab {
    pub field: usize,
    pub harness: usize,
    pub workdir: String,
    pub profile: String,
    pub error: Option<String>,
    /// Pergunta de importação pendente.
    pub question: Option<String>,
}

impl NewTab {
    /// Escolhas do modal: os harnesses e, por último, o chat.
    pub const KINDS: usize = presets::PRESETS.len() + 1;

    pub fn harness(&self) -> &'static str {
        presets::PRESETS
            .get(self.harness % Self::KINDS)
            .copied()
            .unwrap_or("chat")
    }
}

/// Modal de chat: a mensagem e o modelo.
pub struct ChatModal {
    /// 0 = mensagem, 1 = modelo, 2 = botão Enviar.
    pub field: usize,
    pub text: String,
    pub model: String,
    /// Posição nos modelos da configuração, ao passar com as setas.
    pub choice: Option<usize>,
    pub error: Option<String>,
}

/// Etapa do modal de comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdStage {
    Ask,
    Waiting,
    Command,
}

/// Modal de comando: a pergunta, o modelo e o comando sugerido.
pub struct CommandModal {
    pub stage: CmdStage,
    /// 0 = pergunta, 1 = modelo, 2 = tela, 3 = botão Enviar.
    pub field: usize,
    /// Mandar as últimas linhas da tela da aba com a pergunta (desligado ao
    /// abrir; só vai quando o usuário liga).
    pub screen: bool,
    pub question: String,
    pub model: String,
    pub choice: Option<usize>,
    pub command: String,
    pub error: Option<String>,
    /// Número do pedido em curso; respostas de outro pedido são descartadas.
    pub token: u64,
    /// Aba sob o modal.
    pub tab: u32,
    /// A pasta que vai ao modelo: a atual do processo da aba, ou a de
    /// trabalho (protocolo 33).
    pub folder: String,
}

/// O que espera o desbloqueio do cofre.
enum Pending {
    Chat(String, String),
    Command(Box<CommandModal>),
}

pub enum Modal {
    /// A aba terminou: reiniciar, fechar ou deixar (id e código).
    Ended(u32, i32),
    New(NewTab),
    Chat(ChatModal),
    Command(Box<CommandModal>),
    List(usize),
    Rename(String),
    Close,
    Help,
    Net(Box<netscreen::NetScreen>),
    /// Tela de arquivos: os acessos da sessão, o escolhido e um aviso.
    Files(Box<FileScreen>),
    /// Tela de pastas em /mnt/sandmux.
    Folders(Box<FolderScreen>),
    /// Tela de MCPs da aba.
    Mcp(Box<McpScreen>),
    /// Senha do cofre trancado: o texto digitado e o último erro.
    Unlock(String, Option<String>),
    /// Menu de ações: a escolhida.
    Menu(usize),
    /// Página de configurações.
    Settings(Box<settings::Settings>),
    /// Lista de grupos (Ctrl+b G).
    Groups(GroupsModal),
}

/// A lista de grupos: o escolhido e o nome sendo digitado (novo, ou o
/// renomear de um grupo).
pub struct GroupsModal {
    pub sel: usize,
    pub edit: Option<(GroupEdit, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupEdit {
    New,
    Rename(String),
}

/// O que um clique na faixa de uma seção do painel acerta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHit {
    Toggle(String),
    Up(String),
    Down(String),
}

/// O que um clique na página de configurações acerta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Section(usize),
    Item(usize),
}

pub struct FileScreen {
    pub id: u32,
    pub lines: Vec<sandmux_spec::FileEntry>,
    pub sel: usize,
    pub note: Option<String>,
}

pub struct McpScreen {
    pub id: u32,
    pub list: Vec<sandmux_proto::msg::McpEntry>,
    pub sel: usize,
    pub note: Option<String>,
}

pub struct FolderScreen {
    pub id: u32,
    pub list: Vec<sandmux_spec::LiveFolder>,
    pub sel: usize,
    /// Texto da pasta sendo digitada (`a`).
    pub input: Option<String>,
    /// Pasta digitada, esperando `r` ou `w`.
    pub ask: Option<String>,
    pub note: Option<String>,
}

pub struct App {
    pub tabs: Vec<Tab>,
    /// Grupo de abas mostrado (protocolo 30).
    pub group: String,
    /// Os grupos, com o número de abas, do último `sync` (mais o atual).
    pub groups: Vec<(String, usize)>,
    /// Layouts de painéis das abas do grupo (protocolo 32).
    pub layouts: Vec<Pane>,
    /// O painel ativo ampliado para a aba inteira.
    pub zoom: bool,
    /// Divisão pedida, esperando a sessão nova: o painel e a direção.
    pending_split: Option<(u32, Dir)>,
    pub active: usize,
    pub modal: Option<Modal>,
    pub sel: Option<Sel>,
    pub bar_ranges: Vec<(u16, u16, usize)>,
    /// Colunas do ✕ das abas encerradas na barra: o clique fecha a aba.
    pub close_ranges: Vec<(u16, u16, usize)>,
    pub cfg: TuiConfig,
    status: Option<(String, Instant)>,
    size: Size,
    tx: Sender<Ev>,
    next_uid: u64,
    control: Option<Conn>,
    cwd: PathBuf,
    host: Host,
    /// Modos ligados no terminal do usuário e como desfazê-los.
    applied: (Vec<u8>, Vec<u8>),
    /// O terminal está em foco (pelo aviso de foco, `?1004`).
    focused: bool,
    /// Último foco informado ao daemon: em foco e a aba mostrada.
    focus_sent: Option<(bool, Option<u32>)>,
    quit: bool,
    /// Painel lateral ligado (pelo usuário).
    pub panel: bool,
    /// Último `StatsOf` da aba ativa, para o painel.
    pub panel_data: Option<Msg>,
    /// Últimas linhas do registro de rede da aba ativa, para o painel.
    pub panel_net: Vec<NetLine>,
    /// Servidores MCP da aba ativa, para o painel.
    pub panel_mcp: Vec<sandmux_proto::msg::McpEntry>,
    /// Hora da linha de rede mais nova já vista, para as marcas da barra.
    net_since: String,
    /// Seção `[assistant]` da configuração.
    pub assistant: sandmux_policy::assistant::AssistantConfig,
    /// Pedido do assistente feito com o cofre trancado.
    after_unlock: Option<Pending>,
    /// Último número de pedido do modal de comando.
    ask_token: u64,
    /// Clipboard do sistema, para o que a TUI copia.
    clipboard: paste::System,
    /// Cofre: existe e está desbloqueado (relido a cada segundo).
    pub vault: Option<(bool, bool)>,
    /// Linha da barra (a partir de 1) e quantas linhas a barra empurra a
    /// área da aba para baixo (1 com a barra em cima, 0 embaixo).
    pub bar_row: u16,
    pub area_dy: u16,
    /// Posição do botão do modal de comando (x, y, largura), do último desenho.
    pub modal_button: std::cell::Cell<Option<(u16, u16, u16)>>,
    /// Colunas do `≡` do menu na barra.
    pub menu_button: Option<(u16, u16)>,
    /// Linhas clicáveis da página de configurações: linha, colunas e o que é.
    pub settings_hits: std::cell::RefCell<Vec<(u16, u16, u16, Hit)>>,
    /// Faixas das seções do painel: linha, colunas e o que é.
    pub panel_hits: std::cell::RefCell<Vec<(u16, u16, u16, PanelHit)>>,
    /// Seletor de modelo aberto, por cima do modal.
    pub picker: Option<picker::Picker>,
    /// Lista de modelos dos provedores, guardada até a TUI fechar.
    pub models: picker::Models,
}

/// Linhas de rede no painel.
pub const PANEL_NET: u32 = 6;

/// Largura do painel lateral.
pub const PANEL: u16 = 34;
/// Largura mínima do terminal para o painel aparecer.
pub const PANEL_MIN: u16 = 100;

/// O painel aparece com o terminal desta largura?
pub fn panel_visible(on: bool, cols: u16) -> bool {
    on && cols >= PANEL_MIN
}

fn area_size(panel: bool) -> Size {
    let s = client::terminal_size();
    let cols = if panel_visible(panel, s.cols) {
        s.cols - PANEL
    } else {
        s.cols
    };
    Size {
        cols,
        rows: s.rows.saturating_sub(1).max(1),
    }
}

fn new_parser(size: Size) -> vt100::Parser<paste::Clip> {
    let nz = |v: u16| std::num::NonZeroU16::new(v.max(1)).unwrap_or(std::num::NonZeroU16::MIN);
    vt100::Parser::new_with_callbacks(
        nz(size.rows),
        nz(size.cols),
        HISTORY,
        paste::Clip::default(),
    )
}

fn out(bytes: &[u8]) {
    let mut o = std::io::stdout().lock();
    let _ = o.write_all(bytes);
    let _ = o.flush();
}

/// Base64 sem dependência, para o OSC 52.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Lê os quadros de uma conexão e manda ao laço.
fn reader(tx: Sender<Ev>, uid: u64, mut stream: UnixStream) {
    loop {
        match read_frame(&mut stream) {
            Ok(Some(f)) => {
                if tx.send(Ev::Frame(uid, f)).is_err() {
                    return;
                }
            }
            _ => {
                let _ = tx.send(Ev::Closed(uid));
                return;
            }
        }
    }
}

impl App {
    pub fn prefix_name(&self) -> String {
        format!("Ctrl+{}", self.cfg.prefix)
    }

    pub fn status_text(&self) -> String {
        match &self.status {
            Some((s, t)) if t.elapsed() < Duration::from_secs(6) => s.clone(),
            _ => String::new(),
        }
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = Some((s.into(), Instant::now()));
    }

    fn control(&mut self) -> Result<&mut Conn> {
        if self.control.is_none() {
            self.control = Some(client::connect_or_start()?);
        }
        Ok(self.control.as_mut().expect("conexão"))
    }

    fn call(&mut self, msg: &Msg) -> Result<Msg> {
        let r = self.control()?.call(msg);
        if r.is_err() {
            self.control = None;
        }
        r
    }

    /// Anexa uma conexão já pronta (depois de `Attached`/`Created`) à aba.
    fn bind(&mut self, idx: usize, stream: UnixStream) {
        self.next_uid += 1;
        let uid = self.next_uid;
        if let Ok(r) = stream.try_clone() {
            let tx = self.tx.clone();
            std::thread::spawn(move || reader(tx, uid, r));
        }
        if let Some(t) = self.tabs.get_mut(idx) {
            if let Some((_, old)) = t.conn.take() {
                let _ = old.shutdown(Shutdown::Both);
            }
            t.conn = Some((uid, stream));
        }
    }

    /// Abre a conexão anexada de uma sessão existente.
    fn attach(&mut self, idx: usize) {
        let id = self.tabs[idx].id;
        let r = (|| -> Result<UnixStream> {
            let mut c = Conn::connect()?
                .ok_or_else(|| anyhow::anyhow!(tr!("o daemon parou", "the daemon stopped")))?;
            c.send(&Msg::Attach {
                id: Some(id),
                size: self.size,
            })?;
            Ok(c.stream)
        })();
        match r {
            Ok(s) => self.bind(idx, s),
            Err(e) => self.say(trf!("anexar {id}: {e:#}", "attach {id}: {e:#}")),
        }
    }

    /// Lê as respostas até `Created` ou `Error`, guardando os avisos.
    fn until_created(stream: &mut UnixStream) -> Result<(u32, String, Vec<String>)> {
        let mut warnings = Vec::new();
        loop {
            match read_frame(stream)? {
                Some(Frame::Control(Msg::Warning { message })) => warnings.push(message),
                Some(Frame::Control(Msg::Created { id, name })) => return Ok((id, name, warnings)),
                Some(Frame::Control(Msg::Error { message })) => bail!(message),
                Some(_) => {}
                None => bail!(tr!(
                    "o daemon encerrou a conexão",
                    "the daemon closed the connection"
                )),
            }
        }
    }

    fn tab_from(&self, s: &SessionInfo) -> Tab {
        Tab {
            id: s.id,
            name: s.name.clone(),
            state: s.state.clone(),
            workdir: s.workdir.clone(),
            opts: s.opts.clone(),
            parser: new_parser(self.size),
            tracker: ModeTracker::new(),
            conn: None,
            scroll: 0,
            new_domain: false,
            blocked: false,
            ask_restart: false,
            started_at: s.started_at,
            keys: s.keys,
            window: s.window,
        }
    }

    /// Acerta as abas com a lista de sessões do daemon.
    /// Troca o grupo mostrado: as abas que saem são desanexadas (sem
    /// encerrar), e as do grupo novo são anexadas pelo `sync`.
    fn switch_group(&mut self, group: String) {
        if group == self.group {
            return;
        }
        for t in &mut self.tabs {
            if let Some((_, s)) = t.conn.take() {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
        self.tabs.clear();
        self.active = 0;
        self.panel_data = None;
        self.panel_net.clear();
        self.group = group;
        self.sync();
        self.say(trf!("grupo {}", "group {}", self.group));
    }

    /// O grupo anterior (`-1`) ou o próximo (`1`), em ordem de nome.
    fn step_group(&mut self, by: isize) {
        self.sync();
        let n = self.groups.len() as isize;
        if n < 2 {
            return;
        }
        let cur = self
            .groups
            .iter()
            .position(|(g, _)| *g == self.group)
            .unwrap_or(0) as isize;
        let next = self.groups[((cur + by).rem_euclid(n)) as usize].0.clone();
        self.switch_group(next);
    }

    fn sync(&mut self) {
        let (sessions, layouts) = match self.call(&Msg::List) {
            Ok(Msg::Sessions { sessions, layouts }) => (sessions, layouts),
            _ => return,
        };
        let mut groups: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        groups.insert(sandmux_proto::msg::main_group(), 0);
        groups.insert(self.group.clone(), 0);
        for s in &sessions {
            *groups.entry(s.group.clone()).or_default() += 1;
        }
        self.groups = groups.into_iter().collect();
        let sessions: Vec<_> = sessions
            .into_iter()
            .filter(|s| s.group == self.group)
            .collect();
        // Só os layouts com todas as sessões no grupo mostrado.
        let layouts: Vec<Pane> = layouts
            .into_iter()
            .filter(|l| {
                l.leaves()
                    .iter()
                    .all(|i| sessions.iter().any(|s| s.id == *i))
            })
            .collect();
        let relayout = layouts != self.layouts;
        self.layouts = layouts;
        let count = self.tabs.len();
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        self.tabs.retain(|t| sessions.iter().any(|s| s.id == t.id));
        for s in &sessions {
            match self.tabs.iter().position(|t| t.id == s.id) {
                Some(i) => {
                    let t = &mut self.tabs[i];
                    t.name = s.name.clone();
                    t.opts = s.opts.clone();
                    t.started_at = s.started_at;
                    t.keys = s.keys;
                    t.window = s.window;
                    let was_live = t.state == SessionState::Viva;
                    t.state = s.state.clone();
                    if was_live && matches!(t.state, SessionState::Encerrada { .. }) {
                        t.ask_restart = true;
                    }
                    if !was_live && t.state == SessionState::Viva && t.conn.is_none() {
                        t.parser = new_parser(self.size);
                        self.attach(i);
                    }
                }
                None => {
                    let t = self.tab_from(s);
                    self.tabs.push(t);
                    let i = self.tabs.len() - 1;
                    if !matches!(s.state, SessionState::Parada { .. }) {
                        self.attach(i);
                    }
                }
            }
        }
        self.tabs.sort_by_key(|t| t.id);
        self.active = active_id
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            .unwrap_or(self.active.min(self.tabs.len().saturating_sub(1)));
        if self.zoom
            && self
                .tabs
                .get(self.active)
                .is_none_or(|t| self.layout_of(t.id).is_none())
        {
            self.zoom = false;
        }
        if relayout || self.tabs.len() != count {
            self.resize_all();
        }
    }

    /// O layout da aba do painel `id`, quando ela tem painéis.
    pub fn layout_of(&self, id: u32) -> Option<&Pane> {
        self.layouts.iter().find(|l| l.contains(id))
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// As abas da barra: o índice da primeira sessão de cada uma e quantos
    /// painéis a mais ela tem.
    pub fn bar_entries(&self) -> Vec<(usize, usize)> {
        self.tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match self.layout_of(t.id) {
                None => Some((i, 0)),
                Some(l) if l.first() == t.id => Some((i, l.leaves().len() - 1)),
                Some(_) => None,
            })
            .collect()
    }

    /// O índice da primeira sessão da aba do painel ativo.
    pub fn active_entry(&self) -> usize {
        let Some(t) = self.tabs.get(self.active) else {
            return self.active;
        };
        self.layout_of(t.id)
            .and_then(|l| self.index_of(l.first()))
            .unwrap_or(self.active)
    }

    fn area_rect(&self) -> PRect {
        PRect {
            x: 0,
            y: 0,
            w: self.size.cols,
            h: self.size.rows,
        }
    }

    /// Os painéis visíveis da aba ativa (índice e retângulo na área) e os
    /// separadores.
    pub fn visible_panes(&self) -> (Vec<(usize, PRect)>, Vec<Sep>) {
        let full = self.area_rect();
        let Some(t) = self.tabs.get(self.active) else {
            return (Vec::new(), Vec::new());
        };
        match self.layout_of(t.id) {
            Some(l) if !self.zoom => {
                let (rects, seps) = l.rects(full);
                (
                    rects
                        .into_iter()
                        .filter_map(|(id, r)| self.index_of(id).map(|i| (i, r)))
                        .collect(),
                    seps,
                )
            }
            _ => (vec![(self.active, full)], Vec::new()),
        }
    }

    /// Vai à aba da posição `pos` da barra.
    fn goto_entry(&mut self, pos: usize) {
        if let Some(&(i, _)) = self.bar_entries().get(pos) {
            self.leave_zoom();
            self.set_active(i);
        }
    }

    /// A aba seguinte (`1`) ou a anterior (`-1`) da barra.
    fn step_entry(&mut self, by: isize) {
        let e = self.bar_entries();
        if e.is_empty() {
            return;
        }
        let act = self.active_entry();
        let p = e.iter().position(|(i, _)| *i == act).unwrap_or(0) as isize;
        let (i, _) = e[(p + by).rem_euclid(e.len() as isize) as usize];
        self.leave_zoom();
        self.set_active(i);
    }

    fn leave_zoom(&mut self) {
        if self.zoom {
            self.zoom = false;
            self.resize_all();
        }
    }

    /// Grava o layout de uma aba no daemon e o aplica.
    fn set_layout(&mut self, pane: Pane) {
        match self.call(&Msg::SetLayout { pane: pane.clone() }) {
            Ok(_) => {
                let leaves = pane.leaves();
                self.layouts
                    .retain(|l| !l.leaves().iter().any(|i| leaves.contains(i)));
                if leaves.len() > 1 {
                    self.layouts.push(pane);
                }
                self.resize_all();
            }
            Err(e) => self.say(trf!("painéis: {e:#}", "panes: {e:#}")),
        }
    }

    /// Começa uma divisão do painel ativo: o modal de nova aba, com a pasta
    /// dele.
    fn split(&mut self, dir: Dir) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        let (id, wd) = (t.id, t.workdir.clone());
        self.pending_split = Some((id, dir));
        self.open_new();
        if let Some(Modal::New(n)) = self.modal.as_mut() {
            n.workdir = wd.display().to_string();
        }
    }

    /// O foco no painel vizinho, na direção dada.
    fn focus_pane(&mut self, toward: Toward) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        let id = t.id;
        let area = self.area_rect();
        let next = self
            .layout_of(id)
            .and_then(|l| l.neighbor(id, toward, area))
            .and_then(|n| self.index_of(n));
        if let Some(i) = next {
            self.leave_zoom();
            self.set_active(i);
        }
    }

    /// Aumenta ou diminui o painel ativo.
    fn resize_pane(&mut self, delta: i16) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        let id = t.id;
        if let Some(mut l) = self.layout_of(id).cloned()
            && l.resize(id, delta)
        {
            self.set_layout(l);
        }
    }

    /// Copia para o terminal do usuário (OSC 52) e para o clipboard do
    /// sistema, quando há acesso.
    fn copy_text(&mut self, text: &str) {
        out(format!("\x1b]52;c;{}\x07", base64(text.as_bytes())).as_bytes());
        self.clipboard.set_text(text);
    }

    /// O estado do cofre, para a barra.
    fn refresh_vault(&mut self) {
        if let Ok(Msg::VaultInfo {
            exists, unlocked, ..
        }) = self.call(&Msg::VaultStatus)
        {
            self.vault = Some((exists, unlocked));
        }
    }

    /// Abre a pergunta de reinício da aba ativa, quando ela terminou com a
    /// TUI aberta e não há outro modal.
    fn prompt_ended(&mut self) {
        if self.modal.is_some() {
            return;
        }
        let Some(t) = self.tabs.get_mut(self.active) else {
            return;
        };
        if !t.ask_restart {
            return;
        }
        t.ask_restart = false;
        if let SessionState::Encerrada { codigo } = t.state {
            self.modal = Some(Modal::Ended(t.id, codigo));
        }
    }

    /// Encerra a sessão da aba `i` e tira a aba da barra.
    fn close_tab(&mut self, i: usize) {
        let Some(id) = self.tabs.get(i).map(|t| t.id) else {
            return;
        };
        // Um painel: quem fica com o espaço recebe o foco, na mesma aba.
        let stay = self.layout_of(id).and_then(|l| l.sibling(id));
        match self.call(&Msg::Kill { id }) {
            Ok(_) => {
                if matches!(self.modal, Some(Modal::Ended(m, _)) if m == id) {
                    self.modal = None;
                }
                if let Some((_, s)) = self.tabs[i].conn.take() {
                    let _ = s.shutdown(Shutdown::Both);
                }
                self.tabs.remove(i);
                if self.active > i || self.active >= self.tabs.len() {
                    self.active = self.active.saturating_sub(1);
                }
                self.active = self.active.min(self.tabs.len().saturating_sub(1));
                // O layout sem o painel fechado, já, sem esperar o `sync`.
                let layouts = std::mem::take(&mut self.layouts);
                self.layouts = layouts
                    .into_iter()
                    .filter_map(|l| {
                        if l.contains(id) {
                            l.remove(id)
                        } else {
                            Some(l)
                        }
                    })
                    .filter(|l| l.leaves().len() > 1)
                    .collect();
                if let Some(j) = stay.and_then(|s| self.index_of(s)) {
                    self.zoom = false;
                    self.set_active(j);
                }
                self.resize_all();
                self.say(trf!("aba {id} fechada", "tab {id} closed"));
            }
            Err(e) => self.say(trf!("fechar: {e:#}", "close: {e:#}")),
        }
    }

    fn send_active(&mut self, bytes: &[u8]) {
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.scroll = 0;
            if let Some((_, s)) = t.conn.as_mut() {
                let _ = write_data(s, bytes);
            }
        }
    }

    fn set_active(&mut self, i: usize) {
        if i < self.tabs.len() {
            if i != self.active {
                self.panel_data = None;
                self.panel_net.clear();
            }
            self.active = i;
            self.tabs[i].new_domain = false;
            self.tabs[i].blocked = false;
            self.sel = None;
        }
    }

    /// Pede ao daemon o consumo e a proteção da aba ativa, para o painel.
    fn refresh_panel(&mut self) {
        if !self.panel {
            return;
        }
        let Some(id) = self.tabs.get(self.active).map(|t| t.id) else {
            self.panel_data = None;
            return;
        };
        self.panel_data = match self.call(&Msg::Stats { id }) {
            Ok(m @ Msg::StatsOf { .. }) => Some(m),
            _ => None,
        };
        self.panel_net = match self.call(&Msg::Net {
            id: Some(id),
            since: None,
            limit: PANEL_NET,
        }) {
            Ok(Msg::NetLog { lines }) => lines,
            _ => Vec::new(),
        };
        self.panel_mcp = self.mcp_list(id);
    }

    fn mcp_list(&mut self, id: u32) -> Vec<sandmux_proto::msg::McpEntry> {
        match self.call(&Msg::McpList { id }) {
            Ok(Msg::Mcps { list }) => list,
            _ => Vec::new(),
        }
    }

    /// Marca as abas inativas cuja sessão acessou um domínio novo desde a
    /// última consulta.
    fn refresh_marks(&mut self) {
        let Ok(Msg::NetLog { lines }) = self.call(&Msg::Net {
            id: None,
            since: Some(self.net_since.clone()),
            limit: 1000,
        }) else {
            return;
        };
        let active = self.tabs.get(self.active).map(|t| t.id);
        for l in &lines {
            if l.ts > self.net_since {
                self.net_since = l.ts.clone();
            }
            let Some(sid) = l.session else { continue };
            if Some(sid) == active {
                continue;
            }
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == sid) {
                if l.blocked.is_some() {
                    t.blocked = true;
                } else if l.new {
                    t.new_domain = true;
                }
            }
        }
    }

    /// Liga os modos da aba ativa no terminal do usuário, desfazendo os da
    /// anterior. Com um modal aberto, nenhum.
    fn apply_modes(&mut self) {
        let (want, undo) = match (&self.modal, self.tabs.get(self.active)) {
            (None, Some(t)) => (
                modes::host_modes(t.parser.screen(), &t.tracker),
                modes::host_reset(&t.tracker),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        if want != self.applied.0 {
            out(&self.applied.1);
            out(&want);
            // O `host_reset` desliga o aviso de foco, que a TUI usa sempre.
            out(b"\x1b[?1004h");
            self.applied = (want, undo);
        }
    }

    /// O foco do terminal: guarda, repassa à aba ativa só se o harness dela
    /// pediu o aviso de foco.
    fn on_focus(&mut self, focused: bool) {
        self.focused = focused;
        if self.tabs.get(self.active).is_some_and(|t| t.tracker.focus) {
            self.send_active(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
        self.report_focus();
    }

    /// Diz ao daemon o foco e a aba mostrada, quando mudaram: ele não
    /// notifica o desktop sobre a aba que uma TUI em foco mostra.
    fn report_focus(&mut self) {
        let now = (self.focused, self.tabs.get(self.active).map(|t| t.id));
        if self.focus_sent != Some(now)
            && self
                .call(&Msg::Focus {
                    focused: now.0,
                    active: now.1,
                })
                .is_ok()
        {
            self.focus_sent = Some(now);
        }
    }

    fn resize_all(&mut self) {
        self.size = area_size(self.panel);
        // Cada sessão no tamanho do seu painel; ampliada, a área inteira.
        let full = self.area_rect();
        let mut sizes: std::collections::HashMap<u32, Size> = std::collections::HashMap::new();
        for l in &self.layouts {
            for (id, r) in l.rects(full).0 {
                sizes.insert(
                    id,
                    Size {
                        cols: r.w.max(1),
                        rows: r.h.max(1),
                    },
                );
            }
        }
        if self.zoom
            && let Some(t) = self.tabs.get(self.active)
        {
            sizes.insert(t.id, self.size);
        }
        for t in &mut self.tabs {
            let size = sizes.get(&t.id).copied().unwrap_or(self.size);
            let (rows, cols) = t.parser.screen().size();
            if (rows.get(), cols.get()) != (size.rows, size.cols) {
                t.parser.screen_mut().set_size(
                    std::num::NonZeroU16::new(size.rows.max(1)).unwrap(),
                    std::num::NonZeroU16::new(size.cols.max(1)).unwrap(),
                );
            }
            if let Some((_, s)) = t.conn.as_mut() {
                let _ = write_control(s, &Msg::Resize { size });
            }
        }
    }

    fn on_frame(&mut self, uid: u64, f: Frame) {
        let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.conn.as_ref().is_some_and(|(u, _)| *u == uid))
        else {
            return;
        };
        let t = &mut self.tabs[i];
        match f {
            Frame::Data(d) => {
                t.parser.process(&d);
                // Cópias do harness que o daemon deixou passar (clipboard
                // liberado): o emulador as consome, a TUI as repassa.
                let copies = std::mem::take(&mut t.parser.callbacks_mut().pending);
                for c in copies {
                    self.copy_text(&c);
                }
                let t = &mut self.tabs[i];
                t.tracker.feed(&d);
            }
            Frame::Control(Msg::Attached { name, .. }) => {
                t.name = name;
                // O terminal pode ter mudado desde o pedido.
                let size = self.size;
                if let Some((_, s)) = t.conn.as_mut() {
                    let _ = write_control(s, &Msg::Resize { size });
                }
            }
            Frame::Control(Msg::Exited { code, .. }) => {
                if t.state == SessionState::Viva {
                    t.ask_restart = true;
                }
                t.state = SessionState::Encerrada { codigo: code };
            }
            Frame::Control(Msg::Error { message }) => {
                if let SessionState::Viva = t.state {
                } else {
                    t.state = SessionState::Parada { motivo: message };
                }
            }
            Frame::Control(Msg::Detached { reason }) => {
                t.conn = None;
                let id = t.id;
                self.say(trf!("aba {id}: {reason}", "tab {id}: {reason}"));
            }
            _ => {}
        }
    }

    // ---- ações ----

    fn action(&mut self, a: &str) {
        match a {
            "new" => self.open_new(),
            "group-prev" => self.step_group(-1),
            "group-next" => self.step_group(1),
            "groups" => {
                self.sync();
                let sel = self
                    .groups
                    .iter()
                    .position(|(g, _)| *g == self.group)
                    .unwrap_or(0);
                self.modal = Some(Modal::Groups(GroupsModal { sel, edit: None }));
            }
            "next" => self.step_entry(1),
            "prev" => self.step_entry(-1),
            "split-side" => self.split(Dir::Side),
            "split-stack" => self.split(Dir::Stack),
            "zoom" => {
                if self
                    .tabs
                    .get(self.active)
                    .is_some_and(|t| self.layout_of(t.id).is_some())
                {
                    self.zoom = !self.zoom;
                    self.resize_all();
                } else {
                    self.say(tr!(
                        "ampliar: a aba não tem painéis",
                        "zoom: the tab has no panes"
                    ));
                }
            }
            "pane-grow" => self.resize_pane(5),
            "pane-shrink" => self.resize_pane(-5),
            "pane-left" => self.focus_pane(Toward::Left),
            "pane-right" => self.focus_pane(Toward::Right),
            "pane-up" => self.focus_pane(Toward::Up),
            "pane-down" => self.focus_pane(Toward::Down),
            "list" => self.modal = Some(Modal::List(self.active)),
            "rename" if !self.tabs.is_empty() => {
                self.modal = Some(Modal::Rename(self.tabs[self.active].name.clone()))
            }
            "close" if !self.tabs.is_empty() => self.modal = Some(Modal::Close),
            "restart" => self.restart(),
            "paste-image" => self.paste_image(),
            "detach" => self.quit = true,
            "help" => self.modal = Some(Modal::Help),
            "menu" => self.modal = Some(Modal::Menu(0)),
            "enter" => self.enter_active(),
            "settings" => self.open_settings(),
            "network" => self.open_net(),
            "chat" => self.open_chat(),
            "command" => self.open_command(),
            "mcp" => {
                if let Some(id) = self.tabs.get(self.active).map(|t| t.id) {
                    let list = self.mcp_list(id);
                    self.modal = Some(Modal::Mcp(Box::new(McpScreen {
                        id,
                        list,
                        sel: 0,
                        note: None,
                    })));
                }
            }
            "folders" => {
                if let Some(id) = self.tabs.get(self.active).map(|t| t.id) {
                    let list = self.folder_list(id);
                    self.modal = Some(Modal::Folders(Box::new(FolderScreen {
                        id,
                        list,
                        sel: 0,
                        input: None,
                        ask: None,
                        note: None,
                    })));
                }
            }
            "files" => {
                if let Some(id) = self.tabs.get(self.active).map(|t| t.id) {
                    self.modal = Some(Modal::Files(Box::new(FileScreen {
                        id,
                        lines: crate::filescmd::recent(id, 2),
                        sel: 0,
                        note: None,
                    })));
                }
            }
            "panel" => {
                self.panel = !self.panel;
                self.resize_all();
                self.refresh_panel();
            }
            _ => {}
        }
    }

    /// Recolhe ou expande uma seção do painel, ou a move na ordem, e grava
    /// em `[tui]` (`panel-collapsed`, `panel-order`).
    fn panel_click(&mut self, h: PanelHit) {
        let up = matches!(h, PanelHit::Up(_));
        let (key, value) = match h {
            PanelHit::Toggle(s) => {
                let c = &mut self.cfg.panel_collapsed;
                match c.iter().position(|x| *x == s) {
                    Some(i) => {
                        c.remove(i);
                    }
                    None => c.push(s),
                }
                ("panel-collapsed", c.clone())
            }
            PanelHit::Up(s) | PanelHit::Down(s) if self.cfg.panel_order.contains(&s) => {
                let o = &mut self.cfg.panel_order;
                let i = o.iter().position(|x| *x == s).unwrap_or(0);
                let j = if up { i.checked_sub(1) } else { Some(i + 1) };
                match j {
                    Some(j) if j < o.len() => o.swap(i, j),
                    _ => return,
                }
                ("panel-order", o.clone())
            }
            _ => return,
        };
        // Lista vazia apaga a chave; a ordem padrão também.
        let default_order = key == "panel-order"
            && value
                .iter()
                .map(String::as_str)
                .eq(sandmux_policy::config::PANEL_SECTIONS.iter().copied());
        let v: Option<toml_edit::Value> = (!value.is_empty() && !default_order)
            .then(|| toml_edit::Array::from_iter(value).into());
        let path = sandmux_policy::config::path(&self.host);
        if let Err(e) = sandmux_policy::config::edit::set_value(&path, &["tui"], key, v) {
            self.say(trf!(
                "painel: não gravou no config.toml: {e}",
                "panel: could not write to config.toml: {e}"
            ));
        }
    }

    fn open_chat(&mut self) {
        // A configuração do momento: o modelo padrão pode ter mudado.
        if let Ok(c) = Config::load(&sandmux_policy::config::path(&self.host)) {
            self.assistant = c.assistant;
        }
        self.modal = Some(Modal::Chat(ChatModal {
            field: 0,
            text: String::new(),
            model: self.assistant.chat_model.clone().unwrap_or_default(),
            choice: None,
            error: None,
        }));
    }

    fn open_command(&mut self) {
        let Some(tab) = self.tabs.get(self.active).map(|t| t.id) else {
            self.say(tr!(
                "comando: abra uma aba antes",
                "command: open a tab first"
            ));
            return;
        };
        if let Ok(c) = Config::load(&sandmux_policy::config::path(&self.host)) {
            self.assistant = c.assistant;
        }
        // A pasta em que o shell da aba está agora; sem ela, a de trabalho.
        let folder = match self.call(&Msg::Cwd { id: tab }) {
            Ok(Msg::CwdOf { path: Some(p) }) => p.display().to_string(),
            _ => self
                .tabs
                .iter()
                .find(|t| t.id == tab)
                .map(|t| t.workdir.display().to_string())
                .unwrap_or_default(),
        };
        self.modal = Some(Modal::Command(Box::new(CommandModal {
            stage: CmdStage::Ask,
            field: 0,
            question: String::new(),
            model: self.assistant.default_command_model().unwrap_or_default(),
            choice: None,
            command: String::new(),
            error: None,
            token: 0,
            tab,
            screen: false,
            folder,
        })));
    }

    /// Manda a pergunta do modal de comando ao modelo, numa thread.
    fn ask_command(&mut self) {
        let Some(Modal::Command(c)) = &self.modal else {
            return;
        };
        let (question, model, tab, screen, folder) = (
            c.question.trim().to_string(),
            c.model.trim().to_string(),
            c.tab,
            c.screen,
            c.folder.clone(),
        );
        let fail = |app: &mut App, e: String| {
            if let Some(Modal::Command(c)) = &mut app.modal {
                c.error = Some(e);
                c.stage = CmdStage::Ask;
            }
        };
        if question.is_empty() {
            return fail(self, tr!("escreva a pergunta", "write the question").into());
        }
        if model.is_empty() {
            return fail(
                self,
                tr!(
                    "escolha um modelo (<provedor>/<modelo>) ou defina `command-model` em [assistant]",
                    "pick a model (<provider>/<model>) or set `command-model` in [assistant]"
                )
                .into(),
            );
        }
        let assistant = self.assistant.clone();
        match crate::chatcmd::check(&mut |m| self.call(m), &assistant, &model) {
            Err(e) => return fail(self, e),
            Ok(crate::chatcmd::Check::Locked) => {
                if let Some(Modal::Command(c)) = self.modal.take() {
                    self.after_unlock = Some(Pending::Command(c));
                }
                self.modal = Some(Modal::Unlock(String::new(), None));
                return;
            }
            Ok(crate::chatcmd::Check::Ok) => {}
        }
        let shell = presets::system_shell(&self.host)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bash".into());
        self.ask_token += 1;
        let token = self.ask_token;
        if let Some(Modal::Command(c)) = &mut self.modal {
            c.stage = CmdStage::Waiting;
            c.token = token;
            c.error = None;
        }
        // A tela só vai quando o usuário pediu, nesta pergunta.
        let question = match self.tabs.iter().find(|t| t.id == tab) {
            Some(t) if screen => {
                with_screen(&question, &t.parser.screen().contents(), SCREEN_LINES)
            }
            _ => question,
        };
        let tx = self.tx.clone();
        let system = sandmux_policy::assistant::command_prompt(&folder, &shell);
        std::thread::spawn(move || {
            let r = (|| -> Result<String> {
                let mut conn = client::connect_or_start()?;
                match conn.call(&Msg::Ask {
                    model,
                    text: question,
                    vars: client::client_vars(&[])?,
                    system: Some(system),
                })? {
                    Msg::Answer { text } => Ok(text),
                    m => Err(anyhow::anyhow!(trf!(
                        "resposta inesperada do daemon: {m:?}",
                        "unexpected reply from the daemon: {m:?}"
                    ))),
                }
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Ev::Answer(token, r));
        });
    }

    fn on_answer(&mut self, token: u64, r: std::result::Result<String, String>) {
        let Some(Modal::Command(c)) = &mut self.modal else {
            return;
        };
        if c.token != token || c.stage != CmdStage::Waiting {
            return;
        }
        match r {
            Ok(text) => {
                let cmd = sandmux_policy::assistant::clean_command(&text);
                if cmd.is_empty() {
                    c.error = Some(
                        tr!(
                            "o modelo não devolveu um comando",
                            "the model returned no command"
                        )
                        .into(),
                    );
                    c.stage = CmdStage::Ask;
                } else {
                    c.command = cmd;
                    c.stage = CmdStage::Command;
                }
            }
            Err(e) => {
                c.error = Some(e);
                c.stage = CmdStage::Ask;
            }
        }
    }

    /// Escreve o comando do modal na aba sob ele, sem executar.
    /// Envia a mensagem do modal de chat (Enter ou o botão Enviar).
    fn chat_send(&mut self) {
        let Some(Modal::Chat(c)) = self.modal.as_mut() else {
            return;
        };
        let (model, text) = (c.model.trim().to_string(), c.text.trim().to_string());
        if text.is_empty() {
            c.error = Some(tr!("escreva a mensagem", "write the message").into());
            c.field = 0;
        } else if model.is_empty() {
            c.error = Some(
                tr!(
                    "escolha um modelo (<provedor>/<modelo>) ou defina `chat-model` em [assistant]",
                    "pick a model (<provider>/<model>) or set `chat-model` in [assistant]"
                )
                .into(),
            );
            c.field = 1;
        } else {
            self.submit_chat(model, text);
        }
    }

    fn insert_command(&mut self) {
        let Some(Modal::Command(c)) = &mut self.modal else {
            return;
        };
        let cmd = c.command.trim_end_matches(['\n', '\r']).to_string();
        if cmd.trim().is_empty() {
            return;
        }
        let Some(i) = self.tabs.iter().position(|t| t.id == c.tab) else {
            c.error = Some(
                tr!(
                    "a aba sob o modal foi fechada",
                    "the tab under the dialog was closed"
                )
                .into(),
            );
            return;
        };
        let bytes = if cmd.contains('\n') {
            if !self.tabs[i].parser.screen().bracketed_paste() {
                c.error = Some(
                    tr!(
                        "o comando tem várias linhas e a aba não aceita colagem: a quebra de linha o executaria. Edite para uma linha",
                        "the command has several lines and the tab does not accept paste: the line break would run it. Edit it into one line"
                    )
                    .into(),
                );
                return;
            }
            [b"\x1b[200~".as_slice(), cmd.as_bytes(), b"\x1b[201~"].concat()
        } else {
            cmd.into_bytes()
        };
        self.modal = None;
        self.set_active(i);
        self.send_active(&bytes);
    }

    fn chat_error(&mut self, e: String) {
        if let Some(Modal::Chat(c)) = &mut self.modal {
            c.error = Some(e);
        } else {
            self.say(format!("chat: {e}"));
        }
    }

    /// Confere o modelo, o cofre e a chave e cria a aba de chat. Com o cofre
    /// trancado, abre o desbloqueio e cria depois.
    fn submit_chat(&mut self, model: String, text: String) {
        let assistant = self.assistant.clone();
        let r = crate::chatcmd::check(&mut |m| self.call(m), &assistant, &model);
        match r {
            Err(e) => self.chat_error(e),
            Ok(crate::chatcmd::Check::Locked) => {
                self.after_unlock = Some(Pending::Chat(model, text));
                self.modal = Some(Modal::Unlock(String::new(), None));
            }
            Ok(crate::chatcmd::Check::Ok) => {
                let size = self.size;
                let group = self.group.clone();
                let r = (|| -> Result<(UnixStream, u32, Vec<String>, PathBuf)> {
                    let dir = crate::chatcmd::new_tab_dir(&model, Some(&text))?;
                    let created = (|| -> Result<(UnixStream, u32, Vec<String>)> {
                        let mut c = client::connect_or_start()?;
                        c.send(&Msg::Create {
                            opts: Box::new(crate::chatcmd::tab_options(dir.clone())),
                            vars: client::client_vars(&[])?,
                            size,
                            attach: true,
                            group: Some(group.clone()),
                        })?;
                        let mut s = c.stream;
                        let (id, _, warnings) = Self::until_created(&mut s)?;
                        Ok((s, id, warnings))
                    })();
                    match created {
                        Ok((s, id, w)) => Ok((s, id, w, dir)),
                        Err(e) => {
                            crate::chatcmd::discard_dir(&dir);
                            Err(e)
                        }
                    }
                })();
                match r {
                    Ok((s, id, warnings, dir)) => {
                        self.modal = None;
                        self.add_tab(s, id, dir, sandmux::chat::HARNESS, &warnings);
                    }
                    Err(e) => self.chat_error(format!("{e:#}")),
                }
            }
        }
    }

    /// Acrescenta a aba de uma sessão recém-criada e a ativa.
    fn add_tab(
        &mut self,
        s: UnixStream,
        id: u32,
        workdir: PathBuf,
        harness: &str,
        warnings: &[String],
    ) {
        let size = self.size;
        self.tabs.push(Tab {
            id,
            name: String::new(),
            state: SessionState::Viva,
            workdir,
            opts: LaunchOptions::new(harness),
            parser: new_parser(size),
            tracker: ModeTracker::new(),
            conn: None,
            scroll: 0,
            new_domain: false,
            blocked: false,
            ask_restart: false,
            started_at: None,
            keys: Default::default(),
            window: Default::default(),
        });
        self.tabs.sort_by_key(|t| t.id);
        let i = self.tabs.iter().position(|t| t.id == id).unwrap_or(0);
        self.bind(i, s);
        self.set_active(i);
        // Uma divisão pedida: a sessão nova ocupa a parte nova do painel.
        if let Some((target, dir)) = self.pending_split.take()
            && self.index_of(target).is_some()
        {
            let mut l = self
                .layout_of(target)
                .cloned()
                .unwrap_or(Pane::leaf(target));
            if l.split(target, dir, id) {
                self.zoom = false;
                self.set_layout(l);
            }
        }
        if let Some(w) = warnings.first() {
            self.say(trf!("aviso: {w}", "warning: {w}"));
        }
    }

    /// Abre uma aba com um shell dentro do sandbox da aba ativa.
    fn enter_active(&mut self) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        if t.opts.harness == sandmux::launch::ENTER {
            return self.say(tr!(
                "esta aba já é um shell de dentro",
                "this tab is already a shell inside"
            ));
        }
        let mut opts = LaunchOptions::new(sandmux::launch::ENTER);
        opts.workdir = t.workdir.clone();
        opts.args = vec![t.id.to_string()];
        let workdir = t.workdir.clone();
        let size = self.size;
        let group = self.group.clone();
        let r = (|| -> Result<(UnixStream, u32, Vec<String>)> {
            let mut c = client::connect_or_start()?;
            c.send(&Msg::Create {
                opts: Box::new(opts),
                vars: Default::default(),
                size,
                attach: true,
                group: Some(group.clone()),
            })?;
            let mut s = c.stream;
            let (id, _, warnings) = Self::until_created(&mut s)?;
            Ok((s, id, warnings))
        })();
        match r {
            Ok((s, id, w)) => self.add_tab(s, id, workdir, sandmux::launch::ENTER, &w),
            Err(e) => self.say(trf!("entrar: {e:#}", "enter: {e:#}")),
        }
    }

    fn open_new(&mut self) {
        self.modal = Some(Modal::New(NewTab {
            field: 0,
            harness: presets::PRESETS
                .iter()
                .position(|p| *p == "shell")
                .unwrap_or(0),
            workdir: self.cwd.display().to_string(),
            profile: String::new(),
            error: None,
            question: None,
        }));
    }

    fn restart(&mut self) {
        let Some(t) = self.tabs.get(self.active) else {
            return;
        };
        if t.state == SessionState::Viva {
            self.say(tr!(
                "a aba está viva; feche-a antes para relançar",
                "the tab is alive; close it before relaunching"
            ));
            return;
        }
        let (id, opts) = (t.id, t.opts.clone());
        let r = (|| -> Result<UnixStream> {
            let host = Host::detect()?;
            let names = if opts.harness == sandmux::launch::ENTER {
                Vec::new()
            } else {
                sandmux::launch::effective(&host, &opts)?.pass_env_names()
            };
            let vars = client::client_vars(&names)?;
            let mut c = client::connect_or_start()?;
            c.send(&Msg::Restart {
                id,
                vars,
                size: self.size,
                attach: true,
            })?;
            let mut s = c.stream;
            Self::until_created(&mut s)?;
            Ok(s)
        })();
        match r {
            Ok(s) => {
                let i = self.active;
                // A pergunta desta aba perde o sentido.
                if matches!(self.modal, Some(Modal::Ended(m, _)) if m == id) {
                    self.modal = None;
                }
                self.tabs[i].ask_restart = false;
                self.tabs[i].parser = new_parser(self.size);
                self.tabs[i].tracker = ModeTracker::new();
                self.tabs[i].state = SessionState::Viva;
                self.bind(i, s);
                self.say(trf!("aba {id} reiniciada", "tab {id} restarted"));
            }
            Err(e) => self.say(trf!("reiniciar: {e:#}", "restart: {e:#}")),
        }
    }

    fn paste_image(&mut self) {
        let host = match Host::detect() {
            Ok(h) => h,
            Err(e) => return self.say(format!("{e}")),
        };
        let inbox = Layout::new(&host.data_dir).inbox();
        let r = paste::clipboard_image()
            .and_then(|(w, h, rgba)| paste::save_png(&inbox, w, h, &rgba, &paste::stamp()));
        match r {
            Ok(p) => {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let inside = host.h("inbox").join(&name);
                self.send_active(inside.display().to_string().as_bytes());
                self.say(trf!(
                    "imagem colada: ~/inbox/{name}",
                    "image pasted: ~/inbox/{name}"
                ));
            }
            Err(e) => self.say(e),
        }
    }

    /// Cria a sessão do modal de nova aba. `import` responde à pergunta de
    /// importação, quando ela já foi feita.
    fn create(&mut self, import: Option<bool>) {
        let Some(Modal::New(n)) = &self.modal else {
            return;
        };
        // O chat não é harness: abre o modal de chat, que cria a aba.
        if n.harness() == "chat" {
            self.open_chat();
            return;
        }
        let harness = n.harness().to_string();
        let workdir = sandmux::launch::absolute(std::path::Path::new(n.workdir.trim()), &self.cwd);
        let profile = Some(n.profile.trim().to_string()).filter(|p| !p.is_empty());
        let size = self.size;
        let group = self.group.clone();
        let r = (|| -> Result<Created> {
            let host = Host::detect()?;
            presets::plan(&harness, &host, &[], &[])?;
            let mut opts = LaunchOptions::new(&harness);
            opts.workdir = workdir.clone();
            opts.profile = profile.clone();
            let eff = sandmux::launch::effective(&host, &opts)?;
            let layout = Layout::new(&host.data_dir);
            if !layout.home(&harness).exists() {
                let has_state = sandmux_import::importable(&harness)
                    && sandmux_import::has_state(&harness, &host.home);
                match (has_state, import) {
                    (true, None) => {
                        return Ok(Err(trf!(
                            "Primeira execução do {harness}. Importar a configuração existente? s/n",
                            "First run of {harness}. Import the existing configuration? y/n"
                        )));
                    }
                    (true, Some(true)) => {
                        let synthetic = layout.home(&harness);
                        home::ensure(&host, &layout, &harness)?;
                        // No `shell`, a importação substitui o arquivo mínimo
                        // gerado.
                        sandmux_import::import(
                            &harness,
                            &host.home,
                            &synthetic,
                            &layout.manifest(&harness),
                            harness == "shell",
                        )?;
                        if harness == "hermes" {
                            sandmux_policy::hermes_config::ensure_update_check_off(&synthetic)?;
                        }
                    }
                    _ => {
                        home::ensure(&host, &layout, &harness)?;
                    }
                }
            }
            let vars = client::client_vars(&eff.pass_env_names())?;
            let mut c = client::connect_or_start()?;
            c.send(&Msg::Create {
                opts: Box::new(opts),
                vars,
                size,
                attach: true,
                group: Some(group.clone()),
            })?;
            let mut s = c.stream;
            let (id, _, warnings) = Self::until_created(&mut s)?;
            Ok(Ok((s, id, warnings)))
        })();
        match r {
            Ok(Ok((s, id, warnings))) => {
                self.modal = None;
                self.add_tab(s, id, workdir, &harness, &warnings);
            }
            Ok(Err(question)) => {
                if let Some(Modal::New(n)) = &mut self.modal {
                    n.question = Some(question);
                    n.error = None;
                }
            }
            Err(e) => {
                if let Some(Modal::New(n)) = &mut self.modal {
                    n.error = Some(format!("{e:#}"));
                    n.question = None;
                }
            }
        }
    }

    fn open_net(&mut self) {
        let Some(id) = self.tabs.get(self.active).map(|t| t.id) else {
            return;
        };
        match self.load_net(id) {
            Ok(ns) => self.modal = Some(Modal::Net(Box::new(ns))),
            Err(e) => self.say(trf!("rede: {e}", "network: {e}")),
        }
    }

    fn load_net(&mut self, id: u32) -> std::result::Result<netscreen::NetScreen, String> {
        let (opts, lines) = netscreen::fetch(&mut |m| self.call(m), id)?;
        netscreen::build(id, &self.host, opts, lines)
    }

    /// Aplica o efeito de uma ação da tela de rede e recarrega a tela.
    fn net_effect(
        &mut self,
        mut ns: Box<netscreen::NetScreen>,
        r: std::result::Result<netscreen::Effect, String>,
    ) -> Box<netscreen::NetScreen> {
        let note = match r {
            Err(e) => e,
            Ok(netscreen::Effect::File(msg)) => msg,
            Ok(netscreen::Effect::NetEdit { action, value }) => match self.call(&Msg::NetEdit {
                id: ns.id,
                action: action.into(),
                value: value.clone(),
            }) {
                Ok(_) => match action {
                    "deny" => trf!("{value} negado na aba", "{value} denied in the tab"),
                    "allow" => trf!("{value} liberado na aba", "{value} allowed in the tab"),
                    "forget" => trf!("{value} tirado da aba", "{value} removed from the tab"),
                    _ => trf!(
                        "modo da aba: {}",
                        "tab mode: {}",
                        sandmux_policy::config::value_label("network", &value)
                    ),
                },
                Err(e) => format!("{e:#}"),
            },
        };
        if let Ok(mut fresh) = self.load_net(ns.id) {
            fresh.list = ns.list;
            for i in 0..2 {
                let len = if i == 0 {
                    fresh.lines.len()
                } else {
                    fresh.rules.len()
                };
                fresh.sel[i] = ns.sel[i].min(len.saturating_sub(1));
            }
            ns = Box::new(fresh);
        }
        ns.note = Some(note);
        ns
    }

    fn net_key(
        &mut self,
        mut ns: Box<netscreen::NetScreen>,
        k: Key,
    ) -> Option<Box<netscreen::NetScreen>> {
        use netscreen::{Layer, Pending};
        if let Some(p) = ns.ask.clone() {
            let layer = match k {
                Key::Char('a') if p.tab_allowed() => Some(Layer::Tab),
                Key::Char('p') => Some(Layer::Project),
                Key::Char('g') => Some(Layer::Global),
                Key::Esc => {
                    ns.ask = None;
                    return Some(ns);
                }
                _ => None,
            };
            if let Some(l) = layer {
                ns.ask = None;
                let r = netscreen::apply(&self.host, &ns, &p, l);
                return Some(self.net_effect(ns, r));
            }
            return Some(ns);
        }
        ns.note = None;
        match k {
            Key::Esc | Key::Char('q') => return None,
            Key::Tab => ns.list = 1 - ns.list,
            Key::Up | Key::Char('k') => ns.sel[ns.list] = ns.sel[ns.list].saturating_sub(1),
            Key::Down | Key::Char('j') => {
                let last = ns.len().saturating_sub(1);
                ns.sel[ns.list] = (ns.sel[ns.list] + 1).min(last);
            }
            Key::Char(c @ ('n' | 'l')) if ns.list == 0 => {
                match ns.selected_line().and_then(netscreen::pattern_of) {
                    Some(p) if c == 'n' => ns.ask = Some(Pending::Deny(p)),
                    Some(p) => ns.ask = Some(Pending::Allow(p)),
                    None => {
                        ns.note = Some(
                            tr!(
                                "o acesso não tem destino para regra",
                                "the access has no destination for a rule"
                            )
                            .into(),
                        )
                    }
                }
            }
            Key::Char(' ') if ns.list == 1 => {
                if let Some(r) = ns.selected_rule() {
                    ns.ask = Some(Pending::Toggle(r.clone()));
                }
            }
            Key::Char('x') if ns.list == 1 => {
                if let Some(r) = ns.selected_rule().cloned() {
                    let e = netscreen::forget(&self.host, &r);
                    return Some(self.net_effect(ns, e));
                }
            }
            Key::Char('m') => {
                let next = netscreen::next_mode(ns.mode);
                let e = Ok(netscreen::Effect::NetEdit {
                    action: "mode",
                    value: next.to_string(),
                });
                return Some(self.net_effect(ns, e));
            }
            _ => {}
        }
        Some(ns)
    }

    fn files_key(&mut self, mut fs: Box<FileScreen>, k: Key) -> Option<Box<FileScreen>> {
        fs.note = None;
        match k {
            Key::Esc | Key::Char('q') => return None,
            Key::Up | Key::Char('k') => fs.sel = fs.sel.saturating_sub(1),
            Key::Down | Key::Char('j') => {
                fs.sel = (fs.sel + 1).min(fs.lines.len().saturating_sub(1));
            }
            Key::Char(c @ ('b' | 'l')) => {
                if let Some(path) = fs.lines.get(fs.sel).map(|l| l.path.display().to_string()) {
                    let action = if c == 'b' { "file-deny" } else { "file-allow" };
                    fs.note = Some(
                        match self.call(&Msg::NetEdit {
                            id: fs.id,
                            action: action.into(),
                            value: path.clone(),
                        }) {
                            Ok(_) if c == 'b' => {
                                trf!("{path} negado na aba", "{path} denied in the tab")
                            }
                            Ok(_) => trf!("{path} liberado na aba", "{path} allowed in the tab"),
                            Err(e) => format!("{e:#}"),
                        },
                    );
                }
            }
            _ => {}
        }
        Some(fs)
    }

    fn folder_list(&mut self, id: u32) -> Vec<sandmux_spec::LiveFolder> {
        match self.call(&Msg::FolderList { id }) {
            Ok(Msg::Folders { list }) => list,
            _ => Vec::new(),
        }
    }

    fn folders_key(&mut self, mut fs: Box<FolderScreen>, k: Key) -> Option<Box<FolderScreen>> {
        if let Some(text) = fs.input.as_mut() {
            match k {
                Key::Esc => fs.input = None,
                Key::Enter => {
                    let t = text.trim().to_string();
                    fs.input = None;
                    if !t.is_empty() {
                        fs.ask = Some(t);
                    }
                }
                Key::Backspace => {
                    text.pop();
                }
                Key::Char(c) => text.push(c),
                _ => {}
            }
            return Some(fs);
        }
        if let Some(path) = fs.ask.clone() {
            let rw = match k {
                Key::Char('r') => Some(false),
                Key::Char('w') => Some(true),
                Key::Esc => {
                    fs.ask = None;
                    return Some(fs);
                }
                _ => None,
            };
            if let Some(rw) = rw {
                fs.ask = None;
                let path = match path.strip_prefix("~/") {
                    Some(rest) => self.host.home.join(rest).display().to_string(),
                    None => path,
                };
                fs.note = Some(
                    match self.call(&Msg::FolderEdit {
                        id: fs.id,
                        action: "add".into(),
                        value: path.clone(),
                        rw,
                    }) {
                        Ok(_) => trf!("{path} montada", "{path} mounted"),
                        Err(e) => format!("{e:#}"),
                    },
                );
                fs.list = self.folder_list(fs.id);
            }
            return Some(fs);
        }
        fs.note = None;
        match k {
            Key::Esc | Key::Char('q') => return None,
            Key::Up | Key::Char('k') => fs.sel = fs.sel.saturating_sub(1),
            Key::Down | Key::Char('j') => {
                fs.sel = (fs.sel + 1).min(fs.list.len().saturating_sub(1))
            }
            Key::Char('a') => fs.input = Some(String::new()),
            Key::Char('x') => {
                if let Some(name) = fs.list.get(fs.sel).map(|f| f.name.clone()) {
                    fs.note = Some(
                        match self.call(&Msg::FolderEdit {
                            id: fs.id,
                            action: "remove".into(),
                            value: name.clone(),
                            rw: false,
                        }) {
                            Ok(_) => trf!(
                                "/mnt/sandmux/{name} desmontada",
                                "/mnt/sandmux/{name} unmounted"
                            ),
                            Err(e) => format!("{e:#}"),
                        },
                    );
                    fs.list = self.folder_list(fs.id);
                    fs.sel = fs.sel.min(fs.list.len().saturating_sub(1));
                }
            }
            _ => {}
        }
        Some(fs)
    }

    fn mcp_key(&mut self, mut ms: Box<McpScreen>, k: Key) -> Option<Box<McpScreen>> {
        ms.note = None;
        match k {
            Key::Esc | Key::Char('q') => return None,
            Key::Up | Key::Char('k') => ms.sel = ms.sel.saturating_sub(1),
            Key::Down | Key::Char('j') => {
                ms.sel = (ms.sel + 1).min(ms.list.len().saturating_sub(1))
            }
            Key::Char(' ') => {
                if let Some(m) = ms.list.get(ms.sel).cloned() {
                    let edit = |app: &mut App, action: &str| {
                        app.call(&Msg::NetEdit {
                            id: ms.id,
                            action: action.into(),
                            value: m.name.clone(),
                        })
                    };
                    let r = if m.enabled {
                        edit(self, "mcp-deny")
                    } else {
                        // Tira a desabilitação da aba; sem ela, entra na
                        // allowlist da aba.
                        edit(self, "mcp-forget").or_else(|_| edit(self, "mcp-allow"))
                    };
                    ms.list = self.mcp_list(ms.id);
                    let now = ms
                        .list
                        .iter()
                        .find(|x| x.name == m.name)
                        .is_some_and(|x| x.enabled);
                    ms.note = Some(match r {
                        Err(e) => format!("{e:#}"),
                        Ok(_) if now == m.enabled => {
                            trf!(
                                "{} continua como a configuração manda",
                                "{} stays as the configuration says",
                                m.name
                            )
                        }
                        Ok(_) => trf!(
                            "{} {}; vale no próximo lançamento (Ctrl+b r)",
                            "{} {}; takes effect on the next launch (Ctrl+b r)",
                            m.name,
                            if now {
                                tr!("habilitado", "enabled")
                            } else {
                                tr!("desabilitado", "disabled")
                            }
                        ),
                    });
                }
            }
            _ => {}
        }
        Some(ms)
    }

    fn modal_key(&mut self, k: Key) {
        if matches!(self.modal, Some(Modal::Settings(_))) {
            if let Some(Modal::Settings(s)) = self.modal.take() {
                self.modal = self.settings_key(s, k).map(Modal::Settings);
            }
            return;
        }
        if let Some(Modal::Menu(sel)) = self.modal.as_mut() {
            let n = menu_items().len();
            match k {
                Key::Esc | Key::Char('q') => self.modal = None,
                Key::Up => *sel = (*sel + n - 1) % n,
                Key::Down | Key::Tab => *sel = (*sel + 1) % n,
                Key::Enter => {
                    let a = menu_items()[*sel].0;
                    self.modal = None;
                    self.action(a);
                }
                Key::Char(c) => {
                    // A tecla da ação, sem o prefixo, também vale no menu.
                    if let Some(a) = self.cfg.action_for(c).map(str::to_string)
                        && a != "menu"
                    {
                        self.modal = None;
                        self.action(&a);
                    }
                }
                _ => {}
            }
            return;
        }
        if matches!(self.modal, Some(Modal::Mcp(_))) {
            if let Some(Modal::Mcp(ms)) = self.modal.take() {
                self.modal = self.mcp_key(ms, k).map(Modal::Mcp);
            }
            return;
        }
        if matches!(self.modal, Some(Modal::Folders(_))) {
            if let Some(Modal::Folders(fs)) = self.modal.take() {
                self.modal = self.folders_key(fs, k).map(Modal::Folders);
            }
            return;
        }
        if matches!(self.modal, Some(Modal::Files(_))) {
            if let Some(Modal::Files(fs)) = self.modal.take() {
                self.modal = self.files_key(fs, k).map(Modal::Files);
            }
            return;
        }
        if matches!(self.modal, Some(Modal::Net(_))) {
            if let Some(Modal::Net(ns)) = self.modal.take() {
                self.modal = self.net_key(ns, k).map(Modal::Net);
            }
            return;
        }
        let Some(m) = self.modal.as_mut() else {
            return;
        };
        match m {
            Modal::New(n) => {
                if n.question.is_some() {
                    match k {
                        Key::Char('s' | 'S' | 'y' | 'Y') | Key::Enter => self.create(Some(true)),
                        Key::Char('n' | 'N') => self.create(Some(false)),
                        Key::Esc => n.question = None,
                        _ => {}
                    }
                    return;
                }
                match k {
                    Key::Esc => self.modal = None,
                    Key::Tab | Key::Down => n.field = (n.field + 1) % 3,
                    Key::Up => n.field = (n.field + 2) % 3,
                    Key::Left if n.field == 0 => {
                        n.harness = (n.harness + NewTab::KINDS - 1) % NewTab::KINDS
                    }
                    Key::Right if n.field == 0 => n.harness = (n.harness + 1) % NewTab::KINDS,
                    Key::Enter => self.create(None),
                    Key::Backspace => {
                        match n.field {
                            1 => {
                                n.workdir.pop();
                            }
                            2 => {
                                n.profile.pop();
                            }
                            _ => {}
                        };
                    }
                    Key::Char(c) => match n.field {
                        1 => n.workdir.push(c),
                        2 => n.profile.push(c),
                        _ => {}
                    },
                    _ => {}
                }
            }
            Modal::Groups(gm) => {
                if let Some((what, text)) = gm.edit.as_mut() {
                    match k {
                        Key::Esc => gm.edit = None,
                        Key::Backspace => {
                            text.pop();
                        }
                        Key::Char(c) => text.push(c),
                        Key::Enter => {
                            let name = text.trim().to_string();
                            let what = what.clone();
                            if let Err(e) = sandmux_proto::msg::check_group(&name) {
                                self.say(e);
                                return;
                            }
                            self.modal = None;
                            match what {
                                GroupEdit::New => self.switch_group(name),
                                GroupEdit::Rename(old) => {
                                    let has_tabs =
                                        self.groups.iter().any(|(g, n)| *g == old && *n > 0);
                                    let r = if has_tabs {
                                        self.call(&Msg::RenameGroup {
                                            from: old.clone(),
                                            to: name.clone(),
                                        })
                                        .map(|_| ())
                                    } else {
                                        Ok(())
                                    };
                                    match r {
                                        Ok(()) => {
                                            if self.group == old {
                                                self.group = name.clone();
                                            }
                                            self.sync();
                                            self.say(trf!(
                                                "grupo {old} renomeado para {name}",
                                                "group {old} renamed to {name}"
                                            ));
                                        }
                                        Err(e) => self.say(trf!(
                                            "renomear o grupo: {e:#}",
                                            "rename the group: {e:#}"
                                        )),
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                let chosen = self.groups.get(gm.sel).map(|(g, _)| g.clone());
                match k {
                    Key::Esc => self.modal = None,
                    Key::Up => gm.sel = gm.sel.saturating_sub(1),
                    Key::Down => gm.sel = (gm.sel + 1).min(self.groups.len().saturating_sub(1)),
                    Key::Char('n') => gm.edit = Some((GroupEdit::New, String::new())),
                    Key::Char('r') => {
                        if let Some(g) = chosen {
                            gm.edit = Some((GroupEdit::Rename(g.clone()), g));
                        }
                    }
                    Key::Char('m') => {
                        let (Some(g), Some(t)) = (chosen, self.tabs.get(self.active)) else {
                            return;
                        };
                        let id = t.id;
                        // Uma aba com painéis vai inteira.
                        let others: Vec<u32> = self
                            .layout_of(id)
                            .map(|l| l.leaves())
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|i| *i != id)
                            .collect();
                        for o in others {
                            let _ = self.call(&Msg::SetGroup {
                                id: o,
                                group: g.clone(),
                            });
                        }
                        self.modal = None;
                        match self.call(&Msg::SetGroup {
                            id,
                            group: g.clone(),
                        }) {
                            Ok(_) => {
                                self.sync();
                                self.say(trf!(
                                    "aba {id} levada ao grupo {g}",
                                    "tab {id} moved to group {g}"
                                ));
                            }
                            Err(e) => self.say(trf!("mover: {e:#}", "move: {e:#}")),
                        }
                    }
                    Key::Enter => {
                        self.modal = None;
                        if let Some(g) = chosen {
                            self.switch_group(g);
                        }
                    }
                    _ => {}
                }
            }
            Modal::List(sel) => match k {
                Key::Esc => self.modal = None,
                Key::Up => *sel = sel.saturating_sub(1),
                Key::Down => *sel = (*sel + 1).min(self.tabs.len().saturating_sub(1)),
                Key::Enter => {
                    let s = *sel;
                    self.modal = None;
                    self.set_active(s);
                }
                _ => {}
            },
            Modal::Rename(text) => match k {
                Key::Esc => self.modal = None,
                Key::Backspace => {
                    text.pop();
                }
                Key::Char(c) => text.push(c),
                Key::Enter => {
                    let name = text.trim().to_string();
                    let id = self.tabs[self.active].id;
                    self.modal = None;
                    match self.call(&Msg::Rename {
                        id,
                        name: name.clone(),
                    }) {
                        Ok(_) => {
                            self.tabs[self.active].name = name;
                        }
                        Err(e) => self.say(trf!("renomear: {e:#}", "rename: {e:#}")),
                    }
                }
                _ => {}
            },
            Modal::Chat(c) => match k {
                Key::Esc => self.modal = None,
                Key::Tab => c.field = (c.field + 1) % 3,
                Key::Char(_) | Key::Backspace if c.field == 2 => {}
                Key::Right if c.field == 1 => self.open_picker(picker::PickFor::ChatModal),
                Key::Up | Key::Down if c.field == 1 => {
                    let choices = self.assistant.model_choices();
                    if !choices.is_empty() {
                        let n = choices.len();
                        let down = matches!(k, Key::Down);
                        let cur = c
                            .choice
                            .or_else(|| choices.iter().position(|m| *m == c.model));
                        let i = match cur {
                            None => 0,
                            Some(i) if down => (i + 1) % n,
                            Some(i) => (i + n - 1) % n,
                        };
                        c.choice = Some(i);
                        c.model = choices[i].clone();
                        c.error = None;
                    }
                }
                Key::Backspace => {
                    c.error = None;
                    if c.field == 0 {
                        c.text.pop();
                    } else {
                        c.model.pop();
                        c.choice = None;
                    }
                }
                Key::Char(ch) => {
                    c.error = None;
                    if c.field == 0 {
                        c.text.push(ch);
                    } else {
                        c.model.push(ch);
                        c.choice = None;
                    }
                }
                Key::Enter => self.chat_send(),
                _ => {}
            },
            Modal::Command(c) => {
                let choices = self.assistant.model_choices();
                match (c.stage, k) {
                    (_, Key::Esc) => self.modal = None,
                    (CmdStage::Waiting, _) => {}
                    (CmdStage::Ask, Key::Tab) => c.field = (c.field + 1) % 4,
                    (CmdStage::Ask, Key::Char(' ')) if c.field == 2 => c.screen = !c.screen,
                    (CmdStage::Ask, Key::Char(_) | Key::Backspace) if c.field >= 2 => {}
                    (CmdStage::Ask, Key::Right) if c.field == 1 => {
                        self.open_picker(picker::PickFor::CommandModal)
                    }
                    (CmdStage::Command, Key::Tab) => {
                        c.stage = CmdStage::Ask;
                        c.field = 0;
                        c.error = None;
                    }
                    (CmdStage::Ask, Key::Up | Key::Down) if c.field == 1 && !choices.is_empty() => {
                        let n = choices.len();
                        let down = matches!(k, Key::Down);
                        let cur = c
                            .choice
                            .or_else(|| choices.iter().position(|m| *m == c.model));
                        let i = match cur {
                            None => 0,
                            Some(i) if down => (i + 1) % n,
                            Some(i) => (i + n - 1) % n,
                        };
                        c.choice = Some(i);
                        c.model = choices[i].clone();
                        c.error = None;
                    }
                    (CmdStage::Ask, Key::Backspace) => {
                        c.error = None;
                        if c.field == 0 {
                            c.question.pop();
                        } else {
                            c.model.pop();
                            c.choice = None;
                        }
                    }
                    (CmdStage::Ask, Key::Char(ch)) => {
                        c.error = None;
                        if c.field == 0 {
                            c.question.push(ch);
                        } else {
                            c.model.push(ch);
                            c.choice = None;
                        }
                    }
                    (CmdStage::Ask, Key::Enter) => self.ask_command(),
                    (CmdStage::Command, Key::Backspace) => {
                        c.error = None;
                        c.command.pop();
                    }
                    (CmdStage::Command, Key::Char(ch)) => {
                        c.error = None;
                        c.command.push(ch);
                    }
                    (CmdStage::Command, Key::Enter) => self.insert_command(),
                    _ => {}
                }
            }
            Modal::Ended(id, _) => {
                let id = *id;
                let still = self.tabs.get(self.active).is_some_and(|t| {
                    t.id == id && matches!(t.state, SessionState::Encerrada { .. })
                });
                match k {
                    Key::Esc => self.modal = None,
                    Key::Enter | Key::Char('r' | 'R') => {
                        self.modal = None;
                        if still {
                            self.restart();
                        }
                    }
                    Key::Char('x' | 'X') => {
                        self.modal = None;
                        if still {
                            self.close_tab(self.active);
                        }
                    }
                    _ => {}
                }
            }
            Modal::Close => match k {
                Key::Char('s' | 'S' | 'y' | 'Y') => {
                    self.modal = None;
                    self.close_tab(self.active);
                }
                Key::Char('n' | 'N') | Key::Esc => self.modal = None,
                _ => {}
            },
            Modal::Help => {
                if matches!(k, Key::Esc | Key::Enter | Key::Char('q')) {
                    self.modal = None;
                }
            }
            Modal::Net(_)
            | Modal::Files(_)
            | Modal::Folders(_)
            | Modal::Mcp(_)
            | Modal::Menu(_)
            | Modal::Settings(_) => {}
            Modal::Unlock(text, error) => match k {
                Key::Esc => {
                    zeroize::Zeroize::zeroize(text);
                    self.modal = None;
                    self.after_unlock = None;
                    self.say(tr!(
                        "cofre trancado; `sandmux vault unlock` desbloqueia depois",
                        "vault locked; `sandmux vault unlock` unlocks it later"
                    ));
                }
                Key::Backspace => {
                    text.pop();
                }
                Key::Char(c) => {
                    *error = None;
                    text.push(c);
                }
                Key::Enter => {
                    let password = std::mem::take(text);
                    let r = self.call(&Msg::VaultUnlock {
                        password: password.clone(),
                    });
                    let mut password = password;
                    zeroize::Zeroize::zeroize(&mut password);
                    match r {
                        Ok(_) => {
                            self.modal = None;
                            self.say(tr!("cofre desbloqueado", "vault unlocked"));
                            self.sync();
                            match self.after_unlock.take() {
                                Some(Pending::Chat(model, text)) => self.submit_chat(model, text),
                                Some(Pending::Command(c)) => {
                                    self.modal = Some(Modal::Command(c));
                                    self.ask_command();
                                }
                                None => {}
                            }
                        }
                        Err(e) => {
                            if let Some(Modal::Unlock(_, err)) = self.modal.as_mut() {
                                *err = Some(format!("{e:#}"));
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }

    fn mouse(&mut self, m: Mouse) {
        // A pergunta da aba encerrada não prende o mouse: um clique a fecha
        // e segue (no ✕, fecha a aba).
        if matches!(self.modal, Some(Modal::Ended(..))) && !m.release && !m.is_motion() {
            self.modal = None;
        }
        if matches!(self.modal, Some(Modal::Settings(_))) {
            if !m.release
                && !m.is_motion()
                && !m.is_wheel()
                && m.button() == 0
                && let Some(Modal::Settings(s)) = self.modal.take()
            {
                let s = self.settings_click(s, m.x.saturating_sub(1), m.y.saturating_sub(1));
                self.modal = Some(Modal::Settings(s));
            }
            return;
        }
        if self.modal.is_some() {
            if let Some((x, y, w)) = self.modal_button.get()
                && !m.release
                && !m.is_motion()
                && !m.is_wheel()
                && m.button() == 0
            {
                let (mx, my) = (m.x.saturating_sub(1), m.y.saturating_sub(1));
                if my == y && mx >= x && mx < x + w {
                    match &self.modal {
                        Some(Modal::Command(c)) if c.stage == CmdStage::Ask => self.ask_command(),
                        Some(Modal::Command(c)) if c.stage == CmdStage::Command => {
                            self.insert_command()
                        }
                        Some(Modal::Chat(_)) => self.chat_send(),
                        _ => {}
                    }
                }
            }
            return;
        }
        if m.y == self.bar_row {
            if !m.release && !m.is_motion() && !m.is_wheel() && m.button() == 0 {
                let x = m.x.saturating_sub(1);
                let inside = |(a, b, _): &&(u16, u16, usize)| x >= *a && x < *b;
                if self.menu_button.is_some_and(|(a, b)| x >= a && x < b) {
                    self.modal = Some(Modal::Menu(0));
                } else if let Some(&(_, _, i)) = self.close_ranges.iter().find(inside) {
                    self.close_tab(i);
                } else if let Some(&(_, _, i)) = self.bar_ranges.iter().find(inside) {
                    self.set_active(i);
                }
            }
            return;
        }
        if m.x > self.size.cols {
            // Clique no painel lateral: a faixa de uma seção recolhe ou muda
            // a ordem.
            if !m.release && !m.is_motion() && !m.is_wheel() && m.button() == 0 {
                let (x, y) = (m.x.saturating_sub(1), m.y.saturating_sub(1));
                let hit = self
                    .panel_hits
                    .borrow()
                    .iter()
                    .find(|(row, a, b, _)| *row == y && x >= *a && x < *b)
                    .map(|h| h.3.clone());
                if let Some(h) = hit {
                    self.panel_click(h);
                }
            }
            return;
        }
        let dy = self.area_dy;
        // Painéis: um clique em outro painel só põe o foco nele; dentro do
        // painel em foco, o evento é deslocado para a origem dele.
        let mut m = m;
        let (panes, _) = self.visible_panes();
        if panes.len() > 1 {
            let (x, y) = (m.x.saturating_sub(1), m.y.saturating_sub(1 + dy));
            match panes.iter().find(|(_, r)| r.contains(x, y)) {
                Some(&(i, _)) if i != self.active => {
                    if !m.release && !m.is_motion() && !m.is_wheel() {
                        self.set_active(i);
                    }
                    return;
                }
                Some(&(_, r)) => {
                    m.x -= r.x;
                    m.y -= r.y;
                }
                None => return,
            }
        }
        let Some(t) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Some(bytes) = modes::encode_for(t.parser.screen(), &m, dy) {
            if let Some((_, s)) = t.conn.as_mut() {
                let _ = write_data(s, &bytes);
            }
            return;
        }
        if t.parser.screen().mouse_protocol_mode() != vt100::MouseProtocolMode::None {
            return;
        }
        let (row, col) = (m.y.saturating_sub(1 + dy), m.x.saturating_sub(1));
        if m.is_wheel() {
            let max = HISTORY;
            t.scroll = if m.wheel_up() {
                (t.scroll + 3).min(max)
            } else {
                t.scroll.saturating_sub(3)
            };
            t.parser.screen_mut().set_scrollback(t.scroll);
            t.scroll = t.parser.screen().scrollback();
            return;
        }
        if m.button() != 0 {
            return;
        }
        if !m.release && !m.is_motion() {
            self.sel = Some(Sel {
                start: (row, col),
                end: (row, col),
            });
        } else if m.is_motion() {
            if let Some(s) = self.sel.as_mut() {
                s.end = (row, col);
            }
        } else if let Some(s) = self.sel.take()
            && s.start != s.end
        {
            let ((r0, c0), (r1, c1)) = s.ordered();
            let text = t
                .parser
                .screen()
                .contents_between(r0, c0, r1, c1.saturating_add(1));
            let text: String = text
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            self.copy_text(&text);
            self.say(trf!(
                "copiado: {} caracteres",
                "copied: {} characters",
                text.chars().count()
            ));
        }
    }

    fn input(&mut self, inputs: Vec<Input>) {
        for i in inputs {
            match i {
                Input::Send(b) => self.send_active(&b),
                Input::Action(a) => self.action(&a),
                Input::Goto(d) => {
                    let i = if d == 0 { 9 } else { d - 1 };
                    self.goto_entry(i);
                }
                Input::Mouse(m) => self.mouse(m),
                Input::Focus(f) => self.on_focus(f),
                Input::Key(k) if self.picker.is_some() => self.picker_key(k),
                Input::Key(k) => self.modal_key(k),
            }
        }
    }
}

/// Abre a TUI. Devolve o código de saída do comando.
pub fn run(group: Option<String>) -> Result<i32> {
    use std::io::IsTerminal;
    if let Some(g) = &group {
        sandmux_proto::msg::check_group(g).map_err(|e| anyhow::anyhow!(e))?;
    }
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(tr!(
            "a TUI precisa de um terminal interativo na entrada e na saída",
            "the TUI needs an interactive terminal on input and output"
        ));
    }
    let host = Host::detect()?;
    let cfg = Config::load(&sandmux_policy::config::path(&host))?;
    let control = client::connect_or_start()?;

    let mut sigs = SigSet::empty();
    for s in [
        Signal::SIGWINCH,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGINT,
    ] {
        sigs.add(s);
    }
    sigs.thread_block()?;
    let raw = client::RawMode::enter()?;
    out(b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?1004h");
    // O ratatui consulta o cursor ao começar: antes da thread que lê a entrada.
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut term = ratatui::Terminal::new(backend)?;
    term.clear()?;

    let (tx, rx): (Sender<Ev>, Receiver<Ev>) = mpsc::channel();
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            while let Ok(sig) = sigs.wait() {
                let ev = if sig == Signal::SIGWINCH {
                    Ev::Winch
                } else {
                    Ev::Quit
                };
                if tx.send(ev).is_err() {
                    return;
                }
            }
        });
    }
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut stdin = std::io::stdin().lock();
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(Ev::StdinClosed);
                        return;
                    }
                    Ok(n) => {
                        if tx.send(Ev::Stdin(buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if tx.send(Ev::Tick).is_err() {
                    return;
                }
            }
        });
    }

    let keys: Vec<(char, String)> = cfg.tui.keys.iter().map(|(a, k)| (*k, a.clone())).collect();
    let mut parser = Parser::new(cfg.tui.prefix, keys);
    let panel_on = cfg.tui.panel;
    let assistant = cfg.assistant.clone();
    let mut app = App {
        tabs: Vec::new(),
        group: sandmux_proto::msg::main_group(),
        groups: Vec::new(),
        layouts: Vec::new(),
        zoom: false,
        pending_split: None,
        active: 0,
        modal: None,
        sel: None,
        bar_ranges: Vec::new(),
        close_ranges: Vec::new(),
        cfg: cfg.tui,
        status: None,
        size: area_size(panel_on),
        tx,
        next_uid: 0,
        control: Some(control),
        cwd: host.cwd.clone(),
        host: host.clone(),
        applied: (Vec::new(), Vec::new()),
        focused: true,
        focus_sent: None,
        quit: false,
        panel: panel_on,
        panel_data: None,
        panel_net: Vec::new(),
        panel_mcp: Vec::new(),
        net_since: sandmux_net::log::now_local().1,
        assistant,
        after_unlock: None,
        ask_token: 0,
        modal_button: std::cell::Cell::new(None),
        menu_button: None,
        settings_hits: std::cell::RefCell::new(Vec::new()),
        panel_hits: std::cell::RefCell::new(Vec::new()),
        picker: None,
        models: picker::Models::None,
        clipboard: paste::System::default(),
        vault: None,
        bar_row: 1,
        area_dy: 1,
    };
    // Sem `--group`: o principal, ou, vazio, o primeiro grupo com abas.
    app.group = match group {
        Some(g) => g,
        None => match app.call(&Msg::List) {
            Ok(Msg::Sessions { sessions, .. })
                if !sessions
                    .iter()
                    .any(|s| s.group == sandmux_proto::msg::MAIN_GROUP) =>
            {
                sessions
                    .iter()
                    .map(|s| s.group.clone())
                    .min()
                    .unwrap_or_else(sandmux_proto::msg::main_group)
            }
            _ => sandmux_proto::msg::main_group(),
        },
    };
    app.sync();
    if app.tabs.is_empty() {
        app.open_new();
    }
    // Cofre trancado: a senha antes de tudo.
    if let Ok(Msg::VaultInfo {
        exists: true,
        unlocked: false,
        ..
    }) = app.call(&Msg::VaultStatus)
    {
        app.modal = Some(Modal::Unlock(String::new(), None));
    }

    let mut last_draw = Instant::now() - Duration::from_secs(1);
    let mut dirty = true;
    while !app.quit {
        let timeout = if dirty {
            Duration::from_millis(16).saturating_sub(last_draw.elapsed())
        } else {
            Duration::from_millis(500)
        };
        match rx.recv_timeout(timeout) {
            Ok(Ev::Stdin(b)) => {
                parser.modal = app.modal.is_some();
                let inputs = parser.feed(&b);
                app.input(inputs);
                dirty = true;
            }
            Ok(Ev::StdinClosed) | Ok(Ev::Quit) => app.quit = true,
            Ok(Ev::Frame(uid, f)) => {
                app.on_frame(uid, f);
                dirty = true;
            }
            Ok(Ev::Closed(uid)) => {
                for t in &mut app.tabs {
                    if t.conn.as_ref().is_some_and(|(u, _)| *u == uid) {
                        t.conn = None;
                    }
                }
                dirty = true;
            }
            Ok(Ev::Answer(token, r)) => {
                app.on_answer(token, r);
                dirty = true;
            }
            Ok(Ev::Models(r)) => {
                app.models_loaded(r);
                dirty = true;
            }
            Ok(Ev::Tick) => {
                // Um daemon reiniciado esquece o foco: manda de novo.
                app.focus_sent = None;
                app.sync();
                app.refresh_vault();
                app.refresh_panel();
                app.refresh_marks();
                dirty = true;
            }
            Ok(Ev::Winch) => {
                app.resize_all();
                let _ = term.autoresize();
                dirty = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        app.prompt_ended();
        app.report_focus();
        if app.pending_split.is_some()
            && !matches!(
                app.modal,
                Some(Modal::New(_) | Modal::Chat(_) | Modal::Unlock(..))
            )
        {
            app.pending_split = None;
        }
        parser.modal = app.modal.is_some();
        if app.panel && app.panel_data.is_none() && !app.tabs.is_empty() {
            app.refresh_panel();
        }
        if dirty && last_draw.elapsed() >= Duration::from_millis(16) {
            app.apply_modes();
            if let Some(t) = app.tabs.get_mut(app.active) {
                t.parser.screen_mut().set_scrollback(t.scroll);
            }
            term.draw(|f| render::draw(f, &mut app))?;
            last_draw = Instant::now();
            dirty = false;
        }
    }

    // Saída: desanexa tudo e devolve o terminal.
    for t in &mut app.tabs {
        if let Some((_, s)) = t.conn.take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
    out(&app.applied.1);
    out(b"\x1b[?1004l\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[0m\x1b[?25h\x1b[?1049l");
    drop(raw);
    eprintln!(
        "{}",
        tr!(
            "sandmux: TUI fechada; as sessões continuam (sandmux ls)",
            "sandmux: TUI closed; sessions keep running (sandmux ls)"
        )
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_padrao() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64("olá".as_bytes()), "b2zDoQ==");
    }

    #[test]
    fn selecao_ordenada() {
        let s = Sel {
            start: (3, 5),
            end: (1, 2),
        };
        assert!(s.contains(2, 0) && s.contains(1, 2) && s.contains(3, 5));
        assert!(!s.contains(3, 6) && !s.contains(1, 1));
    }
}
