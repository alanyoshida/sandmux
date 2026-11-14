//! `sandmux chat` e as conferências que o modal de chat da TUI também usa:
//! o modelo, o cofre e a chave do provedor.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use sandmux::chat::{self, Conversa};
use sandmux::client;
use sandmux_policy::assistant::AssistantConfig;
use sandmux_policy::{Host, config};
use sandmux_proto::{LaunchOptions, Msg};

/// Resultado da conferência antes de criar a aba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Ok,
    /// O cofre existe e está trancado: desbloquear antes.
    Locked,
}

/// Confere o modelo e, pelo daemon, o cofre e a chave do provedor. O erro é
/// a mensagem para o usuário.
pub fn check(
    call: &mut dyn FnMut(&Msg) -> Result<Msg>,
    cfg: &AssistantConfig,
    model: &str,
) -> std::result::Result<Check, String> {
    let (prov, _) = cfg.resolve(model)?;
    let host = prov.host();
    let add = format!("`sandmux vault add {} --domain {host}`", prov.name);
    let reply = call(&Msg::ChatCheck { host: host.clone() }).map_err(|e| format!("{e:#}"))?;
    let Msg::ChatStatus {
        exists,
        unlocked,
        key,
    } = reply
    else {
        return Err(format!("resposta inesperada do daemon: {reply:?}"));
    };
    if !exists {
        if prov.is_known() {
            return Err(format!(
                "o cofre não existe: crie com `sandmux vault init` e guarde a chave do provedor com {add}"
            ));
        }
        return Ok(Check::Ok);
    }
    if !unlocked {
        return Ok(Check::Locked);
    }
    if !key && prov.is_known() {
        return Err(format!(
            "falta a chave do provedor {} no cofre: {add}",
            prov.name
        ));
    }
    Ok(Check::Ok)
}

/// As opções da sessão de uma aba de chat na pasta `dir`.
pub fn tab_options(dir: PathBuf) -> LaunchOptions {
    let mut o = LaunchOptions::new(chat::HARNESS);
    o.workdir = dir;
    o
}

/// Cria a pasta da aba com a primeira mensagem.
pub fn new_tab_dir(model: &str, text: Option<&str>) -> Result<PathBuf> {
    chat::new_dir(&sandmux::paths::state_dir(), &Conversa::new(model, text))
}

/// Apaga a pasta de uma aba que não chegou a ser criada.
pub fn discard_dir(dir: &std::path::Path) {
    chat::remove_dir(&sandmux::paths::state_dir(), dir);
}

pub fn run(model: Option<String>, text: Vec<String>) -> Result<i32> {
    let host = Host::detect()?;
    let cfg = config::Config::load(&config::path(&host))?;
    let model = model
        .or_else(|| cfg.assistant.chat_model.clone())
        .ok_or_else(|| {
            anyhow!("falta o modelo: use `--model <provedor>/<modelo>` ou defina `chat-model` em [assistant]")
        })?;
    cfg.assistant.resolve(&model).map_err(|e| anyhow!(e))?;
    let text = text.join(" ");
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if !tty && text.trim().is_empty() {
        bail!("sem terminal, `sandmux chat` precisa do texto da pergunta");
    }
    let mut conn = client::connect_or_start()?;
    let mut call = |m: &Msg| conn.call(m);
    if check(&mut call, &cfg.assistant, &model).map_err(|e| anyhow!(e))? == Check::Locked {
        if !tty {
            bail!("o cofre está trancado: rode `sandmux vault unlock` antes");
        }
        let mut input = crate::vaultcmd::Input::new();
        let p = input.secret("Senha do cofre: ")?;
        call(&Msg::VaultUnlock {
            password: p.to_string(),
        })?;
        if check(&mut call, &cfg.assistant, &model).map_err(|e| anyhow!(e))? != Check::Ok {
            bail!("o cofre continua trancado");
        }
    }
    if !tty {
        let reply = conn.call(&Msg::Ask {
            model,
            text,
            vars: client::client_vars(&[])?,
            system: None,
        })?;
        let Msg::Answer { text } = reply else {
            bail!("resposta inesperada do daemon: {reply:?}");
        };
        print!("{text}");
        return Ok(0);
    }
    let first = (!text.trim().is_empty()).then_some(text.as_str());
    let dir = new_tab_dir(&model, first)?;
    let created = conn.call(&Msg::Create {
        opts: Box::new(tab_options(dir.clone())),
        vars: client::client_vars(&[])?,
        size: client::terminal_size(),
        attach: true,
    });
    if let Err(e) = created {
        discard_dir(&dir);
        return Err(e);
    }
    client::attached(conn)
}
