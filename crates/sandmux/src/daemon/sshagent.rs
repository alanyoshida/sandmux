//! Socket do agente SSH de um lançamento: atende o protocolo do agente com
//! as chaves escolhidas que estão no cofre aberto e registra cada assinatura.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::Shared;
use crate::paths;
use crate::sshagent::{self, SignEvent};

/// O socket de um lançamento; some ao ser solto.
pub struct AgentSocket {
    pub path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for AgentSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Quem usa o socket: a etiqueta do lançamento, a pasta e as chaves
/// escolhidas (`*` é todas).
#[derive(Clone)]
struct Owner {
    tag: String,
    workdir: PathBuf,
    names: Vec<String>,
}

/// Cria o socket do agente do lançamento `tag`.
pub fn agent_socket(
    shared: &Arc<Shared>,
    tag: &str,
    workdir: &std::path::Path,
    names: Vec<String>,
) -> std::io::Result<AgentSocket> {
    let dir = paths::runtime_dir().join("ssh");
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(format!("{tag}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let owner = Owner {
        tag: tag.into(),
        workdir: workdir.into(),
        names,
    };
    let (s2, stop2) = (shared.clone(), stop.clone());
    std::thread::Builder::new()
        .name("ssh-agent".into())
        .spawn(move || accept_loop(s2, listener, stop2, owner))?;
    Ok(AgentSocket { path, stop })
}

fn accept_loop(shared: Arc<Shared>, l: UnixListener, stop: Arc<AtomicBool>, owner: Owner) {
    while !stop.load(Ordering::SeqCst) {
        match l.accept() {
            Ok((c, _)) => {
                let _ = c.set_nonblocking(false);
                let (s, o) = (shared.clone(), owner.clone());
                let _ = std::thread::Builder::new()
                    .name("ssh-agent-conn".into())
                    .spawn(move || serve(&s, c, &o));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn serve(shared: &Shared, mut c: UnixStream, owner: &Owner) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(300)));
    loop {
        let mut len = [0u8; 4];
        if c.read_exact(&mut len).is_err() {
            return;
        }
        let n = u32::from_be_bytes(len) as usize;
        if n == 0 || n > sshagent::MAX_MSG {
            return;
        }
        let mut msg = vec![0u8; n];
        if c.read_exact(&mut msg).is_err() {
            return;
        }
        // As chaves são lidas do cofre a cada pedido: um desbloqueio ou uma
        // remoção vale na hora.
        let keys = shared
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ssh_keys(&owner.names);
        let (resp, ev) = sshagent::handle(&msg, &keys);
        drop(keys);
        if let Some(ev) = ev {
            let e = match &ev {
                SignEvent::Signed { name, fingerprint } => sandmux_net::log::Entry::ssh_sign(
                    &owner.tag,
                    &owner.workdir,
                    name,
                    fingerprint,
                    None,
                ),
                SignEvent::Refused { reason } => sandmux_net::log::Entry::ssh_sign(
                    &owner.tag,
                    &owner.workdir,
                    "",
                    "",
                    Some(reason),
                ),
            };
            sandmux_net::log::append(&super::net::log_dir(), &e);
        }
        let mut out = (resp.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(&resp);
        if c.write_all(&out).is_err() {
            return;
        }
    }
}
