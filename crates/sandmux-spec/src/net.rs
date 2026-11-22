//! Política de rede compilada: padrões de destino e regras, como o gateway as
//! recebe. Quem resolve as camadas é o `sandmux-policy`.

use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Nome da máquina, aceito como padrão mesmo sendo um nome "interno".
pub const HOST_NAME: &str = "host.sandmux.internal";

/// Um padrão de destino.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Pattern {
    /// `exemplo.com`.
    Exact(String),
    /// `*.exemplo.com`: qualquer subdomínio, não o próprio domínio.
    Suffix(String),
    /// `10.0.0.5` ou `10.0.0.0/8`.
    Net(Ipv4Addr, u8),
}

fn valid_domain(d: &str) -> bool {
    if d == HOST_NAME {
        return true;
    }
    let labels: Vec<&str> = d.split('.').collect();
    d.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        // Um nome todo de números seria um IP mal escrito.
        && !labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()))
}

impl FromStr for Pattern {
    type Err = String;

    fn from_str(s: &str) -> Result<Pattern, String> {
        let t = s.trim().to_ascii_lowercase();
        let bad = || {
            crate::trf!(
                "padrão de destino inválido: {s:?} (aceitos: exemplo.com, *.exemplo.com, 10.0.0.5 ou 10.0.0.0/8)",
                "invalid destination pattern: {s:?} (accepted: example.com, *.example.com, 10.0.0.5 or 10.0.0.0/8)"
            )
        };
        if let Some((ip, len)) = t.split_once('/') {
            let ip: Ipv4Addr = ip.parse().map_err(|_| bad())?;
            let len: u8 = len.parse().map_err(|_| bad())?;
            if len > 32 {
                return Err(bad());
            }
            return Ok(Pattern::Net(ip, len));
        }
        if let Ok(ip) = t.parse::<Ipv4Addr>() {
            return Ok(Pattern::Net(ip, 32));
        }
        if let Some(rest) = t.strip_prefix("*.") {
            return if valid_domain(rest) {
                Ok(Pattern::Suffix(rest.to_string()))
            } else {
                Err(bad())
            };
        }
        if valid_domain(&t) {
            Ok(Pattern::Exact(t))
        } else {
            Err(bad())
        }
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Exact(d) => write!(f, "{d}"),
            Pattern::Suffix(d) => write!(f, "*.{d}"),
            Pattern::Net(ip, 32) => write!(f, "{ip}"),
            Pattern::Net(ip, n) => write!(f, "{ip}/{n}"),
        }
    }
}

impl Serialize for Pattern {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Pattern, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Pattern {
    /// Casa com um nome (sem diferença de maiúsculas, sem o ponto final).
    pub fn matches_name(&self, name: &str) -> bool {
        let n = name.trim_end_matches('.').to_ascii_lowercase();
        match self {
            Pattern::Exact(d) => n == *d,
            Pattern::Suffix(d) => {
                n.len() > d.len() + 1 && n.ends_with(d.as_str()) && {
                    n.as_bytes()[n.len() - d.len() - 1] == b'.'
                }
            }
            Pattern::Net(..) => n.parse::<Ipv4Addr>().is_ok_and(|ip| self.matches_ip(ip)),
        }
    }

    /// Casa com um IP (só as faixas).
    pub fn matches_ip(&self, ip: Ipv4Addr) -> bool {
        match self {
            Pattern::Net(net, len) => {
                let mask = if *len == 0 { 0 } else { u32::MAX << (32 - len) };
                u32::from(ip) & mask == u32::from(*net) & mask
            }
            _ => false,
        }
    }
}

/// Modo de rede de um sandbox com gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Tudo, menos a denylist.
    Open,
    /// Só os provedores do catálogo de inferência.
    Inference,
    /// Só a allowlist.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    Deny,
    Allow,
}

/// Uma regra efetiva, com a origem (para o registro e as telas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub pattern: Pattern,
    pub kind: RuleKind,
    /// "catálogo de exfiltração", "global", "perfil x", "projeto /p", "aba"…
    pub origin: String,
    pub enabled: bool,
}

/// Política de rede compilada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetPolicy {
    pub mode: PolicyMode,
    pub rules: Vec<Rule>,
    /// Domínios cujo TLS o gateway decifra.
    #[serde(default)]
    pub decrypt: Vec<Pattern>,
    /// Servidores das chaves SSH escolhidas: as conexões SSH só chegam a
    /// eles.
    #[serde(default)]
    pub ssh: Vec<Pattern>,
    /// Portas de entrada: vazia publica todas no `127.0.0.1`; preenchida, só
    /// as dela (e cada uma também no endereço dado).
    #[serde(default)]
    pub ports: Vec<PortRule>,
    /// Portas do host: o `127.0.0.1:porta` do sandbox leva ao do host.
    #[serde(default)]
    pub host_ports: Vec<u16>,
    /// HTTP/2 nas conexões decifradas, quando o cliente e o destino falam.
    #[serde(default = "yes")]
    pub http2: bool,
}

