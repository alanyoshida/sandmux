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
use clap::{Parser, Subcommand};
use sandmux::launch;
use sandmux_policy::{Host, Layout, home, presets};
use sandmux_proto::LaunchOptions;

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
    Tui,
    /// Sobe um harness confinado na pasta atual. Num terminal, cria uma sessão
    /// no daemon e anexa a ela (Ctrl+b d desanexa, Ctrl+b Ctrl+b envia Ctrl+b);
    /// sem terminal, roda direto, em primeiro plano.
    Run(Box<RunArgs>),
    /// Lista as sessões.
    Ls,
    /// Anexa este terminal a uma sessão.
    Attach {
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
    Restart { id: u32 },
    /// Controla o daemon de sessões.
    #[command(subcommand)]
    Daemon(DaemonCmd),
    /// Copia a configuração existente de um harness para o HOME sintético.
    Import(ImportArgs),
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
    Doctor,
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
    /// Roda o gateway do hermes (canais de mensagem e tarefas agendadas) numa
    /// sessão própria do daemon, que reinicia se cair e volta depois de um
    /// reboot.
    #[arg(long)]
    gateway: bool,
    /// Desabilita um servidor MCP do harness nesta aba. Repetível.
    #[arg(long = "mcp-deny", value_name = "NOME")]
    mcp_deny: Vec<String>,
    /// Só estes servidores MCP ficam habilitados nesta aba. Repetível.
    #[arg(long = "mcp-allow", value_name = "NOME")]
    mcp_allow: Vec<String>,
    /// Argumentos repassados ao harness (depois de `--`).
    #[arg(last = true, value_name = "ARGUMENTOS")]
    args: Vec<String>,
}

#[derive(clap::Args)]
struct ImportArgs {
    /// Harness: opencode ou hermes.
    harness: String,
    /// Substitui os arquivos que já existem no HOME sintético.
    #[arg(long)]
    overwrite: bool,
}

fn warn(msg: impl std::fmt::Display) {
    eprintln!("sandmux: aviso: {msg}");
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        None | Some(Cmd::Tui) => tui::run(),
        Some(cmd) => dispatch(cmd),
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("sandmux: erro: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(cmd: Cmd) -> Result<i32> {
    match cmd {
        Cmd::Tui => tui::run(),
        Cmd::Run(a) => cmd_run(*a),
        Cmd::Ls => sessions::ls(),
        Cmd::Attach { id } => sessions::attach(id),
        Cmd::Detach { id } => sessions::detach(id),
        Cmd::Kill { id } => sessions::kill(id),
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
        Cmd::Restart { id } => sessions::restart(id),
        Cmd::Daemon(c) => match c {
            DaemonCmd::Status => sessions::daemon_status(),
            DaemonCmd::Stop => sessions::daemon_stop(),
            DaemonCmd::Restart => sessions::daemon_restart(),
            DaemonCmd::Disable => sessions::daemon_disable(),
        },
        Cmd::Import(a) => cmd_import(a).map(|_| 0),
        Cmd::Config(c) => configcmd::run(c),
        Cmd::Vault(c) => vaultcmd::run(c),
        Cmd::Chat { model, texto } => chatcmd::run(model, texto),
        Cmd::Setup { no_sudo, no_daemon } => setup::run(no_sudo, no_daemon),
        Cmd::Doctor => doctor::run(),
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
                _ => format!("{}/ ({n} arquivos)", k.display()),
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
    eprintln!("sandmux: importação do {harness}");
    list("copiados", &r.copied);
    list("substituídos", &r.overwritten);
    list(
        "diferentes do host, mantidos (use --overwrite para substituir)",
        &r.differing,
    );
    list("ignorados (fora da tabela de importação)", &r.ignored);
    if !r.replaced.is_empty() {
        eprintln!("  segredos trocados por valores falsos:");
        for e in &r.replaced {
            eprintln!("    {}: {} (linha {})", e.file.display(), e.key, e.line);
        }
    }
    if r.is_empty() {
        eprintln!("  nada a importar");
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
    Ok(())
}

fn cmd_import(a: ImportArgs) -> Result<()> {
    let host = Host::detect()?;
    if !presets::exists(&a.harness) {
        return Err(sandmux_policy::PolicyError::UnknownHarness(a.harness).into());
    }
    if !sandmux_import::importable(&a.harness) {
        bail!("o preset {} não tem importação", a.harness);
    }
    let layout = Layout::new(&host.data_dir);
    if !sandmux_import::has_state(&a.harness, &host.home) {
        eprintln!(
            "sandmux: não há configuração do {} para importar",
            a.harness
        );
        return Ok(());
    }
    let created = home::ensure(&host, &layout, &a.harness)?;
    created.warnings.iter().for_each(warn);
    do_import(&host, &layout, &a.harness, a.overwrite)
}

fn ask_yes_no(question: &str) -> Result<bool> {
    eprint!("{question} [S/n] ");
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
        ask_yes_no(&format!(
            "Primeira execução do {} no sandmux. Importar a configuração que você já tem? Os segredos são trocados por valores falsos e o original não é alterado.",
            a.harness
        ))?
    } else {
        bail!(
            "primeira execução do {} sem terminal interativo: passe --import para importar a configuração existente ou --no-import para começar vazio",
            a.harness
        );
    };
    let setup = home::ensure(host, layout, &a.harness)?;
    setup.warnings.iter().for_each(warn);
    if import {
        do_import(host, layout, &a.harness, false)?;
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
        bail!("só o hermes tem gateway: use `sandmux run hermes --gateway`");
    }
    // Recusa cedo um harness não instalado, antes de criar qualquer coisa.
    presets::plan(&a.harness, &host, &[], &[])?;
    let opts = launch_options(&host, &a);
    // Configuração inválida e perfil inexistente também recusam cedo.
    let effective = launch::effective(&host, &opts)?;
    let layout = Layout::new(&host.data_dir);
    first_run(&host, &layout, &a)?;
    if a.gateway {
        if let Some(w) = sandmux_policy::hermes_config::host_gateway_alive(&host.home) {
            warn(&w);
        }
        // O gateway vive no daemon, com ou sem terminal.
        if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
            return sessions::create_detached(opts, &effective.pass_env_names());
        }
    }
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return sessions::run_session(opts, &effective.pass_env_names());
    }
    let prepared = launch::prepare(&host, &opts)?;
    prepared.warnings.iter().for_each(warn);
    Err(prepared.exec())
}
