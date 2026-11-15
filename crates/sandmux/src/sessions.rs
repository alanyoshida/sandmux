//! Comandos de sessão e do daemon no cliente.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sandmux::client::{self, Conn, systemd};
use sandmux::paths;
use sandmux_policy::limits::human;
use sandmux_proto::{LaunchOptions, Msg, SessionInfo};
use sandmux_spec::{tr, trf};

fn need_terminal(what: &str) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!(trf!(
            "{what} precisa de um terminal interativo",
            "{what} needs an interactive terminal"
        ));
    }
    Ok(())
}

/// `run` com terminal: cria a sessão no daemon e anexa. `pass_env` são os
/// nomes da política efetiva, cujos valores vão ao daemon.
pub fn run_session(opts: LaunchOptions, pass_env: &[String], group: Option<String>) -> Result<i32> {
    let vars = client::client_vars(pass_env)?;
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Create {
        opts: Box::new(opts),
        vars,
        size: client::terminal_size(),
        attach: true,
        group,
    })?;
    client::attached(conn)
}

/// Cria a sessão no daemon sem anexar (o gateway sem terminal) e mostra o
/// identificador.
pub fn create_detached(
    opts: LaunchOptions,
    pass_env: &[String],
    group: Option<String>,
) -> Result<i32> {
    let vars = client::client_vars(pass_env)?;
    let mut conn = client::connect_or_start()?;
    match conn.call(&Msg::Create {
        opts: Box::new(opts),
        vars,
        size: client::terminal_size(),
        attach: false,
        group,
    })? {
        Msg::Created { id, name } => {
            println!(
                "{}",
                trf!("sessão {id} criada: {name}", "session {id} created: {name}")
            );
            Ok(0)
        }
        m => Err(anyhow!(trf!(
            "resposta inesperada do daemon: {m:?}",
            "unexpected reply from the daemon: {m:?}"
        ))),
    }
}

fn list(conn: &mut Conn) -> Result<Vec<SessionInfo>> {
    match conn.call(&Msg::List)? {
        Msg::Sessions { sessions } => Ok(sessions),
        m => Err(anyhow!(trf!(
            "resposta inesperada do daemon: {m:?}",
            "unexpected reply from the daemon: {m:?}"
        ))),
    }
}

/// Estado com a nota de memória esgotada.
fn state_text(s: &SessionInfo) -> String {
    let base = state_base(s);
    if s.keys.any() {
        format!("{base} · {}", s.keys)
    } else {
        base
    }
}

fn state_base(s: &SessionInfo) -> String {
    match &s.state {
        sandmux_proto::SessionState::Encerrada { codigo } if s.oom_kills > 0 => {
            trf!(
                "encerrada ({codigo}, memória esgotada)",
                "ended ({codigo}, out of memory)"
            )
        }
        other => other.to_string(),
    }
}

fn of_max(cur: String, max: Option<String>) -> String {
    format!(
        "{cur}/{}",
        max.unwrap_or_else(|| tr!("sem teto", "no cap").into())
    )
}