fn yes() -> bool {
    true
}

/// Uma porta de entrada: `8642` ou `8443@0.0.0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PortRule {
    pub port: u16,
    /// Endereço além do `127.0.0.1`; `None` é só o `127.0.0.1`.
    pub addr: Option<Ipv4Addr>,
}

impl std::str::FromStr for PortRule {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || {
            crate::trf!(
                "porta inválida: {s:?}; use 8642 ou 8443@0.0.0.0",
                "invalid port: {s:?}; use 8642 or 8443@0.0.0.0"
            )
        };
        let (p, a) = match s.trim().split_once('@') {
            Some((p, a)) => (p, Some(a.parse::<Ipv4Addr>().map_err(|_| bad())?)),
            None => (s.trim(), None),
        };
        let port: u16 = p.parse().map_err(|_| bad())?;
        if port == 0 {
            return Err(bad());
        }
        Ok(PortRule {
            port,
            addr: a.filter(|a| *a != Ipv4Addr::LOCALHOST),
        })
    }
}

impl std::fmt::Display for PortRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.addr {
            Some(a) => write!(f, "{}@{a}", self.port),
            None => write!(f, "{}", self.port),
        }
    }
}

impl TryFrom<String> for PortRule {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<PortRule> for String {
    fn from(r: PortRule) -> String {
        r.to_string()
    }
}

impl Default for NetPolicy {
    fn default() -> Self {
        NetPolicy {
            mode: PolicyMode::Open,
            rules: Vec::new(),
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        }
    }
}

/// Motivo do bloqueio de uma conexão SSH fora dos servidores das chaves.
pub const SSH_BLOCKED: &str = "ssh fora dos servidores das chaves";

/// O motivo de um bloqueio para exibir no idioma atual. O registro de rede
/// guarda o motivo sempre em português (é dado, comparado e lido de volta);
/// em inglês, os motivos conhecidos são traduzidos só na exibição, e um
/// motivo desconhecido passa como está.
pub fn reason_text(reason: &str) -> String {
    if !crate::lang::is_en() {
        return reason.to_string();
    }
    const FIXED: &[(&str, &str)] = &[
        (SSH_BLOCKED, "ssh outside the key servers"),
        ("fora do modo só inferência", "outside inference-only mode"),
        ("fora da allowlist", "outside the allowlist"),
        ("sem domínio", "no domain"),
        ("metadados de cloud", "cloud metadata"),
    ];
    if let Some((_, en)) = FIXED.iter().find(|(pt, _)| *pt == reason) {
        return (*en).to_string();
    }
    const PREFIXES: &[(&str, &str)] = &[
        ("catálogo de exfiltração", "exfiltration catalog"),
        ("catálogo de inferência", "inference catalog"),
        ("catálogo comum", "common catalog"),
        ("certificado inválido:", "invalid certificate:"),
        ("o host recusou", "the host refused"),
        ("porta do host", "host port"),
        ("publicar em", "publish on"),
    ];
    if let Some((pt, en)) = PREFIXES.iter().find(|(pt, _)| reason.starts_with(pt)) {
        return format!("{en}{}", &reason[pt.len()..]);
    }
    if let Some(rest) = reason
        .strip_prefix("tempo esgotado em ")
        .and_then(|r| r.strip_suffix(" do host"))
    {
        return format!("timed out on the host's {rest}");
    }
    reason.to_string()
}

/// Resultado da decisão sobre um destino.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Block(String),
}

impl NetPolicy {
    fn deny_hit(&self, names: &[&str], ip: Option<Ipv4Addr>) -> Option<&Rule> {
        self.rules.iter().find(|r| {
            r.enabled
                && r.kind == RuleKind::Deny
                && (names.iter().any(|n| r.pattern.matches_name(n))
                    || ip.is_some_and(|ip| r.pattern.matches_ip(ip)))
        })
    }

    fn allowed_name(&self, name: &str) -> bool {
        self.rules
            .iter()
            .any(|r| r.enabled && r.kind == RuleKind::Allow && r.pattern.matches_name(name))
    }

    fn allowed_ip(&self, ip: Ipv4Addr) -> bool {
        self.rules
            .iter()
            .any(|r| r.enabled && r.kind == RuleKind::Allow && r.pattern.matches_ip(ip))
    }

    /// O motivo do bloqueio, quando uma regra da denylist casa com o nome ou
    /// o IP (as portas do host só obedecem à denylist).
    pub fn denied(&self, names: &[&str], ip: Option<Ipv4Addr>) -> Option<String> {
        self.deny_hit(names, ip).map(Self::deny_reason)
    }

    fn deny_reason(r: &Rule) -> String {
        if r.origin.starts_with("catálogo") {
            format!("{}: {}", r.origin, r.pattern)
        } else {
            format!("denylist: {}", r.pattern)
        }
    }

