//! `sandmux vault`: o cofre de segredos, pelo daemon.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use sandmux::client;
pub(crate) use sandmux::secretdomains::{
    infer_domains_for, known_domains, normalize_domain, provider_of, synth_text,
};
use sandmux::sshagent;
use sandmux_proto::Msg;
use sandmux_spec::{tr, trf};
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

    /// Uma linha comum (com eco): do terminal, ou a próxima linha da entrada.
    pub(crate) fn line(&mut self, prompt: &str) -> Result<String> {
        if let Some(lines) = self.lines.as_mut() {
            return Ok(lines.next().transpose()?.unwrap_or_default());
        }
        eprint!("{prompt}");
        std::io::stderr().flush()?;
        let mut l = String::new();
        std::io::stdin().lock().read_line(&mut l)?;
        Ok(l.trim_end_matches(['\n', '\r']).to_string())
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

/// Edição de uma linha secreta, byte a byte: o texto e o que mostrar.
#[derive(Default)]
pub(crate) struct Masked {
    bytes: Zeroizing<Vec<u8>>,
    /// Numa sequência de escape (colagem entre marcadores, setas): ignorada.
    escape: bool,
}

/// O que fazer depois de um byte.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Escrever isto no terminal (uma bolinha, ou o apagar de uma).
    Echo(&'static str),
    Done,
    Nothing,
}

impl Masked {
    pub(crate) fn feed(&mut self, b: u8) -> Step {
        if self.escape {
            // Fim de uma sequência CSI: uma letra ou `~`.
            if b.is_ascii_alphabetic() || b == b'~' {
                self.escape = false;
            }
            return Step::Nothing;
        }
        match b {
            b'\r' | b'\n' => Step::Done,
            0x1b => {
                self.escape = true;
                Step::Nothing
            }
            0x7f | 0x08 => {
                // Apaga um caractere inteiro (UTF-8).
                while let Some(last) = self.bytes.pop() {
                    if last & 0xC0 != 0x80 {
                        return Step::Echo("\x08 \x08");
                    }
                }
                Step::Nothing
            }
            0x15 => {
                // Ctrl+U: apaga tudo.
                let n = std::str::from_utf8(&self.bytes)
                    .map(|t| t.chars().count())
                    .unwrap_or(0);
                self.bytes.clear();
                if n == 0 {
                    Step::Nothing
                } else {
                    Step::Echo(ERASE_ALL)
                }
            }
            b if b < 0x20 => Step::Nothing,
            b => {
                self.bytes.push(b);
                // Uma bolinha por caractere: não nos bytes de continuação.
                if b & 0xC0 == 0x80 {
                    Step::Nothing
                } else {
                    Step::Echo("•")
                }
            }
        }
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// Apaga a linha do cursor até o começo do texto digitado (o prompt fica).
const ERASE_ALL: &str = "\x1b[u\x1b[K";

/// Lê uma linha do terminal sem mostrar o texto: cada caractere aparece como
/// `•`, Backspace apaga um e Ctrl+U apaga tudo. O terminal volta ao estado de
/// antes no fim, também num erro; Ctrl+C continua interrompendo.
fn read_hidden(prompt: &str) -> Result<Zeroizing<String>> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
    use std::io::Read;
    let stdin = std::io::stdin();
    let old = tcgetattr(&stdin)?;
    let mut t = old.clone();
    t.local_flags.remove(LocalFlags::ECHO | LocalFlags::ICANON);
    // Salva a posição depois do prompt, para o Ctrl+U voltar a ela.
    eprint!("{prompt}\x1b[s");
    std::io::stderr().flush()?;
    tcsetattr(&stdin, SetArg::TCSANOW, &t)?;
    let mut m = Masked::default();
    let mut lock = stdin.lock();
    let mut b = [0u8; 1];
    let r = loop {
        match lock.read(&mut b) {
            Ok(0) => break Ok(()),
            Ok(_) => match m.feed(b[0]) {
                Step::Done => break Ok(()),
                Step::Echo(s) => {
                    eprint!("{s}");
                    let _ = std::io::stderr().flush();
                }
                Step::Nothing => {}
            },
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => break Err(e),
        }
    };
    let _ = tcsetattr(&stdin, SetArg::TCSANOW, &old);
    eprintln!();
    r?;
    Ok(Zeroizing::new(m.text()))
}

/// Pergunta o domínio dono, quando nada o indica.
pub(crate) fn ask_domain(input: &mut Input) -> Result<Vec<String>> {
    let d = input.line(tr!(
        "Domínio do serviço que recebe esta chave (ex.: api.openai.com; um servidor na sua máquina: localhost; vazio: sem troca no tráfego): ",
        "Domain of the service that receives this key (e.g. api.openai.com; a server on your machine: localhost; empty: no swap in traffic): ",
    ))?;
    let d = d.trim();
    Ok(if d.is_empty() {
        Vec::new()
    } else {
        vec![normalize_domain(d)]
    })
}

/// A linha do arquivo importado em que o falso está, com o falso trocado por
/// `<falso>` e cortada em 100 caracteres.
fn import_context(
    host: &sandmux_policy::Host,
    harness: &str,
    e: &sandmux_import::ManifestEntry,
) -> Option<String> {
    let path = sandmux_policy::Layout::new(&host.data_dir)
        .home(harness)
        .join(&e.file);
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().nth(e.line.checked_sub(1)?)?;
    let line = line.trim().replace(&e.fake, tr!("<falso>", "<fake>"));
    Some(line.chars().take(100).collect())
}

/// Senha nova, pedida duas vezes no terminal.
fn new_password(input: &mut Input, what: &str) -> Result<Zeroizing<String>> {
    let a = input.secret(&format!("{what}: "))?;
    if input.lines.is_none() {
        let b = input.secret(&trf!("{what} de novo: ", "{what} again: "))?;
        if *a != *b {
            bail!(tr!("as senhas não conferem", "the passwords do not match"));
        }
    }
    if a.is_empty() {
        bail!(tr!(
            "a senha não pode ser vazia",
            "the password cannot be empty"
        ));
    }
    Ok(a)
}

pub(crate) fn call(msg: &Msg) -> Result<Msg> {
    let mut conn = client::connect_or_start()?;
    conn.call(msg)
}

fn import(input: &mut Input) -> Result<i32> {
    let host = sandmux_policy::Host::detect()?;
    let dir = host.data_dir.join("manifests");
    let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected answer from the daemon"
        ));
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
            let text = synth_text(&host, &m.harness, &e.file);
            let provider = text.as_deref().and_then(|t| provider_of(t, e.line));
            // Com o provedor no nome, duas chaves iguais do mesmo arquivo (o
            // `key` de cada provedor do `auth.json`) não se sobrescrevem.
            let name = match &provider {
                Some(p) => format!("{}:{}:{}.{}", m.harness, e.file.display(), p, e.key),
                None => format!("{}:{}:{}", m.harness, e.file.display(), e.key),
            };
            // A linha do arquivo importado onde o falso está, para o usuário
            // saber de que chave se trata.
            let context = import_context(&host, &m.harness, e);
            let v = input.secret(&trf!(
                "\n{}{}: a importação trocou a chave `{}` em {} (linha {}) por um valor falso:\n{}Cole o valor real (não aparece na tela: cada caractere vira •; Enter vazio pula): ",
                "\n{}{}: the import replaced the key `{}` in {} (line {}) with a fake value:\n{}Paste the real value (not shown on screen: each character becomes •; empty Enter skips): ",
                m.harness,
                provider
                    .as_deref()
                    .map(|p| trf!(", provedor `{p}`", ", provider `{p}`"))
                    .unwrap_or_default(),
                e.key,
                e.file.display(),
                e.line,
                context
                    .map(|c| format!("    {c}\n"))
                    .unwrap_or_default(),
            ))?;
            if v.trim().is_empty() {
                continue;
            }
            // O dono pelo nome da chave, pelo provedor em volta ou pelo
            // `baseURL` dele; sem nada disso, pergunta (sem domínio, não há
            // troca no tráfego).
            let mut domains = infer_domains_for(
                &host,
                &m.harness,
                &e.key,
                provider.as_deref(),
                text.as_deref(),
            );
            if domains.is_empty() {
                domains = ask_domain(input)?;
            }
            call(&Msg::VaultAdd {
                name: name.clone(),
                value: v.trim().to_string(),
                domains: domains.clone(),
                fake: Some(e.fake.clone()),
                tools: Vec::new(),
            })?;
            if domains.is_empty() {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmux: {name} guardado no cofre, sem domínio: não há troca no tráfego",
                        "sandmux: {name} stored in the vault, no domain: no swap in traffic"
                    )
                );
            } else {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmux: {name} guardado no cofre ({})",
                        "sandmux: {name} stored in the vault ({})",
                        domains.join(", ")
                    )
                );
            }
            added += 1;
        }
    }
    if asked == 0 {
        eprintln!(
            "{}",
            tr!(
                "sandmux: nenhuma chave da importação falta no cofre",
                "sandmux: no imported key is missing from the vault"
            )
        );
    } else {
        eprintln!(
            "{}",
            trf!(
                "sandmux: {added} de {asked} chaves guardadas no cofre",
                "sandmux: {added} of {asked} keys stored in the vault"
            )
        );
    }
    Ok(0)
}

