//! Comandos de sessão e do daemon no cliente.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use sandmux::client::{self, Conn, systemd};
use sandmux::paths;
use sandmux_policy::limits::human;
use sandmux_proto::{LaunchOptions, Msg, SessionInfo};

fn need_terminal(what: &str) -> Result<()> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        bail!("{what} precisa de um terminal interativo");
    }
    Ok(())
}

/// `run` com terminal: cria a sessão no daemon e anexa. `pass_env` são os
/// nomes da política efetiva, cujos valores vão ao daemon.
pub fn run_session(opts: LaunchOptions, pass_env: &[String]) -> Result<i32> {
    let vars = client::client_vars(pass_env)?;
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Create {
        opts: Box::new(opts),
        vars,
        size: client::terminal_size(),
        attach: true,
    })?;
    client::attached(conn)
}

/// Cria a sessão no daemon sem anexar (o gateway sem terminal) e mostra o
/// identificador.
pub fn create_detached(opts: LaunchOptions, pass_env: &[String]) -> Result<i32> {
    let vars = client::client_vars(pass_env)?;
    let mut conn = client::connect_or_start()?;
    match conn.call(&Msg::Create {
        opts: Box::new(opts),
        vars,
        size: client::terminal_size(),
        attach: false,
    })? {
        Msg::Created { id, name } => {
            println!("sessão {id} criada: {name}");
            Ok(0)
        }
        m => Err(anyhow!("resposta inesperada do daemon: {m:?}")),
    }
}

fn list(conn: &mut Conn) -> Result<Vec<SessionInfo>> {
    match conn.call(&Msg::List)? {
        Msg::Sessions { sessions } => Ok(sessions),
        m => Err(anyhow!("resposta inesperada do daemon: {m:?}")),
    }
}

/// Estado com a nota de memória esgotada.
fn state_text(s: &SessionInfo) -> String {
    match &s.state {
        sandmux_proto::SessionState::Encerrada { codigo } if s.oom_kills > 0 => {
            format!("encerrada ({codigo}, memória esgotada)")
        }
        other => other.to_string(),
    }
}

fn of_max(cur: String, max: Option<String>) -> String {
    format!("{cur}/{}", max.unwrap_or_else(|| "sem teto".into()))
}

pub fn ls() -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let sessions = list(&mut conn)?;
    if sessions.is_empty() {
        println!("Não há sessões.");
        return Ok(0);
    }
    let rows: Vec<[String; 7]> = sessions
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
                state_text(s),
                s.clients.to_string(),
                mem,
                proc_,
                s.workdir.display().to_string(),
            ]
        })
        .collect();
    let head = [
        "ID", "NOME", "ESTADO", "CLIENTES", "MEMÓRIA", "PROC", "PASTA",
    ];
    let mut w = head.map(|h| h.chars().count());
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |r: [&str; 7]| {
        let mut s = String::new();
        for (i, c) in r.iter().enumerate() {
            if i == 6 {
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
        line([&r[0], &r[1], &r[2], &r[3], &r[4], &r[5], &r[6]]);
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
        bail!("resposta inesperada do daemon");
    };
    let state = match state {
        sandmux_proto::SessionState::Encerrada { codigo } if oom_kills > 0 => {
            format!("encerrada ({codigo}, memória esgotada)")
        }
        other => other.to_string(),
    };
    println!("Sessão {id}: {state}");
    match stats {
        Some(s) => {
            let max = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                m.map(f).unwrap_or_else(|| "sem teto".into())
            };
            println!(
                "  memória: {} em uso, pico de {}, teto de {}",
                human(s.usage.memory),
                human(s.memory_peak),
                max(s.usage.memory_max, &human)
            );
            println!(
                "  processos: {} em uso, pico de {}, teto de {}",
                s.usage.processes,
                s.processes_peak,
                max(s.usage.processes_max, &|n| n.to_string())
            );
            let secs = s.cpu_usec as f64 / 1_000_000.0;
            println!("  CPU usada: {}s", format!("{secs:.1}").replace('.', ","));
        }
        None => println!("  sem consumo: a sessão não está viva ou roda sem tetos"),
    }
    println!("  mortes por falta de memória: {oom_kills}");
    println!("  criações de processo recusadas pelo teto: {fork_refusals}");
    match docker {
        Some(d) => {
            let teto = |m: Option<u64>, f: &dyn Fn(u64) -> String| {
                m.map(f).unwrap_or_else(|| "sem teto".into())
            };
            println!(
                "Docker privado da pasta: rodando, {} contêineres",
                d.containers
            );
            println!(
                "  memória: {} em uso, teto de {}",
                human(d.memory),
                teto(d.memory_max, &human)
            );
            println!(
                "  processos: {} em uso, teto de {}",
                d.processes,
                teto(d.processes_max, &|n| n.to_string())
            );
        }
        None => println!("Docker privado da pasta: parado"),
    }
    if ports.is_empty() {
        println!("Portas publicadas: nenhuma");
    } else {
        println!("Portas publicadas no host:");
        for l in port_lines(&ports) {
            println!("  {l}");
        }
    }
    if let Some(p) = policy {
        println!("Proteção do último lançamento:");
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
    let yes = |b: bool| if b { "sim" } else { "não" };
    let mut l = vec![
        p.level.clone(),
        format!("camada de escrita: {}", p.layer),
        format!("pasta: {} (escrita)", p.workdir.display()),
    ];
    for d in &p.ro {
        l.push(format!("extra: {} (leitura)", d.display()));
    }
    for d in &p.rw {
        l.push(format!("extra: {} (escrita)", d.display()));
    }
    l.push(format!("GPU: {} · KVM: {}", yes(p.gpu), yes(p.kvm)));
    l.push(format!(
        "clipboard: {}",
        if p.clipboard { "liberado" } else { "bloqueado" }
    ));
    if p.pass_env.is_empty() {
        l.push("pass-env: nenhuma".into());
    } else {
        l.push(format!(
            "pass-env: {} (ao alcance do agente)",
            p.pass_env.join(", ")
        ));
    }
    if p.limits {
        let m = p.memory_max.map(human).unwrap_or_else(|| "sem teto".into());
        let n = p
            .processes_max
            .map(|n| n.to_string())
            .unwrap_or_else(|| "sem teto".into());
        l.push(format!("tetos: memória {m}, processos {n}"));
    } else {
        l.push("tetos: sem suporte, sem teto".into());
    }
    l.push(format!("rede: {}", p.network));
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
        other => Err(anyhow!("resposta inesperada do daemon: {other:?}")),
    }
}

