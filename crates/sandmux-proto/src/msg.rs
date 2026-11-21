//! Mensagens de controle e a saudação.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sandmux_spec::{tr, trf};
use serde::{Deserialize, Serialize};

/// Tamanho de um terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    /// O menor número de colunas e o menor de linhas entre vários tamanhos.
    pub fn min_of(sizes: impl IntoIterator<Item = Size>) -> Option<Size> {
        sizes.into_iter().reduce(|a, b| Size {
            cols: a.cols.min(b.cols),
            rows: a.rows.min(b.rows),
        })
    }
}

/// Opções de uma sessão, as mesmas de `sandmux run`. Guardadas na lista de
/// sessões: nunca contêm valores de variáveis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub harness: String,
    /// Pasta de trabalho, absoluta.
    pub workdir: PathBuf,
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    /// Nomes das variáveis de `--pass-env` dadas no `run`.
    pub pass_env: Vec<String>,
    /// Perfil nomeado (`--profile`).
    #[serde(default)]
    pub profile: Option<String>,
    /// Opções de liga e desliga dadas no `run`: `None` quando não foram
    /// dadas, e a configuração decide.
    pub repo: Option<bool>,
    pub kvm: Option<bool>,
    pub gpu: Option<bool>,
    pub allow_clipboard: Option<bool>,
    /// Janela dada no `run` (`--window`, `--no-window`, `--window-direct`;
    /// protocolo 34).
    #[serde(default)]
    pub window: Option<sandmux_spec::window::WindowMode>,
    /// Portal do desktop dado no `run` (`--portal`, `--no-portal`;
    /// protocolo 37).
    #[serde(default)]
    pub portal: Option<bool>,
    /// Tetos dados no `run` (`--memory`, `--processes`), como texto.
    #[serde(default)]
    pub memory: Option<String>,
    #[serde(default)]
    pub processes: Option<String>,
    /// Modo de rede dado no `run` (`--network`).
    #[serde(default)]
    pub network: Option<String>,
    /// Padrões de destino negados e liberados na aba (`--deny`, `--allow`,
    /// `sandmux net deny|allow`).
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub allow: Vec<String>,
    /// Padrões de destino cujo TLS é decifrado (`--decrypt`).
    #[serde(default)]
    pub decrypt: Vec<String>,
    /// Chaves SSH do cofre para o agente da sessão (`--ssh-key`).
    #[serde(default)]
    pub ssh_keys: Vec<String>,
    /// Padrões de arquivo negados e liberados nesta aba (`--file-deny`,
    /// `--file-allow`).
    #[serde(default)]
    pub file_deny: Vec<String>,
    #[serde(default)]
    pub file_allow: Vec<String>,
    /// Portas de entrada da aba (`--port`, protocolo 18).
    #[serde(default)]
    pub ports: Vec<String>,
    /// Portas do host levadas ao `127.0.0.1` do sandbox (`--host-port`,
    /// protocolo 22).
    #[serde(default)]
    pub host_ports: Vec<u16>,
    /// Conexões de banco do cofre para o MCP da aba (`--db`, protocolo 17).
    #[serde(default)]
    pub db: Vec<String>,
    /// Servidores MCP desabilitados e habilitados nesta aba (protocolo 16).
    #[serde(default)]
    pub mcp_deny: Vec<String>,
    #[serde(default)]
    pub mcp_allow: Vec<String>,
    /// Pastas do host em `/mnt/sandmux`, lembradas na aba (protocolo 15).
    #[serde(default)]
    pub folders: Vec<sandmux_spec::LiveFolder>,
    /// Sessão do gateway do hermes (`--gateway`, protocolo 19): reinicia ao
    /// cair e espera o cofre depois de um reboot.
    #[serde(default)]
    pub gateway: bool,
    pub args: Vec<String>,
}

impl LaunchOptions {
    pub fn new(harness: &str) -> Self {
        Self {
            harness: harness.into(),
            workdir: PathBuf::from("/"),
            ro: vec![],
            rw: vec![],
            pass_env: vec![],
            profile: None,
            repo: None,
            kvm: None,
            gpu: None,
            allow_clipboard: None,
            window: None,
            portal: None,
            memory: None,
            processes: None,
            network: None,
            deny: vec![],
            allow: vec![],
            decrypt: vec![],
            ssh_keys: vec![],
            file_deny: vec![],
            file_allow: vec![],
            folders: vec![],
            mcp_deny: vec![],
            mcp_allow: vec![],
            db: vec![],
            ports: vec![],
            host_ports: vec![],
            gateway: false,
            args: vec![],
        }
    }
}

