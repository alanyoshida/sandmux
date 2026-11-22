//! `sandmux`: linha de comando do multiplexador: `run`, os comandos de
//! sessão, `import`, `doctor` e `daemon`.

mod chatcmd;
mod configcmd;
mod dbcmd;
mod doctor;
mod filescmd;
mod sessions;
mod setup;
mod tui;
mod vaultcmd;

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use sandmux::launch;
use sandmux_policy::{Host, Layout, home, presets};
use sandmux_proto::LaunchOptions;
use sandmux_spec::{tr, trf};

#[derive(Parser)]
#[command(
    name = "sandmux",
    version,
    about = "Roda agentes de código dentro de um sandbox"
)]
struct Cli {
    /// Sem subcomando, num terminal, abre a TUI.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Abre a interface com abas, uma por sessão (o mesmo que `sandmux` sem
    /// subcomando).
    Tui {
        /// Grupo de abas mostrado (sem ele, o principal).
        #[arg(long = "group", value_name = "GRUPO")]
        group: Option<String>,
    },
    /// Sobe um harness confinado na pasta atual. Num terminal, cria uma sessão
    /// no daemon e anexa a ela (Ctrl+b d desanexa, Ctrl+b Ctrl+b envia Ctrl+b);
    /// sem terminal, roda direto, em primeiro plano.
    Run(Box<RunArgs>),
    /// Lista as sessões.
    Ls {
        /// Só as sessões deste grupo.
        #[arg(long = "group", value_name = "GRUPO")]
        group: Option<String>,
    },
    /// Grupos de abas: listar, mover uma sessão e renomear.
    #[command(subcommand)]
    Group(sessions::GroupCmd),
    /// Anexa este terminal a uma sessão.
    Attach {
        /// Identificador da sessão (sem ele, a única sessão viva).
        id: Option<u32>,
    },
    /// Abre um shell dentro do sandbox de uma sessão viva, com a mesma visão,
    /// rede e restrições do agente.
    Enter {
        /// Identificador da sessão (sem ele, a única sessão viva).
        id: Option<u32>,
    },
    /// Desanexa todos os terminais de uma sessão, sem encerrá-la.
    Detach { id: u32 },
    /// Encerra uma sessão e a tira da lista.
    Kill { id: u32 },
    /// Mostra o consumo de memória e de processos de uma sessão.
    Stats { id: u32 },
    /// Mostra as últimas conexões de rede, de uma sessão ou de todas, e muda
    /// as regras de rede da aba de uma sessão.
    Net(NetArgs),
    /// Mostra os acessos às pastas do agente, de uma sessão ou de todas.
    Files(FilesArgs),
    /// Monta, desmonta e lista as pastas do host em /mnt/sandmux de uma
    /// sessão, sem relançar o agente.
    #[command(subcommand)]
    Folder(FolderCmd),
    /// Conexões de banco no cofre, para o MCP de banco, e o registro das queries.
    #[command(subcommand)]
    Db(dbcmd::DbCmd),
    /// Lista e habilita ou desabilita os servidores MCP do harness.
    #[command(subcommand)]
    Mcp(McpCmd),
    /// Troca o nome de uma sessão (o nome da aba na TUI).
    Rename { id: u32, nome: String },
    /// Relança uma sessão encerrada ou parada, na mesma pasta e com as mesmas opções.
    Restart {
        id: u32,
        /// Reinicia também uma sessão viva, encerrando o harness antes.
        #[arg(long)]
        force: bool,
    },
    /// Controla o daemon de sessões.
    #[command(subcommand)]
    Daemon(DaemonCmd),
    /// Copia a configuração existente de um harness para o HOME sintético.
    Import(ImportArgs),
    /// Atualiza um harness no host, fora do sandbox, com o atualizador oficial
    /// dele. O sandmux em si é atualizado pelo pacote ou pelo install.sh.
    Update(UpdateArgs),
    /// Mostra e altera o arquivo de configuração (perfis e ajustes).
    #[command(subcommand)]
    Config(configcmd::ConfigCmd),
    /// Cofre de segredos: criar, desbloquear, guardar e listar.
    #[command(subcommand)]
    Vault(vaultcmd::VaultCmd),
    /// Conversa com um modelo cadastrado. Num terminal, abre uma aba de chat
    /// no daemon e anexa; sem terminal, imprime a resposta da pergunta.
    Chat {
        /// Modelo, `<provedor>/<modelo>` (sem ele, `chat-model` de [assistant]).
        #[arg(long)]
        model: Option<String>,
        /// Primeira mensagem (obrigatória sem terminal).
        texto: Vec<String>,
    },
    /// Configura depois de instalar: o perfil do AppArmor do lançador (com
    /// sudo, depois de perguntar), o daemon e o diagnóstico.
    Setup {
        /// Não usa sudo: mostra os comandos do perfil para rodar como root.
        #[arg(long)]
        no_sudo: bool,
        /// Não sobe o daemon.
        #[arg(long)]
        no_daemon: bool,
    },
    /// Mostra se a máquina tem o suporte necessário e o que falta.
    Doctor {
        /// Leva para o cofre as chaves reais encontradas nos HOMEs sintéticos,
        /// deixando valores falsos no lugar.
        #[arg(long)]
        fix: bool,
    },
}

#[derive(clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
struct NetArgs {
    #[command(subcommand)]
    cmd: Option<NetCmd>,
    /// Sessão (sem ela, todas).
    id: Option<u32>,
}

#[derive(clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
struct FilesArgs {
    #[command(subcommand)]
    cmd: Option<FilesCmd>,
    /// Sessão (sem ela, todas).
    id: Option<u32>,
    /// Dias para trás, contando hoje.
    #[arg(long, default_value_t = 1)]
    days: u32,
}

#[derive(Subcommand)]
enum McpCmd {
    /// Lista os servidores MCP da sessão, ou do harness na pasta atual.
    List {
        id: Option<u32>,
        /// Harness, sem sessão (opencode ou hermes).
        #[arg(long, default_value = "opencode")]
        harness: String,
    },
    /// Desabilita um servidor na aba da sessão (vale no próximo lançamento).
    Deny { nome: String, id: u32 },
    /// Habilita só os servidores da allowlist da aba (vale no próximo
    /// lançamento).
    Allow { nome: String, id: u32 },
    /// Tira um servidor das regras da aba.
    Forget { nome: String, id: u32 },
}

#[derive(Subcommand)]
enum FolderCmd {
    /// Monta a pasta em /mnt/sandmux/<nome> na sessão (só leitura, ou com
    /// --rw).
    Add {
        id: u32,
        pasta: PathBuf,
        /// Com leitura e escrita.
        #[arg(long)]
        rw: bool,
    },
    /// Desmonta uma pasta pelo nome.
    Remove { id: u32, nome: String },
    /// Lista as pastas da sessão.
    List { id: u32 },
}

#[derive(Subcommand)]
enum FilesCmd {
    /// Nega um arquivo ou pasta na aba da sessão (caminho com `/` ou `~/`, ou
    /// nome como `*.sql`), com efeito na abertura seguinte.
    Deny { padrao: String, id: u32 },
    /// Libera um arquivo ou pasta na aba da sessão (vence a negação e os
    /// segredos).
    Allow { padrao: String, id: u32 },
    /// Tira um padrão das regras de arquivos da aba da sessão.
    Forget { padrao: String, id: u32 },
}

#[derive(Subcommand)]
enum NetCmd {
    /// Nega um destino na aba da sessão (domínio, *.domínio, IPv4 ou CIDR).
    Deny { padrao: String, id: u32 },
    /// Libera um destino no modo custom, na aba da sessão.
    Allow { padrao: String, id: u32 },
    /// Tira um padrão das regras da aba da sessão.
    Forget { padrao: String, id: u32 },
    /// Troca o modo de rede da aba: aberta, inferencia, custom ou sem-rede.
    Mode { modo: String, id: u32 },
    /// Mostra o modo e as regras efetivas de uma sessão, ou da pasta atual.
    Rules { id: Option<u32> },
}

