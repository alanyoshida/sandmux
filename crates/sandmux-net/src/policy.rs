//! Estado de decisão do gateway: os nomes que o sandbox consultou para cada
//! IP e a política atual, relida do arquivo que o daemon regrava.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use sandmux_spec::net::NetPolicy;

/// Por quanto tempo um IP fica ligado ao nome consultado.
pub const NAME_TTL: Duration = Duration::from_secs(600);
/// Nomes guardados por IP (um IP de CDN serve vários).
pub const NAMES_PER_IP: usize = 8;
/// Intervalo mínimo entre duas verificações do arquivo de política: zero,
/// porque um `stat` custa pouco perto de uma conexão, e assim a regra nova
/// vale na conexão seguinte.
pub const RECHECK: Duration = Duration::ZERO;

/// IP → nomes consultados, do mais recente ao mais antigo.
#[derive(Default)]
pub struct Names(Mutex<HashMap<Ipv4Addr, Vec<(String, Instant)>>>);

impl Names {
    pub fn remember(&self, answers: &[(String, Ipv4Addr)]) {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() > 10_000 {
            g.retain(|_, v| {
                v.retain(|(_, t)| t.elapsed() < NAME_TTL);
                !v.is_empty()
            });
        }
        for (n, ip) in answers {
            let v = g.entry(*ip).or_default();
            v.retain(|(x, _)| x != n);
            v.insert(0, (n.clone(), Instant::now()));
            v.truncate(NAMES_PER_IP);
        }
    }

    /// Nomes vivos de um IP, do mais recente ao mais antigo.
    pub fn of(&self, ip: Ipv4Addr) -> Vec<String> {
        let g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        g.get(&ip)
            .map(|v| {
                v.iter()
                    .filter(|(_, t)| t.elapsed() < NAME_TTL)
                    .map(|(n, _)| n.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct Current {
    policy: Arc<NetPolicy>,
    checked: Instant,
    stamp: Option<(u64, SystemTime)>,
}

/// A política em vigor, relida do arquivo quando ele muda.
pub struct Policy {
    file: Option<PathBuf>,
    cur: Mutex<Current>,
}

impl Policy {
    pub fn new(initial: NetPolicy, file: Option<PathBuf>) -> Policy {
        let stamp = file.as_ref().and_then(|f| stamp(f));
        // O arquivo, quando existe, vence a da especificação: o daemon soma a
        // ele o que o lançador não sabe (os domínios do cofre).
        let initial = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice::<NetPolicy>(&b).ok())
            .unwrap_or(initial);
        Policy {
            file,
            cur: Mutex::new(Current {
                policy: Arc::new(initial),
                checked: Instant::now(),
                stamp,
            }),
        }
    }

    /// A política atual. Verifica o arquivo no máximo a cada [`RECHECK`]; um
    /// arquivo ilegível ou inválido mantém a anterior.
    pub fn get(&self) -> Arc<NetPolicy> {
        let mut c = self.cur.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(f) = &self.file
            && c.checked.elapsed() >= RECHECK
        {
            c.checked = Instant::now();
            let s = stamp(f);
            if s.is_some() && s != c.stamp {
                if let Some(p) = std::fs::read(f)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<NetPolicy>(&b).ok())
                {
                    c.policy = Arc::new(p);
                }
                c.stamp = s;
            }
        }
        c.policy.clone()
    }
}

fn stamp(f: &std::path::Path) -> Option<(u64, SystemTime)> {
    let m = std::fs::metadata(f).ok()?;
    Some((m.len(), m.modified().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandmux_spec::net::{PolicyMode, Rule, RuleKind, Verdict};

    #[test]
    fn varios_nomes_por_ip() {
        let n = Names::default();
        let ip = Ipv4Addr::new(1, 2, 3, 4);
        n.remember(&[("a.test".into(), ip)]);
        n.remember(&[
            ("b.test".into(), ip),
            ("c.test".into(), Ipv4Addr::new(5, 6, 7, 8)),
        ]);
        n.remember(&[("a.test".into(), ip)]);
        assert_eq!(n.of(ip), vec!["a.test".to_string(), "b.test".to_string()]);
        for i in 0..20 {
            n.remember(&[(format!("x{i}.test"), ip)]);
        }
        assert_eq!(n.of(ip).len(), NAMES_PER_IP);
        assert!(n.of(Ipv4Addr::new(9, 9, 9, 9)).is_empty());
    }

    #[test]
    fn rele_o_arquivo_e_mantem_a_anterior_se_invalido() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("p.json");
        let deny = |p: &str| NetPolicy {
            mode: PolicyMode::Open,
            rules: vec![Rule {
                pattern: p.parse().unwrap(),
                kind: RuleKind::Deny,
                origin: "aba".into(),
                enabled: true,
            }],
            decrypt: Vec::new(),
            ssh: Vec::new(),
            ports: Vec::new(),
            host_ports: Vec::new(),
        };
        std::fs::write(&f, serde_json::to_vec(&deny("a.test")).unwrap()).unwrap();
        let pol = Policy::new(deny("a.test"), Some(f.clone()));
        assert!(matches!(pol.get().decide_name("a.test"), Verdict::Block(_)));
        std::fs::write(&f, serde_json::to_vec(&deny("bb.test")).unwrap()).unwrap();
        let p = pol.get();
        assert_eq!(p.decide_name("a.test"), Verdict::Pass);
        assert!(matches!(p.decide_name("bb.test"), Verdict::Block(_)));
        // Arquivo inválido: fica a anterior.
        std::fs::write(&f, b"{ quebrado").unwrap();
        assert!(matches!(
            pol.get().decide_name("bb.test"),
            Verdict::Block(_)
        ));
        // Sem arquivo: a inicial para sempre.
        let fixed = Policy::new(deny("c.test"), None);
        assert!(matches!(
            fixed.get().decide_name("c.test"),
            Verdict::Block(_)
        ));
    }
}