pub fn run(c: VaultCmd) -> Result<i32> {
    let mut input = Input::new();
    match c {
        VaultCmd::Init => {
            let p = new_password(&mut input, tr!("Senha do cofre", "Vault password"))?;
            call(&Msg::VaultInit {
                password: p.to_string(),
            })?;
            eprintln!(
                "{}",
                tr!(
                    "sandmux: cofre criado e desbloqueado",
                    "sandmux: vault created and unlocked"
                )
            );
        }
        VaultCmd::Unlock => {
            let p = input.secret(tr!("Senha do cofre: ", "Vault password: "))?;
            call(&Msg::VaultUnlock {
                password: p.to_string(),
            })?;
            eprintln!(
                "{}",
                tr!("sandmux: cofre desbloqueado", "sandmux: vault unlocked")
            );
        }
        VaultCmd::Lock => {
            call(&Msg::VaultLock)?;
            eprintln!(
                "{}",
                tr!("sandmux: cofre trancado", "sandmux: vault locked")
            );
        }
        VaultCmd::Status => {
            let Msg::VaultInfo {
                exists,
                unlocked,
                secrets,
            } = call(&Msg::VaultStatus)?
            else {
                bail!(tr!(
                    "resposta inesperada do daemon",
                    "unexpected answer from the daemon"
                ));
            };
            match (exists, unlocked) {
                (false, _) => println!(
                    "{}",
                    tr!(
                        "Cofre: não existe (crie com `sandmux vault init`)",
                        "Vault: does not exist (create it with `sandmux vault init`)"
                    )
                ),
                (true, false) => println!(
                    "{}",
                    tr!(
                        "Cofre: trancado (`sandmux vault unlock`)",
                        "Vault: locked (`sandmux vault unlock`)"
                    )
                ),
                (true, true) => println!(
                    "{}",
                    trf!(
                        "Cofre: desbloqueado, {secrets} segredos",
                        "Vault: unlocked, {secrets} secrets"
                    )
                ),
            }
        }
        VaultCmd::Add {
            nome,
            domains,
            tools,
        } => {
            // `localhost` e `127.0.0.1` viram o nome da máquina no sandbox.
            let domains: Vec<String> = domains.iter().map(|d| normalize_domain(d)).collect();
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
            let v = input.secret(&trf!(
                "Valor de {nome} (não aparece na tela: cada caractere vira •): ",
                "Value of {nome} (not shown on screen: each character becomes •): "
            ))?;
            if v.is_empty() {
                bail!(tr!(
                    "o valor não pode ser vazio",
                    "the value cannot be empty"
                ));
            }
            call(&Msg::VaultAdd {
                name: nome.clone(),
                value: v.to_string(),
                domains,
                fake: None,
                tools,
            })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: {nome} guardado no cofre",
                    "sandmux: {nome} stored in the vault"
                )
            );
        }
        VaultCmd::AddSsh {
            nome,
            file,
            servers,
        } => {
            let text = Zeroizing::new(
                std::fs::read_to_string(&file)
                    .with_context(|| trf!("ler {}", "read {}", file.display()))?,
            );
            let pass = if sshagent::is_encrypted(&text).map_err(|e| anyhow!(e))? {
                Some(input.secret(&trf!(
                    "Passphrase de {}: ",
                    "Passphrase for {}: ",
                    file.display()
                ))?)
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
            eprintln!(
                "{}",
                trf!(
                    "sandmux: chave SSH {nome} guardada no cofre",
                    "sandmux: SSH key {nome} stored in the vault"
                )
            );
        }
        VaultCmd::List => {
            let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
                bail!(tr!(
                    "resposta inesperada do daemon",
                    "unexpected answer from the daemon"
                ));
            };
            if entries.is_empty() {
                println!(
                    "{}",
                    tr!("Nenhum segredo no cofre.", "No secrets in the vault.")
                );
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
                    tr!("sem domínio", "no domain").to_string()
                } else {
                    e.domains.join(", ")
                };
                let f = e.fake.as_deref().unwrap_or("-");
                let t = if e.tools.is_empty() {
                    String::new()
                } else {
                    trf!("  ferramentas: {}", "  tools: {}", e.tools.join(", "))
                };
                println!(
                    "{}",
                    trf!(
                        "{}  ({d}){t}  falso: {f}",
                        "{}  ({d}){t}  fake: {f}",
                        e.name
                    )
                );
            }
        }
        VaultCmd::Remove { nome } => {
            call(&Msg::VaultRemove { name: nome.clone() })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: {nome} apagado do cofre",
                    "sandmux: {nome} removed from the vault"
                )
            );
        }
        VaultCmd::Passwd => {
            let old = input.secret(tr!("Senha atual: ", "Current password: "))?;
            let new = new_password(&mut input, tr!("Senha nova", "New password"))?;
            call(&Msg::VaultPasswd {
                old: old.to_string(),
                new: new.to_string(),
            })?;
            eprintln!(
                "{}",
                tr!(
                    "sandmux: senha do cofre trocada",
                    "sandmux: vault password changed"
                )
            );
        }
        VaultCmd::Import => return import(&mut input),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linha_secreta_com_bolinhas() {
        let mut m = Masked::default();
        let mut shown = String::new();
        let mut done = false;
        for b in "sé\x7fcr\x1b[200~et\x1b[201~o\r".bytes() {
            match m.feed(b) {
                Step::Echo(s) => shown.push_str(s),
                Step::Done => {
                    done = true;
                    break;
                }
                Step::Nothing => {}
            }
        }
        assert!(done);
        // "é" (dois bytes) virou uma bolinha e foi apagado inteiro.
        assert_eq!(m.text(), "screto");
        assert_eq!(shown.matches('•').count(), 7);
        assert_eq!(shown.matches("\x08 \x08").count(), 1);
        let mut m = Masked::default();
        for b in b"abc" {
            m.feed(*b);
        }
        assert_eq!(m.feed(0x15), Step::Echo(ERASE_ALL));
        assert_eq!(m.text(), "");
    }
}