/// Estado de uma sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "estado", rename_all = "snake_case")]
pub enum SessionState {
    Viva,
    Encerrada {
        codigo: i32,
    },
    Parada {
        motivo: String,
    },
    /// Restaurada depois de um reboot, esperando o cofre ser desbloqueado
    /// para subir com os valores de `pass-env` guardados nele.
    AguardandoCofre,
    /// Gateway do hermes que terminou: volta depois da espera (protocolo 19).
    Reiniciando {
        segundos: u64,
    },
    /// O daemon subiu no boot, sem ninguém logado: a sessão volta no
    /// primeiro login (protocolo 31).
    AguardandoLogin,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Viva => f.write_str(tr!("viva", "alive")),
            SessionState::Encerrada { codigo } => {
                f.write_str(&trf!("encerrada ({codigo})", "ended ({codigo})"))
            }
            SessionState::Parada { motivo } => {
                f.write_str(&trf!("parada: {motivo}", "stopped: {motivo}"))
            }
            SessionState::AguardandoCofre => {
                f.write_str(tr!("aguardando cofre", "waiting for vault"))
            }
            SessionState::Reiniciando { segundos } => f.write_str(&trf!(
                "reiniciando em {segundos} s",
                "restarting in {segundos} s"
            )),
            SessionState::AguardandoLogin => {
                f.write_str(tr!("aguardando o login", "waiting for login"))
            }
        }
    }
}

/// Uma linha de `sandmux ls`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u32,
    pub name: String,
    pub harness: String,
    pub workdir: PathBuf,
    pub state: SessionState,
    pub clients: usize,
    /// Nomes das variáveis de `pass-env` do último lançamento (nunca os
    /// valores).
    pub pass_env: Vec<String>,
    /// Opções dadas no `run` e o perfil nomeado, para o reinício resolver a
    /// política de novo.
    pub opts: LaunchOptions,
    /// Consumo atual e tetos, quando a sessão está viva e o cgroup é lido.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Processos mortos por falta de memória no último lançamento.
    #[serde(default)]
    pub oom_kills: u64,
    /// Começo do lançamento atual, em segundos Unix (protocolo 23).
    #[serde(default)]
    pub started_at: Option<u64>,
    /// Chaves reais achadas pelo vigia do daemon no HOME sintético do
    /// harness (protocolo 27).
    #[serde(default)]
    pub keys: KeyNote,
    /// Grupo da sessão (protocolo 30).
    #[serde(default = "main_group")]
    pub group: String,
    /// Janela do último lançamento, para a marca na barra (protocolo 34).
    #[serde(default)]
    pub window: sandmux_spec::window::WindowMode,
}

/// O grupo das sessões criadas sem grupo, sempre presente na lista.
pub const MAIN_GROUP: &str = "principal";

pub fn main_group() -> String {
    MAIN_GROUP.to_string()
}

/// Confere um nome de grupo: de 1 a 32 caracteres, entre letras, dígitos,
/// `-`, `_` e `.`.
pub fn check_group(name: &str) -> Result<(), String> {
    let n = name.chars().count();
    let ok = (1..=32).contains(&n)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(trf!(
            "nome de grupo inválido: {name:?}; use de 1 a 32 caracteres, entre letras, dígitos, `-`, `_` e `.`",
            "invalid group name: {name:?}; use 1 to 32 characters among letters, digits, `-`, `_` and `.`"
        ))
    }
}

/// Chaves reais do HOME sintético de uma sessão, contadas pelo vigia.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyNote {
    /// Movidas ao cofre desde o lançamento: o harness ainda tem a real na
    /// memória até a aba ser reiniciada.
    pub moved: u32,
    /// Esperando o cofre (trancado ou ausente).
    pub waiting_vault: u32,
    /// Esperando um domínio (`sandmux doctor --fix`).
    pub no_domain: u32,
}

impl KeyNote {
    /// Há algo a avisar.
    pub fn any(&self) -> bool {
        self.moved + self.waiting_vault + self.no_domain > 0
    }
}

/// O aviso, para a TUI e o `sandmux ls` (vazio quando não há chaves).
impl std::fmt::Display for KeyNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.moved > 0 {
            parts.push(trf!(
                "{} chave(s) movida(s) ao cofre: reinicie a aba (Ctrl+b r)",
                "{} key(s) moved to the vault: restart the tab (Ctrl+b r)",
                self.moved
            ));
        }
        if self.waiting_vault > 0 {
            parts.push(trf!(
                "{} chave(s) esperando o cofre",
                "{} key(s) waiting for the vault",
                self.waiting_vault
            ));
        }
        if self.no_domain > 0 {
            parts.push(trf!(
                "{} chave(s) sem domínio: sandmux doctor --fix",
                "{} key(s) without a domain: sandmux doctor --fix",
                self.no_domain
            ));
        }
        f.write_str(&parts.join(" · "))
    }
}

