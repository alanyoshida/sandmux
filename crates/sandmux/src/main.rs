//! `sandmux`: linha de comando do multiplexador: `run`, os comandos de
//! sessão, `import`, `doctor` e `daemon`.

mod configcmd;
mod doctor;
mod sessions;

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
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sobe um harness confinado na pasta atual. Num terminal, cria uma sessão
    /// no daemon e anexa a ela (Ctrl+b d desanexa, Ctrl+b Ctrl+b envia Ctrl+b);
    /// sem terminal, roda direto, em primeiro plano.
    Run(RunArgs),
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
    /// Mostra se a máquina tem o suporte necessário e o que falta.
    Doctor,
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
        Cmd::Run(a) => cmd_run(a),
        Cmd::Ls => sessions::ls(),
        Cmd::Attach { id } => sessions::attach(id),
        Cmd::Detach { id } => sessions::detach(id),
        Cmd::Kill { id } => sessions::kill(id),
        Cmd::Restart { id } => sessions::restart(id),
        Cmd::Daemon(c) => match c {
            DaemonCmd::Status => sessions::daemon_status(),
            DaemonCmd::Stop => sessions::daemon_stop(),
            DaemonCmd::Restart => sessions::daemon_restart(),
            DaemonCmd::Disable => sessions::daemon_disable(),
        },
        Cmd::Import(a) => cmd_import(a).map(|_| 0),
        Cmd::Config(c) => configcmd::run(c),
        Cmd::Doctor => doctor::run(),
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("sandmux: erro: {e:#}");
            ExitCode::from(1)
        }
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
        args: a.args.clone(),
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
    // Recusa cedo um harness não instalado, antes de criar qualquer coisa.
    presets::plan(&a.harness, &host, &[], &[])?;
    let opts = launch_options(&host, &a);
    // Configuração inválida e perfil inexistente também recusam cedo.
    let effective = launch::effective(&host, &opts)?;
    let layout = Layout::new(&host.data_dir);
    first_run(&host, &layout, &a)?;
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return sessions::run_session(opts, &effective.pass_env_names());
    }
    let prepared = launch::prepare(&host, &opts)?;
    prepared.warnings.iter().for_each(warn);
    Err(prepared.exec())
}