#[derive(Subcommand)]
enum DaemonCmd {
    /// Mostra se o daemon está rodando, a versão, o PID e o número de sessões.
    Status,
    /// Encerra os harnesses e para o daemon; as sessões vivas voltam na próxima subida.
    Stop,
    /// Para o daemon e sobe de novo com o binário instalado, relançando as sessões.
    Restart,
    /// Para o daemon e remove a unidade systemd de usuário.
    Disable,
    /// Início do daemon no boot, sem login (linger do systemd). Sem
    /// subcomando, mostra o estado.
    Boot {
        #[command(subcommand)]
        acao: Option<sessions::BootCmd>,
    },
}

#[derive(clap::Args)]
struct RunArgs {
    /// Harness: opencode, hermes ou shell.
    harness: String,
    /// Pasta de trabalho, no lugar da pasta atual.
    #[arg(long, value_name = "PASTA")]
    workdir: Option<PathBuf>,
    /// Acrescenta um caminho do host em somente leitura (repetível).
    #[arg(long, value_name = "CAMINHO")]
    ro: Vec<PathBuf>,
    /// Acrescenta um caminho do host com leitura e escrita (repetível).
    #[arg(long, value_name = "CAMINHO")]
    rw: Vec<PathBuf>,
    /// Passa uma variável do seu ambiente para dentro do sandbox (repetível).
    #[arg(long = "pass-env", value_name = "NOME")]
    pass_env: Vec<String>,
    /// Na primeira execução, importa a configuração existente sem perguntar.
    #[arg(long, conflicts_with = "no_import")]
    import: bool,
    /// Na primeira execução, começa com o HOME sintético vazio sem perguntar.
    #[arg(long = "no-import")]
    no_import: bool,
    /// Aplica um perfil nomeado do arquivo de configuração.
    #[arg(long, value_name = "NOME")]
    profile: Option<String>,
    /// Numa subpasta de repositório, monta o repositório inteiro em somente leitura.
    #[arg(long, conflicts_with = "no_repo")]
    repo: bool,
    /// Não monta o repositório inteiro, mesmo que a configuração peça.
    #[arg(long = "no-repo")]
    no_repo: bool,
    /// Libera /dev/kvm dentro do sandbox.
    #[arg(long, conflicts_with = "no_kvm")]
    kvm: bool,
    /// Não libera /dev/kvm, mesmo que a configuração peça.
    #[arg(long = "no-kvm")]
    no_kvm: bool,
    /// Expõe a GPU, mesmo que a configuração a desligue.
    #[arg(long, conflicts_with = "no_gpu")]
    gpu: bool,
    /// Não expõe a GPU dentro do sandbox.
    #[arg(long = "no-gpu")]
    no_gpu: bool,
    /// Deixa o agente abrir janelas no desktop e usar o áudio, pelo socket restrito do compositor.
    #[arg(long, conflicts_with_all = ["no_window", "window_direct"])]
    window: bool,
    /// Como --window, pelo socket comum do compositor, que também dá acesso ao clipboard.
    #[arg(long = "window-direct", conflicts_with = "no_window")]
    window_direct: bool,
    /// Sem janela nem áudio, mesmo que a configuração peça.
    #[arg(long = "no-window")]
    no_window: bool,
    /// Liga o portal do desktop (aparência, notificações e abrir link), que já vem ligado por padrão.
    #[arg(long, conflicts_with = "no_portal")]
    portal: bool,
    /// Sem o portal do desktop, mesmo que a configuração peça.
    #[arg(long = "no-portal")]
    no_portal: bool,
    /// Deixa o harness gravar no clipboard do seu terminal (OSC 52).
    #[arg(long = "allow-clipboard", conflicts_with = "no_allow_clipboard")]
    allow_clipboard: bool,
    /// Bloqueia a gravação no clipboard, mesmo que a configuração libere.
    #[arg(long = "no-allow-clipboard")]
    no_allow_clipboard: bool,
    /// Teto de memória: porcentagem da RAM (50%), tamanho (16G) ou max.
    #[arg(long, value_name = "TETO")]
    memory: Option<String>,
    /// Teto de processos e threads: um número ou max.
    #[arg(long, value_name = "TETO")]
    processes: Option<String>,
    /// Modo de rede: aberta, inferencia, custom ou sem-rede.
    #[arg(long, value_name = "MODO")]
    network: Option<String>,
    /// Nega um destino nesta aba (domínio, *.domínio, IPv4 ou CIDR). Repetível.
    #[arg(long, value_name = "PADRÃO")]
    deny: Vec<String>,
    /// Libera um destino no modo custom, nesta aba. Repetível.
    #[arg(long, value_name = "PADRÃO")]
    allow: Vec<String>,
    /// Decifra o TLS deste destino (registro de método e caminho). Repetível.
    #[arg(long, value_name = "PADRÃO")]
    decrypt: Vec<String>,
    /// Chave SSH do cofre que o agente SSH da sessão pode usar (`*` para
    /// todas). Repetível.
    #[arg(long = "ssh-key", value_name = "NOME")]
    ssh_keys: Vec<String>,
    /// Nega um arquivo ou pasta nesta aba (caminho com `/` ou `~/`, ou nome
    /// como `*.sql`). Repetível.
    #[arg(long = "file-deny", value_name = "PADRÃO")]
    file_deny: Vec<String>,
    /// Libera um arquivo ou pasta nesta aba (vence a negação e os segredos).
    /// Repetível.
    #[arg(long = "file-allow", value_name = "PADRÃO")]
    file_allow: Vec<String>,
    /// Conexão de banco do cofre que o MCP de banco da sessão pode usar (`*`
    /// para todas). Repetível.
    #[arg(long = "db", value_name = "NOME")]
    db: Vec<String>,
    /// Publica no host a porta de entrada do sandbox (`8642`, ou `8443@0.0.0.0`
    /// para escutar também nesse endereço). Com alguma, só elas são
    /// publicadas. Repetível.
    #[arg(long = "port", value_name = "PORTA[@ENDEREÇO]")]
    ports: Vec<String>,
    /// Leva a porta do host ao `127.0.0.1` do sandbox: o agente alcança, por
    /// exemplo, o LM Studio em `127.0.0.1:8888` como no host. Repetível.
    #[arg(long = "host-port", value_name = "PORTA")]
    host_ports: Vec<u16>,
    /// Roda o gateway do hermes (canais de mensagem e tarefas agendadas) numa
    /// sessão própria do daemon, que reinicia se cair e volta depois de um
    /// reboot.
    #[arg(long)]
    gateway: bool,
    /// Desabilita um servidor MCP do harness nesta aba. Repetível.
    #[arg(long = "mcp-deny", value_name = "NOME")]
    mcp_deny: Vec<String>,
    /// Grupo de abas da sessão (sem ele, o principal).
    #[arg(long = "group", value_name = "GRUPO")]
    group: Option<String>,
    /// Só estes servidores MCP ficam habilitados nesta aba. Repetível.
    #[arg(long = "mcp-allow", value_name = "NOME")]
    mcp_allow: Vec<String>,
    /// Argumentos repassados ao harness (depois de `--`).
    #[arg(last = true, value_name = "ARGUMENTOS")]
    args: Vec<String>,
}

#[derive(clap::Args)]
struct UpdateArgs {
    /// Harness: opencode, hermes, claude, omp ou agy.
    harness: String,
    /// Não pergunta quando há abas vivas do harness.
    #[arg(long)]
    yes: bool,
    /// Argumentos repassados ao atualizador (depois de `--`).
    #[arg(last = true, value_name = "ARGUMENTOS")]
    args: Vec<String>,
}