/// Consumo atual de uma sessão, com os tetos (`None` é sem teto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub memory: u64,
    pub memory_max: Option<u64>,
    pub processes: u64,
    pub processes_max: Option<u64>,
}

/// O escopo de um sandbox no auxiliar root, para o daemon registrá-lo de
/// novo: o processo 1 (no host), o filtro de `ioctl` e os dispositivos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabScope {
    pub init_pid: u32,
    pub ioctl: bool,
    pub devs: Vec<(u32, u32)>,
}

/// O que foi aplicado no lançamento de uma sessão: a proteção ativa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PolicySummary {
    /// Nível de proteção, com a ABI do Landlock.
    pub level: String,
    /// Camada de escrita das toolchains e caches.
    pub layer: String,
    pub workdir: PathBuf,
    pub ro: Vec<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub gpu: bool,
    pub kvm: bool,
    pub clipboard: bool,
    pub pass_env: Vec<String>,
    /// Tetos ativos (falso: sem suporte, sem teto).
    pub limits: bool,
    pub memory_max: Option<u64>,
    pub processes_max: Option<u64>,
    pub network: String,
    /// Janela do lançamento (protocolo 34).
    #[serde(default)]
    pub window: sandmux_spec::window::WindowMode,
    /// Portal do desktop do lançamento (protocolo 37).
    #[serde(default)]
    pub portal: bool,
    /// De onde vêm as execuções no registro: `kernel` ou
    /// `supervisor: <motivo>` (protocolo 41).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<String>,
    /// Quem decide e registra as aberturas de arquivo: `kernel` ou
    /// `supervisor: <motivo>` (protocolo 43).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<String>,
}

/// Consumo do Docker privado da pasta de uma sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerStats {
    pub memory: u64,
    pub memory_max: Option<u64>,
    pub processes: u64,
    pub processes_max: Option<u64>,
    pub containers: u64,
    /// Por que o confinamento por eBPF não vale; `None` quando vale
    /// (protocolo 39).
    #[serde(default)]
    pub ebpf_off: Option<String>,
    /// Os arquivos novos são vistos por varredura a cada 10 s, sem vigias
    /// de inotify (protocolo 40).
    #[serde(default)]
    pub files_poll: bool,
}

/// Uma porta publicada no `127.0.0.1` do host: a do sandbox (ou do
/// contêiner), a do host e de onde vem (`sandbox` ou o nome do contêiner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortInfo {
    pub guest: u16,
    pub host: u16,
    pub source: String,
    /// Endereço do host; `None` é o `127.0.0.1` (protocolo 18).
    #[serde(default)]
    pub addr: Option<std::net::Ipv4Addr>,
}

/// Detalhe do consumo de uma sessão viva.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub usage: Usage,
    pub memory_peak: u64,
    pub processes_peak: u64,
    /// Tempo de CPU usado, em microssegundos.
    pub cpu_usec: u64,
}

/// Um processo vivo da árvore de uma aba (protocolo 45). Os pids são os do
/// host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcNode {
    pub pid: u32,
    /// O pai na árvore; 0 quando o processo é uma raiz.
    pub ppid: u32,
    pub name: String,
    pub cmdline: String,
    /// Memória residente, em bytes.
    pub rss: u64,
    /// Tempo de CPU usado, em milissegundos.
    pub cpu_ms: u64,
    /// Há quantos segundos o processo começou.
    pub age_secs: u64,
    pub cgroup: String,
}

/// Um segredo do cofre, sem o valor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultEntry {
    pub name: String,
    pub domains: Vec<String>,
    pub fake: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    /// `ssh` numa chave SSH (protocolo 13); vazio num token.
    #[serde(default)]
    pub kind: String,
    /// Impressão digital `SHA256:...` de uma chave SSH.
    #[serde(default)]
    pub fingerprint: String,
}

/// Uma conexão de banco, com a senha (só entre o cliente e o daemon).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbConn {
    pub name: String,
    /// `postgres` ou `mysql`.
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for DbConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConn")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// Uma conexão de banco, sem a senha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbEntry {
    pub name: String,
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
}

/// Um servidor MCP com o estado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpEntry {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub origin: String,
    pub enabled: bool,
}

/// Um segredo com o valor real, para o gateway trocar no tráfego.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Injection {
    pub name: String,
    pub domains: Vec<String>,
    pub fake: String,
    pub value: String,
}

impl std::fmt::Debug for Injection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Injection")
            .field("name", &self.name)
            .field("domains", &self.domains)
            .finish_non_exhaustive()
    }
}

