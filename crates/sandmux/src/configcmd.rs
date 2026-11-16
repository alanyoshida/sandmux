//! `sandmux config`: mostrar, listar e alterar o arquivo de configuração.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use sandmux_policy::Host;
use sandmux_policy::config::{self, Config, Effective, Layer, Origin, Request, edit};
use sandmux_spec::{tr, trf};

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
    /// Define uma chave de liga e desliga (repo, kvm, gpu, clipboard) ou um teto
    /// (memory, processes).
    Set {
        chave: String,
        /// true ou false (também sim ou não); nos tetos, 50%, 16G, 512 ou max.
        valor: String,
        #[command(flatten)]
        alvo: Alvo,
    },
    /// Apaga uma chave de liga e desliga ou um teto.
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
        let abs = abs.canonicalize().map_err(|e| {
            anyhow!(trf!(
                "pasta do projeto {}: {e}",
                "project folder {}: {e}",
                abs.display()
            ))
        })?;
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
            let op = if chave == config::WINDOW_KEY {
                // `true`, `false` (ou sim, não) e `direct`.
                let v = match edit::parse_bool(&valor) {
                    Some(b) => b.to_string(),
                    None => valor.trim().to_lowercase(),
                };
                edit::Op::SetLimit(chave, v)
            } else if config::LIMIT_KEYS.contains(&chave.as_str())
                || config::NET_KEYS.contains(&chave.as_str())
            {
                edit::Op::SetLimit(chave, valor)
            } else {
                let v = edit::parse_bool(&valor).ok_or_else(|| {
                    anyhow!(trf!(
                        "valor inválido para `{chave}`: {valor:?}. Use true ou false",
                        "invalid value for `{chave}`: {valor:?}. Use true or false"
                    ))
                })?;
                edit::Op::Set(chave, v)
            };
            change(&host, &path, &alvo, op)
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
        (true, _) => eprintln!(
            "{}",
            trf!(
                "sandmux: {} atualizado em {target}",
                "sandmux: {} updated in {target}",
                path.display()
            )
        ),
        (false, Some(n)) => eprintln!(
            "{}",
            trf!("sandmux: nada mudou: {n}", "sandmux: nothing changed: {n}")
        ),
        (false, None) => eprintln!("{}", tr!("sandmux: nada mudou", "sandmux: nothing changed")),
    }
    if out.changed
        && let Some(ids) = live_sessions()
        && !ids.is_empty()
    {
        let list = ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "{}",
            trf!(
                "sandmux: as sessões vivas ({list}) continuam com o valor antigo; `sandmux restart <id> --force` relança cada uma",
                "sandmux: the live sessions ({list}) keep the old value; `sandmux restart <id> --force` relaunches each one"
            )
        );
    }
    Ok(0)
}

/// As sessões vivas, quando o daemon já está rodando (sem subir um).
fn live_sessions() -> Option<Vec<u32>> {
    let mut c = sandmux::client::Conn::connect().ok()??;
    match c.call(&sandmux_proto::Msg::List).ok()? {
        sandmux_proto::Msg::Sessions { sessions, .. } => Some(
            sessions
                .iter()
                .filter(|s| s.state == sandmux_proto::SessionState::Viva)
                .map(|s| s.id)
                .collect(),
        ),
        _ => None,
    }
}