#[derive(clap::Args)]
struct ImportArgs {
    /// Harness: opencode, hermes, claude, omp ou agy.
    harness: String,
    /// Substitui os arquivos que já existem no HOME sintético.
    #[arg(long)]
    overwrite: bool,
}

fn warn(msg: impl std::fmt::Display) {
    eprintln!(
        "{}",
        trf!("sandmux: aviso: {msg}", "sandmux: warning: {msg}")
    );
}

fn main() -> ExitCode {
    sandmux::init_language();
    let cli = match Cli::from_arg_matches(&localize(Cli::command()).get_matches()) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let result = match cli.cmd {
        None => tui::run(None),
        Some(cmd) => dispatch(cmd),
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("{}", trf!("sandmux: erro: {e:#}", "sandmux: error: {e:#}"));
            ExitCode::from(1)
        }
    }
}

/// Ajuda em inglês: troca `about`, `long_about` e `help` (casando o texto
/// do doc comment, sem o ponto final) e os nomes de valor pelas traduções.
/// Um texto fora da tabela fica como está.
fn localize(cmd: clap::Command) -> clap::Command {
    if !sandmux_spec::lang::is_en() {
        return cmd;
    }
    localize_cmd(cmd)
}

fn localize_cmd(mut cmd: clap::Command) -> clap::Command {
    if let Some(t) = cmd.get_about().and_then(|s| help_en(&s.to_string())) {
        cmd = cmd.about(t);
    }
    if let Some(t) = cmd.get_long_about().and_then(|s| help_en(&s.to_string())) {
        cmd = cmd.long_about(t);
    }
    cmd = cmd.mut_args(|mut a| {
        if let Some(t) = a.get_help().and_then(|s| help_en(&s.to_string())) {
            a = a.help(t);
        }
        if let Some(t) = a.get_long_help().and_then(|s| help_en(&s.to_string())) {
            a = a.long_help(t);
        }
        if let Some(names) = a.get_value_names() {
            let en: Vec<clap::builder::Str> = names
                .iter()
                .map(|n| {
                    VALUE_NAMES_EN
                        .iter()
                        .find(|(pt, _)| *pt == n.as_str())
                        .map_or_else(|| n.clone(), |(_, en)| (*en).into())
                })
                .collect();
            a = a.value_names(en);
        }
        a
    });
    cmd.mut_subcommands(localize_cmd)
}

/// A tradução de um texto de ajuda, com o ponto final do original.
fn help_en(text: &str) -> Option<String> {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end_matches('.')
            .to_string()
    };
    let key = norm(text);
    let (_, en) = HELP_EN.iter().find(|(pt, _)| norm(pt) == key)?;
    let en = en.trim_end_matches('.');
    Some(if text.trim_end().ends_with('.') {
        format!("{en}.")
    } else {
        en.to_string()
    })
}

/// Nomes de valor da ajuda (`<PASTA>`), português -> inglês.
const VALUE_NAMES_EN: &[(&str, &str)] = &[
    ("PASTA", "FOLDER"),
    ("GRUPO", "GROUP"),
    ("NOVO", "NEW"),
    ("NOME", "NAME"),
    ("CAMINHO", "PATH"),
    ("TETO", "CAP"),
    ("MODO", "MODE"),
    ("PADRÃO", "PATTERN"),
    ("PADRAO", "PATTERN"),
    ("ARGUMENTOS", "ARGS"),
    ("PORTA[@ENDEREÇO]", "PORT[@ADDRESS]"),
    ("PORTA", "PORT"),
    ("TEXTO", "TEXT"),
    ("CHAVE", "KEY"),
    ("VALOR", "VALUE"),
    ("FERRAMENTA", "TOOL"),
    ("DOMÍNIO", "DOMAIN"),
    ("CONFIGURAÇÃO", "CONFIGURATION"),
    ("ARQUIVO", "FILE"),
    ("PROVEDOR", "PROVIDER"),
    ("CONTEXTO", "CONTEXT"),
];

