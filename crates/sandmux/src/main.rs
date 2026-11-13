//! `sandmux`: linha de comando do multiplexador. Nesta fatia, `run`, `import`
//! e `doctor`.

mod doctor;

use std::io::{BufRead, IsTerminal, Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use sandmux_policy::{Host, LayerMode, Layout, RunOptions, compose, home, presets};

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
    /// Sobe um harness confinado na pasta atual.
    Run(RunArgs),
    /// Copia a configuração existente de um harness para o HOME sintético.
    Import(ImportArgs),
    /// Mostra se a máquina tem o suporte necessário e o que falta.
    Doctor,
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
    /// Numa subpasta de repositório, monta o repositório inteiro em somente leitura.
    #[arg(long)]
    repo: bool,
    /// Libera /dev/kvm dentro do sandbox.
    #[arg(long)]
    kvm: bool,
    /// Não expõe a GPU dentro do sandbox.
    #[arg(long = "no-gpu")]
    no_gpu: bool,
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
        Cmd::Import(a) => cmd_import(a).map(|_| 0),
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

pub(crate) fn runner_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sandmux-run")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("sandmux-run"))
}

/// Tenta a trava da camada de escrita. O descritor fica aberto e é herdado
/// pelo lançador, que mantém a trava enquanto o sandbox vive.
fn try_layer_lock(path: &Path) -> Result<Option<std::fs::File>> {
    if let Some(parent) = path.parent() {
        home::private_dir(parent)?;
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("trava {}", path.display()))?;
    // SAFETY: flock e fcntl sobre um descritor válido.
    unsafe {
        if libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 {
            return Ok(None);
        }
        libc::fcntl(f.as_raw_fd(), libc::F_SETFD, 0);
    }
    Ok(Some(f))
}

fn cmd_run(a: RunArgs) -> Result<i32> {
    let host = Host::detect()?;
    if !presets::exists(&a.harness) {
        return Err(sandmux_policy::PolicyError::UnknownHarness(a.harness).into());
    }
    // Recusa cedo um harness não instalado, antes de criar qualquer coisa.
    presets::plan(&a.harness, &host, &[], &[])?;
    let layout = Layout::new(&host.data_dir);
    first_run(&host, &layout, &a)?;
    if a.harness == "hermes" {
        sandmux_policy::hermes_config::ensure_update_check_off(&layout.home("hermes"))?;
    }

    let support = sandmux_launcher::support::check_via(&runner_path());
    if !support.can_launch() {
        bail!(
            "esta máquina não permite lançar o sandbox ({}); rode `sandmux doctor`",
            doctor::short_reason(&support)
        );
    }
    // SANDMUX_DISABLE_OVERLAY só desliga a camada (modo de recuo): nunca afrouxa.
    let overlay_ok =
        support.overlay.is_ok() && std::env::var_os("SANDMUX_DISABLE_OVERLAY").is_none();
    let lock = if overlay_ok {
        try_layer_lock(&layout.layer_lock(&a.harness))?
    } else {
        None
    };
    let mode = match (overlay_ok, &lock) {
        (false, _) => LayerMode::ReadOnlyFallback,
        (true, Some(_)) => LayerMode::Persistent,
        (true, None) => LayerMode::Temporary,
    };
    let opts = RunOptions {
        harness: a.harness.clone(),
        workdir: a.workdir.clone(),
        ro: a.ro.clone(),
        rw: a.rw.clone(),
        pass_env: a.pass_env.clone(),
        repo: a.repo,
        kvm: a.kvm,
        no_gpu: a.no_gpu,
        args: a.args.clone(),
        layer: mode,
    };
    let composed = compose(&host, &opts)?;
    for w in &composed.warnings {
        warn(w);
    }
    for d in &composed.dirs_to_create {
        if d.starts_with(&host.data_dir) {
            home::private_dir(d)?;
        } else {
            std::fs::create_dir_all(d).with_context(|| format!("criar {}", d.display()))?;
        }
    }

    let fd = nix::sys::memfd::memfd_create(c"sandmux-spec", nix::sys::memfd::MFdFlags::empty())
        .map_err(|e| anyhow!("memfd: {e}"))?;
    let mut spec_file = std::fs::File::from(fd);
    spec_file.write_all(composed.spec.to_json().as_bytes())?;
    spec_file.rewind()?;
    let runner = runner_path();
    let err = Command::new(&runner)
        .arg("--spec-fd")
        .arg(spec_file.as_raw_fd().to_string())
        .exec();
    drop(lock);
    Err(anyhow!("executar {}: {err}", runner.display()))
}