/// `sandmux update`: roda o atualizador oficial do harness no host, fora do
/// sandbox e fora do daemon, em primeiro plano, com o terminal do usuário.
pub fn update(harness: String, yes: bool, args: Vec<String>) -> Result<i32> {
    use std::io::{BufRead, Write};
    let host = sandmux_policy::Host::detect()?;
    let cmd = sandmux_policy::presets::updater(&harness, &host, &args)?;
    // Só pergunta ao daemon que já está rodando: sem daemon, não há abas.
    let live: Vec<SessionInfo> = match Conn::connect()? {
        Some(mut c) => list(&mut c)?
            .into_iter()
            .filter(|s| {
                s.harness == harness
                    && matches!(
                        s.state,
                        sandmux_proto::SessionState::Viva
                            | sandmux_proto::SessionState::Reiniciando { .. }
                    )
            })
            .collect(),
        None => vec![],
    };
    let tabs = live
        .iter()
        .map(|s| format!("{} ({})", s.id, s.name))
        .collect::<Vec<_>>()
        .join(", ");
    if !live.is_empty() {
        eprintln!(
            "{}",
            trf!(
                "sandmux: abas vivas do {harness}: {tabs}. O atualizador troca arquivos que elas podem estar usando.",
                "sandmux: live {harness} tabs: {tabs}. The updater replaces files they may be using."
            )
        );
        if !yes {
            if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
                bail!(trf!(
                    "há abas vivas do {harness}; sem terminal, rode de novo com --yes para atualizar mesmo assim",
                    "there are live {harness} tabs; without a terminal, run again with --yes to update anyway"
                ));
            }
            eprint!(
                "{} {} ",
                tr!("Atualizar mesmo assim?", "Update anyway?"),
                tr!("[s/N]", "[y/N]")
            );
            std::io::stderr().flush().ok();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            let t = line.trim().to_lowercase();
            if !matches!(t.as_str(), "s" | "sim" | "y" | "yes") {
                eprintln!(
                    "{}",
                    tr!(
                        "sandmux: nada foi atualizado",
                        "sandmux: nothing was updated"
                    )
                );
                return Ok(1);
            }
        }
    }
    eprintln!(
        "{}",
        trf!(
            "sandmux: rodando fora do sandbox: {}",
            "sandmux: running outside the sandbox: {}",
            cmd.join(" ")
        )
    );
    let status = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .status()
        .map_err(|e| anyhow!("{}: {e}", cmd[0]))?;
    let code = {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
    };
    if status.success() && !live.is_empty() {
        let ids = live
            .iter()
            .map(|s| s.id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "{}",
            trf!(
                "sandmux: reinicie as abas {ids} para usar a versão nova (Ctrl+b r na TUI, ou sandmux restart <id>)",
                "sandmux: restart tabs {ids} to use the new version (Ctrl+b r in the TUI, or sandmux restart <id>)"
            )
        );
    }
    Ok(code)
}

/// `sandmux group list|move|rename`.
pub fn group(cmd: GroupCmd) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    match cmd {
        GroupCmd::List => {
            let sessions = list(&mut conn)?;
            let mut groups: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            groups.insert(sandmux_proto::msg::main_group(), 0);
            for s in &sessions {
                *groups.entry(s.group.clone()).or_default() += 1;
            }
            let w = groups.keys().map(|g| g.chars().count()).max().unwrap_or(0);
            for (g, n) in groups {
                println!("{g:<w$}  {}", trf!("{n} aba(s)", "{n} tab(s)"));
            }
        }
        GroupCmd::Move { id, grupo } => {
            sandmux_proto::msg::check_group(&grupo).map_err(|e| anyhow!(e))?;
            conn.call(&Msg::SetGroup {
                id,
                group: grupo.clone(),
            })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: sessão {id} no grupo {grupo}",
                    "sandmux: session {id} in group {grupo}"
                )
            );
        }
        GroupCmd::Rename { grupo, novo } => {
            sandmux_proto::msg::check_group(&novo).map_err(|e| anyhow!(e))?;
            conn.call(&Msg::RenameGroup {
                from: grupo.clone(),
                to: novo.clone(),
            })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: grupo {grupo} renomeado para {novo}",
                    "sandmux: group {grupo} renamed to {novo}"
                )
            );
        }
    }
    Ok(0)
}

/// Grupos de abas (protocolo 30).
#[derive(clap::Subcommand)]
pub enum GroupCmd {
    /// Lista os grupos, com o número de abas.
    List,
    /// Leva uma sessão para outro grupo.
    Move { id: u32, grupo: String },
    /// Renomeia um grupo (todas as sessões dele).
    Rename { grupo: String, novo: String },
}

pub fn ls(group: Option<String>) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let mut sessions = list(&mut conn)?;
    if let Some(g) = &group {
        sessions.retain(|s| &s.group == g);
    }
    if sessions.is_empty() {
        println!("{}", tr!("Não há sessões.", "No sessions."));
        return Ok(0);
    }
    let rows: Vec<[String; 8]> = sessions
        .iter()
        .map(|s| {
            let (mem, proc_) = match &s.usage {
                Some(u) => (
                    of_max(human(u.memory), u.memory_max.map(human)),
                    of_max(
                        u.processes.to_string(),
                        u.processes_max.map(|n| n.to_string()),
                    ),
                ),
                None => (String::new(), String::new()),
            };
            [
                s.id.to_string(),
                s.name.clone(),
                s.group.clone(),
                state_text(s),
                s.clients.to_string(),
                mem,
                proc_,
                s.workdir.display().to_string(),
            ]
        })
        .collect();
    let head = [
        "ID",
        tr!("NOME", "NAME"),
        tr!("GRUPO", "GROUP"),
        tr!("ESTADO", "STATE"),
        tr!("CLIENTES", "CLIENTS"),
        tr!("MEMÓRIA", "MEMORY"),
        "PROC",
        tr!("PASTA", "FOLDER"),
    ];
    let mut w = head.map(|h| h.chars().count());
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |r: [&str; 8]| {
        let mut s = String::new();
        for (i, c) in r.iter().enumerate() {
            if i == 7 {
                s.push_str(c);
            } else {
                let pad = w[i] - c.chars().count();
                s.push_str(c);
                s.push_str(&" ".repeat(pad + 2));
            }
        }
        println!("{}", s.trim_end());
    };
    line(head);
    for r in &rows {
        line([&r[0], &r[1], &r[2], &r[3], &r[4], &r[5], &r[6], &r[7]]);
    }
    Ok(0)
}