/// Ajuda da linha de comando, português (o doc comment) -> inglês. Cobre
/// `Cli` e os subcomandos daqui, de `config`, de `db` e de `vault`.
const HELP_EN: &[(&str, &str)] = &[
    // Cli
    (
        "Roda agentes de código dentro de um sandbox",
        "Runs coding agents inside a sandbox",
    ),
    (
        "Sem subcomando, num terminal, abre a TUI.",
        "With no subcommand, in a terminal, opens the TUI.",
    ),
    // Cmd
    (
        "Abre a interface com abas, uma por sessão (o mesmo que `sandmux` sem subcomando).",
        "Opens the tabbed interface, one tab per session (the same as `sandmux` with no subcommand).",
    ),
    (
        "Sobe um harness confinado na pasta atual. Num terminal, cria uma sessão no daemon e anexa a ela (Ctrl+b d desanexa, Ctrl+b Ctrl+b envia Ctrl+b); sem terminal, roda direto, em primeiro plano.",
        "Starts a confined harness in the current folder. In a terminal, creates a session in the daemon and attaches to it (Ctrl+b d detaches, Ctrl+b Ctrl+b sends Ctrl+b); without a terminal, runs directly, in the foreground.",
    ),
    ("Lista as sessões.", "Lists the sessions."),
    (
        "Abre um shell dentro do sandbox de uma sessão viva, com a mesma visão, rede e restrições do agente.",
        "Opens a shell inside the sandbox of a live session, with the agent's view, network and restrictions.",
    ),
    (
        "Anexa este terminal a uma sessão.",
        "Attaches this terminal to a session.",
    ),
    (
        "Identificador da sessão (sem ele, a única sessão viva).",
        "Session id (without it, the only alive session).",
    ),
    (
        "Desanexa todos os terminais de uma sessão, sem encerrá-la.",
        "Detaches all terminals from a session, without ending it.",
    ),
    (
        "Encerra uma sessão e a tira da lista.",
        "Ends a session and removes it from the list.",
    ),
    (
        "Mostra o consumo de memória e de processos de uma sessão.",
        "Shows a session's memory and process usage.",
    ),
    (
        "Mostra as últimas conexões de rede, de uma sessão ou de todas, e muda as regras de rede da aba de uma sessão.",
        "Shows the latest network connections, of one session or all, and changes the network rules of a session's tab.",
    ),
    (
        "Mostra os acessos às pastas do agente, de uma sessão ou de todas.",
        "Shows the accesses to the agent's folders, of one session or all.",
    ),
    (
        "Monta, desmonta e lista as pastas do host em /mnt/sandmux de uma sessão, sem relançar o agente.",
        "Mounts, unmounts and lists a session's host folders in /mnt/sandmux, without relaunching the agent.",
    ),
    (
        "Conexões de banco no cofre, para o MCP de banco, e o registro das queries.",
        "Database connections in the vault, for the database MCP, and the query log.",
    ),
    (
        "Lista e habilita ou desabilita os servidores MCP do harness.",
        "Lists and enables or disables the harness's MCP servers.",
    ),
    (
        "Troca o nome de uma sessão (o nome da aba na TUI).",
        "Renames a session (the tab name in the TUI).",
    ),
    (
        "Relança uma sessão encerrada ou parada, na mesma pasta e com as mesmas opções.",
        "Relaunches an ended or stopped session, in the same folder and with the same options.",
    ),
    (
        "Reinicia também uma sessão viva, encerrando o harness antes.",
        "Also restarts a live session, ending the harness first.",
    ),
    (
        "Controla o daemon de sessões.",
        "Controls the session daemon.",
    ),
    (
        "Copia a configuração existente de um harness para o HOME sintético.",
        "Copies a harness's existing configuration into the synthetic HOME.",
    ),
    (
        "Atualiza um harness no host, fora do sandbox, com o atualizador oficial dele. O sandmux em si é atualizado pelo pacote ou pelo install.sh.",
        "Updates a harness on the host, outside the sandbox, with its official updater. sandmux itself is updated by the package or by install.sh.",
    ),
    (
        "Mostra e altera o arquivo de configuração (perfis e ajustes).",
        "Shows and changes the configuration file (profiles and settings).",
    ),
    (
        "Cofre de segredos: criar, desbloquear, guardar e listar.",
        "Secret vault: create, unlock, store and list.",
    ),
    (
        "Conversa com um modelo cadastrado. Num terminal, abre uma aba de chat no daemon e anexa; sem terminal, imprime a resposta da pergunta.",
        "Chats with a registered model. In a terminal, opens a chat tab in the daemon and attaches; without a terminal, prints the answer to the question.",
    ),
    (
        "Modelo, `<provedor>/<modelo>` (sem ele, `chat-model` de [assistant]).",
        "Model, `<provider>/<model>` (without it, `chat-model` from [assistant]).",
    ),
    (
        "Primeira mensagem (obrigatória sem terminal).",
        "First message (required without a terminal).",
    ),
    (
        "Configura depois de instalar: o perfil do AppArmor do lançador (com sudo, depois de perguntar), o daemon e o diagnóstico.",
        "Sets things up after installing: the launcher's AppArmor profile (with sudo, after asking), the daemon and the diagnosis.",
    ),
    (
        "Não usa sudo: mostra os comandos do perfil para rodar como root.",
        "Does not use sudo: shows the profile commands to run as root.",
    ),
    ("Não sobe o daemon.", "Does not start the daemon."),
    (
        "Mostra se a máquina tem o suporte necessário e o que falta.",
        "Shows whether the machine has the required support and what is missing.",
    ),
    (
        "Leva para o cofre as chaves reais encontradas nos HOMEs sintéticos, deixando valores falsos no lugar.",
        "Moves the real keys found in the synthetic HOMEs into the vault, leaving fake values in their place.",
    ),
    // NetArgs, FilesArgs
    ("Sessão (sem ela, todas).", "Session (without it, all)."),
    (
        "Dias para trás, contando hoje.",
        "Days back, counting today.",
    ),
    // McpCmd
    (
        "Lista os servidores MCP da sessão, ou do harness na pasta atual.",
        "Lists the session's MCP servers, or the harness's in the current folder.",
    ),
    (
        "Harness, sem sessão (opencode ou hermes).",
        "Harness, without a session (opencode or hermes).",
    ),
    (
        "Desabilita um servidor na aba da sessão (vale no próximo lançamento).",
        "Disables a server in the session's tab (takes effect on the next launch).",
    ),
    (
        "Habilita só os servidores da allowlist da aba (vale no próximo lançamento).",
        "Enables only the servers in the tab's allowlist (takes effect on the next launch).",
    ),
    (
        "Tira um servidor das regras da aba.",
        "Removes a server from the tab's rules.",
    ),
    // FolderCmd
    (
        "Monta a pasta em /mnt/sandmux/<nome> na sessão (só leitura, ou com --rw).",
        "Mounts the folder at /mnt/sandmux/<name> in the session (read-only, or with --rw).",
    ),
    ("Com leitura e escrita.", "With read and write."),
    (
        "Desmonta uma pasta pelo nome.",
        "Unmounts a folder by name.",
    ),
    ("Lista as pastas da sessão.", "Lists the session's folders."),
    // FilesCmd
    (
        "Nega um arquivo ou pasta na aba da sessão (caminho com `/` ou `~/`, ou nome como `*.sql`), com efeito na abertura seguinte.",
        "Denies a file or folder in the session's tab (a path with `/` or `~/`, or a name like `*.sql`), taking effect on the next open.",
    ),
    (
        "Libera um arquivo ou pasta na aba da sessão (vence a negação e os segredos).",
        "Allows a file or folder in the session's tab (overrides the denial and the secrets).",
    ),
    (
        "Tira um padrão das regras de arquivos da aba da sessão.",
        "Removes a pattern from the file rules of the session's tab.",
    ),
    // NetCmd
    (
        "Nega um destino na aba da sessão (domínio, *.domínio, IPv4 ou CIDR).",
        "Denies a destination in the session's tab (domain, *.domain, IPv4 or CIDR).",
    ),
    (
        "Libera um destino no modo custom, na aba da sessão.",
        "Allows a destination in custom mode, in the session's tab.",
    ),
    (
        "Tira um padrão das regras da aba da sessão.",
        "Removes a pattern from the session's tab rules.",
    ),
    (
        "Troca o modo de rede da aba: aberta, inferencia, custom ou sem-rede.",
        "Changes the tab's network mode: aberta, inferencia, custom or sem-rede.",
    ),
    (
        "Mostra o modo e as regras efetivas de uma sessão, ou da pasta atual.",
        "Shows the effective mode and rules of a session, or of the current folder.",
    ),
    // DaemonCmd
    (
        "Mostra se o daemon está rodando, a versão, o PID e o número de sessões.",
        "Shows whether the daemon is running, its version, its PID and the number of sessions.",
    ),
    (
        "Encerra os harnesses e para o daemon; as sessões vivas voltam na próxima subida.",
        "Ends the harnesses and stops the daemon; alive sessions come back on the next start.",
    ),
    (
        "Para o daemon e sobe de novo com o binário instalado, relançando as sessões.",
        "Stops the daemon and starts it again with the installed binary, relaunching the sessions.",
    ),
    (
        "Para o daemon e remove a unidade systemd de usuário.",
        "Stops the daemon and removes the systemd user unit.",
    ),
    // RunArgs
    (
        "Harness: opencode, hermes ou shell.",
        "Harness: opencode, hermes or shell.",
    ),
    (
        "Pasta de trabalho, no lugar da pasta atual.",
        "Work folder, instead of the current folder.",
    ),
    (
        "Acrescenta um caminho do host em somente leitura (repetível).",
        "Adds a host path as read-only (repeatable).",
    ),
    (
        "Acrescenta um caminho do host com leitura e escrita (repetível).",
        "Adds a host path with read and write (repeatable).",
    ),
    (
        "Passa uma variável do seu ambiente para dentro do sandbox (repetível).",
        "Passes a variable from your environment into the sandbox (repeatable).",
    ),
    (
        "Na primeira execução, importa a configuração existente sem perguntar.",
        "On the first run, imports the existing configuration without asking.",
    ),
    (
        "Na primeira execução, começa com o HOME sintético vazio sem perguntar.",
        "On the first run, starts with an empty synthetic HOME without asking.",
    ),
    (
        "Aplica um perfil nomeado do arquivo de configuração.",
        "Applies a named profile from the configuration file.",
    ),
    (
        "Numa subpasta de repositório, monta o repositório inteiro em somente leitura.",
        "In a repository subfolder, mounts the whole repository as read-only.",
    ),
    (
        "Não monta o repositório inteiro, mesmo que a configuração peça.",
        "Does not mount the whole repository, even if the configuration asks for it.",
    ),
    (
        "Libera /dev/kvm dentro do sandbox.",
        "Allows /dev/kvm inside the sandbox.",
    ),
    (
        "Não libera /dev/kvm, mesmo que a configuração peça.",
        "Does not allow /dev/kvm, even if the configuration asks for it.",
    ),
    (
        "Expõe a GPU, mesmo que a configuração a desligue.",
        "Exposes the GPU, even if the configuration turns it off.",
    ),
    (
        "Deixa o agente abrir janelas no desktop e usar o áudio, pelo socket restrito do compositor.",
        "Lets the agent open windows on the desktop and use audio, through the compositor's restricted socket.",
    ),
    (
        "Como --window, pelo socket comum do compositor, que também dá acesso ao clipboard.",
        "Like --window, through the compositor's plain socket, which also gives access to the clipboard.",
    ),
    (
        "Sem janela nem áudio, mesmo que a configuração peça.",
        "No window or audio, even if the configuration asks for it.",
    ),
    (
        "Liga o portal do desktop (aparência, notificações e abrir link), que já vem ligado por padrão.",
        "Turns on the desktop portal (appearance, notifications and opening links), which is on by default.",
    ),
    (
        "Sem o portal do desktop, mesmo que a configuração peça.",
        "No desktop portal, even if the configuration asks for it.",
    ),
    (
        "Não expõe a GPU dentro do sandbox.",
        "Does not expose the GPU inside the sandbox.",
    ),
    (
        "Deixa o harness gravar no clipboard do seu terminal (OSC 52).",
        "Lets the harness write to your terminal's clipboard (OSC 52).",
    ),
    (
        "Bloqueia a gravação no clipboard, mesmo que a configuração libere.",
        "Blocks writing to the clipboard, even if the configuration allows it.",
    ),
    (
        "Teto de memória: porcentagem da RAM (50%), tamanho (16G) ou max.",
        "Memory cap: percentage of RAM (50%), size (16G) or max.",
    ),
    (
        "Teto de processos e threads: um número ou max.",
        "Process and thread cap: a number or max.",
    ),
    (
        "Modo de rede: aberta, inferencia, custom ou sem-rede.",
        "Network mode: aberta, inferencia, custom or sem-rede.",
    ),
    (
        "Nega um destino nesta aba (domínio, *.domínio, IPv4 ou CIDR). Repetível.",
        "Denies a destination in this tab (domain, *.domain, IPv4 or CIDR). Repeatable.",
    ),
    (
        "Libera um destino no modo custom, nesta aba. Repetível.",
        "Allows a destination in custom mode, in this tab. Repeatable.",
    ),
    (
        "Decifra o TLS deste destino (registro de método e caminho). Repetível.",
        "Decrypts this destination's TLS (logs method and path). Repeatable.",
    ),
    (
        "Chave SSH do cofre que o agente SSH da sessão pode usar (`*` para todas). Repetível.",
        "Vault SSH key the session's SSH agent may use (`*` for all). Repeatable.",
    ),
    (
        "Nega um arquivo ou pasta nesta aba (caminho com `/` ou `~/`, ou nome como `*.sql`). Repetível.",
        "Denies a file or folder in this tab (a path with `/` or `~/`, or a name like `*.sql`). Repeatable.",
    ),
    (
        "Libera um arquivo ou pasta nesta aba (vence a negação e os segredos). Repetível.",
        "Allows a file or folder in this tab (overrides the denial and the secrets). Repeatable.",
    ),
    (
        "Conexão de banco do cofre que o MCP de banco da sessão pode usar (`*` para todas). Repetível.",
        "Vault database connection the session's database MCP may use (`*` for all). Repeatable.",
    ),
    (
        "Publica no host a porta de entrada do sandbox (`8642`, ou `8443@0.0.0.0` para escutar também nesse endereço). Com alguma, só elas são publicadas. Repetível.",
        "Publishes the sandbox's inbound port on the host (`8642`, or `8443@0.0.0.0` to also listen on that address). With any given, only those are published. Repeatable.",
    ),
    (
        "Leva a porta do host ao `127.0.0.1` do sandbox: o agente alcança, por exemplo, o LM Studio em `127.0.0.1:8888` como no host. Repetível.",
        "Brings the host port to the sandbox's `127.0.0.1`: the agent reaches, for example, LM Studio at `127.0.0.1:8888` as on the host. Repeatable.",
    ),
    (
        "Roda o gateway do hermes (canais de mensagem e tarefas agendadas) numa sessão própria do daemon, que reinicia se cair e volta depois de um reboot.",
        "Runs the hermes gateway (messaging channels and scheduled tasks) in its own daemon session, which restarts if it crashes and comes back after a reboot.",
    ),
    (
        "Desabilita um servidor MCP do harness nesta aba. Repetível.",
        "Disables a harness MCP server in this tab. Repeatable.",
    ),
    (
        "Só estes servidores MCP ficam habilitados nesta aba. Repetível.",
        "Only these MCP servers stay enabled in this tab. Repeatable.",
    ),
    (
        "Argumentos repassados ao harness (depois de `--`).",
        "Arguments passed to the harness (after `--`).",
    ),
    // ImportArgs e UpdateArgs
    (
        "Harness: opencode, hermes, claude, omp ou agy.",
        "Harness: opencode, hermes, claude, omp or agy.",
    ),
    (
        "Não pergunta quando há abas vivas do harness.",
        "Does not ask when the harness has live tabs.",
    ),
    (
        "Argumentos repassados ao atualizador (depois de `--`).",
        "Arguments passed to the updater (after `--`).",
    ),
    (
        "Substitui os arquivos que já existem no HOME sintético.",
        "Replaces the files that already exist in the synthetic HOME.",
    ),
    // ConfigCmd (configcmd.rs)
    (
        "Mostra a configuração efetiva para a pasta atual, com a origem de cada valor.",
        "Shows the effective configuration for the current folder, with the origin of each value.",
    ),
    (
        "Pasta de trabalho considerada, no lugar da pasta atual.",
        "Work folder to consider, instead of the current folder.",
    ),
    ("Perfil nomeado considerado.", "Named profile to consider."),
    (
        "Lista os perfis nomeados e as pastas com perfil de projeto.",
        "Lists the named profiles and the folders with a project profile.",
    ),
    (
        "Define uma chave de liga e desliga (repo, kvm, gpu, clipboard) ou um teto (memory, processes).",
        "Sets an on/off key (repo, kvm, gpu, clipboard) or a cap (memory, processes).",
    ),
    (
        "true ou false (também sim ou não); nos tetos, 50%, 16G, 512 ou max.",
        "true or false (also sim or não); for caps, 50%, 16G, 512 or max.",
    ),
    (
        "Apaga uma chave de liga e desliga ou um teto.",
        "Clears an on/off key or a cap.",
    ),
    (
        "Acrescenta um item a uma lista (ro, rw, pass-env).",
        "Adds an item to a list (ro, rw, pass-env).",
    ),
    (
        "Retira um item de uma lista.",
        "Removes an item from a list.",
    ),
    (
        "Onde a mudança é gravada: exatamente um dos três.",
        "Where the change is written: exactly one of the three.",
    ),
    (
        "Na seção [global], que vale para toda execução.",
        "In the [global] section, which applies to every run.",
    ),
    (
        "No perfil nomeado [profile.NOME].",
        "In the named profile [profile.NAME].",
    ),
    (
        "No perfil do projeto da pasta (sem pasta, a atual).",
        "In the folder's project profile (without a folder, the current one).",
    ),
    // DbCmd (dbcmd.rs)
    (
        "Guarda uma conexão no cofre; a senha é lida sem eco (ou da entrada padrão).",
        "Saves a connection in the vault; the password is read without echo (or from standard input).",
    ),
    ("`postgres` ou `mysql`.", "`postgres` or `mysql`."),
    (
        "Padrão: 5432 (postgres) ou 3306 (mysql).",
        "Default: 5432 (postgres) or 3306 (mysql).",
    ),
    (
        "Lista as conexões (sem as senhas).",
        "Lists the connections (without the passwords).",
    ),
    ("Apaga uma conexão.", "Removes a connection."),
    (
        "Mostra as queries registradas, de uma sessão ou de todas.",
        "Shows the logged queries, of one session or all.",
    ),
    // VaultCmd (vaultcmd.rs)
    (
        "Cria o cofre, pedindo a senha.",
        "Creates the vault, asking for the password.",
    ),
    (
        "Desbloqueia o cofre no daemon.",
        "Unlocks the vault in the daemon.",
    ),
    (
        "Tranca o cofre e apaga os segredos da memória do daemon.",
        "Locks the vault and wipes the secrets from the daemon's memory.",
    ),
    (
        "Diz se o cofre existe e se está desbloqueado.",
        "Says whether the vault exists and whether it is unlocked.",
    ),
    (
        "Guarda um segredo; o valor é lido do terminal sem eco (ou da entrada padrão).",
        "Saves a secret; the value is read from the terminal without echo (or from standard input).",
    ),
    (
        "Domínio dono do segredo (padrão de destino). Repetível.",
        "Domain that owns the secret (destination pattern). Repeatable.",
    ),
    (
        "Ferramenta que usa o segredo: gh, git, npm, pip, cargo ou docker. Repetível; sem ela, vem dos domínios conhecidos.",
        "Tool that uses the secret: gh, git, npm, pip, cargo or docker. Repeatable; without it, it comes from the known domains.",
    ),
    (
        "Guarda uma chave SSH privada (Ed25519, ECDSA ou RSA) para o agente SSH do sandbox; a passphrase, se houver, é pedida sem eco.",
        "Saves a private SSH key (Ed25519, ECDSA or RSA) for the sandbox's SSH agent; the passphrase, if any, is asked without echo.",
    ),
    (
        "Arquivo da chave privada OpenSSH.",
        "OpenSSH private key file.",
    ),
    (
        "Servidor git da chave (domínio exato). Repetível; ao menos um.",
        "The key's git server (exact domain). Repeatable; at least one.",
    ),
    (
        "Guarda uma identidade de cloud: o sandbox recebe um falso, e o real fica fora dele.",
        "Saves a cloud identity: the sandbox gets a fake, and the real one stays outside it.",
    ),
    (
        "Provedor: gcp ou kubernetes.",
        "Provider: gcp or kubernetes.",
    ),
    (
        "Nome da identidade (no gcp); no kubernetes, o nome vem de cada contexto.",
        "Name of the identity (for gcp); for kubernetes, the name comes from each context.",
    ),
    (
        "Contexto do kubeconfig do host (repetível; sem ele, todos os contextos).",
        "Context of the host kubeconfig (repeatable; without it, every context).",
    ),
    (
        "Copia para o cofre o servidor, a CA e a credencial de cada contexto (sem plugin exec).",
        "Copies the server, the CA and the credential of each context into the vault (no exec plugin).",
    ),
    (
        "Configuração do gcloud do host (referência: o real vem do gcloud do host).",
        "Configuration of the host gcloud (reference: the real one comes from the host gcloud).",
    ),
    (
        "Copia para o cofre o ADC do gcloud do host (application_default_credentials.json).",
        "Copies the host gcloud's ADC (application_default_credentials.json) into the vault.",
    ),
    (
        "Copia para o cofre a chave JSON de uma conta de serviço.",
        "Copies a service account JSON key into the vault.",
    ),
    (
        "Torna a identidade a padrão do provedor.",
        "Makes the identity the provider's default.",
    ),
    (
        "Lista os segredos (só os nomes e os domínios).",
        "Lists the secrets (only the names and the domains).",
    ),
    ("Apaga um segredo.", "Removes a secret."),
    ("Troca a senha do cofre.", "Changes the vault password."),
    (
        "Início do daemon no boot, sem login (linger do systemd). Sem subcomando, mostra o estado.",
        "Daemon start at boot, without a login (systemd linger). Without a subcommand, shows the state.",
    ),
    (
        "Liga o linger do usuário: o daemon sobe no boot, sem login.",
        "Turns on the user's linger: the daemon starts at boot, without a login.",
    ),
    (
        "Desliga o linger do usuário.",
        "Turns off the user's linger.",
    ),
    (
        "Grupo de abas mostrado (sem ele, o principal).",
        "Tab group shown (without it, the main one).",
    ),
    (
        "Só as sessões deste grupo.",
        "Only the sessions of this group.",
    ),
    (
        "Grupos de abas: listar, mover uma sessão e renomear.",
        "Tab groups: list, move a session and rename.",
    ),
    (
        "Lista os grupos, com o número de abas.",
        "Lists the groups, with the number of tabs.",
    ),
    (
        "Leva uma sessão para outro grupo.",
        "Moves a session to another group.",
    ),
    (
        "Renomeia um grupo (todas as sessões dele).",
        "Renames a group (all of its sessions).",
    ),
    (
        "Grupo de abas da sessão (sem ele, o principal).",
        "Tab group of the session (without it, the main one).",
    ),
    (
        "Senha do cofre no keyring do sistema, para o daemon abrir o cofre sozinho. Sem subcomando, mostra o estado.",
        "Vault password in the system keyring, so the daemon opens the vault by itself. Without a subcommand, shows the state.",
    ),
    (
        "Guarda a senha do cofre no keyring, depois de conferi-la.",
        "Stores the vault password in the keyring, after checking it.",
    ),
    (
        "Apaga a senha do cofre do keyring.",
        "Deletes the vault password from the keyring.",
    ),
    (
        "Pede os valores reais das chaves que a importação trocou por valores falsos.",
        "Asks for the real values of the keys that the import replaced with fake values.",
    ),
    (
        "Troca só o valor de um segredo (uma chave que venceu), mantendo o falso e os domínios.",
        "Replaces only the value of a secret (a key that expired), keeping the fake and the domains.",
    ),
];

