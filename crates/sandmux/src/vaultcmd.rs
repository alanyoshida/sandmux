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
    /// Guarda uma identidade de cloud: o sandbox recebe um falso, e o real fica fora dele.
    AddCloud {
        /// Provedor: gcp, kubernetes ou aws.
        #[arg(value_name = "PROVEDOR")]
        provedor: String,
        /// Nome da identidade (no gcp); no kubernetes e na aws, o nome vem de cada contexto ou perfil.
        nome: Option<String>,
        /// Contexto do kubeconfig do host (repetível; sem ele, todos os contextos).
        #[arg(long, value_name = "CONTEXTO")]
        context: Vec<String>,
        /// Perfil da AWS do host (repetível; sem ele, todos os perfis).
        #[arg(long, value_name = "PERFIL")]
        profile: Vec<String>,
        /// Copia para o cofre a credencial de cada contexto ou perfil (sem plugin exec nem SSO).
        #[arg(long)]
        copy: bool,
        /// Configuração do gcloud do host (referência: o real vem do gcloud do host).
        #[arg(long, value_name = "CONFIGURAÇÃO")]
        configuration: Option<String>,
        /// Copia para o cofre o ADC do gcloud do host (application_default_credentials.json).
        #[arg(long)]
        copy_adc: bool,
        /// Copia para o cofre a chave JSON de uma conta de serviço.
        #[arg(long, value_name = "ARQUIVO")]
        copy_key: Option<std::path::PathBuf>,
        /// Torna a identidade a padrão do provedor.
        #[arg(long)]
        default: bool,
    },
    /// Lista os segredos (só os nomes e os domínios).
    List,
    /// Apaga um segredo.
    Remove { nome: String },
    /// Troca só o valor de um segredo (uma chave que venceu), mantendo o falso e os domínios.
    Set { nome: String },
    /// Troca a senha do cofre.
    Passwd,
    /// Pede os valores reais das chaves que a importação trocou por valores falsos.
    Import,
    /// Senha do cofre no keyring do sistema, para o daemon abrir o cofre
    /// sozinho. Sem subcomando, mostra o estado.
    Keyring {
        #[command(subcommand)]
        acao: Option<KeyringCmd>,
    },
}

#[derive(Subcommand)]
pub enum KeyringCmd {
    /// Guarda a senha do cofre no keyring, depois de conferi-la.
    On,
    /// Apaga a senha do cofre do keyring.
    Off,
}

fn keyring(acao: Option<KeyringCmd>, input: &mut Input) -> Result<()> {
    match acao {
        None => {
            let Msg::KeyringInfo {
                enabled,
                available,
                error,
            } = call(&Msg::KeyringStatus)?
            else {
                bail!(tr!(
                    "resposta inesperada do daemon",
                    "unexpected answer from the daemon"
                ));
            };
            let text = match (enabled, available, error) {
                (false, _, _) => tr!(
                    "Keyring: desligado (ligue com `sandmux vault keyring on`)",
                    "Keyring: off (turn it on with `sandmux vault keyring on`)"
                )
                .to_string(),
                (true, true, None) => tr!(
                    "Keyring: ligado; o daemon abre o cofre sozinho",
                    "Keyring: on; the daemon opens the vault by itself"
                )
                .to_string(),
                (true, _, e) => trf!(
                    "Keyring: ligado, com problema: {}",
                    "Keyring: on, with a problem: {}",
                    e.unwrap_or_default()
                ),
            };
            println!("{text}");
        }
        Some(KeyringCmd::On) => {
            let p = input.secret(tr!("Senha do cofre: ", "Vault password: "))?;
            call(&Msg::KeyringOn {
                password: p.to_string(),
            })?;
            eprintln!(
                "{}",
                tr!(
                    "sandmux: senha do cofre guardada no keyring; o daemon abre o cofre sozinho.\nQualquer programa seu fora do sandbox pode lê-la enquanto o keyring da sessão está aberto;\n`sandmux vault keyring off` a apaga.",
                    "sandmux: vault password stored in the keyring; the daemon opens the vault by itself.\nAny program of yours outside the sandbox can read it while the session keyring is open;\n`sandmux vault keyring off` deletes it."
                )
            );
        }
        Some(KeyringCmd::Off) => {
            call(&Msg::KeyringOff)?;
            eprintln!(
                "{}",
                tr!(
                    "sandmux: senha do cofre apagada do keyring",
                    "sandmux: vault password deleted from the keyring"
                )
            );
        }
    }
    Ok(())
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

/// Leva ao cofre, pelo daemon, os tokens reais do login do Google do `agy`
/// (`agy:access` e `agy:refresh`) e grava em `target` o arquivo com os
/// falsos. Sem cofre que aceite, nada é gravado.
pub(crate) fn store_agy_login(
    login: &sandmux_import::agy_login::Login,
    target: &std::path::Path,
) -> Result<()> {
    let reals = login.reals();
    let roles: Vec<String> = reals.iter().map(|(r, _)| r.clone()).collect();
    let Msg::Captured { fakes } = call(&Msg::VaultCapture {
        tag: "import".into(),
        harness: "agy".into(),
        items: reals,
    })?
    else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected answer from the daemon"
        ));
    };
    if fakes.len() != roles.len() {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected answer from the daemon"
        ));
    }
    let pairs: Vec<(String, String)> = roles.into_iter().zip(fakes).collect();
    sandmux_import::agy_login::write(target, &login.with_fakes(&pairs))?;
    Ok(())
}