/// `sandmux stats <id>`: o detalhe do consumo de uma sessão.
pub fn stats(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let Msg::StatsOf {
        id,
        state,
        stats,
        oom_kills,
        fork_refusals,
        docker,
        policy,
        ports,
    } = conn.call(&Msg::Stats { id })?
    else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected reply from the daemon"
        ));
    };
    let state = match state {
        sandmux_proto::SessionState::Encerrada { codigo } if oom_kills > 0 => {
            trf!(
                "encerrada ({codigo}, memória esgotada)",
                "ended ({codigo}, out of memory)"
            )
        }
        other => other.to_string(),
    };
    println!("{}", trf!("Sessão {id}: {state}", "Session {id}: {state}"));
    match stats {
        Some(s) => {
            let max = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                m.map(f).unwrap_or_else(|| tr!("sem teto", "no cap").into())
            };
            println!(
                "{}",
                trf!(
                    "  memória: {} em uso, pico de {}, teto de {}",
                    "  memory: {} in use, peak of {}, cap of {}",
                    human(s.usage.memory),
                    human(s.memory_peak),
                    max(s.usage.memory_max, &human)
                )
            );
            println!(
                "{}",
                trf!(
                    "  processos: {} em uso, pico de {}, teto de {}",
                    "  processes: {} in use, peak of {}, cap of {}",
                    s.usage.processes,
                    s.processes_peak,
                    max(s.usage.processes_max, &|n| n.to_string())
                )
            );
            let secs = s.cpu_usec as f64 / 1_000_000.0;
            if sandmux_spec::lang::is_en() {
                println!("  CPU used: {secs:.1}s");
            } else {
                println!("  CPU usada: {}s", format!("{secs:.1}").replace('.', ","));
            }
        }
        None => println!(
            "{}",
            tr!(
                "  sem consumo: a sessão não está viva ou roda sem tetos",
                "  no usage: the session is not alive or runs without caps"
            )
        ),
    }
    println!(
        "{}",
        trf!(
            "  mortes por falta de memória: {oom_kills}",
            "  out-of-memory kills: {oom_kills}"
        )
    );
    println!(
        "{}",
        trf!(
            "  criações de processo recusadas pelo teto: {fork_refusals}",
            "  process creations refused by the cap: {fork_refusals}"
        )
    );
    match docker {
        Some(d) => {
            let teto = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                m.map(f).unwrap_or_else(|| tr!("sem teto", "no cap").into())
            };
            println!(
                "{}",
                trf!(
                    "Docker privado da pasta: rodando, {} contêineres",
                    "Folder's private Docker: running, {} containers",
                    d.containers
                )
            );
            println!(
                "{}",
                trf!(
                    "  memória: {} em uso, teto de {}",
                    "  memory: {} in use, cap of {}",
                    human(d.memory),
                    teto(d.memory_max, &human)
                )
            );
            println!(
                "{}",
                trf!(
                    "  processos: {} em uso, teto de {}",
                    "  processes: {} in use, cap of {}",
                    d.processes,
                    teto(d.processes_max, &|n| n.to_string())
                )
            );
        }
        None => println!(
            "{}",
            tr!(
                "Docker privado da pasta: parado",
                "Folder's private Docker: stopped"
            )
        ),
    }
    if ports.is_empty() {
        println!(
            "{}",
            tr!("Portas publicadas: nenhuma", "Published ports: none")
        );
    } else {
        println!(
            "{}",
            tr!("Portas publicadas no host:", "Ports published on the host:")
        );
        for l in port_lines(&ports) {
            println!("  {l}");
        }
    }
    if let Some(p) = policy {
        println!(
            "{}",
            tr!(
                "Proteção do último lançamento:",
                "Protection of the last launch:"
            )
        );
        for l in policy_lines(&p) {
            println!("  {l}");
        }
    }
    Ok(0)
}

