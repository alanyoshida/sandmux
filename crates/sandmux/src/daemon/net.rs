//! Registro de rede visto pelo daemon: as linhas de cada sessão, pelas
//! etiquetas dos lançamentos dela, e a retenção dos arquivos.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sandmux_proto::msg::NetLine;
use sandmux_spec::trf;

use super::Shared;
use crate::paths;

/// Retenção padrão: dias e soma dos arquivos.
pub const DAYS: u32 = 30;
pub const MAX_BYTES: u64 = 500 << 20;
/// Teto de linhas de uma resposta.
pub const MAX_LINES: u32 = 1000;
/// Linhas guardadas em memória para as consultas (de ontem e de hoje).
const TAIL_LINES: usize = 50_000;

static TAIL: std::sync::Mutex<Option<sandmux_net::log::Tail>> = std::sync::Mutex::new(None);

pub fn log_dir() -> PathBuf {
    paths::state_dir().join("net")
}

/// Arquivo de política de rede de um lançamento.
pub fn policy_path(tag: &str) -> PathBuf {
    paths::runtime_dir().join("net").join(format!("{tag}.json"))
}

/// Grava a política de forma atômica (arquivo temporário e `rename`), numa
/// pasta 700.
pub fn write_policy(
    path: &std::path::Path,
    p: &sandmux_spec::net::NetPolicy,
) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = path.parent().expect("pasta da política");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(&serde_json::to_vec(p).map_err(std::io::Error::other)?)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// A política com os domínios donos dos segredos do cofre desbloqueado na
/// lista de decifração.
pub fn with_vault(
    shared: &Shared,
    mut p: sandmux_spec::net::NetPolicy,
) -> sandmux_spec::net::NetPolicy {
    let domains = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .domains();
    for d in &domains {
        if let Ok(pat) = d.parse::<sandmux_spec::net::Pattern>()
            && !p.decrypt.contains(&pat)
        {
            p.decrypt.push(pat);
        }
    }
    // Modo só inferência: os provedores com segredo no cofre.
    p.restrict_inference(&domains);
    p
}

/// A política com os servidores das chaves SSH escolhidas (`names`) do
/// cofre desbloqueado: as únicas conexões SSH que passam.
pub fn with_ssh(
    shared: &Shared,
    mut p: sandmux_spec::net::NetPolicy,
    names: &[String],
) -> sandmux_spec::net::NetPolicy {
    p.ssh = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ssh_servers(names)
        .iter()
        .filter_map(|d| d.parse().ok())
        .collect();
    p
}

/// Recompila a política de todas as sessões vivas (depois de uma mudança de
/// configuração ou do cofre).
pub fn recompile_all(shared: &Arc<Shared>) {
    let ids: Vec<u32> = shared
        .lock()
        .sessions
        .values()
        .filter(|s| s.live.is_some())
        .map(|s| s.id)
        .collect();
    for id in ids {
        if let Err(e) = recompile(shared, id) {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: sessão {id}: política de rede mantida: {e:#}",
                    "sandmuxd: session {id}: network policy kept: {e:#}"
                )
            );
        }
    }
}

/// Resultado de recompilar a política de uma sessão.
pub enum Recompiled {
    /// Nada mudou.
    Same,
    /// Gravada no arquivo do lançamento.
    Written,
    /// A mudança entra ou sai de `sem-rede`: só no próximo lançamento.
    NextLaunch,
}

/// Recompila a política de rede da sessão `id`, se viva, e regrava o arquivo
/// do lançamento quando mudou.
pub fn recompile(shared: &Shared, id: u32) -> anyhow::Result<Recompiled> {
    let (opts, file, old, tag) = {
        let st = shared.lock();
        let Some(s) = st.sessions.get(&id) else {
            return Ok(Recompiled::Same);
        };
        if s.live.is_none() {
            return Ok(Recompiled::Same);
        }
        (
            s.opts.clone(),
            s.policy_file.clone(),
            s.net_policy.clone(),
            s.tags.last().cloned(),
        )
    };
    let mut host = shared.base.clone();
    host.cwd = opts.workdir.clone();
    let eff = crate::launch::effective(&host, &opts)?;
    // A política de arquivos do lançamento atual, relida pelo supervisor.
    if let Some(tag) = &tag {
        let f = crate::launch::file_policy_path(tag);
        if f.exists() {
            let workdir =
                std::fs::canonicalize(&opts.workdir).unwrap_or_else(|_| opts.workdir.clone());
            crate::launch::write_file_policy(&f, &eff.file_policy(&host.home, &workdir))?;
        }
    }
    let new = eff
        .net_policy()
        .map(|p| with_ssh(shared, with_vault(shared, p), &eff.ssh_keys));
    let desc = eff.network.value.describe();
    if new == old {
        return Ok(Recompiled::Same);
    }
    let (Some(new), Some(file), Some(_)) = (new, file, old) else {
        return Ok(Recompiled::NextLaunch);
    };
    write_policy(&file, &new)?;
    let mut st = shared.lock();
    let workdir = opts.workdir.clone();
    if let Some(s) = st.sessions.get_mut(&id) {
        s.net_policy = Some(new);
        // O painel e o `stats` mostram o modo em vigor.
        if let Some(p) = s.policy.as_mut() {
            p.network = desc.into();
        }
    }
    drop(st);
    refresh_engine(shared, &workdir);
    Ok(Recompiled::Written)
}