fn dispatch(cmd: Cmd) -> Result<i32> {
    match cmd {
        Cmd::Tui { group } => tui::run(group),
        Cmd::Run(a) => cmd_run(*a),
        Cmd::Ls { group } => sessions::ls(group),
        Cmd::Group(c) => sessions::group(c),
        Cmd::Attach { id } => sessions::attach(id),
        Cmd::Detach { id } => sessions::detach(id),
        Cmd::Kill { id } => sessions::kill(id),
        Cmd::Enter { id } => {
            let mut opts = LaunchOptions::new(launch::ENTER);
            opts.args = id.map(|i| vec![i.to_string()]).unwrap_or_default();
            sessions::run_session(opts, &[], None)
        }
        Cmd::Stats { id } => sessions::stats(id),
        Cmd::Net(a) => match a.cmd {
            None => sessions::net(a.id),
            Some(NetCmd::Deny { padrao, id }) => sessions::net_edit(id, "deny", &padrao),
            Some(NetCmd::Allow { padrao, id }) => sessions::net_edit(id, "allow", &padrao),
            Some(NetCmd::Forget { padrao, id }) => sessions::net_edit(id, "forget", &padrao),
            Some(NetCmd::Mode { modo, id }) => sessions::net_edit(id, "mode", &modo),
            Some(NetCmd::Rules { id }) => sessions::net_rules(id),
        },
        Cmd::Files(a) => match a.cmd {
            None => filescmd::run(a.id, a.days),
            Some(FilesCmd::Deny { padrao, id }) => sessions::net_edit(id, "file-deny", &padrao),
            Some(FilesCmd::Allow { padrao, id }) => sessions::net_edit(id, "file-allow", &padrao),
            Some(FilesCmd::Forget { padrao, id }) => sessions::net_edit(id, "file-forget", &padrao),
        },
        Cmd::Db(c) => dbcmd::run(c),
        Cmd::Mcp(c) => match c {
            McpCmd::List { id, harness } => sessions::mcp_list(id, &harness),
            McpCmd::Deny { nome, id } => sessions::net_edit(id, "mcp-deny", &nome),
            McpCmd::Allow { nome, id } => sessions::net_edit(id, "mcp-allow", &nome),
            McpCmd::Forget { nome, id } => sessions::net_edit(id, "mcp-forget", &nome),
        },
        Cmd::Folder(c) => match c {
            FolderCmd::Add { id, pasta, rw } => sessions::folder_add(id, &pasta, rw),
            FolderCmd::Remove { id, nome } => sessions::folder_remove(id, &nome),
            FolderCmd::List { id } => sessions::folder_list(id),
        },
        Cmd::Rename { id, nome } => sessions::rename(id, &nome),
        Cmd::Restart { id, force } => sessions::restart(id, force),
        Cmd::Daemon(c) => match c {
            DaemonCmd::Status => sessions::daemon_status(),
            DaemonCmd::Stop => sessions::daemon_stop(),
            DaemonCmd::Restart => sessions::daemon_restart(),
            DaemonCmd::Disable => sessions::daemon_disable(),
            DaemonCmd::Boot { acao } => sessions::daemon_boot(acao),
        },
        Cmd::Import(a) => cmd_import(a).map(|_| 0),
        Cmd::Update(a) => sessions::update(a.harness, a.yes, a.args),
        Cmd::Config(c) => configcmd::run(c),
        Cmd::Vault(c) => vaultcmd::run(c),
        Cmd::Chat { model, texto } => chatcmd::run(model, texto),
        Cmd::Setup { no_sudo, no_daemon } => setup::run(no_sudo, no_daemon),
        Cmd::Doctor { fix } => doctor::run_with(fix),
    }
}

