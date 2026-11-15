//! Cofre no daemon: trancado ao subir, aberto só na memória enquanto o
//! usuário não o tranca.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sandmux_proto::msg::VaultEntry;
use sandmux_spec::{tr, trf};
use sandmux_vault::{FileStore, Secret, Store, Unlocked, VaultError};

/// Arquivo do cofre no diretório de dados.
pub fn path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("vault.json")
}

pub struct VaultState {
    store: FileStore,
    open: Option<Unlocked>,
}

fn err(e: VaultError) -> String {
    e.to_string()
}

/// Falso de um token capturado: o prefixo do real até o último `-` dos 15
/// primeiros caracteres (como `sk-ant-oat01-`), mais `sandmux` e letras
/// aleatórias, para passar pelas conferências de formato do cliente.
pub fn capture_fake(real: &str) -> String {
    let head: String = real.chars().take(15).collect();
    let prefix = head.rfind('-').map(|i| &head[..=i]).unwrap_or("");
    let rand = sandmux_vault::new_fake();
    let tail: String = rand.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    format!("{prefix}sandmux{tail}")
}

fn locked() -> &'static str {
    tr!(
        "o cofre está trancado; rode `sandmux vault unlock`",
        "the vault is locked; run `sandmux vault unlock`"
    )
}

fn missing() -> &'static str {
    tr!(
        "não há cofre; crie um com `sandmux vault init`",
        "there is no vault; create one with `sandmux vault init`"
    )
}

impl VaultState {
    pub fn new(data_dir: &std::path::Path) -> VaultState {
        VaultState {
            store: FileStore(path(data_dir)),
            open: None,
        }
    }

    pub fn exists(&self) -> bool {
        self.store.0.exists()
    }

    pub fn unlocked(&self) -> bool {
        self.open.is_some()
    }

    pub fn count(&self) -> usize {
        self.open.as_ref().map_or(0, |o| o.contents.secrets.len())
    }

    fn save(&self) -> Result<(), String> {
        let o = self.open.as_ref().ok_or(locked())?;
        let bytes = o.seal().map_err(err)?;
        self.store
            .save(&bytes)
            .map_err(|e| trf!("gravar o cofre: {e}", "write the vault: {e}"))
    }

    fn opened(&mut self) -> Result<&mut Unlocked, String> {
        if !self.exists() {
            return Err(missing().into());
        }
        self.open.as_mut().ok_or_else(|| locked().to_string())
    }

    pub fn init(&mut self, password: &str) -> Result<(), String> {
        if self.exists() {
            return Err(trf!(
                "o cofre já existe em {}",
                "the vault already exists at {}",
                self.store.0.display()
            ));
        }
        if password.is_empty() {
            return Err(tr!("a senha não pode ser vazia", "the password cannot be empty").into());
        }
        self.open = Some(Unlocked::create(password).map_err(err)?);
        self.save()
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), String> {
        let bytes = self
            .store
            .load()
            .map_err(|e| trf!("ler o cofre: {e}", "read the vault: {e}"))?
            .ok_or(missing())?;
        let mut o = Unlocked::open(&bytes, password).map_err(err)?;
        let mut filled = o.fill_fakes();
        // Segredos gravados antes das ferramentas ganham as dos domínios.
        for s in &mut o.contents.secrets {
            if s.tools.is_empty() && s.is_token() {
                let t = sandmux_policy::presets::tools_for_secret(&s.domains, &s.value);
                if !t.is_empty() {
                    s.tools = t;
                    filled = true;
                }
            }
        }
        self.open = Some(o);
        // Segredos gravados antes do valor falso ganham um.
        if filled {
            self.save()?;
        }
        Ok(())
    }

    /// Confere a senha com o arquivo, sem mexer no cofre aberto.
    pub fn check(&self, password: &str) -> Result<(), String> {
        let bytes = self
            .store
            .load()
            .map_err(|e| trf!("ler o cofre: {e}", "read the vault: {e}"))?
            .ok_or(missing())?;
        Unlocked::open(&bytes, password).map(|_| ()).map_err(err)
    }

    /// Tranca e apaga da memória (o `Drop` do conteúdo zera os valores).
    pub fn lock(&mut self) {
        self.open = None;
    }

    pub fn passwd(&mut self, old: &str, new: &str) -> Result<(), String> {
        if new.is_empty() {
            return Err(tr!("a senha não pode ser vazia", "the password cannot be empty").into());
        }
        let bytes = self
            .store
            .load()
            .map_err(|e| trf!("ler o cofre: {e}", "read the vault: {e}"))?
            .ok_or(missing())?;
        let mut o = Unlocked::open(&bytes, old).map_err(err)?;
        o.change_password(new).map_err(err)?;
        self.open = Some(o);
        self.save()
    }