/// Aplica um `NetEdit` à camada da aba da sessão `id`: guarda nas opções,
/// grava a lista de sessões e recompila. Devolve um aviso quando a mudança
/// só vale no próximo lançamento.
pub fn edit(shared: &Shared, id: u32, action: &str, value: &str) -> Result<Option<String>, String> {
    let canon = |v: &str| {
        v.parse::<sandmux_spec::net::Pattern>()
            .map(|p| p.to_string())
            .map_err(|e| e.to_string())
    };
    let live = {
        let mut st = shared.lock();
        let s = st
            .sessions
            .get_mut(&id)
            .ok_or_else(|| trf!("não existe a sessão {id}", "session {id} does not exist"))?;
        match action {
            "deny" | "allow" => {
                let p = canon(value)?;
                let list = if action == "deny" {
                    &mut s.opts.deny
                } else {
                    &mut s.opts.allow
                };
                if !list.contains(&p) {
                    list.push(p);
                }
            }
            "forget" => {
                let p = canon(value)?;
                let before = s.opts.deny.len() + s.opts.allow.len();
                s.opts.deny.retain(|x| *x != p);
                s.opts.allow.retain(|x| *x != p);
                if s.opts.deny.len() + s.opts.allow.len() == before {
                    return Err(trf!(
                        "{p} não está nas regras da aba da sessão {id}",
                        "{p} is not in the tab rules of session {id}"
                    ));
                }
            }
            "mode" => {
                let m = sandmux_policy::config::NetworkMode::parse(value)?;
                s.opts.network = Some(m.to_string());
            }
            // Política de arquivos da aba (protocolo 14).
            "file-deny" | "file-allow" => {
                sandmux_policy::config::valid_file_pattern(value)?;
                let list = if action == "file-deny" {
                    &mut s.opts.file_deny
                } else {
                    &mut s.opts.file_allow
                };
                if !list.iter().any(|x| x == value) {
                    list.push(value.to_string());
                }
            }
            // MCPs da aba (protocolo 16): valem no próximo lançamento.
            "mcp-deny" | "mcp-allow" | "mcp-forget" => {
                if value.is_empty() || value.contains(char::is_whitespace) {
                    return Err(trf!(
                        "nome de servidor MCP inválido: {value:?}",
                        "invalid MCP server name: {value:?}"
                    ));
                }
                match action {
                    "mcp-deny" => {
                        s.opts.mcp_allow.retain(|x| x != value);
                        if !s.opts.mcp_deny.iter().any(|x| x == value) {
                            s.opts.mcp_deny.push(value.to_string());
                        }
                    }
                    "mcp-allow" => {
                        s.opts.mcp_deny.retain(|x| x != value);
                        if !s.opts.mcp_allow.iter().any(|x| x == value) {
                            s.opts.mcp_allow.push(value.to_string());
                        }
                    }
                    _ => {
                        let before = s.opts.mcp_deny.len() + s.opts.mcp_allow.len();
                        s.opts.mcp_deny.retain(|x| x != value);
                        s.opts.mcp_allow.retain(|x| x != value);
                        if s.opts.mcp_deny.len() + s.opts.mcp_allow.len() == before {
                            return Err(trf!(
                                "{value} não está nos MCPs da aba da sessão {id}",
                                "{value} is not in the tab MCPs of session {id}"
                            ));
                        }
                    }
                }
                shared.persist(&st);
                return Ok(Some(trf!(
                    "os MCPs valem no próximo lançamento da sessão {id} (`sandmux restart {id}` ou Ctrl+b r)",
                    "the MCPs take effect on the next launch of session {id} (`sandmux restart {id}` or Ctrl+b r)"
                )));
            }
            "file-forget" => {
                let before = s.opts.file_deny.len() + s.opts.file_allow.len();
                s.opts.file_deny.retain(|x| x != value);
                s.opts.file_allow.retain(|x| x != value);
                if s.opts.file_deny.len() + s.opts.file_allow.len() == before {
                    return Err(trf!(
                        "{value} não está nas regras de arquivos da aba da sessão {id}",
                        "{value} is not in the tab file rules of session {id}"
                    ));
                }
            }
            other => {
                return Err(trf!(
                    "ação de rede desconhecida: {other}",
                    "unknown network action: {other}"
                ));
            }
        }
        let live = s.live.is_some();
        shared.persist(&st);
        live
    };
    if !live {
        return Ok(Some(trf!(
            "a sessão {id} não está viva; a regra vale no próximo lançamento",
            "session {id} is not alive; the rule takes effect on the next launch"
        )));
    }
    match recompile(shared, id) {
        Ok(Recompiled::NextLaunch) => Ok(Some(trf!(
            "entrar ou sair de sem-rede só vale no próximo lançamento: `sandmux kill {id}` e `sandmux restart {id}`",
            "entering or leaving sem-rede only takes effect on the next launch: `sandmux kill {id}` and `sandmux restart {id}`"
        ))),
        Ok(_) => Ok(None),
        Err(e) => Err(format!("{e:#}")),
    }
}

