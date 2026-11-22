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
    Real(crate::cloud::Real),
    Failed { why: String, at: Instant },
}

impl Drop for Entry {
    fn drop(&mut self) {
        if let Entry::Real(r) = self {
            zeroize::Zeroize::zeroize(&mut r.token);
            if let Some(k) = r.kube.as_mut() {
                zeroize::Zeroize::zeroize(&mut k.key_pem);
            }
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

/// O resultado de uma identidade: o real (com a validade), ou a falha.
pub type Resolved = Result<crate::cloud::Real, String>;

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
                Some(Entry::Real(r)) if r.expires >= now + MARGIN => {
                    return Ok(r.clone());
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
                e.insert(id.name.clone(), Entry::Real(r.clone()));
                Ok(r)
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
                Ok(mut real) => {
                    shared.cloud.recovered(&id.name);
                    inj.value = std::mem::take(&mut real.token);
                    inj.expires = real.expires;
                    inj.kube = real.kube.take();
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
                    kube: None,
                })
            }
        };
        // Vale até 1000 + 3600: o segundo pedido usa o mesmo.
        assert_eq!(c.get(&id(), 1000, fetch(4600)).unwrap().token, "real-1");
        assert_eq!(c.get(&id(), 2000, fetch(9999)).unwrap().token, "real-1");
        // A menos de 5 minutos do fim, pede um novo antes de seguir.
        assert_eq!(c.get(&id(), 4400, fetch(8000)).unwrap().token, "real-2");
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
            assert_eq!(r.err(), Some("sem gcloud".into()));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(c.first_failure("gcp:trabalho"));
        assert!(!c.first_failure("gcp:trabalho"));
        c.recovered("gcp:trabalho");
        assert!(c.first_failure("gcp:trabalho"));
    }

    #[test]
    fn real_sem_validade_relido_em_10_minutos() {
        let c = Cache::default();
        let calls = AtomicU32::new(0);
        let fetch = |now: u64| {
            let calls = &calls;
            move |_: &CloudIdentity| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(crate::cloud::Real {
                    token: String::new(),
                    expires: now + crate::kube::NO_EXPIRY,
                    kube: Some(Default::default()),
                })
            }
        };
        assert!(c.get(&id(), 1000, fetch(1000)).unwrap().kube.is_some());
        c.get(&id(), 1000 + 599, fetch(1599)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        c.get(&id(), 1000 + 601, fetch(1601)).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