/// A fonte de uma identidade de cloud, para mostrar.
pub(crate) fn cloud_source(c: &sandmux_proto::msg::CloudInfo) -> String {
    match c.source.as_str() {
        "host" if c.provider == "aws" => {
            trf!("host: perfil {}", "host: profile {}", c.configuration)
        }
        "copy" if c.provider == "aws" => {
            trf!("cópia: perfil {}", "copy: profile {}", c.configuration)
        }
        "host" if c.provider == "kubernetes" => {
            trf!("host: contexto {}", "host: context {}", c.configuration)
        }
        "host" => trf!(
            "host: configuração {}",
            "host: configuration {}",
            c.configuration
        ),
        "copy" => trf!("cópia: contexto {}", "copy: context {}", c.configuration),
        "adc" => tr!("cópia: ADC", "copy: ADC").into(),
        "key" => tr!("cópia: conta de serviço", "copy: service account").into(),
        other => other.to_string(),
    }
}

/// A linha de uma identidade de cloud em `vault list`, sem token nem chave.
fn cloud_line(name: &str, fake: &str, c: &sandmux_proto::msg::CloudInfo) -> String {
    if c.provider == "aws" {
        let default = if c.default {
            tr!("  (padrão)", "  (default)")
        } else {
            ""
        };
        let region = if c.region.is_empty() { "-" } else { &c.region };
        return trf!(
            "{name}  cloud  aws  {}  região: {region}{default}  falso: {fake}",
            "{name}  cloud  aws  {}  region: {region}{default}  fake: {fake}",
            cloud_source(c)
        );
    }
    if c.provider == "kubernetes" {
        let default = if c.default {
            tr!("  (padrão)", "  (default)")
        } else {
            ""
        };
        return trf!(
            "{name}  cloud  kubernetes  {}  servidor: {}{default}  falso: {fake}",
            "{name}  cloud  kubernetes  {}  server: {}{default}  fake: {fake}",
            cloud_source(c),
            c.server
        );
    }
    let account = if c.account.is_empty() {
        "-".to_string()
    } else {
        c.account.clone()
    };
    let project = if c.project.is_empty() {
        String::new()
    } else {
        trf!("  projeto: {}", "  project: {}", c.project)
    };
    let default = if c.default {
        tr!("  (padrão)", "  (default)")
    } else {
        ""
    };
    trf!(
        "{name}  cloud  {}  {}  conta: {account}{project}{default}  falso: {fake}",
        "{name}  cloud  {}  {}  account: {account}{project}{default}  fake: {fake}",
        c.provider,
        cloud_source(c)
    )
}