pub fn detach(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Detach { id })?;
    eprintln!("sandmux: sessão {id} desanexada de todos os terminais");
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
        bail!("resposta inesperada do daemon");
    };
    if lines.is_empty() {
        match id {
            Some(id) => println!("Nenhuma conexão registrada da sessão {id} desde ontem."),
            None => println!("Nenhuma conexão registrada desde ontem."),
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
            anyhow::bail!("nome de servidor MCP inválido: {value:?}");
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
        "deny" => format!("{value} negado"),
        "allow" => format!("{value} liberado"),
        "forget" | "file-forget" => format!("{value} tirado das regras"),
        "file-deny" => format!("arquivo {value} negado"),
        "mcp-deny" => format!("MCP {value} desabilitado"),
        "mcp-allow" => format!("MCP {value} habilitado"),
        "mcp-forget" => format!("MCP {value} tirado das regras"),
        "file-allow" => format!("arquivo {value} liberado"),
        _ => format!("modo de rede {value}"),
    };
    eprintln!("sandmux: sessão {id}: {what} na aba");
    Ok(0)
}

/// Linhas de `sandmux net rules`: o modo e cada regra, com o estado e a
/// origem.
pub fn rules_lines(eff: &sandmux_policy::config::Effective) -> Vec<String> {
    use sandmux_policy::config::NetworkMode;
    use sandmux_policy::netcatalog::INFERENCE_ORIGIN;
    use sandmux_spec::net::RuleKind;
    let mode = eff.network.value;
    let mut out = vec![format!("Modo: {}; {}", mode.describe(), eff.network.origin)];
    let row = |r: &sandmux_spec::net::Rule| {
        format!(
            "  {:<10} {:<36} {}",
            if r.enabled { "ligado" } else { "desligado" },
            r.pattern.to_string(),
            r.origin
        )
    };
    let section =
        |out: &mut Vec<String>, title: String, f: &dyn Fn(&sandmux_spec::net::Rule) -> bool| {
            out.push(String::new());
            out.push(title);
            out.extend(eff.net_rules.iter().filter(|r| f(r)).map(row));
        };
    section(&mut out, "Denylist (todos os modos):".into(), &|r| {
        r.kind == RuleKind::Deny
    });
    let note = |m: NetworkMode| {
        if mode == m {
            ""
        } else {
            ", sem efeito neste modo"
        }
    };
    section(
        &mut out,
        format!("Allowlist (modo custom{}):", note(NetworkMode::Custom)),
        &|r| r.kind == RuleKind::Allow && r.origin != INFERENCE_ORIGIN,
    );
    section(
        &mut out,
        format!(
            "Provedores (modo só inferência{}):",
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
            let s = sessions
                .into_iter()
                .find(|s| s.id == id)
                .ok_or_else(|| anyhow!("não existe a sessão {id}"))?;
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
        Some(id) => println!("Sessão {id} ({})", opts.workdir.display()),
        None => println!("Pasta {}", opts.workdir.display()),
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
                (Some(b), _) => format!("bloqueado: {b}"),
                (None, true) => "novo".into(),
                _ => String::new(),
            };
            if let Some(s) = &l.secret {
                if !nota.is_empty() {
                    nota.push_str(", ");
                }
                if l.proto == "ssh-sign" {
                    nota.push_str(&format!("assinatura ssh: {s}"));
                } else {
                    nota.push_str(&format!("chave: {s}"));
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
        "HORA",
        "SESSÃO",
        "PROTO",
        "DOMÍNIO",
        "DESTINO",
        "ENVIADO / RECEBIDO",
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
    eprintln!("sandmux: sessão {id} renomeada para {}", name.trim());
    Ok(0)
}

pub fn kill(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    conn.call(&Msg::Kill { id })?;
    eprintln!("sandmux: sessão {id} encerrada");
    Ok(0)
}

pub fn restart(id: u32) -> Result<i32> {
    let mut conn = client::connect_or_start()?;
    let sessions = list(&mut conn)?;
    let Some(s) = sessions.iter().find(|s| s.id == id) else {
        bail!("não existe a sessão {id}");
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
        eprintln!("sandmux: sessão {id} ({name}) relançada");
    }
    Ok(0)
}

pub fn daemon_status() -> Result<i32> {
    let Some(mut conn) = Conn::connect()? else {
        println!(
            "O daemon não está rodando (socket: {}).",
            paths::socket_path().display()
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
            println!("O daemon está rodando.");
            println!("  PID: {pid}");
            println!("  versão: {version} (protocolo {protocol})");
            println!("  iniciado por: {start_mode}");
            println!("  sessões: {sessions}");
            println!("  socket: {}", socket.display());
            Ok(0)
        }
        m => Err(anyhow!("resposta inesperada do daemon: {m:?}")),
    }
}

/// O processo existe e não é um zumbi?
fn process_alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => false,
        Ok(s) => !s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')),
    }
}

/// Para o daemon e espera o processo terminar. Devolve se havia daemon.
fn stop_daemon() -> Result<bool> {
    let Some(mut conn) = Conn::connect()? else {
        return Ok(false);
    };
    let pid = conn.info.pid;
    conn.call(&Msg::Stop)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while paths::socket_path().exists() || process_alive(pid) {
        if Instant::now() >= deadline {
            bail!("o daemon (PID {pid}) não parou");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(true)
}

pub fn daemon_stop() -> Result<i32> {
    if stop_daemon()? {
        eprintln!("sandmux: daemon parado; as sessões vivas voltam na próxima subida");
    } else {
        eprintln!("sandmux: o daemon não está rodando");
    }
    Ok(0)
}

pub fn daemon_restart() -> Result<i32> {
    stop_daemon()?;
    client::start_daemon()?;
    let conn = client::wait_for_daemon(Duration::from_secs(5))?;
    eprintln!(
        "sandmux: daemon reiniciado (PID {}, versão {})",
        conn.info.pid, conn.info.version
    );
    Ok(0)
}

pub fn daemon_disable() -> Result<i32> {
    stop_daemon()?;
    let call =
        std::env::var("SANDMUX_DAEMON_START").as_deref() != Ok("direct") && systemd::available();
    if systemd::disable(call)? {
        eprintln!(
            "sandmux: daemon parado e unidade {} removida",
            systemd::unit_path().display()
        );
    } else {
        eprintln!("sandmux: daemon parado; não havia unidade systemd instalada");
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
            "sandmux: sessão {id}: {} em {} ({})",
            f.path.display(),
            f.dest().display(),
            if f.rw {
                "leitura e escrita"
            } else {
                "somente leitura"
            }
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
    eprintln!("sandmux: sessão {id}: /mnt/sandmux/{nome} desmontada");
    Ok(0)
}

fn folders_of(id: u32) -> Result<Vec<sandmux_spec::LiveFolder>> {
    let mut conn = client::connect_or_start()?;
    match conn.call(&Msg::FolderList { id })? {
        Msg::Folders { list } => Ok(list),
        _ => anyhow::bail!("resposta inesperada do daemon"),
    }
}

pub fn folder_list(id: u32) -> Result<i32> {
    let list = folders_of(id)?;
    if list.is_empty() {
        println!("Nenhuma pasta em /mnt/sandmux.");
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
                _ => anyhow::bail!("resposta inesperada do daemon"),
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
        println!("Nenhum servidor MCP.");
    }
    for m in list {
        println!(
            "{}  {}  {}  {}  {}",
            if m.enabled {
                "habilitado   "
            } else {
                "desabilitado "
            },
            m.name,
            m.kind,
            m.origin,
            m.target
        );
    }
    Ok(0)
}