/// As portas publicadas, uma por linha: `3000 → 127.0.0.1:3000 (sandbox)`.
pub fn port_lines(ports: &[sandmux_proto::msg::PortInfo]) -> Vec<String> {
    ports
        .iter()
        .map(|p| {
            let a = p.addr.unwrap_or(std::net::Ipv4Addr::LOCALHOST);
            format!("{} → {a}:{} ({})", p.guest, p.host, p.source)
        })
        .collect()
}

/// O resumo da proteção em linhas curtas, para o `stats` e o painel.
pub fn policy_lines(p: &sandmux_proto::msg::PolicySummary) -> Vec<String> {
    let yes = |b: bool| {
        if b {
            tr!("sim", "yes")
        } else {
            tr!("não", "no")
        }
    };
    let mut l = vec![
        sandmux::launch::localize_summary(&p.level),
        trf!(
            "camada de escrita: {}",
            "write layer: {}",
            sandmux::launch::localize_summary(&p.layer)
        ),
        trf!(
            "pasta: {} (escrita)",
            "folder: {} (write)",
            p.workdir.display()
        ),
    ];
    for d in &p.ro {
        l.push(trf!("extra: {} (leitura)", "extra: {} (read)", d.display()));
    }
    for d in &p.rw {
        l.push(trf!(
            "extra: {} (escrita)",
            "extra: {} (write)",
            d.display()
        ));
    }
    l.push(format!("GPU: {} · KVM: {}", yes(p.gpu), yes(p.kvm)));
    l.push(format!(
        "clipboard: {}",
        if p.clipboard {
            tr!("liberado", "allowed")
        } else {
            tr!("bloqueado", "blocked")
        }
    ));
    if p.pass_env.is_empty() {
        l.push(tr!("pass-env: nenhuma", "pass-env: none").into());
    } else {
        l.push(trf!(
            "pass-env: {} (ao alcance do agente)",
            "pass-env: {} (within the agent's reach)",
            p.pass_env.join(", ")
        ));
    }
    if p.limits {
        let m = p
            .memory_max
            .map(human)
            .unwrap_or_else(|| tr!("sem teto", "no cap").into());
        let n = p
            .processes_max
            .map(|n| n.to_string())
            .unwrap_or_else(|| tr!("sem teto", "no cap").into());
        l.push(trf!(
            "tetos: memória {m}, processos {n}",
            "caps: memory {m}, processes {n}"
        ));
    } else {
        l.push(
            tr!(
                "tetos: sem suporte, sem teto",
                "caps: not supported, no cap"
            )
            .into(),
        );
    }
    l.push(trf!(
        "rede: {}",
        "network: {}",
        sandmux::launch::localize_summary(&p.network)
    ));
    l
}

pub fn attach(id: Option<u32>) -> Result<i32> {
    need_terminal("`sandmux attach`")?;
    let mut conn = client::connect_or_start()?;
    conn.send(&Msg::Attach {
        id,
        size: client::terminal_size(),
    })?;
    // A primeira resposta é o erro ou o começo do anexo.
    let mut peek = conn.stream.try_clone()?;
    match sandmux_proto::read_frame(&mut peek)? {
        Some(sandmux_proto::Frame::Control(Msg::Error { message })) => Err(anyhow!(message)),
        Some(sandmux_proto::Frame::Control(Msg::Attached { id, name })) => {
            // Reinsere o evento já lido para o laço do modo anexado.
            client::attached_after(conn, id, name)
        }
        other => Err(anyhow!(trf!(
            "resposta inesperada do daemon: {other:?}",
            "unexpected reply from the daemon: {other:?}"
        ))),
    }
}

pub fn detach(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Detach { id })?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: sessão {id} desanexada de todos os terminais",
            "sandmux: session {id} detached from all terminals"
        )
    );
    Ok(0)
}