/// Uma linha do registro de rede, com a sessão dona (`None` para um `run`
/// direto ou um lançamento que a lista de sessões já não conhece).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetLine {
    pub session: Option<u32>,
    pub ts: String,
    pub tag: String,
    pub proto: String,
    pub domain: String,
    pub ip: String,
    pub port: u16,
    pub up: u64,
    pub down: u64,
    pub ms: u64,
    pub blocked: Option<String>,
    pub new: bool,
    /// Pedido decifrado: método, caminho sem a query e status.
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    /// Nome do segredo trocado no pedido.
    #[serde(default)]
    pub secret: Option<String>,
    /// O processo da conexão: pid, executável e a fonte (protocolo 42).
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub proc_via: Option<String>,
}

/// Mensagens de controle, nos dois sentidos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Msg {
    // --- saudação
    Hello {
        protocol: u32,
        version: String,
        data_dir: PathBuf,
        /// Pasta de configuração do cliente (protocolo 2).
        #[serde(default)]
        config_dir: PathBuf,
        /// Idioma do cliente (`pt` ou `en`), em que o daemon responde
        /// (protocolo 24); vazio, o do daemon.
        #[serde(default)]
        lang: String,
    },
    Welcome {
        protocol: u32,
        version: String,
        pid: u32,
        start_mode: String,
    },
    Refused {
        message: String,
    },

    // --- pedidos do cliente
    /// Cria uma sessão. `vars` são a lista segura, o `SHELL` e os valores de
    /// `--pass-env`, só em memória.
    Create {
        /// Em caixa: as opções são a maior parte de uma mensagem, e as filas de
        /// saída do daemon guardam muitas mensagens.
        opts: Box<LaunchOptions>,
        vars: BTreeMap<String, String>,
        size: Size,
        attach: bool,
        /// Grupo da sessão (protocolo 30); `None` é o `principal`.
        #[serde(default)]
        group: Option<String>,
    },
    Restart {
        id: u32,
        vars: BTreeMap<String, String>,
        size: Size,
        attach: bool,
        /// Reinicia também uma sessão viva, encerrando-a antes (protocolo
        /// 35).
        #[serde(default)]
        force: bool,
    },
    Attach {
        id: Option<u32>,
        size: Size,
    },
    Detach {
        id: u32,
    },
    Kill {
        id: u32,
    },
    List,
    Resize {
        size: Size,
    },
    Status,
    Stop,
    /// Detalhe do consumo de uma sessão.
    Stats {
        id: u32,
    },
    /// A árvore de processos de uma sessão (protocolo 45).
    ProcessTree {
        id: u32,
    },
    /// Troca o nome de uma sessão.
    Rename {
        id: u32,
        name: String,
    },
    /// Troca o layout de painéis da aba das folhas dele; um layout de uma
    /// folha só desfaz a divisão (protocolo 32).
    SetLayout {
        pane: crate::pane::Pane,
    },
    /// Leva a sessão para outro grupo (protocolo 30).
    SetGroup {
        id: u32,
        group: String,
    },
    /// Leva todas as sessões de um grupo para o nome novo.
    RenameGroup {
        from: String,
        to: String,
    },
    /// Últimas linhas do registro de rede (protocolo 7): de uma sessão ou de
    /// todas, só as de `since` em diante (`AAAA-MM-DDTHH:MM:SS`), no máximo
    /// `limit`.
    Net {
        id: Option<u32>,
        since: Option<String>,
        limit: u32,
    },

    /// Muda a camada da aba de uma sessão (protocolo 8): `action` é `deny`,
    /// `allow` ou `forget` (com um padrão de destino) ou `mode` (com o modo).
    NetEdit {
        id: u32,
        action: String,
        value: String,
    },

    // --- cofre (protocolo 10)
    VaultStatus,
    /// Cria o cofre, já desbloqueado.
    VaultInit {
        password: String,
    },
    VaultUnlock {
        password: String,
    },
    VaultLock,
    /// Estado do keyring do sistema (protocolo 28).
    KeyringStatus,
    /// A TUI ganhou ou perdeu o foco do terminal, e a aba que mostra
    /// (protocolo 29). O daemon não notifica o desktop sobre a aba que uma
    /// TUI em foco mostra.
    Focus {
        focused: bool,
        active: Option<u32>,
    },
    /// Guarda a senha do cofre no keyring, depois de conferir com o arquivo.
    KeyringOn {
        password: String,
    },
    /// Apaga a senha do cofre do keyring.
    KeyringOff,
    /// A pasta atual do processo em primeiro plano da sessão (protocolo 33).
    Cwd {
        id: u32,
    },
    VaultPasswd {
        old: String,
        new: String,
    },
    /// Acrescenta ou substitui um segredo pelo nome.
    VaultAdd {
        name: String,
        value: String,
        domains: Vec<String>,
        fake: Option<String>,
        /// Ferramentas (protocolo 12); vazio deriva dos domínios conhecidos.
        #[serde(default)]
        tools: Vec<String>,
    },
    /// Guarda uma chave SSH sem passphrase (protocolo 13).
    VaultAddSsh {
        name: String,
        key: String,
        servers: Vec<String>,
    },
    VaultRemove {
        name: String,
    },
    /// Troca os domínios (ou os servidores de uma chave SSH) de um segredo,
    /// sem o valor (protocolo 24).
    VaultSetDomains {
        name: String,
        domains: Vec<String>,
    },
    /// Troca o valor de um segredo de token, mantendo o falso, os domínios e
    /// as ferramentas (protocolo 38).
    VaultSetValue {
        name: String,
        value: String,
    },
    VaultList,
    /// Os modelos dos provedores do assistente (protocolo 25).
    ListModels,
    /// Monta (`add`, com `rw`) ou desmonta (`remove`, pelo nome) uma pasta em
    /// `/mnt/sandmux` na aba da sessão (protocolo 15).
    FolderEdit {
        id: u32,
        action: String,
        value: String,
        #[serde(default)]
        rw: bool,
    },
    FolderList {
        id: u32,
    },
    /// Guarda uma conexão de banco no cofre (protocolo 17).
    DbAdd {
        conn: DbConn,
    },
    DbList,
    /// Os servidores MCP do harness da sessão (protocolo 16).
    McpList {
        id: u32,
    },
    /// Os segredos com domínio, com os valores reais, para o gateway de um
    /// lançamento (protocolo 11).
    VaultInject {
        tag: String,
    },
    /// O processo de uma conexão TCP do lançamento `tag`, pela porta de
    /// origem e pelo destino, do gateway ao daemon (protocolo 42).
    ConnProcess {
        tag: String,
        sport: u16,
        ip: String,
        port: u16,
    },
    /// O lançador da aba `tag` pede o confinamento dos arquivos pelo kernel,
    /// antes de liberar o comando; `mounts` são as montagens (`mnt_id`) das
    /// pastas do registro, e `roots` e `exclude` as pastas na visão do
    /// agente, que filtram as aberturas (protocolo 43). O processo vem das
    /// credenciais do socket.
    ConfineTab {
        tag: String,
        mounts: Vec<u32>,
        #[serde(default)]
        roots: Vec<PathBuf>,
        #[serde(default)]
        exclude: Vec<PathBuf>,
        /// O escopo do sandbox pelo auxiliar root (Landlock abaixo do ABI
        /// 6): com ele, o user namespace vem logo depois do quadro, num byte
        /// com o descritor por `SCM_RIGHTS` (protocolo 44).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<TabScope>,
    },
    /// Tokens reais capturados numa resposta de login (papel e valor), do
    /// gateway de um lançamento ao daemon, que os guarda e devolve os falsos
    /// (protocolo 26). `harness` é o dono do login (`claude`, `agy`), que
    /// dá o nome e os domínios dos segredos (protocolo 36).
    VaultCapture {
        tag: String,
        #[serde(default)]
        harness: String,
        items: Vec<(String, String)>,
    },

    /// O cofre e a chave do host de um provedor, para o chat (protocolo 20).
    ChatCheck {
        host: String,
    },
    /// Uma pergunta respondida numa execução confinada, sem aba (protocolo
    /// 20). `vars` como no `Create`.
    Ask {
        model: String,
        text: String,
        vars: BTreeMap<String, String>,
        /// Texto de sistema (protocolo 21; o modal de comando).
        #[serde(default)]
        system: Option<String>,
    },

    // --- respostas e eventos do daemon
    Ok,
    Error {
        message: String,
    },
    Warning {
        message: String,
    },
    Created {
        id: u32,
        name: String,
    },
    Sessions {
        sessions: Vec<SessionInfo>,
        /// Os layouts de painéis das abas (protocolo 32).
        #[serde(default)]
        layouts: Vec<crate::pane::Pane>,
    },
    Attached {
        id: u32,
        name: String,
    },
    /// O reenvio do histórico e da tela terminou.
    Replayed,
    Exited {
        id: u32,
        code: i32,
    },
    Detached {
        reason: String,
    },
    StatsOf {
        id: u32,
        state: SessionState,
        /// `None` quando a sessão não está viva ou não tem cgroup.
        stats: Option<Stats>,
        oom_kills: u64,
        fork_refusals: u64,
        /// Docker privado da pasta, quando está rodando (em caixa, pelo
        /// tamanho da mensagem).
        #[serde(default)]
        docker: Option<Box<DockerStats>>,
        /// Proteção aplicada no último lançamento (em caixa: é a maior
        /// parte da mensagem).
        #[serde(default)]
        policy: Option<Box<PolicySummary>>,
        /// Portas publicadas da sessão e do Docker da pasta (protocolo 9).
        #[serde(default)]
        ports: Vec<PortInfo>,
    },
    /// Os processos vivos da sessão, pelo kernel. Sem a observação pelo
    /// kernel, nenhum nó e o motivo em `unavailable`.
    ProcessTreeOf {
        id: u32,
        nodes: Vec<ProcNode>,
        unavailable: Option<String>,
    },
    NetLog {
        lines: Vec<NetLine>,
    },
    VaultInfo {
        exists: bool,
        unlocked: bool,
        secrets: usize,
    },
    /// Resposta ao `KeyringStatus`: ligado pelo usuário, e se o keyring
    /// responde (com o erro, quando não).
    /// Resposta ao `Cwd`: `None` quando não dá para ler.
    CwdOf {
        path: Option<PathBuf>,
    },
    KeyringInfo {
        enabled: bool,
        available: bool,
        error: Option<String>,
    },
    /// Resposta ao `ChatCheck`: o cofre existe, está desbloqueado e tem
    /// segredo para o host.
    ChatStatus {
        exists: bool,
        unlocked: bool,
        key: bool,
    },
    /// Resposta ao `Ask`.
    Answer {
        text: String,
    },
    /// Resposta ao `VaultCapture`: os falsos, na ordem dos itens.
    Captured {
        fakes: Vec<String>,
    },
    /// Resposta ao `ListModels`: `provedor/modelo` e os avisos dos
    /// provedores que falharam.
    Models {
        ids: Vec<String>,
        notes: Vec<String>,
    },
    Folders {
        list: Vec<sandmux_spec::LiveFolder>,
    },
    Mcps {
        list: Vec<McpEntry>,
    },
    DbEntries {
        list: Vec<DbEntry>,
    },
    VaultEntries {
        entries: Vec<VaultEntry>,
    },
    Injections {
        secrets: Vec<Injection>,
    },
    /// Resposta ao `ConnProcess`: o processo, quando o kernel o registrou, e
    /// se a aba está pelo kernel (protocolo 42).
    ConnProcessReply {
        pid: Option<u32>,
        exe: Option<String>,
        kernel: bool,
    },
    /// Resposta ao `ConfineTab`: os arquivos da aba pelo kernel, ou o motivo
    /// de ficar com o supervisor (protocolo 43).
    ConfineTabReply {
        kernel: bool,
        reason: Option<String>,
        /// O daemon vigia o escopo do sandbox e o registra de novo quando o
        /// auxiliar reinicia (protocolo 44).
        #[serde(default)]
        scope: bool,
    },
    StatusInfo {
        pid: u32,
        protocol: u32,
        version: String,
        start_mode: String,
        sessions: usize,
        socket: PathBuf,
    },
}

