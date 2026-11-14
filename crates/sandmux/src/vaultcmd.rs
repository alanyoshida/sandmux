//! `sandmux vault`: o cofre de segredos, pelo daemon.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use sandmux::client;
use sandmux::sshagent;
use sandmux_proto::Msg;
use zeroize::Zeroizing;

#[derive(Subcommand)]
pub enum VaultCmd {
    /// Cria o cofre, pedindo a senha.
    Init,
    /// Desbloqueia o cofre no daemon.
    Unlock,
    /// Tranca o cofre e apaga os segredos da memória do daemon.
    Lock,
    /// Diz se o cofre existe e se está desbloqueado.
    Status,
    /// Guarda um segredo; o valor é lido do terminal sem eco (ou da entrada padrão).
    Add {
        nome: String,
        /// Domínio dono do segredo (padrão de destino). Repetível.
        #[arg(long = "domain", value_name = "PADRÃO")]
        domains: Vec<String>,
        /// Ferramenta que usa o segredo: gh, git, npm, pip, cargo ou docker.
        /// Repetível; sem ela, vem dos domínios conhecidos.
        #[arg(long = "tool", value_name = "FERRAMENTA")]
        tools: Vec<String>,
    },
    /// Guarda uma chave SSH privada (Ed25519, ECDSA ou RSA) para o agente SSH
    /// do sandbox; a passphrase, se houver, é pedida sem eco.
    AddSsh {
        nome: String,
        /// Arquivo da chave privada OpenSSH.
        #[arg(long = "file", value_name = "CHAVE")]
        file: std::path::PathBuf,
        /// Servidor git da chave (domínio exato). Repetível; ao menos um.
        #[arg(long = "server", value_name = "DOMÍNIO", required = true)]
        servers: Vec<String>,
    },
    /// Lista os segredos (só os nomes e os domínios).
    List,
    /// Apaga um segredo.
    Remove { nome: String },
    /// Troca a senha do cofre.
    Passwd,
    /// Pede os valores reais das chaves que a importação trocou por valores falsos.
    Import,
}

/// Linhas lidas da entrada padrão quando ela não é um terminal (para scripts).
pub(crate) struct Input {
    lines: Option<std::io::Lines<std::io::StdinLock<'static>>>,
}

impl Input {
    pub(crate) fn new() -> Input {
        let tty = std::io::stdin().is_terminal();
        Input {
            lines: (!tty).then(|| std::io::stdin().lock().lines()),
        }
    }

    /// Um segredo: do terminal sem eco, ou a próxima linha da entrada.
    pub(crate) fn secret(&mut self, prompt: &str) -> Result<Zeroizing<String>> {
        if let Some(lines) = self.lines.as_mut() {
            let l = lines.next().transpose()?.unwrap_or_default();
            return Ok(Zeroizing::new(l));
        }
        read_hidden(prompt)
    }
}

/// Lê uma linha do terminal com o eco desligado, restaurando o terminal no
/// fim (também num erro).
fn read_hidden(prompt: &str) -> Result<Zeroizing<String>> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
    let stdin = std::io::stdin();
    let old = tcgetattr(&stdin)?;
    let mut t = old.clone();
    t.local_flags.remove(LocalFlags::ECHO);
    t.local_flags.insert(LocalFlags::ECHONL);
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    tcsetattr(&stdin, SetArg::TCSANOW, &t)?;
    let mut line = Zeroizing::new(String::new());
    let r = stdin.lock().read_line(&mut line);
    let _ = tcsetattr(&stdin, SetArg::TCSANOW, &old);
    r?;
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Ok(line)
}

/// Senha nova, pedida duas vezes no terminal.
fn new_password(input: &mut Input, what: &str) -> Result<Zeroizing<String>> {
    let a = input.secret(&format!("{what}: "))?;
    if input.lines.is_none() {
        let b = input.secret(&format!("{what} de novo: "))?;
        if *a != *b {
            bail!("as senhas não conferem");
        }
    }
    if a.is_empty() {
        bail!("a senha não pode ser vazia");
    }
    Ok(a)
}

