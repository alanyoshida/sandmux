//! `sandmux config`: mostrar, listar e alterar o arquivo de configuração.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use sandmux_policy::Host;
use sandmux_policy::config::{self, Config, Effective, Layer, Origin, Request, edit};

#[derive(Subcommand)]
pub enum ConfigCmd {
    /// Mostra a configuração efetiva para a pasta atual, com a origem de cada valor.
    Show {
        /// Pasta de trabalho considerada, no lugar da pasta atual.
        #[arg(long, value_name = "PASTA")]
        workdir: Option<PathBuf>,
        /// Perfil nomeado considerado.
        #[arg(long, value_name = "NOME")]
        profile: Option<String>,
    },
    /// Lista os perfis nomeados e as pastas com perfil de projeto.
    List,
    /// Define uma chave de liga e desliga (repo, kvm, gpu, clipboard).
    Set {
        chave: String,
        /// true ou false (também sim ou não).
        valor: String,
        #[command(flatten)]
        alvo: Alvo,
    },
    /// Apaga uma chave de liga e desliga.
    Unset {
        chave: String,
        #[command(flatten)]
        alvo: Alvo,
    },
    /// Acrescenta um item a uma lista (ro, rw, pass-env).
    Add {
        chave: String,
        item: String,
        #[command(flatten)]
        alvo: Alvo,
    },
    /// Retira um item de uma lista.
    Remove {
        chave: String,
        item: String,
        #[command(flatten)]
        alvo: Alvo,
    },
}

/// Onde a mudança é gravada: exatamente um dos três.
#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct Alvo {
    /// Na seção [global], que vale para toda execução.
    #[arg(long)]
    global: bool,
    /// No perfil nomeado [profile.NOME].
    #[arg(long, value_name = "NOME")]
    profile: Option<String>,
    /// No perfil do projeto da pasta (sem pasta, a atual).
    #[arg(long, value_name = "PASTA", num_args = 0..=1, default_missing_value = ".")]
    project: Option<PathBuf>,
}

impl Alvo {
    fn target(&self, host: &Host) -> Result<edit::Target> {
        if self.global {
            return Ok(edit::Target::Global);
        }
        if let Some(n) = &self.profile {
            return Ok(edit::Target::Profile(n.clone()));
        }
        let p = self.project.as_ref().expect("grupo obrigatório");
        let abs = sandmux::launch::absolute(p, &host.cwd);
        let abs = abs
            .canonicalize()
            .map_err(|e| anyhow!("pasta do projeto {}: {e}", abs.display()))?;
        Ok(edit::Target::Project(abs))
    }
}

pub fn run(cmd: ConfigCmd) -> Result<i32> {
    let host = Host::detect()?;
    let path = config::path(&host);
    match cmd {
        ConfigCmd::Show { workdir, profile } => show(&host, &path, workdir, profile),
        ConfigCmd::List => list(&path),
        ConfigCmd::Set { chave, valor, alvo } => {
            let v = edit::parse_bool(&valor).ok_or_else(|| {
                anyhow!("valor inválido para `{chave}`: {valor:?}. Use true ou false")
            })?;
            change(&host, &path, &alvo, edit::Op::Set(chave, v))
        }
        ConfigCmd::Unset { chave, alvo } => change(&host, &path, &alvo, edit::Op::Unset(chave)),
        ConfigCmd::Add { chave, item, alvo } => {
            change(&host, &path, &alvo, edit::Op::Add(chave, item))
        }
        ConfigCmd::Remove { chave, item, alvo } => {
            change(&host, &path, &alvo, edit::Op::Remove(chave, item))
        }
    }
}

fn change(host: &Host, path: &Path, alvo: &Alvo, op: edit::Op) -> Result<i32> {
    let target = alvo.target(host)?;
    let out = edit::apply(path, host, &target, &op)?;
    match (out.changed, out.notice) {
        (true, _) => eprintln!("sandmux: {} atualizado em {target}", path.display()),
        (false, Some(n)) => eprintln!("sandmux: nada mudou: {n}"),
        (false, None) => eprintln!("sandmux: nada mudou"),
    }
    Ok(0)
}

