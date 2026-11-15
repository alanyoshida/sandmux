//! Pastas ao vivo em `/mnt/sandmux`: guardadas na aba e montadas pelo
//! supervisor da sessão viva.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use sandmux_spec::{LiveFolder, trf};

use super::Shared;

/// Um nome livre para a pasta: o último componente, com `-2`, `-3`...
fn free_name(path: &std::path::Path, taken: &[LiveFolder]) -> String {
    let base: String = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pasta")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.is_empty() || base == "." || base == ".." {
        "pasta".into()
    } else {
        base
    };
    if !taken.iter().any(|f| f.name == base) && base != "PASTAS.txt" {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|n| !taken.iter().any(|f| f.name == *n))
        .unwrap_or(base)
}

/// Manda um pedido ao supervisor do lançamento `tag`.
fn send(tag: &str, req: &sandmux_launcher::mounter::Request) -> Result<(), String> {
    let path = crate::launch::control_socket_path(tag);
    let mut s = UnixStream::connect(&path).map_err(|e| {
        trf!(
            "o supervisor da sessão não atende ({e})",
            "the session supervisor does not answer ({e})"
        )
    })?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    s.write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    BufReader::new(&s)
        .read_line(&mut out)
        .map_err(|e| e.to_string())?;
    let r: sandmux_launcher::mounter::Reply = serde_json::from_str(&out)
        .map_err(|e| trf!("resposta do supervisor: {e}", "supervisor reply: {e}"))?;
    r.error.map_or(Ok(()), Err)
}

/// As pastas da aba da sessão `id`.
pub fn list(shared: &Shared, id: u32) -> Result<Vec<LiveFolder>, String> {
    let st = shared.lock();
    let s = st
        .sessions
        .get(&id)
        .ok_or_else(|| trf!("não existe a sessão {id}", "session {id} does not exist"))?;
    Ok(s.opts.folders.clone())
}

/// Monta ou desmonta. Devolve um aviso, quando há.
pub fn edit(
    shared: &Shared,
    id: u32,
    action: &str,
    value: &str,
    rw: bool,
) -> Result<Option<String>, String> {
    let mut warning = None;
    let (live_tag, req) = {
        let mut st = shared.lock();
        let s = st
            .sessions
            .get_mut(&id)
            .ok_or_else(|| trf!("não existe a sessão {id}", "session {id} does not exist"))?;
        let tag = s.live.as_ref().and(s.tags.last().cloned());
        let req = match action {
            "add" => {
                let p = PathBuf::from(value);
                let p = std::fs::canonicalize(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                if !p.is_dir() {
                    return Err(trf!(
                        "{} não é uma pasta",
                        "{} is not a folder",
                        p.display()
                    ));
                }
                let mut host = shared.base.clone();
                host.cwd = s.opts.workdir.clone();
                sandmux_policy::check_dangerous(&host, &p, rw).map_err(|e| e.to_string())?;
                let creds = sandmux_policy::compose::credentials_touched(&host, &p);
                if !creds.is_empty() {
                    warning = Some(trf!(
                        "{} dá ao agente acesso a credenciais reais: {}",
                        "{} gives the agent access to real credentials: {}",
                        p.display(),
                        creds
                            .iter()
                            .map(|c| c.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if let Some(f) = s.opts.folders.iter().find(|f| f.path == p) {
                    return Err(trf!(
                        "{} já está em /mnt/sandmux/{}",
                        "{} is already at /mnt/sandmux/{}",
                        p.display(),
                        f.name
                    ));
                }
                let folder = LiveFolder {
                    name: free_name(&p, &s.opts.folders),
                    path: p,
                    rw,
                };
                s.opts.folders.push(folder.clone());
                sandmux_launcher::mounter::Request::Add {
                    folder,
                    all: s.opts.folders.clone(),
                }
            }
            "remove" => {
                let before = s.opts.folders.len();
                s.opts.folders.retain(|f| f.name != value);
                if s.opts.folders.len() == before {
                    return Err(trf!(
                        "a sessão {id} não tem a pasta {value}",
                        "session {id} has no folder {value}"
                    ));
                }
                sandmux_launcher::mounter::Request::Remove {
                    name: value.to_string(),
                    all: s.opts.folders.clone(),
                }
            }
            other => {
                return Err(trf!(
                    "ação de pasta desconhecida: {other}",
                    "unknown folder action: {other}"
                ));
            }
        };
        shared.persist(&st);
        (tag, req)
    };
    let Some(tag) = live_tag else {
        let note = trf!(
            "a sessão {id} não está viva; vale no próximo lançamento",
            "session {id} is not alive; it takes effect on the next launch"
        );
        return Ok(Some(
            warning.map_or(note.clone(), |w| format!("{w}; {note}")),
        ));
    };
    if let Err(e) = send(&tag, &req) {
        // Desfaz na aba o que não montou.
        if let sandmux_launcher::mounter::Request::Add { folder, .. } = &req {
            let mut st = shared.lock();
            if let Some(s) = st.sessions.get_mut(&id) {
                s.opts.folders.retain(|f| f.name != folder.name);
            }
            shared.persist(&st);
        }
        return Err(e);
    }
    Ok(warning)
}
