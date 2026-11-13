//! Preparo de um lançamento: verificação do suporte, trava da camada,
//! composição da especificação e memfd. Usado pelo `run` sem terminal e pelo
//! daemon, para que a política seja composta num lugar só.

use std::fs::File;
use std::io::{Seek, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use sandmux_launcher::support::{self, SupportReport};
use sandmux_policy::{Host, LayerMode, Layout, RunOptions, compose, home};
use sandmux_proto::LaunchOptions;

/// Caminho de um binário irmão (na mesma pasta do executável atual), com o
/// nome puro como recuo, para ser procurado no `PATH`.
pub fn sibling(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// O lançador `sandmux-run`.
pub fn runner_path() -> PathBuf {
    sibling("sandmux-run")
}

/// Motivo curto de uma máquina sem suporte.
pub fn short_reason(r: &SupportReport) -> String {
    if !r.landlock_ok() {
        return format!(
            "Landlock ABI {} encontrado, exige {}",
            r.landlock_abi,
            sandmux_launcher::REQUIRED_ABI
        );
    }
    match &r.userns {
        Err(e) => e.clone(),
        Ok(()) => "motivo desconhecido".into(),
    }
}

/// Tenta a trava da camada de escrita. O descritor sai com `CLOEXEC`; quem
/// lança o libera só no processo do lançador, que mantém a trava enquanto o
/// sandbox vive.
fn try_layer_lock(path: &Path) -> Result<Option<File>> {
    if let Some(parent) = path.parent() {
        home::private_dir(parent)?;
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("trava {}", path.display()))?;
    // SAFETY: flock sobre um descritor válido.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Ok(None);
    }
    Ok(Some(f))
}

/// Um lançamento pronto: a especificação num memfd e a trava da camada.
#[derive(Debug)]
pub struct Prepared {
    spec: File,
    lock: Option<File>,
    pub warnings: Vec<String>,
    pub runner: PathBuf,
}

impl Prepared {
    /// Descritores que o lançador precisa herdar.
    pub fn inherited_fds(&self) -> Vec<RawFd> {
        let mut v = vec![self.spec.as_raw_fd()];
        if let Some(l) = &self.lock {
            v.push(l.as_raw_fd());
        }
        v
    }

    /// O comando do lançador. Quem o executa precisa liberar
    /// [`Prepared::inherited_fds`] no processo filho.
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.runner);
        c.arg("--spec-fd").arg(self.spec.as_raw_fd().to_string());
        c
    }

    /// Substitui o processo atual pelo lançador.
    pub fn exec(self) -> anyhow::Error {
        for fd in self.inherited_fds() {
            // SAFETY: fcntl sobre descritores válidos deste processo.
            unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
        }
        let err = self.command().exec();
        anyhow!("executar {}: {err}", self.runner.display())
    }
}

/// Prepara o lançamento de `opts` para `host`. O HOME sintético é criado
/// vazio se não existir (a pergunta da importação é feita antes, por quem
/// tem terminal).
pub fn prepare(host: &Host, opts: &LaunchOptions) -> Result<Prepared> {
    let layout = Layout::new(&host.data_dir);
    let mut warnings = Vec::new();
    if !layout.home(&opts.harness).exists() {
        let setup = home::ensure(host, &layout, &opts.harness)?;
        warnings.extend(setup.warnings);
    }
    if opts.harness == "hermes" {
        sandmux_policy::hermes_config::ensure_update_check_off(&layout.home("hermes"))?;
    }

    let runner = runner_path();
    let report = support::check_via(&runner);
    if !report.can_launch() {
        bail!(
            "esta máquina não permite lançar o sandbox ({}); rode `sandmux doctor`",
            short_reason(&report)
        );
    }
    // SANDMUX_DISABLE_OVERLAY só desliga a camada (modo de recuo): nunca afrouxa.
    let overlay_ok =
        report.overlay.is_ok() && std::env::var_os("SANDMUX_DISABLE_OVERLAY").is_none();
    let lock = if overlay_ok {
        try_layer_lock(&layout.layer_lock(&opts.harness))?
    } else {
        None
    };
    let mode = match (overlay_ok, &lock) {
        (false, _) => LayerMode::ReadOnlyFallback,
        (true, Some(_)) => LayerMode::Persistent,
        (true, None) => LayerMode::Temporary,
    };
    let run = RunOptions {
        harness: opts.harness.clone(),
        workdir: Some(opts.workdir.clone()),
        ro: opts.ro.clone(),
        rw: opts.rw.clone(),
        pass_env: opts.pass_env.clone(),
        repo: opts.repo,
        kvm: opts.kvm,
        no_gpu: opts.no_gpu,
        args: opts.args.clone(),
        layer: mode,
    };
    let composed = compose(host, &run)?;
    warnings.extend(composed.warnings);
    for d in &composed.dirs_to_create {
        if d.starts_with(&host.data_dir) {
            home::private_dir(d)?;
        } else {
            std::fs::create_dir_all(d).with_context(|| format!("criar {}", d.display()))?;
        }
    }

    let fd = nix::sys::memfd::memfd_create(c"sandmux-spec", nix::sys::memfd::MFdFlags::MFD_CLOEXEC)
        .map_err(|e| anyhow!("memfd: {e}"))?;
    let mut spec = File::from(fd);
    spec.write_all(composed.spec.to_json().as_bytes())?;
    spec.rewind()?;
    Ok(Prepared {
        spec,
        lock,
        warnings,
        runner,
    })
}

/// Caminho absoluto, relativo a `base` quando preciso. Não resolve links nem
/// exige existência: a composição confere e dá o erro certo.
pub fn absolute(p: &Path, base: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}