fn file_line(path: &Path) -> String {
    if path.exists() {
        format!("Arquivo: {}", path.display())
    } else {
        format!("Arquivo: {} (não existe)", path.display())
    }
}

fn show(
    host: &Host,
    path: &Path,
    workdir: Option<PathBuf>,
    profile: Option<String>,
) -> Result<i32> {
    let cfg = Config::load(path)?;
    let wd = match workdir {
        Some(w) => {
            let abs = sandmux::launch::absolute(&w, &host.cwd);
            abs.canonicalize()
                .map_err(|e| anyhow!("pasta de trabalho {}: {e}", abs.display()))?
        }
        None => host.cwd.clone(),
    };
    let run = Layer::default();
    let eff = config::resolve(
        &cfg,
        host,
        &Request {
            harness: "shell",
            workdir: &wd,
            profile: profile.as_deref(),
            run: &run,
        },
    )?;
    println!("{}", file_line(path));
    println!("Pasta de trabalho: {}", wd.display());
    match &eff.project {
        Some(p) => println!("Perfil do projeto: {}", p.display()),
        None => println!("Perfil do projeto: nenhum"),
    }
    if let Some(p) = &eff.profile {
        println!("Perfil nomeado: {p}");
    }
    println!();
    print_table(&rows(&eff));
    Ok(0)
}

fn rows(eff: &Effective) -> Vec<[String; 3]> {
    let mut rows = Vec::new();
    for k in config::BOOL_KEYS {
        let s = eff.setting(k).expect("chave de liga e desliga");
        rows.push([k.to_string(), s.value.to_string(), s.origin.to_string()]);
    }
    let mut list = |key: &str, items: Vec<(String, &Origin)>| {
        if items.is_empty() {
            rows.push([key.into(), "(nenhum)".into(), Origin::Default.to_string()]);
        }
        for (v, o) in items {
            rows.push([key.into(), v, o.to_string()]);
        }
    };
    let extras = |w: bool| -> Vec<(String, &Origin)> {
        eff.extras
            .iter()
            .filter(|e| e.writable == w)
            .map(|e| (e.path.display().to_string(), &e.origin))
            .collect()
    };
    list("ro", extras(false));
    list("rw", extras(true));
    list(
        "pass-env",
        eff.pass_env.iter().map(|(n, o)| (n.clone(), o)).collect(),
    );
    rows
}

fn print_table(rows: &[[String; 3]]) {
    let head = ["CHAVE", "VALOR", "ORIGEM"];
    let mut w = head.map(|h| h.chars().count());
    for r in rows {
        for i in 0..2 {
            w[i] = w[i].max(r[i].chars().count());
        }
    }
    let line = |r: [&str; 3]| {
        println!(
            "{:<w0$}  {:<w1$}  {}",
            r[0],
            r[1],
            r[2],
            w0 = w[0],
            w1 = w[1]
        );
    };
    line(head);
    for r in rows {
        line([&r[0], &r[1], &r[2]]);
    }
}

fn list(path: &Path) -> Result<i32> {
    let cfg = Config::load(path)?;
    println!("{}", file_line(path));
    let section = |name: &str, l: &Layer| {
        println!("  {name}");
        let keys = l.describe();
        if keys.is_empty() {
            println!("    (vazio)");
        }
        for k in keys {
            println!("    {k}");
        }
    };
    if cfg.profiles.is_empty() {
        println!("Perfis: nenhum");
    } else {
        println!("Perfis:");
        for (n, l) in &cfg.profiles {
            section(n, l);
        }
    }
    if cfg.projects.is_empty() {
        println!("Projetos: nenhum");
    } else {
        println!("Projetos:");
        for (p, l) in &cfg.projects {
            let name = if p.is_dir() {
                p.display().to_string()
            } else {
                format!("{} (pasta não existe)", p.display())
            };
            section(&name, l);
        }
    }
    if !cfg.global.is_empty() {
        println!("Global:");
        for k in cfg.global.describe() {
            println!("    {k}");
        }
    }
    Ok(0)
}
