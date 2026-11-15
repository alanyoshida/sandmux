//! Histórico de tela cifrado em disco (`[global] screen-history`): a saída de
//! cada aba vai, a cada 2 segundos, para `<estado>/history/<id>.bin`, cifrada
//! com a chave do histórico guardada no cofre. Desligado, nada é gravado.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sandmux_spec::{tr, trf};
use sandmux_vault::history;
use zeroize::Zeroizing;

use super::Shared;

/// Teto do que espera na memória, por aba, com o cofre trancado.
pub const BUF_CAP: usize = 2 * 1024 * 1024;
/// Acima disso, o arquivo é compactado para [`KEEP`].
const FILE_CAP: u64 = 8 * 1024 * 1024;
const KEEP: usize = 4 * 1024 * 1024;

/// Linha que marca a volta da aba depois de um reinício.
pub fn separator() -> Vec<u8> {
    format!(
        "\r\n\x1b[2m── {} ──\x1b[0m\r\n",
        tr!("sandmux: a aba voltou", "sandmux: the tab came back")
    )
    .into_bytes()
}

/// Estado do histórico no daemon.
#[derive(Default)]
pub struct History {
    enabled: AtomicBool,
}

impl History {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

pub fn dir() -> PathBuf {
    crate::paths::state_dir().join("history")
}

fn file(id: u32) -> PathBuf {
    dir().join(format!("{id}.bin"))
}

fn read_enabled(shared: &Shared) -> bool {
    sandmux_policy::config::screen_history_enabled(
        &shared.base.config_dir.join(sandmux_policy::config::FILE),
    )
}

/// A chave do histórico, com o cofre aberto (criada no primeiro uso).
fn key(shared: &Shared) -> Option<Zeroizing<[u8; 32]>> {
    shared
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .history_key()
}

fn append(id: u32, key: &[u8; 32], data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir())?;
    let path = file(id);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(&history::seal(key, id, data))?;
    drop(f);
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > FILE_CAP {
        compact(id, key)?;
    }
    Ok(())
}

/// Regrava o arquivo só com os [`KEEP`] bytes finais, por troca atômica.
fn compact(id: u32, key: &[u8; 32]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = file(id);
    let Ok(all) = history::open(key, id, &std::fs::read(&path)?) else {
        return Ok(());
    };
    let keep = &all[all.len().saturating_sub(KEEP)..];
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(&history::seal(key, id, keep))?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

/// Grava o que a aba tem no buffer (com a chave) e devolve o histórico
/// decifrado do arquivo. `None` sem a chave ou sem arquivo.
fn flush_and_read(shared: &Shared, id: u32, key: &[u8; 32]) -> Option<Vec<u8>> {
    let buf = shared
        .lock()
        .sessions
        .get_mut(&id)
        .map(|s| std::mem::take(&mut s.hist_buf))
        .unwrap_or_default();
    if !buf.is_empty() {
        let _ = append(id, key, &buf);
    }
    let bytes = std::fs::read(file(id)).ok()?;
    match history::open(key, id, &bytes) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: histórico da sessão {id} não decifra; apagando",
                    "sandmuxd: history of session {id} does not decrypt; deleting"
                )
            );
            let _ = std::fs::remove_file(file(id));
            None
        }
    }
}

/// O que fazer com o histórico no lançamento de uma aba.
pub enum Preload {
    /// Desligado, ou sem arquivo: nada.
    None,
    /// Passar ao parser novo, antes da saída nova.
    Bytes(Vec<u8>),
    /// Há arquivo, mas o cofre está trancado: volta no desbloqueio.
    Pending,
}

/// Chamado antes de relançar a aba (sem a trava do estado).
pub fn preload(shared: &Shared, id: u32) -> Preload {
    if !shared.disk_history.enabled() || !file(id).exists() {
        return Preload::None;
    }
    match key(shared) {
        Some(k) => match flush_and_read(shared, id, &k) {
            Some(b) if !b.is_empty() => Preload::Bytes(b),
            _ => Preload::None,
        },
        None => Preload::Pending,
    }
}

