//! Desbloqueio do cofre pela senha guardada no keyring do sistema, quando o
//! usuário ligou o keyring (`sandmux vault keyring on`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use sandmux_spec::{tr, trf};

use super::Shared;

/// Intervalo mínimo entre duas tentativas pelo estado do cofre.
const EVERY: Duration = Duration::from_secs(5);

/// O que acontece depois de o cofre ser aberto, pela senha digitada ou pelo
/// keyring: as sessões que esperavam sobem, as regras de rede são
/// recompiladas e o vigia de chaves acorda.
pub fn after_unlock(shared: &Arc<Shared>) {
    shared.secrets.kick();
    super::session::vault_unlocked(shared);
    let s = shared.clone();
    std::thread::spawn(move || super::net::recompile_all(&s));
}

/// Registra a nota do keyring só quando ela muda, para a TUI que pergunta o
/// estado do cofre não encher o registro.
fn note(shared: &Shared, msg: String) {
    let mut last = shared
        .keyring_note
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if last.as_deref() != Some(msg.as_str()) {
        eprintln!("sandmuxd: {msg}");
        *last = Some(msg);
    }
}

/// Tenta abrir o cofre pelo keyring. `force` ignora o intervalo mínimo (na
/// subida do daemon). Sem o keyring ligado, não consulta nada.
pub fn try_unlock(shared: &Arc<Shared>, force: bool) {
    let data_dir = &shared.base.data_dir;
    if !crate::keyring::enabled(data_dir) {
        return;
    }
    {
        let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        if !v.exists() || v.unlocked() {
            return;
        }
    }
    // A trava fica com a tentativa inteira: um pedido de estado que chega
    // durante outra tentativa espera por ela e vê o resultado.
    let mut last = shared
        .keyring_last
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unlocked()
    {
        return;
    }
    if !force && last.is_some_and(|t| t.elapsed() < EVERY) {
        return;
    }
    *last = Some(Instant::now());
    let pw = match crate::keyring::get(data_dir) {
        Ok(Some(pw)) => pw,
        Ok(None) => {
            return note(
                shared,
                tr!(
                    "keyring: ligado, mas sem a senha do cofre; rode `sandmux vault keyring on`",
                    "keyring: on, but without the vault password; run `sandmux vault keyring on`"
                )
                .into(),
            );
        }
        Err(e) => {
            return note(
                shared,
                trf!(
                    "keyring: o cofre fica trancado ({e})",
                    "keyring: the vault stays locked ({e})"
                ),
            );
        }
    };
    let r = {
        let mut v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        if v.unlocked() {
            return;
        }
        v.unlock(&pw)
    };
    drop(last);
    match r {
        Ok(()) => {
            note(
                shared,
                tr!(
                    "keyring: cofre aberto com a senha do keyring",
                    "keyring: vault opened with the keyring password"
                )
                .into(),
            );
            after_unlock(shared);
        }
        Err(_) => note(
            shared,
            tr!(
                "keyring: a senha guardada não abre o cofre; rode `sandmux vault keyring on` com a senha atual",
                "keyring: the stored password does not open the vault; run `sandmux vault keyring on` with the current password"
            )
            .into(),
        ),
    }
}

/// `KeyringOn`: confere a senha com o arquivo, guarda no keyring e, com o
/// cofre trancado, já o abre.
pub fn on(shared: &Arc<Shared>, password: &str) -> Result<(), String> {
    {
        let v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        if !v.exists() {
            return Err(tr!(
                "não há cofre: crie um com `sandmux vault init`",
                "there is no vault: create one with `sandmux vault init`"
            )
            .into());
        }
        v.check(password)?;
    }
    crate::keyring::set(&shared.base.data_dir, password)
        .map_err(|e| trf!("guardar no keyring: {e}", "store in the keyring: {e}"))?;
    *shared
        .keyring_note
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    let opened = {
        let mut v = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        !v.unlocked() && v.unlock(password).is_ok()
    };
    if opened {
        after_unlock(shared);
    }
    Ok(())
}

/// `KeyringStatus`: ligado e, quando ligado, se o keyring responde.
pub fn status(shared: &Arc<Shared>) -> sandmux_proto::Msg {
    let data_dir = &shared.base.data_dir;
    let enabled = crate::keyring::enabled(data_dir);
    let (available, error) = if enabled {
        match crate::keyring::get(data_dir) {
            Ok(Some(_)) => (true, None),
            Ok(None) => (
                true,
                Some(
                    tr!(
                        "o keyring não tem a senha do cofre",
                        "the keyring does not have the vault password"
                    )
                    .into(),
                ),
            ),
            Err(e) => (false, Some(e)),
        }
    } else {
        (false, None)
    };
    sandmux_proto::Msg::KeyringInfo {
        enabled,
        available,
        error,
    }
}

/// Depois do `passwd`: regrava a senha nova no keyring ligado.
pub fn after_passwd(shared: &Arc<Shared>, new: &str) -> Result<(), String> {
    if !crate::keyring::enabled(&shared.base.data_dir) {
        return Ok(());
    }
    crate::keyring::set(&shared.base.data_dir, new).map_err(|e| {
        trf!(
            "a senha do cofre foi trocada, mas o keyring não foi atualizado ({e}); rode `sandmux vault keyring on` com a senha nova",
            "the vault password was changed, but the keyring was not updated ({e}); run `sandmux vault keyring on` with the new password"
        )
    })
}