/// Agrupa caminhos longos pela pasta de três níveis, para o relatório caber na tela.
fn summarize(items: &[PathBuf]) -> Vec<String> {
    let mut groups: Vec<(PathBuf, usize)> = Vec::new();
    for i in items {
        let key: PathBuf = if i.components().count() > 3 {
            i.components().take(3).collect()
        } else {
            i.clone()
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => groups.push((key, 1)),
        }
    }
    groups
        .into_iter()
        .map(|(k, n)| {
            let single = items.iter().find(|i| i.starts_with(&k));
            match (n, single) {
                (1, Some(s)) => s.display().to_string(),
                _ => trf!("{}/ ({n} arquivos)", "{}/ ({n} files)", k.display()),
            }
        })
        .collect()
}

fn print_import_report(harness: &str, r: &sandmux_import::Report) {
    let list = |title: &str, items: &[PathBuf]| {
        if !items.is_empty() {
            eprintln!("  {title}:");
            for i in summarize(items) {
                eprintln!("    {i}");
            }
        }
    };
    eprintln!(
        "{}",
        trf!(
            "sandmux: importação do {harness}",
            "sandmux: {harness} import"
        )
    );
    list(tr!("copiados", "copied"), &r.copied);
    list(tr!("substituídos", "replaced"), &r.overwritten);
    list(
        tr!(
            "diferentes do host, mantidos (use --overwrite para substituir)",
            "different from the host, kept (use --overwrite to replace)"
        ),
        &r.differing,
    );
    list(
        tr!(
            "ignorados (fora da tabela de importação)",
            "ignored (outside the import table)"
        ),
        &r.ignored,
    );
    if !r.replaced.is_empty() {
        eprintln!(
            "{}",
            tr!(
                "  segredos trocados por valores falsos:",
                "  secrets replaced with fake values:"
            )
        );
        for e in &r.replaced {
            eprintln!(
                "{}",
                trf!(
                    "    {}: {} (linha {})",
                    "    {}: {} (line {})",
                    e.file.display(),
                    e.key,
                    e.line
                )
            );
        }
        eprintln!(
            "{}",
            tr!(
                "  Para o gateway trocar o falso pelo real no tráfego, guarde os valores no cofre:\n    sandmux vault init      (se o cofre ainda não existe)\n    sandmux vault import",
                "  For the gateway to swap the fake for the real value in traffic, store the values in the vault:\n    sandmux vault init      (if the vault does not exist yet)\n    sandmux vault import"
            )
        );
    }
    if !r.local_ports.is_empty() {
        eprintln!(
            "{}",
            tr!(
                "  serviços locais citados na configuração: no sandbox, o 127.0.0.1 é o do próprio sandbox.",
                "  local services mentioned in the configuration: in the sandbox, 127.0.0.1 is the sandbox's own."
            )
        );
        eprintln!(
            "{}",
            tr!(
                "  Para o harness alcançar a porta do host como está configurado, rode:",
                "  For the harness to reach the host port as configured, run:"
            )
        );
        for p in &r.local_ports {
            eprintln!("    sandmux config add host-ports {p} --global");
        }
    }
    if r.is_empty() {
        eprintln!("{}", tr!("  nada a importar", "  nothing to import"));
    }
}