/// `sandmux net [<id>]`: as últimas linhas do registro de rede.
pub fn net(id: Option<u32>) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let Msg::NetLog { lines } = conn.call(&Msg::Net {
        id,
        since: None,
        limit: 50,
    })?
    else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected reply from the daemon"
        ));
    };
    if lines.is_empty() {
        match id {
            Some(id) => println!(
                "{}",
                trf!(
                    "Nenhuma conexão registrada da sessão {id} desde ontem.",
                    "No connections logged for session {id} since yesterday."
                )
            ),
            None => println!(
                "{}",
                tr!(
                    "Nenhuma conexão registrada desde ontem.",
                    "No connections logged since yesterday."
                )
            ),
        }
        return Ok(0);
    }
    for l in net_table(&lines) {
        println!("{l}");
    }
    Ok(0)
}

/// `sandmux net deny|allow|forget|mode`: muda a camada da aba de uma sessão.
pub fn net_edit(id: u32, action: &str, value: &str) -> Result<i32> {
    if action.starts_with("mcp-") {
        if value.is_empty() || value.contains(char::is_whitespace) {
            anyhow::bail!(trf!(
                "nome de servidor MCP inválido: {value:?}",
                "invalid MCP server name: {value:?}"
            ));
        }
    } else if action.starts_with("file-") {
        sandmux_policy::config::valid_file_pattern(value).map_err(|e| anyhow!(e))?;
    } else if action != "mode" {
        value
            .parse::<sandmux_spec::net::Pattern>()
            .map_err(|e| anyhow!(e))?;
    }
    let mut conn = client::connect_or_start()?;
    // Um aviso (regra que só vale no próximo lançamento) é mostrado pela
    // própria conexão.
    conn.call(&Msg::NetEdit {
        id,
        action: action.into(),
        value: value.into(),
    })?;
    let what = match action {
        "deny" => trf!("{value} negado", "{value} denied"),
        "allow" => trf!("{value} liberado", "{value} allowed"),
        "forget" | "file-forget" => trf!(
            "{value} tirado das regras",
            "{value} removed from the rules"
        ),
        "file-deny" => trf!("arquivo {value} negado", "file {value} denied"),
        "mcp-deny" => trf!("MCP {value} desabilitado", "MCP {value} disabled"),
        "mcp-allow" => trf!("MCP {value} habilitado", "MCP {value} enabled"),
        "mcp-forget" => trf!(
            "MCP {value} tirado das regras",
            "MCP {value} removed from the rules"
        ),
        "file-allow" => trf!("arquivo {value} liberado", "file {value} allowed"),
        _ => trf!("modo de rede {value}", "network mode {value}"),
    };
    eprintln!(
        "{}",
        trf!(
            "sandmux: sessão {id}: {what} na aba",
            "sandmux: session {id}: {what} in the tab"
        )
    );
    Ok(0)
}

/// Linhas de `sandmux net rules`: o modo e cada regra, com o estado e a
/// origem.
pub fn rules_lines(eff: &sandmux_policy::config::Effective) -> Vec<String> {
    use sandmux_policy::config::NetworkMode;
    use sandmux_policy::netcatalog::INFERENCE_ORIGIN;
    use sandmux_spec::net::RuleKind;
    let mode = eff.network.value;
    let mut out = vec![trf!(
        "Modo: {}; {}",
        "Mode: {}; {}",
        mode.describe(),
        eff.network.origin
    )];
    let row = |r: &sandmux_spec::net::Rule| {
        format!(
            "  {:<10} {:<36} {}",
            if r.enabled {
                tr!("ligado", "on")
            } else {
                tr!("desligado", "off")
            },
            r.pattern.to_string(),
            sandmux_policy::config::origin_text(&r.origin)
        )
    };
    let section =
        |out: &mut Vec<String>, title: String, f: &dyn Fn(&sandmux_spec::net::Rule) -> bool| {
            out.push(String::new());
            out.push(title);
            out.extend(eff.net_rules.iter().filter(|r| f(r)).map(row));
        };
    section(
        &mut out,
        tr!("Denylist (todos os modos):", "Denylist (all modes):").into(),
        &|r| r.kind == RuleKind::Deny,
    );
    let note = |m: NetworkMode| {
        if mode == m {
            ""
        } else {
            tr!(", sem efeito neste modo", ", no effect in this mode")
        }
    };
    section(
        &mut out,
        trf!(
            "Allowlist (modo custom{}):",
            "Allowlist (custom mode{}):",
            note(NetworkMode::Custom)
        ),
        &|r| r.kind == RuleKind::Allow && r.origin != INFERENCE_ORIGIN,
    );
    section(
        &mut out,
        trf!(
            "Provedores (modo só inferência{}):",
            "Providers (inference-only mode{}):",
            note(NetworkMode::Inferencia)
        ),
        &|r| r.kind == RuleKind::Allow && r.origin == INFERENCE_ORIGIN,
    );
    out
}