/// `sandmux vault add-cloud`: confere a fonte no host e manda ao daemon.
fn add_cloud(
    provedor: String,
    nome: String,
    configuration: Option<String>,
    copy_adc: bool,
    copy_key: Option<std::path::PathBuf>,
    default: bool,
) -> Result<()> {
    if provedor != "gcp" {
        bail!(trf!(
            "provedor de cloud desconhecido: {provedor}; use gcp",
            "unknown cloud provider: {provedor}; use gcp"
        ));
    }
    let n = usize::from(configuration.is_some())
        + usize::from(copy_adc)
        + usize::from(copy_key.is_some());
    if n != 1 {
        bail!(tr!(
            "use exatamente uma fonte: --configuration, --copy-adc ou --copy-key",
            "use exactly one source: --configuration, --copy-adc or --copy-key"
        ));
    }
    if !sandmux::cloud::valid_name(&nome) {
        bail!(trf!(
            "nome de identidade inválido: {nome:?}; use letras, números, - e _",
            "invalid identity name: {nome:?}; use letters, digits, - and _"
        ));
    }
    let mut info = sandmux_proto::msg::CloudInfo {
        provider: provedor.clone(),
        default,
        ..Default::default()
    };
    let mut secret = Zeroizing::new(String::new());
    if let Some(conf) = configuration {
        let (account, project) =
            sandmux::cloud::host_configuration(&conf).map_err(|e| anyhow!(e))?;
        info.source = "host".into();
        info.configuration = conf;
        info.account = account;
        info.project = project;
    } else if copy_adc {
        let path = gcloud_config_dir().join("application_default_credentials.json");
        let text = Zeroizing::new(
            std::fs::read_to_string(&path)
                .with_context(|| trf!("ler {}", "read {}", path.display()))?,
        );
        info.account = sandmux::cloud::check_adc(&text).map_err(|e| anyhow!(e))?;
        info.project = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["quota_project_id"].as_str().map(str::to_string))
            .unwrap_or_default();
        info.source = "adc".into();
        *secret = text.to_string();
    } else if let Some(file) = copy_key {
        let text = Zeroizing::new(
            std::fs::read_to_string(&file)
                .with_context(|| trf!("ler {}", "read {}", file.display()))?,
        );
        let (account, project) = sandmux::cloud::check_key(&text).map_err(|e| anyhow!(e))?;
        info.source = "key".into();
        info.account = account;
        info.project = project;
        *secret = text.to_string();
    }
    call(&Msg::VaultAddCloud {
        cloud: Box::new(sandmux_proto::msg::CloudAdd {
            name: nome.clone(),
            info,
            secret: secret.to_string(),
        }),
    })?;
    eprintln!(
        "{}",
        trf!(
            "sandmux: identidade {provedor}:{nome} guardada no cofre; cada aba recebe um falso, e o real fica fora do sandbox.\nO agente pode fazer o que o IAM dessa identidade permite: use uma sem permissão de criar chaves de conta de serviço\nnem de ler o Secret Manager (veja docs/cloud.md).",
            "sandmux: identity {provedor}:{nome} stored in the vault; each tab gets a fake, and the real one stays outside the sandbox.\nThe agent can do what the IAM of this identity allows: use one without permission to create service account keys\nor to read Secret Manager (see docs/cloud.md)."
        )
    );
    Ok(())
}