fn do_import(host: &Host, layout: &Layout, harness: &str, overwrite: bool) -> Result<()> {
    let synthetic = layout.home(harness);
    home::private_dir(&synthetic)?;
    let report = sandmux_import::import(
        harness,
        &host.home,
        &synthetic,
        &layout.manifest(harness),
        overwrite,
    )?;
    if harness == "hermes" {
        sandmux_policy::hermes_config::ensure_update_check_off(&synthetic)?;
    }
    print_import_report(harness, &report);
    if harness == "agy"
        && let Some(note) = import_agy_login(host, &synthetic, overwrite)
    {
        eprintln!("  {note}");
    }
    Ok(())
}

/// O login do Google do `agy` no host: os tokens reais vão ao cofre pelo
/// daemon (`agy:access` e `agy:refresh`), e o HOME sintético recebe o mesmo
/// arquivo com os falsos. Sem cofre que aceite, nada é gravado. Devolve a
/// linha do relatório, sem nenhum token.
fn import_agy_login(host: &Host, synthetic: &std::path::Path, overwrite: bool) -> Option<String> {
    use sandmux_import::agy_login::{Login, TOKEN_FILE};
    let login = Login::read(&host.home.join(TOKEN_FILE))?;
    let target = synthetic.join(TOKEN_FILE);
    if target.exists() && !overwrite {
        return Some(
            tr!(
                "login do Google: o HOME sintético já tem um; use --overwrite para trazer o do host",
                "Google login: the synthetic HOME already has one; use --overwrite to bring the host's"
            )
            .into(),
        );
    }
    if login.reals().is_empty() {
        return None;
    }
    match vaultcmd::store_agy_login(&login, &target) {
        Ok(()) => Some(
            tr!(
                "login do Google levado ao cofre (agy:access e agy:refresh); o HOME sintético tem os valores falsos",
                "Google login moved to the vault (agy:access and agy:refresh); the synthetic HOME has the fake values"
            )
            .into(),
        ),
        Err(e) => Some(login_not_imported(&format!("{e:#}"))),
    }
}

fn login_not_imported(why: &str) -> String {
    trf!(
        "login do Google do host não importado: {why}. Abra o cofre (sandmux vault unlock, ou sandmux vault init) e rode sandmux import agy --overwrite",
        "host Google login not imported: {why}. Open the vault (sandmux vault unlock, or sandmux vault init) and run sandmux import agy --overwrite"
    )
}

fn cmd_import(a: ImportArgs) -> Result<()> {
    let host = Host::detect()?;
    if !presets::exists(&a.harness) {
        return Err(sandmux_policy::PolicyError::UnknownHarness(a.harness).into());
    }
    if !sandmux_import::importable(&a.harness) {
        bail!(trf!(
            "o preset {} não tem importação",
            "the {} preset has no import",
            a.harness
        ));
    }
    let layout = Layout::new(&host.data_dir);
    if !sandmux_import::has_state(&a.harness, &host.home) {
        eprintln!(
            "{}",
            trf!(
                "sandmux: não há configuração do {} para importar",
                "sandmux: there is no {} configuration to import",
                a.harness
            )
        );
        return Ok(());
    }
    let created = home::ensure(&host, &layout, &a.harness)?;
    created.warnings.iter().for_each(warn);
    do_import(&host, &layout, &a.harness, a.overwrite)
}