    fn outside(&self) -> String {
        match self.mode {
            PolicyMode::Inference => "fora do modo só inferência".into(),
            _ => "fora da allowlist".into(),
        }
    }

    /// No modo só inferência, com algum provedor da allowlist entre os
    /// domínios `owned` (os dos segredos do cofre), desliga os provedores que
    /// não estão entre eles.
    pub fn restrict_inference(&mut self, owned: &[String]) {
        if self.mode != PolicyMode::Inference {
            return;
        }
        // O domínio dono pode ser um padrão (`*.googleapis.com`), que casa
        // com o provedor do catálogo.
        let has = |r: &Rule| {
            owned.iter().any(|d| {
                r.pattern.matches_name(d)
                    || match (&r.pattern, d.parse::<Pattern>()) {
                        (Pattern::Exact(name), Ok(p)) => p.matches_name(name),
                        _ => false,
                    }
            })
        };
        let any = self
            .rules
            .iter()
            .any(|r| r.kind == RuleKind::Allow && r.enabled && has(r));
        if !any {
            return;
        }
        for r in &mut self.rules {
            if r.kind == RuleKind::Allow && !has(r) {
                r.enabled = false;
            }
        }
    }

    /// Decide uma conexão SSH pelos nomes ligados a ela: só os servidores das
    /// chaves escolhidas.
    pub fn decide_ssh(&self, names: &[&str]) -> Verdict {
        if names
            .iter()
            .any(|n| self.ssh.iter().any(|p| p.matches_name(n)))
        {
            Verdict::Pass
        } else {
            Verdict::Block(SSH_BLOCKED.into())
        }
    }

    /// O TLS deste nome (o SNI) deve ser decifrado?
    pub fn decrypts(&self, sni: &str) -> bool {
        self.decrypt.iter().any(|p| p.matches_name(sni))
    }

    /// Decide uma consulta de DNS por um nome.
    pub fn decide_name(&self, name: &str) -> Verdict {
        if let Some(r) = self.deny_hit(&[name], None) {
            return Verdict::Block(Self::deny_reason(r));
        }
        match self.mode {
            PolicyMode::Open => Verdict::Pass,
            _ if self.allowed_name(name) => Verdict::Pass,
            _ => Verdict::Block(self.outside()),
        }
    }

    /// Decide uma conexão: `sni` é o SNI ou o `Host`, quando há; `names` são
    /// os nomes ligados ao IP, do mais recente ao mais antigo.
    pub fn decide_conn(&self, ip: Ipv4Addr, sni: Option<&str>, names: &[String]) -> Verdict {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        match self.mode {
            PolicyMode::Open => {
                let looked: Vec<&str> = match sni {
                    Some(s) => vec![s],
                    None => names.first().copied().into_iter().collect(),
                };
                match self.deny_hit(&looked, Some(ip)) {
                    Some(r) => Verdict::Block(Self::deny_reason(r)),
                    None => Verdict::Pass,
                }
            }
            PolicyMode::Inference | PolicyMode::Custom => {
                let mut all = names.clone();
                all.extend(sni);
                if let Some(r) = self.deny_hit(&all, Some(ip)) {
                    return Verdict::Block(Self::deny_reason(r));
                }
                let ip_ok = self.allowed_ip(ip) || names.iter().any(|n| self.allowed_name(n));
                if !ip_ok {
                    return if names.is_empty() && !self.allowed_ip(ip) {
                        Verdict::Block("sem domínio".into())
                    } else {
                        Verdict::Block(self.outside())
                    };
                }
                match sni {
                    Some(s) if !self.allowed_name(s) => Verdict::Block(self.outside()),
                    _ => Verdict::Pass,
                }
            }
        }
    }
}

