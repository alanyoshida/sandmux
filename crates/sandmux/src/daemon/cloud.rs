//! O real das identidades de cloud no daemon: obtido fora do sandbox, só na
//! memória, com a validade, e renovado antes do fim. O gateway o recebe como
//! uma injeção do host pedido.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sandmux_proto::msg::Injection;

use super::Shared;
use super::vault::CloudIdentity;

/// A folga mínima do real entregue ao gateway.
pub const MARGIN: u64 = 300;
/// Depois de uma falha, quanto esperar antes de tentar o host de novo.
const RETRY_AFTER: Duration = Duration::from_secs(20);

/// Um real obtido, ou a falha, com quando.
enum Entry {
    Real { token: String, expires: u64 },
    Failed { why: String, at: Instant },
}

impl Drop for Entry {
    fn drop(&mut self) {
        if let Entry::Real { token, .. } = self {
            zeroize::Zeroize::zeroize(token);
        }
    }
}

/// O cache dos reais, por identidade, e quem já recebeu o aviso da falha.
#[derive(Default)]
pub struct Cache {
    entries: Mutex<HashMap<String, Entry>>,
    /// Uma trava por identidade: um pedido ao host de cada vez.
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    warned: Mutex<HashSet<String>>,
}

/// O resultado de uma identidade: o real e a validade, ou a falha.
pub type Resolved = Result<(String, u64), String>;

impl Cache {
    /// O real de `id`, com pelo menos [`MARGIN`] de validade em `now`;
    /// `fetch` obtém um novo. Uma falha recente não chama `fetch` de novo.
    pub fn get(
        &self,
        id: &CloudIdentity,
        now: u64,
        fetch: impl FnOnce(&CloudIdentity) -> Result<crate::cloud::Real, String>,
    ) -> Resolved {
        let lock = self
            .locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id.name.clone())
            .or_default()
            .clone();
        let _one = lock.lock().unwrap_or_else(|e| e.into_inner());
        {
            let e = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            match e.get(&id.name) {
                Some(Entry::Real { token, expires }) if *expires >= now + MARGIN => {
                    return Ok((token.clone(), *expires));
                }
                Some(Entry::Failed { why, at }) if at.elapsed() < RETRY_AFTER => {
                    return Err(why.clone());
                }
                _ => {}
            }
        }
        let got = fetch(id);
        let mut e = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        match got {
            Ok(r) => {
                let out = (r.token.clone(), r.expires);
                e.insert(
                    id.name.clone(),
                    Entry::Real {
                        token: r.token,
                        expires: r.expires,
                    },
                );
                Ok(out)
            }
            Err(why) => {
                e.insert(
                    id.name.clone(),
                    Entry::Failed {
                        why: why.clone(),
                        at: Instant::now(),
                    },
                );
                Err(why)
            }
        }
    }

    /// Esquece os reais (o cofre trancado, uma identidade apagada).
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// A primeira falha de uma identidade, desde o último real dela.
    fn first_failure(&self, name: &str) -> bool {
        self.warned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string())
    }

    fn recovered(&self, name: &str) {
        self.warned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
    }
}

/// As injeções das identidades de cloud cujo domínio casa com `host`, com o
/// real atual (ou a falha, sem real). Chamado sem a trava do cofre.
pub fn injections(shared: &Arc<Shared>, host: &str) -> Vec<Injection> {
    if host.is_empty() {
        return Vec::new();
    }
    let ids: Vec<CloudIdentity> = shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .cloud_identities()
        .into_iter()
        .filter(|i| {
            i.domains
                .iter()
                .filter_map(|d| d.parse::<sandmux_spec::net::Pattern>().ok())
                .any(|p| p.matches_name(host))
        })
        .collect();
    ids.iter()
        .map(|id| {
            let r = shared
                .cloud
                .get(id, crate::cloud::now(), |i| crate::cloud::resolve(&i.data));
            let mut inj = Injection {
                name: id.name.clone(),
                domains: id.domains.clone(),
                fake: id.fake.clone(),
                refresh_fake: id.data.info.refresh_fake.clone(),
                ..Default::default()
            };
            match r {
                Ok((token, expires)) => {
                    shared.cloud.recovered(&id.name);
                    inj.value = token;
                    inj.expires = expires;
                }
                Err(why) => {
                    if shared.cloud.first_failure(&id.name) {
                        eprintln!("sandmuxd: {}: {why}", id.name);
                        shared.notify.send(super::notify::Event::Cloud {
                            name: id.name.clone(),
                            why: why.clone(),
                        });
                    }
                    inj.error = why;
                }
            }
            inj
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn id() -> CloudIdentity {
        CloudIdentity {
            name: "gcp:trabalho".into(),
            fake: "ya29.sandmux-fake-a".into(),
            domains: vec!["*.googleapis.com".into()],
            data: Default::default(),
        }
    }

    #[test]
    fn renova_antes_do_fim_da_validade() {
        let c = Cache::default();
        let calls = AtomicU32::new(0);
        let fetch = |exp: u64| {
            let calls = &calls;
            move |_: &CloudIdentity| {
                let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(crate::cloud::Real {
                    token: format!("real-{n}"),
                    expires: exp,
                })
            }
        };
        // Vale até 1000 + 3600: o segundo pedido usa o mesmo.
        assert_eq!(c.get(&id(), 1000, fetch(4600)).unwrap().0, "real-1");
        assert_eq!(c.get(&id(), 2000, fetch(9999)).unwrap().0, "real-1");
        // A menos de 5 minutos do fim, pede um novo antes de seguir.
        assert_eq!(c.get(&id(), 4400, fetch(8000)).unwrap().0, "real-2");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn falha_recente_nao_chama_o_host_de_novo() {
        let c = Cache::default();
        let calls = AtomicU32::new(0);
        for _ in 0..3 {
            let r = c.get(&id(), 1000, |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err("sem gcloud".into())
            });
            assert_eq!(r, Err("sem gcloud".into()));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(c.first_failure("gcp:trabalho"));
        assert!(!c.first_failure("gcp:trabalho"));
        c.recovered("gcp:trabalho");
        assert!(c.first_failure("gcp:trabalho"));
    }
}