/// `sandmux net rules [<id>]`, calculado no cliente: com `<id>`, pelas
/// opções guardadas da sessão; sem ele, pela pasta atual.
pub fn net_rules(id: Option<u32>) -> Result<i32> {
    let mut host = sandmux_policy::Host::detect()?;
    let opts = match id {
        Some(id) => {
            let mut conn = client::connect_or_start()?;
            let sessions = list(&mut conn)?;
            let s = sessions.into_iter().find(|s| s.id == id).ok_or_else(|| {
                anyhow!(trf!(
                    "não existe a sessão {id}",
                    "session {id} does not exist"
                ))
            })?;
            s.opts
        }
        None => {
            let mut o = LaunchOptions::new("shell");
            o.workdir = host.cwd.clone();
            o
        }
    };
    host.cwd = opts.workdir.clone();
    let eff = sandmux::launch::effective(&host, &opts)?;
    match id {
        Some(id) => println!(
            "{}",
            trf!(
                "Sessão {id} ({})",
                "Session {id} ({})",
                opts.workdir.display()
            )
        ),
        None => println!("{}", trf!("Pasta {}", "Folder {}", opts.workdir.display())),
    }
    for l in rules_lines(&eff) {
        println!("{l}");
    }
    Ok(0)
}

/// Linhas da tabela de `sandmux net`.
pub fn net_table(lines: &[sandmux_proto::msg::NetLine]) -> Vec<String> {
    let rows: Vec<[String; 7]> = lines
        .iter()
        .map(|l| {
            let hora = l.ts.split('T').nth(1).unwrap_or(&l.ts).to_string();
            let sessao = l.session.map_or_else(
                || {
                    if l.tag.starts_with("run-") {
                        "run".into()
                    } else {
                        "-".into()
                    }
                },
                |s| s.to_string(),
            );
            let destino = match (&l.method, &l.path) {
                (Some(m), Some(p)) => format!(
                    "{m} {p} {}",
                    l.status.map(|s| s.to_string()).unwrap_or_default()
                ),
                _ if l.ip.is_empty() => "-".to_string(),
                _ => format!("{}:{}", l.ip, l.port),
            };
            let mut nota = match (&l.blocked, l.new) {
                (Some(b), _) => trf!(
                    "bloqueado: {}",
                    "blocked: {}",
                    sandmux_spec::net::reason_text(b)
                ),
                (None, true) => tr!("novo", "new").into(),
                _ => String::new(),
            };
            if let Some(s) = &l.secret {
                if !nota.is_empty() {
                    nota.push_str(", ");
                }
                if l.proto == "ssh-sign" {
                    nota.push_str(&trf!("assinatura ssh: {s}", "ssh signature: {s}"));
                } else {
                    nota.push_str(&trf!("chave: {s}", "key: {s}"));
                }
            }
            [
                hora,
                sessao,
                l.proto.clone(),
                l.domain.clone(),
                destino,
                format!("{} / {}", human(l.up), human(l.down)),
                nota,
            ]
        })
        .collect();
    let head = [
        tr!("HORA", "TIME"),
        tr!("SESSÃO", "SESSION"),
        "PROTO",
        tr!("DOMÍNIO", "DOMAIN"),
        tr!("DESTINO", "DESTINATION"),
        tr!("ENVIADO / RECEBIDO", "SENT / RECEIVED"),
        "",
    ];
    let mut w = head.map(|h| h.chars().count());
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let fmt = |cells: [&str; 7]| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{c:<width$}", width = w[i]))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    std::iter::once(fmt(head))
        .chain(rows.iter().map(|r| fmt(r.each_ref().map(String::as_str))))
        .collect()
}

pub fn rename(id: u32, name: &str) -> Result<i32> {
    sandmux_proto::msg::check_session_name(name).map_err(|e| anyhow!(e))?;
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Rename {
        id,
        name: name.to_string(),
    })?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: sessão {id} renomeada para {}",
            "sandmux: session {id} renamed to {}",
            name.trim()
        )
    );
    Ok(0)
}

pub fn kill(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Kill { id })?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: sessão {id} encerrada",
            "sandmux: session {id} ended"
        )
    );
    Ok(0)
}

