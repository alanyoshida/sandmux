//! Tela de rede da aba: acessos recentes e regras efetivas, com as ações de
//! negar, liberar, ligar e desligar itens e trocar o modo.

use std::path::PathBuf;

use sandmux_policy::Host;
use sandmux_policy::config::edit::{Op, Target};
use sandmux_policy::config::{self, NetworkMode};
use sandmux_policy::netcatalog;
use sandmux_proto::msg::NetLine;
use sandmux_proto::{LaunchOptions, Msg};
use sandmux_spec::net::{Rule, RuleKind};
use sandmux_spec::{tr, trf};

/// Acessos mostrados.
pub const LINES: u32 = 50;

/// Ação esperando a escolha da camada.
#[derive(Debug, Clone)]
pub enum Pending {
    Deny(String),
    Allow(String),
    /// Ligar ou desligar um item.
    Toggle(Rule),
}

impl Pending {
    pub fn prompt(&self) -> String {
        match self {
            Pending::Deny(p) => trf!(
                "negar {p} em: a aba · p projeto · g global · Esc cancela",
                "deny {p} in: a tab · p project · g global · Esc cancels"
            ),
            Pending::Allow(p) => trf!(
                "liberar {p} em: a aba · p projeto · g global · Esc cancela",
                "allow {p} in: a tab · p project · g global · Esc cancels"
            ),
            Pending::Toggle(r) => trf!(
                "{} {} em: p projeto · g global · Esc cancela",
                "{} {} in: p project · g global · Esc cancels",
                if r.enabled {
                    tr!("desligar", "turn off")
                } else {
                    tr!("ligar", "turn on")
                },
                r.pattern
            ),
        }
    }

    /// Só negar e liberar podem ir para a aba.
    pub fn tab_allowed(&self) -> bool {
        !matches!(self, Pending::Toggle(_))
    }
}

/// Camada escolhida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Tab,
    Project,
    Global,
}

pub struct NetScreen {
    pub id: u32,
    /// 0: acessos; 1: regras.
    pub list: usize,
    pub sel: [usize; 2],
    pub lines: Vec<NetLine>,
    pub rules: Vec<Rule>,
    pub mode: NetworkMode,
    pub mode_origin: String,
    /// Pasta do perfil de projeto que vale, ou a pasta de trabalho.
    pub project: PathBuf,
    pub profile: Option<String>,
    pub ask: Option<Pending>,
    pub note: Option<String>,
}

impl NetScreen {
    pub fn len(&self) -> usize {
        if self.list == 0 {
            self.lines.len()
        } else {
            self.rules.len()
        }
    }

    pub fn selected_line(&self) -> Option<&NetLine> {
        if self.list == 0 {
            self.lines.get(self.sel[0])
        } else {
            None
        }
    }

    pub fn selected_rule(&self) -> Option<&Rule> {
        if self.list == 1 {
            self.rules.get(self.sel[1])
        } else {
            None
        }
    }
}

/// Padrão para negar ou liberar o destino de um acesso: o domínio, ou o IP
/// quando não há domínio válido.
pub fn pattern_of(l: &NetLine) -> Option<String> {
    if let Ok(p) = l.domain.parse::<sandmux_spec::net::Pattern>() {
        return Some(p.to_string());
    }
    l.ip.parse::<std::net::Ipv4Addr>()
        .ok()
        .map(|ip| ip.to_string())
}

/// Monta a tela a partir das opções da sessão e das linhas do registro.
pub fn build(
    id: u32,
    host: &Host,
    opts: LaunchOptions,
    mut lines: Vec<NetLine>,
) -> Result<NetScreen, String> {
    let mut h = host.clone();
    h.cwd = opts.workdir.clone();
    let eff = sandmux::launch::effective(&h, &opts).map_err(|e| format!("{e:#}"))?;
    lines.reverse();
    Ok(NetScreen {
        id,
        list: 0,
        sel: [0, 0],
        lines,
        rules: eff.net_rules.clone(),
        mode: eff.network.value,
        mode_origin: config::rule_origin(&eff.network.origin),
        project: eff.project.clone().unwrap_or_else(|| opts.workdir.clone()),
        profile: opts.profile.clone(),
        ask: None,
        note: None,
    })
}

/// Opções e linhas atuais da sessão, pedidas ao daemon.
pub fn fetch(
    call: &mut dyn FnMut(&Msg) -> anyhow::Result<Msg>,
    id: u32,
) -> Result<(LaunchOptions, Vec<NetLine>), String> {
    let opts = match call(&Msg::List) {
        Ok(Msg::Sessions { sessions }) => sessions
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.opts)
            .ok_or_else(|| trf!("não existe a sessão {id}", "session {id} does not exist"))?,
        Ok(_) => {
            return Err(tr!(
                "resposta inesperada do daemon",
                "unexpected reply from the daemon"
            )
            .into());
        }
        Err(e) => return Err(format!("{e:#}")),
    };
    let lines = match call(&Msg::Net {
        id: Some(id),
        since: None,
        limit: LINES,
    }) {
        Ok(Msg::NetLog { lines }) => lines,
        _ => Vec::new(),
    };
    Ok((opts, lines))
}

/// Grava uma mudança no arquivo de configuração.
fn edit(host: &Host, target: &Target, op: Op) -> Result<bool, String> {
    config::edit::apply(&config::path(host), host, target, &op)
        .map(|o| o.changed)
        .map_err(|e| e.to_string())
}

fn target(ns: &NetScreen, layer: Layer) -> Target {
    match layer {
        Layer::Global => Target::Global,
        _ => Target::Project(ns.project.clone()),
    }
}