/// A política do Docker da pasta: a mais restrita entre as sessões vivas da
/// pasta, ou a da pasta pela configuração quando não há sessão viva.
pub fn engine_policy(shared: &Shared, workdir: &std::path::Path) -> sandmux_spec::net::NetPolicy {
    let list: Vec<Option<sandmux_spec::net::NetPolicy>> = shared
        .lock()
        .sessions
        .values()
        .filter(|s| s.live.is_some() && super::session::same_dir(&s.opts.workdir, workdir))
        .map(|s| s.net_policy.clone())
        .collect();
    if !list.is_empty() {
        return with_docker_secrets(shared, sandmux_spec::net::strictest(&list));
    }
    let mut host = shared.base.clone();
    host.cwd = workdir.to_path_buf();
    let mut opts = sandmux_proto::LaunchOptions::new("shell");
    opts.workdir = workdir.to_path_buf();
    let own = crate::launch::effective(&host, &opts)
        .map(|e| e.net_policy())
        .unwrap_or_else(|_| Some(Default::default()));
    with_docker_secrets(shared, sandmux_spec::net::strictest(&[own]))
}

/// A lista de decifração do Docker: os domínios donos dos segredos `docker`
/// do cofre desbloqueado.
fn with_docker_secrets(
    shared: &Shared,
    mut p: sandmux_spec::net::NetPolicy,
) -> sandmux_spec::net::NetPolicy {
    let domains = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .tool_domains("docker");
    for d in &domains {
        if let Ok(pat) = d.parse::<sandmux_spec::net::Pattern>()
            && !p.decrypt.contains(&pat)
        {
            p.decrypt.push(pat);
        }
    }
    p
}

/// Regrava a política do Docker da pasta, quando ele está rodando.
pub fn refresh_engine(shared: &Shared, workdir: &std::path::Path) {
    if let Some(f) = shared.docker.policy_file(workdir) {
        let p = engine_policy(shared, workdir);
        if let Err(e) = write_policy(&f, &p) {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: política do Docker de {}: {e}",
                    "sandmuxd: Docker policy of {}: {e}",
                    workdir.display()
                )
            );
        }
    }
}

/// Vigia o arquivo de configuração (a cada segundo) e recompila a política
/// das sessões vivas quando ele muda.
pub fn start_policy_watch(shared: Arc<Shared>) {
    let path = sandmux_policy::config::path(&shared.base);
    let stamp = move || {
        std::fs::metadata(&path)
            .ok()
            .map(|m| (m.len(), m.modified().ok()))
    };
    let _ = std::thread::Builder::new()
        .name("politica-rede".into())
        .spawn(move || {
            let mut last = stamp();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = stamp();
                if now == last {
                    continue;
                }
                last = now;
                let (ids, dirs): (Vec<u32>, Vec<std::path::PathBuf>) = shared
                    .lock()
                    .sessions
                    .values()
                    .filter(|s| s.live.is_some())
                    .map(|s| (s.id, s.opts.workdir.clone()))
                    .unzip();
                // O Docker de uma pasta sem sessão que mudou de política
                // também (a configuração da pasta vale para ele).
                for w in &dirs {
                    refresh_engine(&shared, w);
                }
                for id in ids {
                    match recompile(&shared, id) {
                        Ok(Recompiled::NextLaunch) => eprintln!(
                            "{}",
                            trf!(
                                "sandmuxd: sessão {id}: a mudança de modo de rede vale no próximo lançamento",
                                "sandmuxd: session {id}: the network mode change takes effect on the next launch"
                            )
                        ),
                        Ok(_) => {}
                        Err(e) => eprintln!(
                            "{}",
                            trf!(
                                "sandmuxd: sessão {id}: política de rede mantida: {e:#}",
                                "sandmuxd: session {id}: network policy kept: {e:#}"
                            )
                        ),
                    }
                }
            }
        });
}

