//! Senha do cofre no keyring do sistema, pelo Secret Service (gnome-keyring,
//! KWallet, KeePassXC). Opcional: só com o marcador `vault.keyring` na pasta
//! de dados o daemon consulta o keyring. O item fica com o serviço `sandmux`
//! e o usuário igual à pasta de dados, para duas instalações (ou os testes)
//! não se misturarem.

use std::path::{Path, PathBuf};
use std::time::Duration;

use zeroize::Zeroizing;

const SERVICE: &str = "sandmux";

/// Quanto esperar o keyring responder antes de desistir.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// O marcador de keyring ligado (sem nada secreto dentro).
pub fn marker(data_dir: &Path) -> PathBuf {
    data_dir.join("vault.keyring")
}

/// O usuário ligou o keyring.
pub fn enabled(data_dir: &Path) -> bool {
    marker(data_dir).is_file()
}

/// Roda `f` numa thread própria, com o limite de tempo: o Secret Service
/// pode demorar (um diálogo do keyring), e o chamador não pode ficar preso.
fn with_timeout<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("keyring".into())
        .spawn(move || {
            let _ = tx.send(f());
        });
    rx.recv_timeout(TIMEOUT).unwrap_or_else(|_| {
        Err(sandmux_spec::tr!(
            "o keyring não respondeu a tempo",
            "the keyring did not answer in time"
        )
        .into())
    })
}

fn entry(data_dir: &Path) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &data_dir.display().to_string()).map_err(|e| e.to_string())
}

/// A senha guardada; `None` quando o keyring não tem o item.
pub fn get(data_dir: &Path) -> Result<Option<Zeroizing<String>>, String> {
    let dir = data_dir.to_path_buf();
    with_timeout(move || match entry(&dir)?.get_password() {
        Ok(p) => Ok(Some(Zeroizing::new(p))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    })
}

/// Guarda a senha e cria o marcador.
pub fn set(data_dir: &Path, password: &str) -> Result<(), String> {
    let dir = data_dir.to_path_buf();
    let pw = Zeroizing::new(password.to_string());
    with_timeout(move || entry(&dir)?.set_password(&pw).map_err(|e| e.to_string()))?;
    std::fs::write(marker(data_dir), b"").map_err(|e| e.to_string())
}

/// Apaga o marcador e a senha guardada (sem erro se ela não existia).
pub fn clear(data_dir: &Path) -> Result<(), String> {
    match std::fs::remove_file(marker(data_dir)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    let dir = data_dir.to_path_buf();
    with_timeout(move || match entry(&dir)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    })
}