/// `sandmux vault add-cloud kubernetes`: uma identidade por contexto do
/// kubeconfig do host (ou só os de `--context`), por referência ou cópia.
fn add_kube(only: Vec<String>, copy: bool) -> Result<()> {
    use sandmux::kube;
    let kubeconfig = kube::host_kubeconfig();
    let view = kube::view(&kubeconfig, None).map_err(|e| anyhow!(e))?;
    let (all, current) = kube::contexts(&view);
    if all.is_empty() {
        bail!(trf!(
            "o kubeconfig do host ({kubeconfig}) não tem contexto",
            "the host kubeconfig ({kubeconfig}) has no context"
        ));
    }
    for c in &only {
        if !all.iter().any(|x| &x.name == c) {
            bail!(trf!(
                "o contexto {c} não existe no kubeconfig do host",
                "context {c} does not exist in the host kubeconfig"
            ));
        }
    }
    let chosen: Vec<&kube::Context> = all
        .iter()
        .filter(|c| only.is_empty() || only.contains(&c.name))
        .collect();
    // Confere todos antes de guardar algum: com `--context`, uma recusa não
    // deixa metade; com todos os contextos, o que não dá é pulado.
    let mut ready = Vec::new();
    for c in &chosen {
        let checked = kube::material(&view, &c.name).and_then(|m| {
            kube::check(&m, &c.name, copy)?;
            Ok(m)
        });
        let m = match checked {
            Ok(m) => m,
            Err(e) if only.is_empty() => {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmux: contexto {} pulado: {e}",
                        "sandmux: context {} skipped: {e}",
                        c.name
                    )
                );
                continue;
            }
            Err(e) => bail!(e),
        };
        let Some(name) = sandmux_spec::net::kube_name(&c.name) else {
            bail!(trf!(
                "o contexto {:?} não dá um nome de identidade",
                "context {:?} does not give an identity name",
                c.name
            ));
        };
        let secret = if copy {
            Zeroizing::new(kube::copied(&m).map_err(|e| anyhow!(e))?)
        } else {
            Zeroizing::new(String::new())
        };
        ready.push((name, *c, secret));
    }
    let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected answer from the daemon"
        ));
    };
    let mut taken: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    let mut saved = Vec::new();
    for (name, c, secret) in ready {
        let full = format!("kube:{name}");
        if taken.contains(&full) {
            eprintln!(
                "{}",
                trf!(
                    "sandmux: contexto {} pulado: o cofre já tem {full}",
                    "sandmux: context {} skipped: the vault already has {full}",
                    c.name
                )
            );
            continue;
        }
        call(&Msg::VaultAddCloud {
            cloud: Box::new(sandmux_proto::msg::CloudAdd {
                name: name.clone(),
                info: sandmux_proto::msg::CloudInfo {
                    provider: "kubernetes".into(),
                    source: if copy { "copy" } else { "host" }.into(),
                    configuration: c.name.clone(),
                    server: c.server.clone(),
                    namespace: c.namespace.clone(),
                    kubeconfig: if copy {
                        String::new()
                    } else {
                        kubeconfig.clone()
                    },
                    default: c.name == current,
                    ..Default::default()
                },
                secret: secret.to_string(),
            }),
        })?;
        taken.push(full.clone());
        saved.push(format!("  {full}  ({})", c.name));
    }
    if saved.is_empty() {
        eprintln!(
            "{}",
            tr!(
                "sandmux: nenhuma identidade nova de Kubernetes",
                "sandmux: no new Kubernetes identity"
            )
        );
        return Ok(());
    }
    eprintln!(
        "{}\n{}\n{}",
        tr!(
            "sandmux: identidades de Kubernetes guardadas no cofre:",
            "sandmux: Kubernetes identities stored in the vault:"
        ),
        saved.join("\n"),
        tr!(
            "Cada aba recebe um kubeconfig só com falsos, e o real fica fora do sandbox.\nO agente pode fazer o que o RBAC de cada contexto permite: prefira uma ServiceAccount com o mínimo,\ne apague os contextos de produção que o agente não deve ver (`sandmux vault remove kube:<nome>`).",
            "Each tab gets a kubeconfig with fakes only, and the real one stays outside the sandbox.\nThe agent can do what the RBAC of each context allows: prefer a ServiceAccount with the minimum,\nand remove the production contexts the agent must not see (`sandmux vault remove kube:<name>`)."
        )
    );
    Ok(())
}