/// Dias e teto da configuração de `[global]`, ou os padrões.
fn retention(shared: &Shared) -> (u32, u64) {
    match sandmux_policy::config::Config::load(&sandmux_policy::config::path(&shared.base)) {
        Ok(c) => (
            c.net_log_days.unwrap_or(DAYS),
            c.net_log_max.unwrap_or(MAX_BYTES),
        ),
        Err(e) => {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: aviso: {e}; retenção do registro de rede com os padrões",
                    "sandmuxd: warning: {e}; network log retention with the defaults"
                )
            );
            (DAYS, MAX_BYTES)
        }
    }
}

/// Aplica a retenção ao subir e depois de hora em hora.
pub fn start_retention(shared: Arc<Shared>) {
    let _ = std::thread::Builder::new()
        .name("retencao-rede".into())
        .spawn(move || {
            loop {
                let (days, max) = retention(&shared);
                for p in sandmux_net::log::prune(&log_dir(), days, max) {
                    eprintln!(
                        "{}",
                        trf!(
                            "sandmuxd: registro de rede apagado: {}",
                            "sandmuxd: network log deleted: {}",
                            p.display()
                        )
                    );
                }
                // Os registros de arquivos e de queries seguem a mesma retenção.
                for sub in ["files", "db"] {
                    for p in sandmux_net::log::prune(&paths::state_dir().join(sub), days, max) {
                        eprintln!(
                            "{}",
                            trf!(
                                "sandmuxd: registro apagado: {}",
                                "sandmuxd: log deleted: {}",
                                p.display()
                            )
                        );
                    }
                }
                std::thread::sleep(Duration::from_secs(3600));
            }
        });
}

/// Últimas linhas do registro (de hoje e de ontem), de uma sessão ou de todas.
/// O filtro roda sobre as linhas em memória, sem reler os arquivos.
pub fn lines(
    shared: &Shared,
    id: Option<u32>,
    since: Option<&str>,
    limit: u32,
) -> Result<Vec<NetLine>, String> {
    let owners: HashMap<String, u32> = {
        let st = shared.lock();
        if let Some(id) = id
            && !st.sessions.contains_key(&id)
        {
            return Err(trf!(
                "não existe a sessão {id}",
                "session {id} does not exist"
            ));
        }
        st.sessions
            .values()
            .flat_map(|s| s.tags.iter().map(move |t| (t.clone(), s.id)))
            .collect()
    };
    let mut tail = TAIL.lock().unwrap_or_else(|e| e.into_inner());
    let tail = tail.get_or_insert_with(|| sandmux_net::log::Tail::new(&log_dir(), TAIL_LINES));
    let limit = limit.clamp(1, MAX_LINES) as usize;
    // Das mais novas para trás, até o limite; devolvidas em ordem.
    let mut out: Vec<NetLine> = tail
        .refresh()
        .iter()
        .rev()
        // Inclusive: a hora tem resolução de segundos, e quem pergunta com
        // `since` prefere rever uma linha a perder outra do mesmo segundo.
        .take_while(|e| since.is_none_or(|s| e.ts.as_str() >= s))
        .map(|e| (owners.get(&e.tag).copied(), e))
        .filter(|(session, _)| id.is_none() || *session == id)
        .take(limit)
        .map(|(session, e)| NetLine {
            session,
            ts: e.ts.clone(),
            tag: e.tag.clone(),
            proto: e.proto.clone(),
            domain: e.domain.clone(),
            ip: e.ip.clone(),
            port: e.port,
            up: e.up,
            down: e.down,
            ms: e.ms,
            blocked: e.blocked.clone(),
            new: e.new,
            method: e.method.clone(),
            path: e.path.clone(),
            status: e.status,
            secret: e.secret.clone(),
        })
        .collect();
    out.reverse();
    Ok(out)
}