/// A política mais restrita entre várias (`None` é sem rede), para o Docker
/// que as sessões de uma pasta dividem: vale o modo mais restrito (sem rede,
/// só inferência, custom, aberta); entre as do mesmo modo, a denylist é a
/// soma e a allowlist fica com os itens ligados em todas. Sem rede vira
/// custom sem allowlist (nada passa). Sem nenhuma, a aberta padrão.
pub fn strictest(list: &[Option<NetPolicy>]) -> NetPolicy {
    let rank = |p: &Option<NetPolicy>| match p.as_ref().map(|p| p.mode) {
        None => 3,
        Some(PolicyMode::Inference) => 2,
        Some(PolicyMode::Custom) => 1,
        Some(PolicyMode::Open) => 0,
    };
    let Some(top) = list.iter().map(rank).max() else {
        return NetPolicy::default();
    };
    let same: Vec<&NetPolicy> = list
        .iter()
        .filter(|p| rank(p) == top)
        .filter_map(Option::as_ref)
        .collect();
    let mut rules: Vec<Rule> = Vec::new();
    for p in list.iter().flatten() {
        for r in p
            .rules
            .iter()
            .filter(|r| r.enabled && r.kind == RuleKind::Deny)
        {
            if !rules.iter().any(|x| x.pattern == r.pattern) {
                rules.push(r.clone());
            }
        }
    }
    if top == 3 {
        return NetPolicy {
            mode: PolicyMode::Custom,
            rules,
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
    }
    let first = same[0];
    for r in first
        .rules
        .iter()
        .filter(|r| r.enabled && r.kind == RuleKind::Allow)
    {
        let everywhere = same.iter().all(|p| {
            p.rules
                .iter()
                .any(|x| x.enabled && x.kind == RuleKind::Allow && x.pattern == r.pattern)
        });
        if everywhere
            && !rules
                .iter()
                .any(|x| x.kind == RuleKind::Allow && x.pattern == r.pattern)
        {
            rules.push(r.clone());
        }
    }
    NetPolicy {
        mode: first.mode,
        rules,
        decrypt: Vec::new(),
        ssh: Vec::new(),
        ports: Vec::new(),
        host_ports: Vec::new(),
        http2: true,
    }
}

/// Endpoints de login cujas respostas trazem tokens reais: o gateway os
/// guarda no cofre e entrega falsos ao sandbox (harness dono, host e caminho
/// exatos, POST). Cada endpoint só é capturado nas abas do harness dono: o de
/// token do Google também serve ao `gcloud` e a outros clientes.
pub const CAPTURE_ENDPOINTS: &[(&str, &str, &str)] = &[
    ("claude", "platform.claude.com", "/v1/oauth/token"),
    (
        "claude",
        "api.anthropic.com",
        "/api/oauth/claude_cli/create_api_key",
    ),
    ("agy", "oauth2.googleapis.com", "/token"),
];

/// Campos de primeiro nível da resposta que levam token, e o papel de cada
/// um no cofre (`<harness>:<papel>`).
pub const CAPTURE_FIELDS: &[(&str, &str)] = &[
    ("access_token", "access"),
    ("refresh_token", "refresh"),
    ("raw_key", "api-key"),
];

/// Hosts que a aba do Claude Code sempre decifra, para a captura.
pub const CLAUDE_HOSTS: &[&str] = &["platform.claude.com", "api.anthropic.com"];

/// Hosts que a aba do Antigravity CLI sempre decifra: o endpoint de token e
/// as APIs do Google que recebem o token de acesso.
pub const AGY_HOSTS: &[&str] = &["*.googleapis.com"];

/// Os harnesses com login capturado.
pub const LOGIN_HARNESSES: &[&str] = &["claude", "agy"];

/// Os hosts que a aba de `harness` sempre decifra, para a captura.
pub fn capture_hosts(harness: &str) -> &'static [&'static str] {
    match harness {
        "claude" => CLAUDE_HOSTS,
        "agy" => AGY_HOSTS,
        _ => &[],
    }
}

/// O endpoint de captura de `host` e `path` na aba de `harness`, se for um.
pub fn is_capture(harness: &str, host: &str, path: &str) -> bool {
    CAPTURE_ENDPOINTS
        .iter()
        .any(|(h, d, p)| *h == harness && host.eq_ignore_ascii_case(d) && path == *p)
}

/// Os domínios donos de um token capturado (`role`) do login de `harness`.
pub fn login_domains(harness: &str, role: &str) -> Option<&'static [&'static str]> {
    Some(match (harness, role) {
        ("claude", "access") => &["api.anthropic.com", "claude.ai", "platform.claude.com"],
        ("claude", "refresh") => &["platform.claude.com"],
        ("claude", "api-key") => &["api.anthropic.com"],
        ("agy", "access") => &["*.googleapis.com"],
        ("agy", "refresh") => &["oauth2.googleapis.com"],
        _ => return None,
    })
}

/// O prefixo do falso de um token capturado: o do formato do provedor, que
/// o harness pode conferir. `None` deixa o prefixo ser tirado do real.
pub fn login_fake_prefix(harness: &str, role: &str) -> Option<&'static str> {
    match (harness, role) {
        ("agy", "access") => Some("ya29."),
        ("agy", "refresh") => Some("1//"),
        _ => None,
    }
}

/// Prefixos dos falsos de uma identidade de GCP: o do token de acesso e o do
/// refresh token, no formato do Google.
pub const GCP_ACCESS_PREFIX: &str = "ya29.";
pub const GCP_REFRESH_PREFIX: &str = "1//";

/// Variável só de teste que troca o domínio das APIs do Google (por
/// `host.sandmux.internal`, com um servidor local).
pub const GCP_TEST_DOMAIN_ENV: &str = "SANDMUX_TEST_GCP_DOMAIN";

/// O domínio dono de uma identidade de GCP: as APIs do Google, que incluem o
/// endpoint de token (`oauth2.googleapis.com`).
pub fn gcp_domain() -> String {
    std::env::var(GCP_TEST_DOMAIN_ENV)
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "*.googleapis.com".into())
}