fn file_line(path: &Path) -> String {
    if path.exists() {
        trf!("Arquivo: {}", "File: {}", path.display())
    } else {
        trf!(
            "Arquivo: {} (não existe)",
            "File: {} (does not exist)",
            path.display()
        )
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
            abs.canonicalize().map_err(|e| {
                anyhow!(trf!(
                    "pasta de trabalho {}: {e}",
                    "work folder {}: {e}",
                    abs.display()
                ))
            })?
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
    println!(
        "{}",
        trf!("Pasta de trabalho: {}", "Work folder: {}", wd.display())
    );
    match &eff.project {
        Some(p) => println!(
            "{}",
            trf!("Perfil do projeto: {}", "Project profile: {}", p.display())
        ),
        None => println!(
            "{}",
            tr!("Perfil do projeto: nenhum", "Project profile: none")
        ),
    }
    if let Some(p) = &eff.profile {
        println!("{}", trf!("Perfil nomeado: {p}", "Named profile: {p}"));
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
    rows.push([
        "window".into(),
        eff.window.value.to_string(),
        eff.window.origin.to_string(),
    ]);
    rows.push([
        "memory".into(),
        eff.memory.value.to_string(),
        eff.memory.origin.to_string(),
    ]);
    rows.push([
        "processes".into(),
        eff.processes.value.to_string(),
        eff.processes.origin.to_string(),
    ]);
    rows.push([
        "network".into(),
        {
            let v = eff.network.value.to_string();
            let l = sandmux_policy::config::value_label("network", &v);
            if l == v { v } else { format!("{l} ({v})") }
        },
        eff.network.origin.to_string(),
    ]);
    let mut list = |key: &str, items: Vec<(String, &Origin)>| {
        if items.is_empty() {
            rows.push([
                key.into(),
                tr!("(nenhum)", "(none)").into(),
                Origin::Default.to_string(),
            ]);
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
    for (k, v) in [
        ("file-deny", &eff.file_deny),
        ("file-allow", &eff.file_allow),
        ("secret-files", &eff.secret_files),
        ("secret-files-off", &eff.secret_files_off),
    ] {
        if !v.is_empty() {
            rows.push([
                k.into(),
                v.join(", "),
                tr!("somada das camadas", "summed across layers").into(),
            ]);
        }
    }
    if eff.ssh_keys.is_empty() {
        rows.push([
            "ssh-keys".into(),
            tr!("(nenhuma)", "(none)").into(),
            Origin::Default.to_string(),
        ]);
    } else {
        rows.push([
            "ssh-keys".into(),
            eff.ssh_keys.join(", "),
            tr!("somada das camadas", "summed across layers").into(),
        ]);
    }
    // Regras de rede das camadas; os catálogos num resumo (o detalhe está em
    // `sandmux net rules`).
    use sandmux_spec::net::RuleKind;
    for (key, kind) in [("deny", RuleKind::Deny), ("allow", RuleKind::Allow)] {
        let mine: Vec<_> = eff
            .net_rules
            .iter()
            .filter(|r| r.kind == kind && !r.origin.starts_with("catálogo"))
            .collect();
        if mine.is_empty() {
            rows.push([
                key.into(),
                tr!("(nenhum)", "(none)").into(),
                Origin::Default.to_string(),
            ]);
        }
        for r in mine {
            let v = if r.enabled {
                r.pattern.to_string()
            } else {
                trf!("{} (desligado)", "{} (off)", r.pattern)
            };
            rows.push([
                key.into(),
                v,
                trf!("modificada ({})", "modified ({})", r.origin),
            ]);
        }
    }
    let count = |origin: &dyn Fn(&str) -> bool| {
        let all: Vec<_> = eff.net_rules.iter().filter(|r| origin(&r.origin)).collect();
        (all.iter().filter(|r| r.enabled).count(), all.len())
    };
    let (ex_on, ex_all) = count(&|o| o == sandmux_policy::netcatalog::EXFIL_ORIGIN);
    let (inf_on, inf_all) = count(&|o| o == sandmux_policy::netcatalog::INFERENCE_ORIGIN);
    let groups: Vec<&str> = sandmux_policy::netcatalog::COMMON
        .iter()
        .filter(|(g, _)| {
            eff.net_rules
                .iter()
                .any(|r| r.enabled && r.origin == sandmux_policy::netcatalog::common_origin(g))
        })
        .map(|(g, _)| *g)
        .collect();
    let default = ex_on == ex_all && inf_on == inf_all && groups.is_empty();
    rows.push([
        tr!("catálogos", "catalogs").into(),
        trf!(
            "exfiltração {ex_on}/{ex_all}, inferência {inf_on}/{inf_all}, comuns: {}",
            "exfiltration {ex_on}/{ex_all}, inference {inf_on}/{inf_all}, common: {}",
            if groups.is_empty() {
                tr!("nenhum", "none").to_string()
            } else {
                groups.join(", ")
            }
        ),
        if default {
            Origin::Default.to_string()
        } else {
            tr!("modificada", "modified").into()
        },
    ]);
    rows
}

fn print_table(rows: &[[String; 3]]) {
    let head = [
        tr!("CHAVE", "KEY"),
        tr!("VALOR", "VALUE"),
        tr!("ORIGEM", "ORIGIN"),
    ];
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
            println!("    {}", tr!("(vazio)", "(empty)"));
        }
        for k in keys {
            println!("    {k}");
        }
    };
    if cfg.profiles.is_empty() {
        println!("{}", tr!("Perfis: nenhum", "Profiles: none"));
    } else {
        println!("{}", tr!("Perfis:", "Profiles:"));
        for (n, l) in &cfg.profiles {
            section(n, l);
        }
    }
    if cfg.projects.is_empty() {
        println!("{}", tr!("Projetos: nenhum", "Projects: none"));
    } else {
        println!("{}", tr!("Projetos:", "Projects:"));
        for (p, l) in &cfg.projects {
            let name = if p.is_dir() {
                p.display().to_string()
            } else {
                trf!(
                    "{} (pasta não existe)",
                    "{} (folder does not exist)",
                    p.display()
                )
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