/// `sandmux vault add-cloud aws`: uma identidade por perfil do host (ou só
/// os de `--profile`), por referência ou cópia.
fn add_aws(only: Vec<String>, copy: bool) -> Result<()> {
    use sandmux::aws;
    let (config, credentials) = aws::host_files();
    let read = |p: &std::path::Path| std::fs::read_to_string(p).unwrap_or_default();
    let all = aws::profiles(&read(&config), &read(&credentials));
    if all.is_empty() {
        bail!(trf!(
            "o host não tem perfil da AWS ({} e {})",
            "the host has no AWS profile ({} and {})",
            config.display(),
            credentials.display()
        ));
    }
    for p in &only {
        if !all.iter().any(|x| &x.name == p) {
            bail!(trf!(
                "o perfil {p} não existe no host",
                "profile {p} does not exist on the host"
            ));
        }
    }
    let chosen: Vec<&aws::Profile> = all
        .iter()
        .filter(|p| only.is_empty() || only.contains(&p.name))
        .collect();
    // A padrão: o perfil `default`; sem ele no host, o de `AWS_PROFILE`.
    // Sem nenhum dos dois, o daemon deixa a padrão com a primeira guardada.
    let default_profile = if all.iter().any(|p| p.name == "default") {
        "default".to_string()
    } else {
        std::env::var("AWS_PROFILE").unwrap_or_default()
    };
    // Confere todos antes de guardar algum: com `--profile`, uma recusa não
    // deixa metade; com todos os perfis, o que não dá é pulado.
    let mut ready = Vec::new();
    for p in &chosen {
        let secret = if copy {
            match aws::copy_key(p) {
                Ok((id, secret)) => Zeroizing::new(
                    serde_json::to_string(&sandmux_proto::msg::AwsCreds {
                        access_key_id: id,
                        secret,
                        ..Default::default()
                    })
                    .map_err(|e| anyhow!(e))?,
                ),
                Err(e) if only.is_empty() => {
                    eprintln!(
                        "{}",
                        trf!(
                            "sandmux: perfil {} pulado: {e}",
                            "sandmux: profile {} skipped: {e}",
                            p.name
                        )
                    );
                    continue;
                }
                Err(e) => bail!(e),
            }
        } else {
            Zeroizing::new(String::new())
        };
        let Some(name) = sandmux_spec::net::kube_name(&p.name) else {
            bail!(trf!(
                "o perfil {:?} não dá um nome de identidade",
                "profile {:?} does not give an identity name",
                p.name
            ));
        };
        ready.push((name, *p, secret));
    }
    let Msg::VaultEntries { entries } = call(&Msg::VaultList)? else {
        bail!(tr!(
            "resposta inesperada do daemon",
            "unexpected answer from the daemon"
        ));
    };
    let mut taken: Vec<String> = entries.into_iter().map(|e| e.name).collect();
    let mut saved = Vec::new();
    for (name, p, secret) in ready {
        let full = format!("aws:{name}");
        if taken.contains(&full) {
            eprintln!(
                "{}",
                trf!(
                    "sandmux: perfil {} pulado: o cofre já tem {full}",
                    "sandmux: profile {} skipped: the vault already has {full}",
                    p.name
                )
            );
            continue;
        }
        call(&Msg::VaultAddCloud {
            cloud: Box::new(sandmux_proto::msg::CloudAdd {
                name: name.clone(),
                info: sandmux_proto::msg::CloudInfo {
                    provider: "aws".into(),
                    source: if copy { "copy" } else { "host" }.into(),
                    configuration: p.name.clone(),
                    region: p.region.clone(),
                    env: if copy { Vec::new() } else { aws::host_env() },
                    default: p.name == default_profile,
                    ..Default::default()
                },
                secret: secret.to_string(),
            }),
        })?;
        taken.push(full.clone());
        saved.push(format!("  {full}  ({})", p.name));
    }
    if saved.is_empty() {
        eprintln!(
            "{}",
            tr!(
                "sandmux: nenhuma identidade nova da AWS",
                "sandmux: no new AWS identity"
            )
        );
        return Ok(());
    }
    eprintln!(
        "{}\n{}\n{}",
        tr!(
            "sandmux: identidades da AWS guardadas no cofre:",
            "sandmux: AWS identities stored in the vault:"
        ),
        saved.join("\n"),
        tr!(
            "Cada aba recebe pares falsos, e o gateway assina cada pedido com a chave real, fora do sandbox.\nO agente pode fazer o que o IAM de cada perfil permite: prefira um papel com o mínimo, sem iam:CreateAccessKey\nnem leitura do Secrets Manager, e apague os perfis de produção que o agente não deve ver (`sandmux vault remove aws:<nome>`).",
            "Each tab gets fake pairs, and the gateway signs each request with the real key, outside the sandbox.\nThe agent can do what the IAM of each profile allows: prefer a role with the minimum, without iam:CreateAccessKey\nor Secrets Manager reads, and remove the production profiles the agent must not see (`sandmux vault remove aws:<name>`)."
        )
    );
    Ok(())
}