pub fn restart(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let sessions = list(&mut conn)?;
    let Some(s) = sessions.iter().find(|s| s.id == id) else {
        bail!(trf!(
            "não existe a sessão {id}",
            "session {id} does not exist"
        ));
    };
    // A política é resolvida de novo: os nomes de `pass-env` vêm da
    // configuração de agora, com as opções e o perfil guardados.
    let host = sandmux_policy::Host::detect()?;
    let names = sandmux::launch::effective(&host, &s.opts)?.pass_env_names();
    let vars = client::client_vars(&names)?;
    let attach = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let reply = conn.call(&Msg::Restart {
        id,
        vars,
        size: client::terminal_size(),
        attach,
    })?;
    if attach {
        return client::attached(conn);
    }
    if let Msg::Created { id, name } = reply {
        eprintln!(
            "{}",
            trf!(
                "sandmux: sessão {id} ({name}) relançada",
                "sandmux: session {id} ({name}) relaunched"
            )
        );
    }
    Ok(0)
}

pub fn daemon_status() -> Result<i32> {
    let Some(mut conn) = Conn::connect()? else {
        println!(
            "{}",
            trf!(
                "O daemon não está rodando (socket: {}).",
                "The daemon is not running (socket: {}).",
                paths::socket_path().display()
            )
        );
        return Ok(1);
    };
    match conn.call(&Msg::Status)? {
        Msg::StatusInfo {
            pid,
            protocol,
            version,
            start_mode,
            sessions,
            socket,
        } => {
            println!(
                "{}",
                tr!("O daemon está rodando.", "The daemon is running.")
            );
            println!("  PID: {pid}");
            println!(
                "{}",
                trf!(
                    "  versão: {version} (protocolo {protocol})",
                    "  version: {version} (protocol {protocol})"
                )
            );
            println!(
                "{}",
                trf!("  iniciado por: {start_mode}", "  started by: {start_mode}")
            );
            println!(
                "{}",
                trf!("  sessões: {sessions}", "  sessions: {sessions}")
            );
            println!("  socket: {}", socket.display());
            Ok(0)
        }
        m => Err(anyhow!(trf!(
            "resposta inesperada do daemon: {m:?}",
            "unexpected reply from the daemon: {m:?}"
        ))),
    }
}

/// O processo existe e não é um zumbi?
fn process_alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => false,
        Ok(s) => !s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')),
    }
}

/// O PID do daemon pela trava, quando o processo é mesmo um `sandmuxd`.
fn locked_daemon_pid() -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(paths::lock_path())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    (comm.trim() == "sandmuxd" && process_alive(pid)).then_some(pid)
}