/// Prefixos do ID falso de uma identidade da AWS e de uma credencial
/// capturada dentro da aba (temporária, como os `ASIA…` da AWS).
pub const AWS_ID_PREFIX: &str = "AKIASANDMUX";
pub const AWS_CAPTURED_PREFIX: &str = "ASIASANDMUX";

/// Variável só de teste que troca o domínio da AWS (por
/// `host.sandmux.internal`, com um servidor local).
pub const AWS_TEST_DOMAIN_ENV: &str = "SANDMUX_TEST_AWS_DOMAIN";

/// O domínio dono de uma identidade da AWS.
pub fn aws_domain() -> String {
    std::env::var(AWS_TEST_DOMAIN_ENV)
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "*.amazonaws.com".into())
}

/// O sufixo dos nomes sintéticos dos API servers do Kubernetes na aba.
pub const KUBE_SUFFIX: &str = "kube.sandmux.internal";
/// Prefixo do token falso de uma identidade de Kubernetes.
pub const KUBE_FAKE_PREFIX: &str = "sandmux-fake-kube-";

/// O nome de uma identidade de Kubernetes a partir do contexto: minúsculas,
/// `[a-z0-9-]`, sem `-` repetido nem nas pontas, até 40 caracteres (cabe
/// num rótulo de DNS). `None` quando não sobra nada.
pub fn kube_name(context: &str) -> Option<String> {
    let mut out = String::new();
    for c in context.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(40);
    let out = out.trim_end_matches('-').to_string();
    (!out.is_empty()).then_some(out)
}

/// O domínio sintético de uma identidade de Kubernetes.
pub fn kube_domain(name: &str) -> String {
    format!("{name}.{KUBE_SUFFIX}")
}

/// O nome da identidade de um domínio sintético (`kind-dev` de
/// `kind-dev.kube.sandmux.internal`).
pub fn kube_label(host: &str) -> Option<&str> {
    let host = host.trim_end_matches('.');
    let n = host.len().checked_sub(KUBE_SUFFIX.len())?;
    if !host.is_char_boundary(n) || !host[n..].eq_ignore_ascii_case(KUBE_SUFFIX) {
        return None;
    }
    let rest = host[..n].strip_suffix('.')?;
    (!rest.is_empty() && !rest.contains('.')).then_some(rest)
}

/// O endereço de um API server `https://host[:porta][/prefixo]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeServer {
    /// O host, sem colchetes num IPv6.
    pub host: String,
    pub port: u16,
    /// `host:porta` como no URL (o cabeçalho `Host`).
    pub authority: String,
    /// O caminho antes da API (`/k8s/clusters/c-1` no Rancher), sem a `/`
    /// final; vazio quase sempre.
    pub prefix: String,
}

/// Lê o servidor de um cluster; `None` sem `https://` ou sem host.
pub fn kube_server(server: &str) -> Option<KubeServer> {
    let rest = server.trim().strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().ok()?,
            None if after.is_empty() => 443,
            None => return None,
        };
        (h.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (authority.to_string(), 443),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(KubeServer {
        host,
        port,
        authority: authority.to_string(),
        prefix: path.trim_end_matches('/').to_string(),
    })
}

/// Segundos desde 1970 de um instante RFC 3339 (`AAAA-MM-DDTHH:MM:SS[.fff]`
/// com `Z` ou um fuso `±HH:MM`).
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let s = s.trim();
    let (s, offset) = if let Some(x) = s.strip_suffix('Z').or_else(|| s.strip_suffix('z')) {
        (x, 0i64)
    } else {
        let i = s.len().checked_sub(6)?;
        let (main, tz) = s.split_at(i);
        let sign = match tz.as_bytes().first()? {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let (h, m) = tz[1..].split_once(':')?;
        (
            main,
            sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60),
        )
    };
    let (date, time) = s.split_once('T').or_else(|| s.split_once(' '))?;
    let mut d = date.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    // Dias desde 1970 (algoritmo de Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hh * 3600 + mm * 60 + ss - offset;
    u64::try_from(secs).ok()
}

/// O harness dono de um segredo de login (`claude:access` é do `claude`).
pub fn login_owner(secret: &str) -> Option<&'static str> {
    let (h, _) = secret.split_once(':')?;
    LOGIN_HARNESSES.iter().copied().find(|x| *x == h)
}