/// A pasta de configuração do `gcloud` do host (`CLOUDSDK_CONFIG`, ou a
/// padrão).
fn gcloud_config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("CLOUDSDK_CONFIG").filter(|d| !d.is_empty()) {
        return d.into();
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(".config/gcloud")
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
        VaultCmd::Keyring { acao } => keyring(acao, &mut input)?,
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
        VaultCmd::AddCloud {
            provedor,
            nome,
            context,
            profile,
            copy,
            configuration,
            copy_adc,
            copy_key,
            default,
        } => {
            if provedor == "aws" {
                if nome.is_some()
                    || !context.is_empty()
                    || configuration.is_some()
                    || copy_adc
                    || copy_key.is_some()
                    || default
                {
                    bail!(tr!(
                        "na aws, use só --profile e --copy: o nome vem de cada perfil",
                        "for aws, use only --profile and --copy: the name comes from each profile"
                    ));
                }
                add_aws(profile, copy)?;
            } else if provedor == "kubernetes" {
                if !profile.is_empty() {
                    bail!(tr!(
                        "--profile é da aws; no kubernetes, use --context",
                        "--profile is for aws; for kubernetes, use --context"
                    ));
                }
                if nome.is_some()
                    || configuration.is_some()
                    || copy_adc
                    || copy_key.is_some()
                    || default
                {
                    bail!(tr!(
                        "no kubernetes, use só --context e --copy: o nome vem de cada contexto",
                        "for kubernetes, use only --context and --copy: the name comes from each context"
                    ));
                }
                add_kube(context, copy)?;
            } else {
                if !context.is_empty() || !profile.is_empty() || copy {
                    bail!(tr!(
                        "--context, --profile e --copy são do kubernetes e da aws; no gcp, use --configuration, --copy-adc ou --copy-key",
                        "--context, --profile and --copy are for kubernetes and aws; for gcp, use --configuration, --copy-adc or --copy-key"
                    ));
                }
                let Some(nome) = nome else {
                    bail!(tr!(
                        "falta o nome da identidade",
                        "the identity name is missing"
                    ));
                };
                add_cloud(provedor, nome, configuration, copy_adc, copy_key, default)?;
            }
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
                if let Some(c) = &e.cloud {
                    println!(
                        "{}",
                        cloud_line(&e.name, e.fake.as_deref().unwrap_or("-"), c)
                    );
                    continue;
                }
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
        VaultCmd::Set { nome } => {
            let v = input.secret(&trf!(
                "Valor novo de {nome} (não aparece na tela: cada caractere vira •): ",
                "New value of {nome} (not shown on screen: each character becomes •): "
            ))?;
            if v.is_empty() {
                bail!(tr!(
                    "o valor não pode ser vazio",
                    "the value cannot be empty"
                ));
            }
            call(&Msg::VaultSetValue {
                name: nome.clone(),
                value: v.to_string(),
            })?;
            eprintln!(
                "{}",
                trf!(
                    "sandmux: valor de {nome} trocado; o falso e os domínios continuam os mesmos, e o valor novo vale na próxima conexão",
                    "sandmux: value of {nome} replaced; the fake and the domains stay the same, and the new value applies on the next connection"
                )
            );
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