/// Para o daemon e espera o processo terminar. Devolve se havia daemon.
/// Um daemon de outra versão do protocolo, que recusa a conversa, é parado
/// pelo SIGTERM, que ele trata como o `stop` (as sessões vivas voltam na
/// subida seguinte).
fn stop_daemon() -> Result<bool> {
    let pid = match Conn::connect() {
        Ok(None) => return Ok(false),
        Ok(Some(mut conn)) => {
            let pid = conn.info.pid;
            conn.call(&Msg::Stop)?;
            pid
        }
        Err(e) => {
            let Some(pid) = locked_daemon_pid() else {
                return Err(e);
            };
            eprintln!(
                "{}",
                trf!(
                    "sandmux: o daemon em execução (PID {pid}) é de outra versão; parando-o pelo sinal",
                    "sandmux: the running daemon (PID {pid}) is from another version; stopping it with a signal"
                )
            );
            // SAFETY: kill com um PID e um sinal válidos.
            if unsafe { libc::kill(pid as i32, libc::SIGTERM) } < 0 {
                return Err(e);
            }
            pid
        }
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while paths::socket_path().exists() || process_alive(pid) {
        if Instant::now() >= deadline {
            bail!(trf!(
                "o daemon (PID {pid}) não parou",
                "the daemon (PID {pid}) did not stop"
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(true)
}

pub fn daemon_stop() -> Result<i32> {
    if stop_daemon()? {
        eprintln!(
            "{}",
            tr!(
                "sandmux: daemon parado; as sessões vivas voltam na próxima subida",
                "sandmux: daemon stopped; the alive sessions come back on the next start"
            )
        );
    } else {
        eprintln!(
            "{}",
            tr!(
                "sandmux: o daemon não está rodando",
                "sandmux: the daemon is not running"
            )
        );
    }
    Ok(0)
}

pub fn daemon_restart() -> Result<i32> {
    stop_daemon()?;
    client::start_daemon()?;
    let conn = client::wait_for_daemon(Duration::from_secs(5))?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: daemon reiniciado (PID {}, versão {})",
            "sandmux: daemon restarted (PID {}, version {})",
            conn.info.pid,
            conn.info.version
        )
    );
    Ok(0)
}

pub fn daemon_disable() -> Result<i32> {
    stop_daemon()?;
    let call =
        std::env::var("SANDMUX_DAEMON_START").as_deref() != Ok("direct") && systemd::available();
    if systemd::disable(call)? {
        eprintln!(
            "{}",
            trf!(
                "sandmux: daemon parado e unidade {} removida",
                "sandmux: daemon stopped and unit {} removed",
                systemd::unit_path().display()
            )
        );
    } else {
        eprintln!(
            "{}",
            tr!(
                "sandmux: daemon parado; não havia unidade systemd instalada",
                "sandmux: daemon stopped; no systemd unit was installed"
            )
        );
    }
    Ok(0)
}

/// `sandmux folder add`: o caminho vale a partir da pasta atual.
pub fn folder_add(id: u32, pasta: &std::path::Path, rw: bool) -> Result<i32> {
    let p = std::path::absolute(pasta)?;
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::FolderEdit {
        id,
        action: "add".into(),
        value: p.display().to_string(),
        rw,
    })?;
    let list = folders_of(id)?;
    if let Some(f) = list
        .iter()
        .find(|f| f.path == std::fs::canonicalize(&p).unwrap_or(p.clone()))
    {
        eprintln!(
            "{}",
            trf!(
                "sandmux: sessão {id}: {} em {} ({})",
                "sandmux: session {id}: {} at {} ({})",
                f.path.display(),
                f.dest().display(),
                if f.rw {
                    tr!("leitura e escrita", "read and write")
                } else {
                    tr!("somente leitura", "read-only")
                }
            )
        );
    }
    Ok(0)
}

pub fn folder_remove(id: u32, nome: &str) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::FolderEdit {
        id,
        action: "remove".into(),
        value: nome.into(),
        rw: false,
    })?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: sessão {id}: /mnt/sandmux/{nome} desmontada",
            "sandmux: session {id}: /mnt/sandmux/{nome} unmounted"
        )
    );
    Ok(0)
}

fn folders_of(id: u32) -> Result<Vec<sandmux_spec::LiveFolder>> {
    let mut conn = client::connect_or_start()?;
    match conn.call(&Msg::FolderList { id })? {
        Msg::Folders { list } => Ok(list),
        _ => anyhow::bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected reply from the daemon"
        )),
    }
}

pub fn folder_list(id: u32) -> Result<i32> {
    let list = folders_of(id)?;
    if list.is_empty() {
        println!(
            "{}",
            tr!(
                "Nenhuma pasta em /mnt/sandmux.",
                "No folders in /mnt/sandmux."
            )
        );
    }
    for f in list {
        println!(
            "{}  {}  {}",
            f.dest().display(),
            if f.rw { "rw" } else { "ro" },
            f.path.display()
        );
    }
    Ok(0)
}

/// `sandmux mcp list`: da sessão, ou do harness na pasta atual.
pub fn mcp_list(id: Option<u32>, harness: &str) -> Result<i32> {
    let list = match id {
        Some(id) => {
            let mut conn = client::connect_or_start()?;
            match conn.call(&Msg::McpList { id })? {
                Msg::Mcps { list } => list,
                _ => anyhow::bail!(tr!(
                    "resposta inesperada do daemon",
                    "unexpected reply from the daemon"
                )),
            }
        }
        None => {
            let host = sandmux_policy::Host::detect()?;
            let mut opts = sandmux_proto::LaunchOptions::new(harness);
            opts.workdir = std::env::current_dir()?;
            sandmux::launch::mcp_entries(&host, &opts)?
        }
    };
    if list.is_empty() {
        println!("{}", tr!("Nenhum servidor MCP.", "No MCP servers."));
    }
    for m in list {
        println!(
            "{}  {}  {}  {}  {}",
            if m.enabled {
                tr!("habilitado   ", "enabled ")
            } else {
                tr!("desabilitado ", "disabled")
            },
            m.name,
            m.kind,
            m.origin,
            m.target
        );
    }
    Ok(0)
}