/// O segredo vale na aba de `harness`? Os de login só na aba do dono.
pub fn secret_for_harness(secret: &str, harness: &str) -> bool {
    login_owner(secret).is_none_or(|h| h == harness)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Pattern {
        s.parse().unwrap()
    }

    fn rule(s: &str, kind: RuleKind) -> Rule {
        Rule {
            pattern: p(s),
            kind,
            origin: "global".into(),
            enabled: true,
        }
    }

    #[test]
    fn captura_so_na_aba_do_dono() {
        assert!(is_capture("agy", "oauth2.googleapis.com", "/token"));
        assert!(!is_capture("shell", "oauth2.googleapis.com", "/token"));
        assert!(!is_capture("claude", "oauth2.googleapis.com", "/token"));
        assert!(is_capture(
            "claude",
            "Platform.Claude.com",
            "/v1/oauth/token"
        ));
        assert!(!is_capture("agy", "platform.claude.com", "/v1/oauth/token"));
        assert!(!is_capture("agy", "oauth2.googleapis.com", "/tokeninfo"));
    }

    #[test]
    fn segredos_de_login_so_no_dono() {
        assert!(secret_for_harness("agy:access", "agy"));
        assert!(!secret_for_harness("agy:access", "shell"));
        assert!(!secret_for_harness("claude:refresh", "agy"));
        assert!(!secret_for_harness("claude:refresh", ""));
        assert!(secret_for_harness("openrouter", "shell"));
        assert!(secret_for_harness("empresa:token", "shell"));
        assert_eq!(
            login_domains("agy", "access"),
            Some(&["*.googleapis.com"][..])
        );
        assert_eq!(login_domains("agy", "api-key"), None);
        assert_eq!(capture_hosts("agy"), AGY_HOSTS);
        assert!(capture_hosts("shell").is_empty());
    }

    #[test]
    fn formatos_aceitos_e_recusados() {
        assert_eq!(p("Exemplo.COM"), Pattern::Exact("exemplo.com".into()));
        assert_eq!(p("*.exemplo.com"), Pattern::Suffix("exemplo.com".into()));
        assert_eq!(p("10.0.0.5"), Pattern::Net(Ipv4Addr::new(10, 0, 0, 5), 32));
        assert_eq!(p("192.168.0.0/16").to_string(), "192.168.0.0/16");
        assert_eq!(
            p("host.sandmux.internal").to_string(),
            "host.sandmux.internal"
        );
        for bad in [
            "http://exemplo.com/x",
            "exemplo",
            "*.com.",
            "-a.com",
            "a..com",
            "10.0.0.0/33",
            "1.2.3",
            "*",
            "ex mplo.com",
            "",
        ] {
            assert!(bad.parse::<Pattern>().is_err(), "{bad}");
        }
        let j = serde_json::to_string(&p("*.a.test")).unwrap();
        assert_eq!(j, "\"*.a.test\"");
        assert_eq!(serde_json::from_str::<Pattern>(&j).unwrap(), p("*.a.test"));
    }

    #[test]
    fn casamento() {
        assert!(p("*.exemplo.com").matches_name("a.exemplo.com"));
        assert!(p("*.exemplo.com").matches_name("A.B.Exemplo.com."));
        assert!(!p("*.exemplo.com").matches_name("exemplo.com"));
        assert!(!p("*.exemplo.com").matches_name("xexemplo.com"));
        assert!(p("exemplo.com").matches_name("EXEMPLO.com"));
        assert!(!p("exemplo.com").matches_name("a.exemplo.com"));
        assert!(p("192.168.0.0/16").matches_ip(Ipv4Addr::new(192, 168, 1, 10)));
        assert!(!p("192.168.0.0/16").matches_ip(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(p("0.0.0.0/0").matches_ip(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!p("a.test").matches_ip(Ipv4Addr::new(1, 1, 1, 1)));
    }

    #[test]
    fn so_inferencia_pelos_provedores_do_cofre() {
        let mut p = NetPolicy {
            mode: PolicyMode::Inference,
            rules: vec![
                rule("openrouter.ai", RuleKind::Allow),
                rule("api.openai.com", RuleKind::Allow),
            ],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        // Sem segredo de provedor: o catálogo inteiro.
        let mut sem = p.clone();
        sem.restrict_inference(&["host.sandmux.internal".into()]);
        assert_eq!(sem.decide_name("api.openai.com"), Verdict::Pass);
        p.restrict_inference(&["openrouter.ai".into()]);
        assert_eq!(p.decide_name("openrouter.ai"), Verdict::Pass);
        assert!(matches!(p.decide_name("api.openai.com"), Verdict::Block(_)));
        // Fora do modo só inferência, nada muda.
        let mut c = NetPolicy {
            mode: PolicyMode::Custom,
            rules: vec![rule("a.test", RuleKind::Allow)],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        c.restrict_inference(&["openrouter.ai".into()]);
        assert_eq!(c.decide_name("a.test"), Verdict::Pass);
    }

    #[test]
    fn so_inferencia_com_o_login_do_agy() {
        let mut p = NetPolicy {
            mode: PolicyMode::Inference,
            rules: vec![
                rule("openrouter.ai", RuleKind::Allow),
                rule("oauth2.googleapis.com", RuleKind::Allow),
                rule("cloudcode-pa.googleapis.com", RuleKind::Allow),
            ],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        // O domínio dono é um padrão: casa com os provedores do Google.
        p.restrict_inference(&["*.googleapis.com".into(), "oauth2.googleapis.com".into()]);
        assert_eq!(p.decide_name("cloudcode-pa.googleapis.com"), Verdict::Pass);
        assert_eq!(p.decide_name("oauth2.googleapis.com"), Verdict::Pass);
        assert!(matches!(p.decide_name("openrouter.ai"), Verdict::Block(_)));
    }

    #[test]
    fn mais_restrita() {
        let pol = |mode, rules: Vec<Rule>| {
            Some(NetPolicy {
                mode,
                rules,
                decrypt: Vec::new(),
                ssh: Vec::new(),
                ports: Vec::new(),
                host_ports: Vec::new(),
                http2: true,
            })
        };
        let open = pol(PolicyMode::Open, vec![rule("a.test", RuleKind::Deny)]);
        let c1 = pol(
            PolicyMode::Custom,
            vec![
                rule("x.test", RuleKind::Allow),
                rule("y.test", RuleKind::Allow),
            ],
        );
        let c2 = pol(
            PolicyMode::Custom,
            vec![
                rule("y.test", RuleKind::Allow),
                rule("b.test", RuleKind::Deny),
            ],
        );
        let s = strictest(&[open.clone(), c1.clone(), c2.clone()]);
        assert_eq!(s.mode, PolicyMode::Custom);
        // Denylist somada de todas; allowlist só com o que as duas custom têm.
        assert_eq!(s.decide_name("y.test"), Verdict::Pass);
        assert!(matches!(s.decide_name("x.test"), Verdict::Block(_)));
        assert!(matches!(s.decide_name("a.test"), Verdict::Block(_)));
        assert!(matches!(s.decide_name("b.test"), Verdict::Block(_)));
        let inf = pol(
            PolicyMode::Inference,
            vec![rule("openrouter.ai", RuleKind::Allow)],
        );
        assert_eq!(strictest(&[c1.clone(), inf]).mode, PolicyMode::Inference);
        // Sem rede: nada passa.
        let none = strictest(&[open.clone(), None]);
        assert!(matches!(
            none.decide_name("qualquer.test"),
            Verdict::Block(_)
        ));
        assert_eq!(strictest(&[]), NetPolicy::default());
        assert_eq!(
            strictest(std::slice::from_ref(&open)).mode,
            PolicyMode::Open
        );
    }

    #[test]
    fn decisoes() {
        let ip = Ipv4Addr::new(203, 0, 113, 5);
        let open = NetPolicy {
            mode: PolicyMode::Open,
            rules: vec![rule("proibido.test", RuleKind::Deny)],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        assert_eq!(open.decide_name("ok.test"), Verdict::Pass);
        assert!(
            matches!(open.decide_name("proibido.test"), Verdict::Block(m) if m == "denylist: proibido.test")
        );
        assert!(matches!(
            open.decide_conn(ip, Some("proibido.test"), &[]),
            Verdict::Block(_)
        ));
        // CDN: o SNI permitido vence o nome antigo do IP.
        assert_eq!(
            open.decide_conn(ip, Some("ok.test"), &["proibido.test".into()]),
            Verdict::Pass
        );
        assert!(matches!(
            open.decide_conn(ip, None, &["proibido.test".into()]),
            Verdict::Block(_)
        ));
        assert_eq!(open.decide_conn(ip, None, &[]), Verdict::Pass);

        let custom = NetPolicy {
            mode: PolicyMode::Custom,
            rules: vec![
                rule("*.exemplo.com", RuleKind::Allow),
                rule("api.exemplo.com", RuleKind::Deny),
                rule("192.168.0.0/16", RuleKind::Allow),
                rule("b.test", RuleKind::Allow),
            ],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        // Negar sempre vence.
        assert_eq!(custom.decide_name("www.exemplo.com"), Verdict::Pass);
        assert!(matches!(
            custom.decide_name("api.exemplo.com"),
            Verdict::Block(_)
        ));
        assert_eq!(
            custom.decide_name("outro.test"),
            Verdict::Block("fora da allowlist".into())
        );
        // SNI forjado num IP que não veio de consulta.
        assert_eq!(
            custom.decide_conn(ip, Some("www.exemplo.com"), &[]),
            Verdict::Block("sem domínio".into())
        );
        // IP com vários nomes: vale o Host liberado.
        let names = vec!["a.test".to_string(), "b.test".to_string()];
        assert_eq!(
            custom.decide_conn(ip, Some("b.test"), &names),
            Verdict::Pass
        );
        assert!(matches!(
            custom.decide_conn(ip, Some("a.test"), &names),
            Verdict::Block(_)
        ));
        assert_eq!(custom.decide_conn(ip, None, &names), Verdict::Pass);
        // Faixa da rede local.
        assert_eq!(
            custom.decide_conn(Ipv4Addr::new(192, 168, 1, 10), None, &[]),
            Verdict::Pass
        );
        // Item desligado não libera.
        let mut off = custom.clone();
        off.rules[3].enabled = false;
        assert!(matches!(
            off.decide_conn(ip, Some("b.test"), &names),
            Verdict::Block(_)
        ));

        let inf = NetPolicy {
            mode: PolicyMode::Inference,
            rules: vec![rule("openrouter.ai", RuleKind::Allow)],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
            http2: true,
        };
        assert_eq!(
            inf.decide_name("exemplo.com"),
            Verdict::Block("fora do modo só inferência".into())
        );
        assert_eq!(inf.decide_name("openrouter.ai"), Verdict::Pass);
    }

    #[test]
    fn ssh_so_para_os_servidores() {
        let mut p = NetPolicy::default();
        assert_eq!(
            p.decide_ssh(&["github.com"]),
            Verdict::Block(SSH_BLOCKED.into())
        );
        p.ssh = vec!["github.com".parse().unwrap()];
        assert_eq!(p.decide_ssh(&["x.com", "github.com"]), Verdict::Pass);
        assert!(matches!(p.decide_ssh(&["gitlab.com"]), Verdict::Block(_)));
        // Política antiga, sem o campo.
        let old: NetPolicy = serde_json::from_str(r#"{"mode":"open","rules":[]}"#).unwrap();
        assert!(old.ssh.is_empty());
    }

    #[test]
    fn regras_de_porta() {
        let r: PortRule = "8642".parse().unwrap();
        assert_eq!((r.port, r.addr), (8642, None));
        let r: PortRule = "8443@0.0.0.0".parse().unwrap();
        assert_eq!(r.addr, Some(Ipv4Addr::UNSPECIFIED));
        assert_eq!(r.to_string(), "8443@0.0.0.0");
        assert_eq!("80@127.0.0.1".parse::<PortRule>().unwrap().addr, None);
        for bad in ["0", "x", "70000", "80@::1", "80@nome"] {
            assert!(bad.parse::<PortRule>().is_err(), "{bad}");
        }
        let p: NetPolicy =
            serde_json::from_str(r#"{"mode":"open","rules":[],"ports":["8443@0.0.0.0"]}"#).unwrap();
        assert_eq!(p.ports[0].port, 8443);
    }

    #[test]
    fn nome_de_kubernetes_do_contexto() {
        assert_eq!(kube_name("kind-dev").as_deref(), Some("kind-dev"));
        assert_eq!(
            kube_name("arn:aws:eks:us-east-1:123:cluster/prod").as_deref(),
            Some("arn-aws-eks-us-east-1-123-cluster-prod")
        );
        assert_eq!(
            kube_name("gke_app-dev_europe-west1_Main").as_deref(),
            Some("gke-app-dev-europe-west1-main")
        );
        assert_eq!(kube_name("--Ünico__").as_deref(), Some("nico"));
        assert_eq!(kube_name("::"), None);
        let long = kube_name(&format!("{}-b", "a".repeat(39))).unwrap();
        assert!(long.len() <= 40 && !long.ends_with('-'), "{long}");
        let d = kube_domain("kind-dev");
        assert_eq!(d, "kind-dev.kube.sandmux.internal");
        assert!(d.parse::<Pattern>().is_ok());
        assert_eq!(kube_label(&d), Some("kind-dev"));
        assert_eq!(
            kube_label("KIND-dev.Kube.Sandmux.Internal."),
            Some("KIND-dev")
        );
        assert_eq!(kube_label("a.b.kube.sandmux.internal"), None);
        assert_eq!(kube_label("kube.sandmux.internal"), None);
        assert_eq!(kube_label("xkube.sandmux.internal"), None);
    }

    #[test]
    fn servidor_do_cluster() {
        let k = kube_server("https://h.example:6443/k8s/c-1/").unwrap();
        assert_eq!(
            (
                k.host.as_str(),
                k.port,
                k.authority.as_str(),
                k.prefix.as_str()
            ),
            ("h.example", 6443, "h.example:6443", "/k8s/c-1")
        );
        let k = kube_server("https://[::1]:6443").unwrap();
        assert_eq!(
            (k.host.as_str(), k.port, k.prefix.as_str()),
            ("::1", 6443, "")
        );
        assert_eq!(kube_server("https://h").unwrap().port, 443);
        assert!(kube_server("http://h:80").is_none());
        assert!(kube_server("https://:80").is_none());
        assert!(kube_server("https://h:x").is_none());
    }

    #[test]
    fn instante_rfc3339() {
        let z = parse_rfc3339("2026-11-22T18:00:00Z").unwrap();
        assert_eq!(parse_rfc3339("2026-11-22T18:00:00+00:00"), Some(z));
        assert_eq!(parse_rfc3339("2026-11-22T15:00:00-03:00"), Some(z));
        assert_eq!(parse_rfc3339("2026-11-22T18:00:00.123Z"), Some(z));
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert!(parse_rfc3339("ontem").is_none());
    }
}