/// Tira `item` de `key` em todas as camadas do arquivo que o têm (para ligar
/// de novo um item desligado). Devolve se alguma mudou.
fn remove_everywhere(host: &Host, ns: &NetScreen, key: &str, item: &str) -> bool {
    let mut targets = vec![Target::Global, Target::Project(ns.project.clone())];
    if let Some(p) = &ns.profile {
        targets.push(Target::Profile(p.clone()));
    }
    let mut changed = false;
    for t in targets {
        changed |= edit(host, &t, Op::Remove(key.into(), item.into())).unwrap_or(false);
    }
    changed
}

/// O que fazer com a sessão depois de uma ação: mandar um `NetEdit` ou nada
/// (a mudança foi no arquivo, que o daemon vigia).
pub enum Effect {
    NetEdit { action: &'static str, value: String },
    File(String),
}

/// Aplica a ação pendente na camada escolhida.
pub fn apply(host: &Host, ns: &NetScreen, p: &Pending, layer: Layer) -> Result<Effect, String> {
    match (p, layer) {
        (Pending::Deny(pat), Layer::Tab) => Ok(Effect::NetEdit {
            action: "deny",
            value: pat.clone(),
        }),
        (Pending::Allow(pat), Layer::Tab) => Ok(Effect::NetEdit {
            action: "allow",
            value: pat.clone(),
        }),
        (Pending::Deny(pat), l) => {
            edit(host, &target(ns, l), Op::Add("deny".into(), pat.clone()))?;
            Ok(Effect::File(trf!(
                "{pat} negado em {}",
                "{pat} denied in {}",
                target(ns, l)
            )))
        }
        (Pending::Allow(pat), l) => {
            edit(host, &target(ns, l), Op::Add("allow".into(), pat.clone()))?;
            Ok(Effect::File(trf!(
                "{pat} liberado em {}",
                "{pat} allowed in {}",
                target(ns, l)
            )))
        }
        (Pending::Toggle(_), Layer::Tab) => Err(tr!(
            "itens de catálogo e da allowlist ficam no projeto ou no global",
            "catalog and allowlist items go in the project or the global layer"
        )
        .into()),
        (Pending::Toggle(r), l) => toggle(host, ns, r, l),
    }
}

fn toggle(host: &Host, ns: &NetScreen, r: &Rule, l: Layer) -> Result<Effect, String> {
    let pat = r.pattern.to_string();
    let group = netcatalog::COMMON
        .iter()
        .find(|(g, _)| r.origin == netcatalog::common_origin(g))
        .map(|(g, _)| *g);
    let catalog_item =
        r.origin == netcatalog::EXFIL_ORIGIN || r.origin == netcatalog::INFERENCE_ORIGIN;
    let (key, item, on) = match (group, catalog_item, r.kind) {
        (Some(g), _, _) => ("catalog", g.to_string(), !r.enabled),
        (None, true, _) => ("catalog-off", pat.clone(), r.enabled),
        (None, false, RuleKind::Allow) => ("allow-off", pat.clone(), r.enabled),
        (None, false, RuleKind::Deny) => {
            return Err(tr!(
                "uma regra da denylist sai com x, não se desliga",
                "a denylist rule is removed with x, not turned off"
            )
            .into());
        }
    };
    // `on` = acrescentar a chave na camada; senão, tirar de onde estiver.
    if on {
        edit(host, &target(ns, l), Op::Add(key.into(), item.clone()))?;
    } else if !remove_everywhere(host, ns, key, &item) {
        return Err(trf!(
            "{item} está em `{key}` de uma camada que a TUI não edita; use `sandmux config`",
            "{item} is in `{key}` of a layer the TUI does not edit; use `sandmux config`"
        ));
    }
    Ok(Effect::File(format!(
        "{} {}",
        if r.enabled {
            tr!("desligado:", "turned off:")
        } else {
            tr!("ligado:", "turned on:")
        },
        if key == "catalog" {
            trf!("grupo {item}", "group {item}")
        } else {
            pat
        }
    )))
}

/// Tira a regra selecionada da camada de onde veio.
pub fn forget(host: &Host, r: &Rule) -> Result<Effect, String> {
    if r.origin.starts_with("catálogo") {
        return Err(tr!(
            "item de catálogo: use espaço para ligar ou desligar",
            "catalog item: use space to turn it on or off"
        )
        .into());
    }
    let pat = r.pattern.to_string();
    let key = if r.kind == RuleKind::Deny {
        "deny"
    } else {
        "allow"
    };
    if r.origin == "aba" {
        return Ok(Effect::NetEdit {
            action: "forget",
            value: pat,
        });
    }
    let t = if r.origin == "global" {
        Target::Global
    } else if let Some(p) = r.origin.strip_prefix("projeto ") {
        Target::Project(PathBuf::from(p))
    } else if let Some(n) = r.origin.strip_prefix("perfil ") {
        Target::Profile(n.to_string())
    } else {
        return Err(trf!(
            "origem desconhecida: {}",
            "unknown origin: {}",
            r.origin
        ));
    };
    edit(host, &t, Op::Remove(key.into(), pat.clone()))?;
    Ok(Effect::File(trf!(
        "{pat} tirado de {t}",
        "{pat} removed from {t}"
    )))
}

/// Próximo modo da troca com `m` (sem-rede só no próximo lançamento, por
/// isso fica fora do ciclo).
pub fn next_mode(m: NetworkMode) -> NetworkMode {
    match m {
        NetworkMode::Aberta => NetworkMode::Inferencia,
        NetworkMode::Inferencia => NetworkMode::Custom,
        _ => NetworkMode::Aberta,
    }
}