pub(crate) fn call(msg: &Msg) -> Result<Msg> {
    let mut conn = client::connect_or_start()?;
    conn.call(msg)
}

/// Domínios conhecidos pelo nome de uma chave da importação ou de um
/// segredo guardado sem `--domain`.
pub fn known_domains(key: &str) -> Vec<String> {
    let k = key.to_ascii_lowercase();
    // O segredo do webhook do Telegram é conferido pelo próprio hermes nos
    // pedidos que chegam: fica fora da troca.
    if k.contains("webhook") {
        return Vec::new();
    }
    let table: &[(&str, &[&str])] = &[
        ("openrouter", &["openrouter.ai"]),
        ("anthropic", &["api.anthropic.com"]),
        ("openai", &["api.openai.com"]),
        ("deepseek", &["api.deepseek.com"]),
        ("mistral", &["api.mistral.ai"]),
        ("groq", &["api.groq.com"]),
        ("gemini", &["generativelanguage.googleapis.com"]),
        ("google", &["generativelanguage.googleapis.com"]),
        ("xai", &["api.x.ai"]),
        ("telegram", &["api.telegram.org"]),
        ("discord", &["discord.com", "gateway.discord.gg"]),
        ("slack", &["slack.com"]),
    ];
    table
        .iter()
        .find(|(n, _)| k.contains(n))
        .map(|(_, d)| d.iter().map(|x| x.to_string()).collect())
        .unwrap_or_default()
}

fn import(input: &mut Input) -> Result<i32> {
    let host = sandmux_policy::Host::detect()?;
    let dir = host.data_dir.join("manifests");
    let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
        bail!("resposta inesperada do daemon");
    };
    let known: Vec<String> = entries.iter().filter_map(|e| e.fake.clone()).collect();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut asked = 0;
    let mut added = 0;
    for f in files {
        let m: sandmux_import::Manifest = match std::fs::read(&f)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
        {
            Some(m) => m,
            None => continue,
        };
        for e in &m.entries {
            if known.contains(&e.fake) {
                continue;
            }
            asked += 1;
            let name = format!("{}:{}:{}", m.harness, e.file.display(), e.key);
            let v = input.secret(&format!(
                "{}: {} em {} (linha {}). Valor real (vazio pula): ",
                m.harness,
                e.key,
                e.file.display(),
                e.line
            ))?;
            if v.trim().is_empty() {
                continue;
            }
            call(&Msg::VaultAdd {
                name: name.clone(),
                value: v.trim().to_string(),
                domains: known_domains(&e.key),
                fake: Some(e.fake.clone()),
                tools: Vec::new(),
            })?;
            eprintln!("sandmux: {name} guardado no cofre");
            added += 1;
        }
    }
    if asked == 0 {
        eprintln!("sandmux: nenhuma chave da importação falta no cofre");
    } else {
        eprintln!("sandmux: {added} de {asked} chaves guardadas no cofre");
    }
    Ok(0)
}