/// Confere um nome de sessão: de 1 a 40 caracteres, sem caracteres de
/// controle.
pub fn check_session_name(name: &str) -> Result<(), String> {
    let n = name.chars().count();
    if n == 0 || name.trim().is_empty() {
        return Err(tr!(
            "o nome da sessão não pode ser vazio",
            "the session name cannot be empty"
        )
        .into());
    }
    if n > 40 {
        return Err(trf!(
            "o nome da sessão tem {n} caracteres; o máximo é 40",
            "the session name has {n} characters; the maximum is 40"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(tr!(
            "o nome da sessão não pode ter caracteres de controle",
            "the session name cannot contain control characters"
        )
        .into());
    }
    Ok(())
}

/// Resposta do daemon à saudação: a mensagem a enviar e se a conexão segue.
pub fn answer_hello(
    hello: &Msg,
    protocol: u32,
    version: &str,
    pid: u32,
    start_mode: &str,
    data_dir: &std::path::Path,
    config_dir: &std::path::Path,
) -> (Msg, bool) {
    match hello {
        Msg::Hello {
            protocol: p,
            data_dir: d,
            config_dir: c,
            ..
        } => {
            if *p != protocol {
                // O cliente compara e explica; o daemon só encerra a conexão.
                let welcome = Msg::Welcome {
                    protocol,
                    version: version.into(),
                    pid,
                    start_mode: start_mode.into(),
                };
                (welcome, false)
            } else if d != data_dir {
                (
                    Msg::Refused {
                        message: trf!(
                            "o daemon usa o diretório de dados {} e este terminal usa {}; defina o mesmo XDG_DATA_HOME nos dois ou rode `sandmux daemon restart` neste terminal",
                            "the daemon uses the data directory {} and this terminal uses {}; set the same XDG_DATA_HOME in both or run `sandmux daemon restart` in this terminal",
                            data_dir.display(),
                            d.display()
                        ),
                    },
                    false,
                )
            } else if c != config_dir {
                (
                    Msg::Refused {
                        message: trf!(
                            "o daemon usa a pasta de configuração {} e este terminal usa {}; defina o mesmo XDG_CONFIG_HOME nos dois ou rode `sandmux daemon restart` neste terminal",
                            "the daemon uses the configuration folder {} and this terminal uses {}; set the same XDG_CONFIG_HOME in both or run `sandmux daemon restart` in this terminal",
                            config_dir.display(),
                            c.display()
                        ),
                    },
                    false,
                )
            } else {
                (
                    Msg::Welcome {
                        protocol,
                        version: version.into(),
                        pid,
                        start_mode: start_mode.into(),
                    },
                    true,
                )
            }
        }
        _ => (
            Msg::Refused {
                message: tr!(
                    "a conexão precisa começar pela saudação",
                    "the connection must start with the greeting"
                )
                .into(),
            },
            false,
        ),
    }
}

/// Dados do daemon depois de uma saudação aceita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonInfo {
    pub protocol: u32,
    pub version: String,
    pub pid: u32,
    pub start_mode: String,
}

/// Confere, no cliente, a resposta à saudação.
pub fn check_welcome(reply: &Msg, protocol: u32, version: &str) -> Result<DaemonInfo, String> {
    match reply {
        Msg::Welcome {
            protocol: p,
            version: v,
            pid,
            start_mode,
        } => {
            if *p != protocol {
                Err(trf!(
                    "o daemon em execução usa o protocolo {p} (sandmux {v}) e este cliente usa o protocolo {protocol} (sandmux {version}); rode `sandmux daemon restart` para trocar o daemon pela versão instalada (os agentes são relançados e o histórico de tela se perde)",
                    "the running daemon uses protocol {p} (sandmux {v}) and this client uses protocol {protocol} (sandmux {version}); run `sandmux daemon restart` to replace the daemon with the installed version (the agents are relaunched and the screen history is lost)"
                ))
            } else {
                Ok(DaemonInfo {
                    protocol: *p,
                    version: v.clone(),
                    pid: *pid,
                    start_mode: start_mode.clone(),
                })
            }
        }
        Msg::Refused { message } => Err(message.clone()),
        other => Err(trf!(
            "resposta inesperada do daemon: {other:?}",
            "unexpected reply from the daemon: {other:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn hello(p: u32, dir: &str) -> Msg {
        hello_cfg(p, dir, "/c")
    }

    fn hello_cfg(p: u32, dir: &str, cfg: &str) -> Msg {
        Msg::Hello {
            protocol: p,
            version: "0.2.0".into(),
            data_dir: dir.into(),
            config_dir: cfg.into(),
            lang: String::new(),
        }
    }

    #[test]
    fn pasta_de_configuracao_diferente_recusada() {
        let (reply, ok) = answer_hello(
            &hello_cfg(1, "/d", "/tmp/cfg/sandmux"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/d"),
            Path::new("/home/ana/.config/sandmux"),
        );
        assert!(!ok);
        let err = check_welcome(&reply, 1, "0.1.0").unwrap_err();
        assert!(
            err.contains("/tmp/cfg/sandmux") && err.contains("/home/ana/.config/sandmux"),
            "{err}"
        );
    }

    #[test]
    fn consumo_e_stats_ida_e_volta() {
        let u = Usage {
            memory: 1 << 20,
            memory_max: None,
            processes: 3,
            processes_max: Some(8192),
        };
        let m = Msg::StatsOf {
            id: 1,
            state: SessionState::Viva,
            stats: Some(Stats {
                usage: u,
                memory_peak: 2 << 20,
                processes_peak: 4,
                cpu_usec: 10,
            }),
            oom_kills: 1,
            fork_refusals: 2,
            docker: Some(Box::new(DockerStats {
                ebpf_off: None,
                files_poll: false,
                memory: 1,
                memory_max: None,
                processes: 2,
                processes_max: Some(10),
                containers: 3,
            })),
            ports: vec![PortInfo {
                guest: 3000,
                host: 3001,
                source: "sandbox".into(),
                addr: Some(std::net::Ipv4Addr::UNSPECIFIED),
            }],
            policy: Some(Box::new(PolicySummary {
                level: "nível 1".into(),
                kvm: true,
                ro: vec!["/d".into()],
                ..Default::default()
            })),
        };
        let back: Msg = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
        for m in [
            Msg::ProcessTree { id: 2 },
            Msg::ProcessTreeOf {
                id: 2,
                nodes: vec![ProcNode {
                    pid: 10,
                    ppid: 0,
                    name: "bash".into(),
                    cmdline: "bash -l".into(),
                    rss: 4096,
                    cpu_ms: 12,
                    age_secs: 3,
                    cgroup: "/user.slice/x.scope".into(),
                }],
                unavailable: None,
            },
            Msg::ProcessTreeOf {
                id: 2,
                nodes: Vec::new(),
                unavailable: Some("supervisor: sem o auxiliar".into()),
            },
        ] {
            let back: Msg = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
            assert_eq!(back, m);
        }
        // Opções do protocolo 2, sem os tetos, ainda são lidas.
        let o: LaunchOptions = serde_json::from_str(
            r#"{"harness":"shell","workdir":"/w","ro":[],"rw":[],"pass_env":[],"profile":null,
                "repo":null,"kvm":null,"gpu":null,"allow_clipboard":null,"args":[]}"#,
        )
        .unwrap();
        assert_eq!((o.memory, o.processes), (None, None));
    }

    #[test]
    fn opcoes_sem_valor_e_perfil() {
        let mut o = LaunchOptions::new("shell");
        o.kvm = Some(false);
        o.profile = Some("vm".into());
        let back: LaunchOptions =
            serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back, o);
        assert_eq!(back.gpu, None);
    }

    #[test]
    fn saudacao_aceita() {
        let (reply, ok) = answer_hello(
            &hello(1, "/d"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/d"),
            Path::new("/c"),
        );
        assert!(ok);
        let info = check_welcome(&reply, 1, "0.2.0").unwrap();
        assert_eq!(info.pid, 42);
        assert_eq!(info.start_mode, "direct");
    }

    #[test]
    fn protocolo_diferente_recusado_dos_dois_lados() {
        let (reply, ok) = answer_hello(
            &hello(2, "/d"),
            1,
            "0.1.0",
            42,
            "systemd",
            Path::new("/d"),
            Path::new("/c"),
        );
        assert!(!ok, "o daemon encerra a conexão");
        let err = check_welcome(&reply, 2, "0.2.0").unwrap_err();
        assert!(
            err.contains("protocolo 1 (sandmux 0.1.0)")
                && err.contains("protocolo 2 (sandmux 0.2.0)")
                && err.contains("sandmux daemon restart"),
            "{err}"
        );
    }

    #[test]
    fn diretorio_de_dados_diferente_recusado() {
        let (reply, ok) = answer_hello(
            &hello(1, "/b"),
            1,
            "0.1.0",
            42,
            "direct",
            Path::new("/a"),
            Path::new("/c"),
        );
        assert!(!ok);
        let err = check_welcome(&reply, 1, "0.1.0").unwrap_err();
        assert!(err.contains("/a") && err.contains("/b"), "{err}");
    }

    #[test]
    fn primeira_mensagem_precisa_ser_saudacao() {
        let (reply, ok) = answer_hello(
            &Msg::List,
            1,
            "0.1.0",
            1,
            "direct",
            Path::new("/a"),
            Path::new("/c"),
        );
        assert!(!ok);
        assert!(matches!(reply, Msg::Refused { .. }));
    }

    #[test]
    fn menor_tamanho() {
        let s = Size::min_of([
            Size {
                cols: 200,
                rows: 30,
            },
            Size {
                cols: 100,
                rows: 50,
            },
        ]);
        assert_eq!(
            s,
            Some(Size {
                cols: 100,
                rows: 30
            })
        );
        assert_eq!(Size::min_of([]), None);
    }

    #[test]
    fn cofre_e_estado_novo() {
        let s = SessionState::AguardandoCofre;
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"estado":"aguardando_cofre"}"#);
        assert_eq!(serde_json::from_str::<SessionState>(&j).unwrap(), s);
        assert_eq!(s.to_string(), "aguardando cofre");
        let m = Msg::VaultAdd {
            name: "openrouter".into(),
            value: "v".into(),
            domains: vec!["openrouter.ai".into()],
            fake: None,
            tools: vec![],
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains(r#""t":"vault_add""#), "{j}");
        assert_eq!(serde_json::from_str::<Msg>(&j).unwrap(), m);
    }

    #[test]
    fn injecao_sem_valor_no_debug() {
        let i = Injection {
            name: "openrouter".into(),
            domains: vec!["openrouter.ai".into()],
            fake: "sandmux-fake-x".into(),
            value: "sk-real".into(),
        };
        assert!(!format!("{i:?}").contains("sk-real"));
        let m = Msg::Injections { secrets: vec![i] };
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<Msg>(&j).unwrap(), m);
        let q = Msg::VaultInject { tag: "s1-1".into() };
        assert!(serde_json::to_string(&q).unwrap().contains("vault_inject"));
    }
}