    pub fn add(&mut self, mut s: Secret) -> Result<(), String> {
        if s.name.trim().is_empty() {
            return Err(tr!(
                "o nome do segredo não pode ser vazio",
                "the secret name cannot be empty"
            )
            .into());
        }
        if let Some(t) = s
            .tools
            .iter()
            .find(|t| !sandmux_policy::presets::TOOLS.contains(&t.as_str()))
        {
            return Err(trf!(
                "ferramenta desconhecida: {t}; use {}",
                "unknown tool: {t}; use {}",
                sandmux_policy::presets::TOOLS.join(", ")
            ));
        }
        if !s.is_token() {
            return Err(tr!(
                "use `sandmux vault add-ssh` ou `sandmux db add` para esse tipo",
                "use `sandmux vault add-ssh` or `sandmux db add` for this kind"
            )
            .into());
        }
        if s.tools.is_empty() {
            s.tools = sandmux_policy::presets::tools_for_secret(&s.domains, &s.value);
        }
        // Canais: o falso imita o formato do token.
        if s.fake.is_none() {
            s.fake =
                sandmux_policy::toolsecrets::channel_fake(&s.tools, &sandmux_vault::new_fake());
        }
        self.opened()?.put(s);
        self.save()
    }

    /// Guarda uma chave SSH sem passphrase com os servidores dela (domínios
    /// exatos).
    pub fn add_ssh(
        &mut self,
        name: String,
        key: String,
        servers: Vec<String>,
    ) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err(tr!(
                "o nome da chave não pode ser vazio",
                "the key name cannot be empty"
            )
            .into());
        }
        crate::sshagent::parse_key(&key)?;
        if servers.is_empty() {
            return Err(tr!(
                "falta o servidor da chave: use --server <domínio>",
                "the key server is missing: use --server <domain>"
            )
            .into());
        }
        for s in &servers {
            if !matches!(s.parse(), Ok(sandmux_spec::net::Pattern::Exact(_))) {
                return Err(trf!(
                    "servidor inválido: {s}; use um domínio exato, como github.com",
                    "invalid server: {s}; use an exact domain, like github.com"
                ));
            }
        }
        self.opened()?.put(Secret {
            name,
            value: key,
            domains: servers,
            fake: None,
            tools: Vec::new(),
            kind: sandmux_vault::KIND_SSH.into(),
        });
        self.save()
    }

    /// As chaves SSH de `names` (`*` é todas) presentes no cofre aberto.
    pub fn ssh_keys(&self, names: &[String]) -> Vec<crate::sshagent::AgentKey> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        let all = names.iter().any(|n| n == "*");
        o.contents
            .secrets
            .iter()
            .filter(|s| s.is_ssh() && (all || names.contains(&s.name)))
            .filter_map(|s| {
                Some(crate::sshagent::AgentKey {
                    name: s.name.clone(),
                    key: crate::sshagent::parse_key(&s.value).ok()?,
                })
            })
            .collect()
    }

    /// Os servidores das chaves SSH de `names` (`*` é todas), com o cofre
    /// aberto.
    pub fn ssh_servers(&self, names: &[String]) -> Vec<String> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        let all = names.iter().any(|n| n == "*");
        let mut out: Vec<String> = Vec::new();
        for s in o
            .contents
            .secrets
            .iter()
            .filter(|s| s.is_ssh() && (all || names.contains(&s.name)))
        {
            for d in &s.domains {
                if !out.contains(d) {
                    out.push(d.clone());
                }
            }
        }
        out
    }

    /// Os nomes de `names` que não são chaves SSH do cofre aberto (`None`
    /// com ele trancado).
    pub fn unknown_ssh(&self, names: &[String]) -> Option<Vec<String>> {
        let o = self.open.as_ref()?;
        Some(
            names
                .iter()
                .filter(|n| {
                    *n != "*"
                        && !o
                            .contents
                            .secrets
                            .iter()
                            .any(|s| s.is_ssh() && &s.name == *n)
                })
                .cloned()
                .collect(),
        )
    }

    /// Guarda uma conexão de banco (o JSON com a senha fica cifrado).
    pub fn add_db(&mut self, c: sandmux_proto::msg::DbConn) -> Result<(), String> {
        if c.name.trim().is_empty() || c.name.contains(char::is_whitespace) {
            return Err(trf!(
                "nome de conexão inválido: {:?}",
                "invalid connection name: {:?}",
                c.name
            ));
        }
        if c.kind != "postgres" && c.kind != "mysql" {
            return Err(trf!(
                "tipo de banco desconhecido: {}; use postgres ou mysql",
                "unknown database kind: {}; use postgres or mysql",
                c.kind
            ));
        }
        if c.host.is_empty() || c.database.is_empty() || c.user.is_empty() {
            return Err(tr!(
                "faltam o host, o banco ou o usuário",
                "the host, the database or the user is missing"
            )
            .into());
        }
        let value = serde_json::to_string(&c).map_err(|e| e.to_string())?;
        self.opened()?.put(Secret {
            name: c.name,
            value,
            domains: Vec::new(),
            fake: None,
            tools: Vec::new(),
            kind: sandmux_vault::KIND_DB.into(),
        });
        self.save()
    }

    /// As conexões de banco, sem as senhas.
    pub fn db_list(&mut self) -> Result<Vec<sandmux_proto::msg::DbEntry>, String> {
        Ok(self
            .opened()?
            .contents
            .secrets
            .iter()
            .filter(|s| s.kind == sandmux_vault::KIND_DB)
            .filter_map(|s| serde_json::from_str::<sandmux_proto::msg::DbConn>(&s.value).ok())
            .map(|c| sandmux_proto::msg::DbEntry {
                name: c.name,
                kind: c.kind,
                host: c.host,
                port: c.port,
                database: c.database,
                user: c.user,
            })
            .collect())
    }

    /// As conexões de `names` (`*` é todas) do cofre aberto, com as senhas.
    pub fn db_conns(&self, names: &[String]) -> Vec<sandmux_proto::msg::DbConn> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        let all = names.iter().any(|n| n == "*");
        o.contents
            .secrets
            .iter()
            .filter(|s| s.kind == sandmux_vault::KIND_DB && (all || names.contains(&s.name)))
            .filter_map(|s| serde_json::from_str(&s.value).ok())
            .collect()
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if !self.opened()?.remove(name) {
            return Err(trf!(
                "não existe o segredo {name:?} no cofre",
                "secret {name:?} is not in the vault"
            ));
        }
        self.save()
    }

    /// Troca os domínios de um token, ou os servidores de uma chave SSH,
    /// validados como na criação; o valor não muda.
    pub fn set_domains(&mut self, name: &str, domains: Vec<String>) -> Result<(), String> {
        let domains: Vec<String> = domains
            .into_iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect();
        let o = self.opened()?;
        let Some(s) = o.contents.secrets.iter_mut().find(|s| s.name == name) else {
            return Err(trf!(
                "não existe o segredo {name:?} no cofre",
                "secret {name:?} is not in the vault"
            ));
        };
        if s.is_ssh() {
            if domains.is_empty() {
                return Err(tr!(
                    "uma chave SSH precisa de ao menos um servidor",
                    "an SSH key needs at least one server"
                )
                .into());
            }
            for d in &domains {
                if !matches!(d.parse(), Ok(sandmux_spec::net::Pattern::Exact(_))) {
                    return Err(trf!(
                        "servidor inválido: {d}; use um domínio exato, como github.com",
                        "invalid server: {d}; use an exact domain, like github.com"
                    ));
                }
            }
        } else if s.is_token() {
            for d in &domains {
                if let Err(e) = d.parse::<sandmux_spec::net::Pattern>() {
                    return Err(trf!("domínio inválido {d}: {e}", "invalid domain {d}: {e}"));
                }
            }
        } else {
            return Err(tr!(
                "use `sandmux db` para as conexões de banco",
                "use `sandmux db` for database connections"
            )
            .into());
        }
        s.domains = domains;
        self.save()
    }

    /// Guarda um token capturado numa resposta de login (`role`: `access`,
    /// `refresh` ou `api-key`) como `claude:<role>`, e devolve o falso dele:
    /// o mesmo de antes quando o segredo já existe, senão um novo com o
    /// prefixo do real.
    pub fn capture(&mut self, role: &str, real: &str) -> Result<String, String> {
        let domains: &[&str] = match role {
            "access" => &["api.anthropic.com", "claude.ai", "platform.claude.com"],
            "refresh" => &["platform.claude.com"],
            "api-key" => &["api.anthropic.com"],
            other => {
                return Err(trf!(
                    "papel de token desconhecido: {other}",
                    "unknown token role: {other}"
                ));
            }
        };
        let name = format!("claude:{role}");
        let o = self.opened()?;
        let fake = match o.contents.secrets.iter_mut().find(|s| s.name == name) {
            Some(s) => {
                s.value = real.to_string();
                s.fake.clone().unwrap_or_default()
            }
            None => String::new(),
        };
        let fake = if fake.is_empty() {
            let fake = capture_fake(real);
            o.put(Secret {
                name,
                value: real.to_string(),
                domains: domains.iter().map(|d| d.to_string()).collect(),
                fake: Some(fake.clone()),
                tools: Vec::new(),
                kind: String::new(),
            });
            fake
        } else {
            fake
        };
        self.save()?;
        Ok(fake)
    }

    pub fn list(&mut self) -> Result<Vec<VaultEntry>, String> {
        Ok(self
            .opened()?
            .contents
            .secrets
            .iter()
            .map(|s| VaultEntry {
                name: s.name.clone(),
                domains: s.domains.clone(),
                fake: s.fake.clone(),
                tools: s.tools.clone(),
                kind: s.kind.clone(),
                fingerprint: if s.is_ssh() {
                    crate::sshagent::parse_key(&s.value)
                        .map(|k| crate::sshagent::fingerprint(&k))
                        .unwrap_or_default()
                } else {
                    String::new()
                },
            })
            .collect())
    }

    /// Os segredos com domínio e falso, com os valores reais (vazio com o
    /// cofre trancado).
    pub fn injections(&self) -> Vec<sandmux_proto::msg::Injection> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        o.contents
            .secrets
            .iter()
            .filter(|s| !s.domains.is_empty() && s.is_token())
            .filter_map(|s| {
                Some(sandmux_proto::msg::Injection {
                    name: s.name.clone(),
                    domains: s.domains.clone(),
                    fake: s.fake.clone()?,
                    value: s.value.clone(),
                })
            })
            .collect()
    }

    /// O valor falso do segredo dono de `host`, com o cofre desbloqueado.
    pub fn fake_for(&self, host: &str) -> Option<String> {
        let o = self.open.as_ref()?;
        let s = o.contents.secrets.iter().find(|s| {
            s.is_token()
                && s.domains.iter().any(|d| {
                    d.parse::<sandmux_spec::net::Pattern>()
                        .is_ok_and(|p| p.matches_name(host))
                })
        })?;
        s.fake.clone()
    }

    /// As chaves falsas dos provedores conhecidos que têm segredo no cofre:
    /// (variável, valor falso).
    pub fn fake_env(&self) -> Vec<(String, String)> {
        sandmux_policy::presets::PROVIDERS
            .iter()
            .filter_map(|(var, domain)| Some((var.to_string(), self.fake_for(domain)?)))
            .collect()
    }

    /// Os valores falsos para as ferramentas: um por domínio exato de cada
    /// segredo (curingas e redes não servem às ferramentas).
    pub fn tool_secrets(&self) -> Vec<sandmux_policy::toolsecrets::ToolSecret> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for s in &o.contents.secrets {
            let Some(fake) = &s.fake else { continue };
            for d in &s.domains {
                let Ok(sandmux_spec::net::Pattern::Exact(domain)) = d.parse() else {
                    continue;
                };
                for t in &s.tools {
                    out.push(sandmux_policy::toolsecrets::ToolSecret {
                        tool: t.clone(),
                        domain: domain.clone(),
                        fake: fake.clone(),
                    });
                }
            }
        }
        out
    }

    /// Domínios donos dos segredos de uma ferramenta, com o cofre desbloqueado.
    pub fn tool_domains(&self, tool: &str) -> Vec<String> {
        self.open
            .as_ref()
            .map(|o| {
                o.contents
                    .secrets
                    .iter()
                    .filter(|s| s.tools.iter().any(|t| t == tool))
                    .flat_map(|s| s.domains.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Domínios donos dos segredos, com o cofre desbloqueado.
    pub fn domains(&self) -> Vec<String> {
        self.open
            .as_ref()
            .map(|o| {
                o.contents
                    .secrets
                    .iter()
                    .filter(|s| s.is_token())
                    .flat_map(|s| s.domains.iter().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Guarda os valores de `pass-env` de uma sessão. `false` quando o cofre
    /// não existe ou está trancado.
    pub fn put_pass_env(&mut self, id: u32, vars: BTreeMap<String, String>) -> bool {
        let Ok(o) = self.opened() else {
            return false;
        };
        o.contents.pass_env.insert(id, vars);
        self.save().is_ok()
    }

    /// Valores de `pass-env` guardados de uma sessão.
    pub fn pass_env(&self, id: u32) -> Option<BTreeMap<String, String>> {
        self.open.as_ref()?.contents.pass_env.get(&id).cloned()
    }

    /// Tira os valores de uma sessão (no `kill`).
    pub fn forget_session(&mut self, id: u32) {
        if let Ok(o) = self.opened()
            && o.contents.pass_env.remove(&id).is_some()
        {
            let _ = self.save();
        }
    }

    /// Tira os valores de sessões que não existem mais.
    pub fn prune(&mut self, alive: &[u32]) {
        if let Ok(o) = self.opened() {
            let before = o.contents.pass_env.len();
            o.contents.pass_env.retain(|id, _| alive.contains(id));
            if o.contents.pass_env.len() != before {
                let _ = self.save();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh_key(dir: &std::path::Path) -> String {
        let f = dir.join("id");
        let ok = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&f)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        std::fs::read_to_string(f).unwrap()
    }

    #[test]
    fn chave_ssh_fora_da_troca() {
        let d = tempfile::tempdir().unwrap();
        let mut v = VaultState::new(d.path());
        v.init("s").unwrap();
        v.add_ssh(
            "github".into(),
            ssh_key(d.path()),
            vec!["github.com".into()],
        )
        .unwrap();
        v.add(Secret {
            name: "or".into(),
            value: "sk".into(),
            domains: vec!["openrouter.ai".into()],
            fake: None,
            tools: Vec::new(),
            kind: String::new(),
        })
        .unwrap();
        assert!(
            v.add(Secret {
                name: "x".into(),
                value: "y".into(),
                domains: Vec::new(),
                fake: None,
                tools: Vec::new(),
                kind: sandmux_vault::KIND_SSH.into(),
            })
            .is_err()
        );
        assert_eq!(v.domains(), vec!["openrouter.ai".to_string()]);
        assert_eq!(v.injections().len(), 1);
        assert!(v.tool_secrets().is_empty());
        assert_eq!(v.tool_domains("git"), Vec::<String>::new());
        let list = v.list().unwrap();
        let gh = list.iter().find(|e| e.name == "github").unwrap();
        assert_eq!((gh.kind.as_str(), gh.fake.as_deref()), ("ssh", None));
        assert!(gh.fingerprint.starts_with("SHA256:"));

        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(v.ssh_keys(&names(&["github"])).len(), 1);
        assert_eq!(v.ssh_keys(&names(&["*"])).len(), 1);
        assert!(
            v.ssh_keys(&names(&["or"])).is_empty(),
            "token não é chave SSH"
        );
        assert_eq!(v.ssh_servers(&names(&["*"])), names(&["github.com"]));
        assert_eq!(
            v.unknown_ssh(&names(&["github", "outra"])),
            Some(names(&["outra"]))
        );
        assert!(
            v.add_ssh("y".into(), "lixo".into(), vec!["a.com".into()])
                .is_err()
        );
        v.lock();
        assert!(v.ssh_keys(&names(&["*"])).is_empty());
        assert!(v.ssh_servers(&names(&["*"])).is_empty());
        assert_eq!(v.unknown_ssh(&names(&["outra"])), None);
    }

    #[test]
    fn tokens_capturados_com_falso_estavel() {
        let d = tempfile::tempdir().unwrap();
        let mut v = VaultState::new(d.path());
        // Trancado: recusa.
        assert!(v.capture("access", "sk-ant-oat01-REAL1").is_err());
        v.init("s").unwrap();
        let f1 = v.capture("access", "sk-ant-oat01-REAL1").unwrap();
        assert!(f1.starts_with("sk-ant-oat01-sandmux"), "{f1}");
        assert!(!f1.contains("REAL"));
        // A renovação troca o valor e mantém o falso.
        let f2 = v.capture("access", "sk-ant-oat01-REAL2").unwrap();
        assert_eq!(f1, f2);
        let inj = v.injections();
        let a = inj.iter().find(|i| i.name == "claude:access").unwrap();
        assert_eq!(a.value, "sk-ant-oat01-REAL2");
        assert!(a.domains.iter().any(|d| d == "api.anthropic.com"));
        let r = v.capture("refresh", "sk-ant-ort01-R").unwrap();
        assert!(r.starts_with("sk-ant-ort01-sandmux"));
        assert!(v.capture("outro", "x").is_err());
        assert!(capture_fake("semhifen").starts_with("sandmux"));
    }
}