/// Refaz o parser da aba com o histórico do arquivo e redesenha os clientes.
fn rebuild(shared: &Shared, id: u32, key: &[u8; 32]) {
    let Some(bytes) = flush_and_read(shared, id, key) else {
        if let Some(s) = shared.lock().sessions.get_mut(&id) {
            s.hist_pending = false;
        }
        return;
    };
    let mut st = shared.lock();
    let Some(s) = st.sessions.get_mut(&id) else {
        return;
    };
    s.hist_pending = false;
    s.rebuild_parser(&bytes);
    let mut redraw = b"\x1b[H\x1b[2J".to_vec();
    redraw.extend(s.replay());
    s.broadcast(|| super::Out::Data(redraw.clone()));
}

/// Lê a opção antes da restauração, para o lançamento das abas saber dela.
pub fn init(shared: &Shared) {
    shared
        .disk_history
        .enabled
        .store(read_enabled(shared), Ordering::Relaxed);
}

pub fn start(shared: Arc<Shared>) {
    let _ = std::thread::Builder::new()
        .name("historico".into())
        .spawn(move || run(&shared));
}

fn run(shared: &Arc<Shared>) {
    let mut last_cfg: Option<Instant> = None;
    let mut warned_drop = false;
    loop {
        if last_cfg.is_none_or(|t| t.elapsed() >= Duration::from_secs(10)) {
            let on = read_enabled(shared);
            let was = shared.disk_history.enabled.swap(on, Ordering::Relaxed);
            if !on && (was || last_cfg.is_none()) {
                // Desligado: nada fica no disco nem na memória.
                let _ = std::fs::remove_dir_all(dir());
                for s in shared.lock().sessions.values_mut() {
                    s.hist_buf = Vec::new();
                    s.hist_pending = false;
                }
            }
            last_cfg = Some(Instant::now());
        }
        if shared.disk_history.enabled() {
            tick(shared, &mut warned_drop);
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn tick(shared: &Shared, warned_drop: &mut bool) {
    let ids: Vec<u32> = shared.lock().sessions.keys().copied().collect();
    // Arquivos de abas que não existem mais (fechadas).
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name
                .strip_suffix(".bin")
                .or_else(|| name.strip_suffix(".tmp"))
                .and_then(|n| n.parse::<u32>().ok());
            if id.is_none_or(|i| !ids.contains(&i)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let Some(k) = key(shared) else {
        // Cofre trancado: os buffers esperam, com o teto.
        let full = shared
            .lock()
            .sessions
            .values()
            .any(|s| s.hist_buf.len() >= BUF_CAP);
        if full && !*warned_drop {
            eprintln!(
                "{}",
                tr!(
                    "sandmuxd: histórico em disco: o cofre está trancado e o buffer de uma aba encheu; a saída mais antiga se perde",
                    "sandmuxd: history on disk: the vault is locked and a tab's buffer filled up; the oldest output is lost"
                )
            );
            *warned_drop = true;
        }
        return;
    };
    *warned_drop = false;
    let (bufs, pending): (Vec<(u32, Vec<u8>)>, Vec<u32>) = {
        let mut st = shared.lock();
        let mut bufs = Vec::new();
        let mut pending = Vec::new();
        for s in st.sessions.values_mut() {
            if s.hist_pending {
                pending.push(s.id);
            } else if !s.hist_buf.is_empty() {
                bufs.push((s.id, std::mem::take(&mut s.hist_buf)));
            }
        }
        (bufs, pending)
    };
    for (id, data) in bufs {
        if let Err(e) = append(id, &k, &data) {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: histórico da sessão {id}: {e}",
                    "sandmuxd: history of session {id}: {e}"
                )
            );
        }
    }
    for id in pending {
        rebuild(shared, id, &k);
    }
}