fn ask_yes_no(question: &str) -> Result<bool> {
    eprint!("{question} {} ", tr!("[S/n]", "[Y/n]"));
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let t = line.trim().to_lowercase();
    Ok(t.is_empty() || t == "s" || t == "sim" || t == "y" || t == "yes")
}

/// Primeira execução do harness: cria o HOME sintético, oferecendo a importação.
fn first_run(host: &Host, layout: &Layout, a: &RunArgs) -> Result<()> {
    if layout.home(&a.harness).exists() {
        return Ok(());
    }
    let has_state =
        sandmux_import::importable(&a.harness) && sandmux_import::has_state(&a.harness, &host.home);
    let import = if !has_state {
        false
    } else if a.import {
        true
    } else if a.no_import {
        false
    } else if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        ask_yes_no(&trf!(
            "Primeira execução do {} no sandmux. Importar a configuração que você já tem? Os segredos são trocados por valores falsos e o original não é alterado.",
            "First run of {} in sandmux. Import the configuration you already have? Secrets are replaced with fake values and the original is not changed.",
            a.harness
        ))?
    } else if a.harness == "shell" {
        // O `shell` era usado em scripts antes de ter importação: sem
        // terminal, começa sem importar.
        false
    } else {
        bail!(trf!(
            "primeira execução do {} sem terminal interativo: passe --import para importar a configuração existente ou --no-import para começar vazio",
            "first run of {} without an interactive terminal: pass --import to import the existing configuration or --no-import to start empty",
            a.harness
        ));
    };
    let setup = home::ensure(host, layout, &a.harness)?;
    setup.warnings.iter().for_each(warn);
    if import {
        // O HOME acabou de ser criado: no `shell`, a importação substitui o
        // arquivo mínimo gerado.
        do_import(host, layout, &a.harness, a.harness == "shell")?;
    }
    Ok(())
}

/// Opções de lançamento a partir dos argumentos, com caminhos absolutos.
fn launch_options(host: &Host, a: &RunArgs) -> LaunchOptions {
    let abs = |p: &PathBuf| launch::absolute(p, &host.cwd);
    LaunchOptions {
        harness: a.harness.clone(),
        workdir: a
            .workdir
            .as_ref()
            .map(abs)
            .unwrap_or_else(|| host.cwd.clone()),
        ro: a.ro.iter().map(abs).collect(),
        rw: a.rw.iter().map(abs).collect(),
        pass_env: a.pass_env.clone(),
        profile: a.profile.clone(),
        repo: flag(a.repo, a.no_repo),
        kvm: flag(a.kvm, a.no_kvm),
        gpu: flag(a.gpu, a.no_gpu),
        allow_clipboard: flag(a.allow_clipboard, a.no_allow_clipboard),
        portal: flag(a.portal, a.no_portal),
        window: match (a.window, a.window_direct, a.no_window) {
            (_, true, _) => Some(sandmux_spec::window::WindowMode::Direct),
            (true, _, _) => Some(sandmux_spec::window::WindowMode::On),
            (_, _, true) => Some(sandmux_spec::window::WindowMode::Off),
            _ => None,
        },
        memory: a.memory.clone(),
        processes: a.processes.clone(),
        network: a.network.clone(),
        deny: a.deny.clone(),
        allow: a.allow.clone(),
        decrypt: a.decrypt.clone(),
        ssh_keys: a.ssh_keys.clone(),
        file_deny: a.file_deny.clone(),
        file_allow: a.file_allow.clone(),
        folders: vec![],
        db: a.db.clone(),
        ports: a.ports.clone(),
        host_ports: a.host_ports.clone(),
        mcp_deny: a.mcp_deny.clone(),
        mcp_allow: a.mcp_allow.clone(),
        gateway: a.gateway,
        // O gateway é o `hermes gateway run`, com os argumentos depois.
        args: if a.gateway {
            ["gateway".to_string(), "run".to_string()]
                .into_iter()
                .chain(a.args.iter().cloned())
                .collect()
        } else {
            a.args.clone()
        },
    }
}

/// Par de opções positiva e negativa: `None` quando nenhuma foi dada.
fn flag(on: bool, off: bool) -> Option<bool> {
    match (on, off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

fn cmd_run(a: RunArgs) -> Result<i32> {
    let host = Host::detect()?;
    if !presets::exists(&a.harness) {
        return Err(sandmux_policy::PolicyError::UnknownHarness(a.harness).into());
    }
    if a.gateway && a.harness != "hermes" {
        bail!(tr!(
            "só o hermes tem gateway: use `sandmux run hermes --gateway`",
            "only hermes has a gateway: use `sandmux run hermes --gateway`"
        ));
    }
    if let Some(g) = &a.group {
        sandmux_proto::msg::check_group(g).map_err(|e| anyhow::anyhow!(e))?;
    }
    // Recusa cedo um harness não instalado, antes de criar qualquer coisa.
    presets::plan(&a.harness, &host, &[], &[])?;
    let opts = launch_options(&host, &a);
    // Configuração inválida e perfil inexistente também recusam cedo.
    let effective = launch::effective(&host, &opts)?;
    let layout = Layout::new(&host.data_dir);
    first_run(&host, &layout, &a)?;
    if let Some(w) = sandmux::window::warning(effective.window.value) {
        warn(&w);
    }
    if a.gateway {
        if let Some(w) = sandmux_policy::hermes_config::host_gateway_alive(&host.home) {
            warn(&w);
        }
        // O gateway vive no daemon, com ou sem terminal.
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
            return sessions::create_detached(opts, &effective.pass_env_names(), a.group.clone());
        }
    }
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return sessions::run_session(opts, &effective.pass_env_names(), a.group.clone());
    }
    let prepared = launch::prepare(&host, &opts)?;
    prepared.warnings.iter().for_each(warn);
    Err(prepared.exec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nomes de valor que já estão em inglês (vêm do nome do campo).
    const VALUE_NAMES_OK: &[&str] = &[
        "ID", "HARNESS", "MODEL", "KIND", "HOST", "PORT", "DATABASE", "USER", "DAYS", "ITEM",
    ];

    /// Percorre a árvore inteira do clap e junta os textos sem tradução.
    fn missing(cmd: &clap::Command, path: &str, out: &mut Vec<String>) {
        let path = format!("{path} {}", cmd.get_name());
        for t in [cmd.get_about(), cmd.get_long_about()]
            .into_iter()
            .flatten()
        {
            if help_en(&t.to_string()).is_none() {
                out.push(format!("{path}: {t}"));
            }
        }
        for a in cmd.get_arguments() {
            for t in [a.get_help(), a.get_long_help()].into_iter().flatten() {
                if help_en(&t.to_string()).is_none() {
                    out.push(format!("{path} {}: {t}", a.get_id()));
                }
            }
            // Flags sem valor não mostram nome de valor.
            let names = if a.get_action().takes_values() {
                a.get_value_names().unwrap_or_default()
            } else {
                &[]
            };
            for n in names {
                let n = n.as_str();
                if !VALUE_NAMES_OK.contains(&n) && !VALUE_NAMES_EN.iter().any(|(pt, _)| *pt == n) {
                    out.push(format!("{path} {}: nome de valor {n}", a.get_id()));
                }
            }
        }
        for s in cmd.get_subcommands() {
            missing(s, &path, out);
        }
    }

    #[test]
    fn toda_ajuda_tem_traducao() {
        let mut out = Vec::new();
        missing(&Cli::command(), "", &mut out);
        assert!(out.is_empty(), "sem tradução:\n{}", out.join("\n"));
    }

    #[test]
    fn traducao_mantem_o_ponto() {
        assert_eq!(
            help_en("Lista as sessões").as_deref(),
            Some("Lists the sessions")
        );
        assert_eq!(
            help_en("Lista as sessões.").as_deref(),
            Some("Lists the sessions.")
        );
        assert_eq!(help_en("texto que não existe"), None);
    }
}