pub fn run(c: VaultCmd) -> Result<i32> {
    let mut input = Input::new();
    match c {
        VaultCmd::Init => {
            let p = new_password(&mut input, "Senha do cofre")?;
            call(&Msg::VaultInit {
                password: p.to_string(),
            })?;
            eprintln!("sandmux: cofre criado e desbloqueado");
        }
        VaultCmd::Unlock => {
            let p = input.secret("Senha do cofre: ")?;
            call(&Msg::VaultUnlock {
                password: p.to_string(),
            })?;
            eprintln!("sandmux: cofre desbloqueado");
        }
        VaultCmd::Lock => {
            call(&Msg::VaultLock)?;
            eprintln!("sandmux: cofre trancado");
        }
        VaultCmd::Status => {
            let Msg::VaultInfo {
                exists,
                unlocked,
                secrets,
            } = call(&Msg::VaultStatus)?
            else {
                bail!("resposta inesperada do daemon");
            };
            match (exists, unlocked) {
                (false, _) => println!("Cofre: não existe (crie com `sandmux vault init`)"),
                (true, false) => println!("Cofre: trancado (`sandmux vault unlock`)"),
                (true, true) => println!("Cofre: desbloqueado, {secrets} segredos"),
            }
        }
        VaultCmd::Add {
            nome,
            domains,
            tools,
        } => {
            for d in &domains {
                d.parse::<sandmux_spec::net::Pattern>()
                    .map_err(|e| anyhow!(e))?;
            }
            // Sem `--domain`, um nome conhecido (como `telegram`) traz os domínios.
            let domains = if domains.is_empty() {
                known_domains(&nome)
            } else {
                domains
            };
            let v = input.secret(&format!("Valor de {nome}: "))?;
            if v.is_empty() {
                bail!("o valor não pode ser vazio");
            }
            call(&Msg::VaultAdd {
                name: nome.clone(),
                value: v.to_string(),
                domains,
                fake: None,
                tools,
            })?;
            eprintln!("sandmux: {nome} guardado no cofre");
        }
        VaultCmd::AddSsh {
            nome,
            file,
            servers,
        } => {
            let text = Zeroizing::new(
                std::fs::read_to_string(&file)
                    .with_context(|| format!("ler {}", file.display()))?,
            );
            let pass = if sshagent::is_encrypted(&text).map_err(|e| anyhow!(e))? {
                Some(input.secret(&format!("Passphrase de {}: ", file.display()))?)
            } else {
                None
            };
            let key = Zeroizing::new(
                sshagent::import_key(&text, pass.as_deref().map(|p| p.as_str()))
                    .map_err(|e| anyhow!(e))?,
            );
            call(&Msg::VaultAddSsh {
                name: nome.clone(),
                key: key.to_string(),
                servers,
            })?;
            eprintln!("sandmux: chave SSH {nome} guardada no cofre");
        }
        VaultCmd::List => {
            let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
                bail!("resposta inesperada do daemon");
            };
            if entries.is_empty() {
                println!("Nenhum segredo no cofre.");
            }
            for e in entries {
                if e.kind == "db" {
                    println!("{}  db  (sandmux db list)", e.name);
                    continue;
                }
                if e.kind == "ssh" {
                    println!(
                        "{}  ssh  ({})  {}",
                        e.name,
                        e.domains.join(", "),
                        e.fingerprint
                    );
                    continue;
                }
                let d = if e.domains.is_empty() {
                    "sem domínio".to_string()
                } else {
                    e.domains.join(", ")
                };
                let f = e.fake.as_deref().unwrap_or("-");
                let t = if e.tools.is_empty() {
                    String::new()
                } else {
                    format!("  ferramentas: {}", e.tools.join(", "))
                };
                println!("{}  ({d}){t}  falso: {f}", e.name);
            }
        }
        VaultCmd::Remove { nome } => {
            call(&Msg::VaultRemove { name: nome.clone() })?;
            eprintln!("sandmux: {nome} apagado do cofre");
        }
        VaultCmd::Passwd => {
            let old = input.secret("Senha atual: ")?;
            let new = new_password(&mut input, "Senha nova")?;
            call(&Msg::VaultPasswd {
                old: old.to_string(),
                new: new.to_string(),
            })?;
            eprintln!("sandmux: senha do cofre trocada");
        }
        VaultCmd::Import => return import(&mut input),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominios_pelo_nome_da_chave() {
        assert_eq!(known_domains("OPENROUTER_API_KEY"), vec!["openrouter.ai"]);
        assert_eq!(known_domains("openrouter.key"), vec!["openrouter.ai"]);
        assert!(known_domains("<token>").is_empty());
        assert_eq!(
            known_domains("discord"),
            vec!["discord.com", "gateway.discord.gg"]
        );
        assert_eq!(known_domains("SLACK_APP_TOKEN"), vec!["slack.com"]);
        assert_eq!(
            known_domains("TELEGRAM_BOT_TOKEN"),
            vec!["api.telegram.org"]
        );
        assert!(known_domains("TELEGRAM_WEBHOOK_SECRET").is_empty());
    }
}
