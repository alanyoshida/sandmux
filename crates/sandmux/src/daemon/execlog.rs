//! Registro de execuções pelo kernel: quando o escopo de uma aba aparece, o
//! daemon pede ao auxiliar root a observação das execuções do escopo e grava
//! cada uma no registro de arquivos. Enquanto o kernel não assume, quem
//! registra é o supervisor do sandbox; o daemon o desliga e religa pela
//! chave `exec_log` da política de arquivos ao vivo.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sandmux_spec::FileEntry;
use sandmux_spec::{tr, trf};
use serde_json::Value;

use super::Shared;
use crate::helper::Client;

/// Teto de conexões guardadas por aba.
pub const MAX_CONNS: usize = 4096;

/// A chave de uma conexão: a porta de origem, o IP e a porta de destino.
pub type ConnKey = (u16, String, u16);
/// O processo de uma conexão: o pid e o executável.
pub type ConnProc = (u32, String);

/// As conexões TCP de uma aba vistas pelo kernel: (porta de origem, IP e
/// porta de destino) → (pid, executável). As mais velhas saem primeiro.
#[derive(Default)]
pub struct Conns {
    map: std::collections::HashMap<ConnKey, ConnProc>,
    order: std::collections::VecDeque<ConnKey>,
}

impl Conns {
    pub fn insert(&mut self, key: ConnKey, v: ConnProc) {
        if self.map.insert(key.clone(), v).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MAX_CONNS {
            if let Some(k) = self.order.pop_front() {
                self.map.remove(&k);
            }
        }
    }

    pub fn get(&self, key: &ConnKey) -> Option<&ConnProc> {
        self.map.get(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// O processo de uma conexão do lançamento `tag`: o pid e o executável, se
/// o kernel a registrou, e se a aba está pelo kernel.
pub fn conn_process(
    shared: &Shared,
    tag: &str,
    sport: u16,
    ip: &str,
    port: u16,
) -> (Option<(u32, String)>, bool) {
    let st = shared.lock();
    let Some(s) = st
        .sessions
        .values()
        .find(|s| s.tags.last().is_some_and(|t| t == tag))
    else {
        return (None, false);
    };
    (
        s.conns.get(&(sport, ip.to_string(), port)).cloned(),
        s.exec_obs.is_some(),
    )
}

/// A observação de uma aba: a conexão com o auxiliar. Soltá-la desfaz a
/// observação (a thread que lê os eventos termina junto).
pub struct ExecObs {
    _client: Client,
}

/// Grava `exec_log` na política ao vivo do lançamento `tag` (`Some(false)`:
/// o supervisor para; `None`: volta a registrar).
pub fn set_supervisor(tag: &str, on: bool) {
    let f = crate::launch::file_policy_path(tag);
    let Ok(b) = std::fs::read(&f) else { return };
    let Ok(mut p) = serde_json::from_slice::<sandmux_spec::files::FilePolicy>(&b) else {
        return;
    };
    p.exec_log = if on { None } else { Some(false) };
    let _ = crate::launch::write_file_policy(&f, &p);
}

fn set_state(shared: &Shared, id: u32, pid: i32, exec: String, obs: Option<ExecObs>) -> bool {
    let mut st = shared.lock();
    let Some(s) = st.sessions.get_mut(&id) else {
        return false;
    };
    if s.pid() != Some(pid) {
        return false;
    }
    if let Some(p) = s.policy.as_mut() {
        p.exec = Some(exec);
    }
    if obs.is_some() {
        s.exec_obs = obs;
    }
    true
}

/// As linhas do registro de uma mensagem de eventos de execução.
pub fn entries(msg: &Value, tag: &str, workdir: &Path) -> Vec<FileEntry> {
    let (_, ts) = sandmux_net::log::now_local();
    msg["ev"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "exec")
        .filter_map(|e| {
            let path = e["path"].as_str().filter(|p| !p.is_empty())?;
            Some(FileEntry {
                ts: ts.clone(),
                tag: tag.to_string(),
                workdir: workdir.to_path_buf(),
                path: PathBuf::from(path),
                access: "exec".into(),
                blocked: None,
                new: false,
                args: e["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect(),
                via: Some("kernel".into()),
            })
        })
        .collect()
}

/// Liga a observação do escopo `cg` da sessão `id` (lançamento `pid`), numa
/// thread. Sem o auxiliar, a aba fica com o supervisor, e o motivo vai para
/// a proteção do lançamento.
pub fn start(shared: Arc<Shared>, id: u32, pid: i32, cg: PathBuf) {
    let _ = std::thread::Builder::new()
        .name("registro-exec".into())
        .spawn(move || run(&shared, id, pid, &cg));
}

fn run(shared: &Arc<Shared>, id: u32, pid: i32, cg: &Path) {
    let client = match connect(cg) {
        Ok(c) => c,
        Err(why) => {
            set_state(shared, id, pid, format!("supervisor: {why}"), None);
            return;
        }
    };
    let Some(rx) = client.take_events() else {
        return;
    };
    let (tag, workdir) = {
        let st = shared.lock();
        let Some(s) = st.sessions.get(&id) else {
            return;
        };
        (
            s.tags.last().cloned().unwrap_or_default(),
            std::fs::canonicalize(&s.opts.workdir).unwrap_or_else(|_| s.opts.workdir.clone()),
        )
    };
    if !set_state(
        shared,
        id,
        pid,
        "kernel".into(),
        Some(ExecObs { _client: client }),
    ) {
        return;
    }
    set_supervisor(&tag, false);
    for msg in rx {
        for e in entries(&msg, &tag, &workdir) {
            super::dockerfiles::write_entry(&e);
        }
        let conns = conn_events(&msg);
        if !conns.is_empty() {
            let mut st = shared.lock();
            if let Some(s) = st.sessions.get_mut(&id) {
                for (k, v) in conns {
                    s.conns.insert(k, v);
                }
            }
        }
    }
    // A conexão caiu: com a aba ainda no mesmo lançamento, o supervisor volta.
    let why = tr!(
        "supervisor: a conexão com o auxiliar root caiu",
        "supervisor: the connection to the root helper dropped"
    );
    if set_state(shared, id, pid, why.into(), None) {
        set_supervisor(&tag, true);
        let mut st = shared.lock();
        if let Some(s) = st.sessions.get_mut(&id) {
            s.exec_obs = None;
        }
    }
}

/// As conexões de uma mensagem de eventos, com o executável lido em
/// `/proc/<pid>/exe` (o processo quase sempre ainda vive).
pub fn conn_events(msg: &Value) -> Vec<(ConnKey, ConnProc)> {
    msg["ev"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["kind"] == "conn")
        .filter_map(|e| {
            let pid = e["pid"].as_u64()? as u32;
            let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            Some((
                (
                    e["sport"].as_u64()? as u16,
                    e["ip"].as_str()?.to_string(),
                    e["dport"].as_u64()? as u16,
                ),
                (pid, exe),
            ))
        })
        .collect()
}

fn connect(cg: &Path) -> Result<Client, String> {
    match sandmux_ebpf::lsm_active() {
        Ok(true) => {}
        Ok(false) => {
            return Err(tr!(
                "o BPF LSM não está ativo no kernel",
                "the BPF LSM is not active in the kernel"
            )
            .into());
        }
        Err(e) => return Err(e),
    }
    let path = crate::helper::socket_path();
    let c = Client::connect(&path).map_err(|e| {
        trf!(
            "o auxiliar root não responde em {}: {e}",
            "the root helper does not answer at {}: {e}",
            path.display()
        )
    })?;
    c.status()?;
    c.observe(cg)?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapa_de_conexoes() {
        let mut c = Conns::default();
        for i in 0..(MAX_CONNS + 10) {
            c.insert((i as u16, "1.1.1.1".into(), 443), (i as u32, "/x".into()));
        }
        assert_eq!(c.len(), MAX_CONNS);
        assert!(c.get(&(0, "1.1.1.1".into(), 443)).is_none());
        assert_eq!(
            c.get(&((MAX_CONNS + 9) as u16, "1.1.1.1".into(), 443)),
            Some(&((MAX_CONNS + 9) as u32, "/x".into()))
        );
        let msg = serde_json::json!({"ev": [
            {"kind": "conn", "pid": std::process::id(), "sport": 40000, "dport": 443, "ip": "1.1.1.1"},
            {"kind": "exec", "pid": 1, "path": "/x"}
        ]});
        let v = conn_events(&msg);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, (40000, "1.1.1.1".into(), 443));
        assert!(!v[0].1.1.is_empty(), "o executável do próprio teste");
    }

    #[test]
    fn eventos_de_execucao_viram_linhas() {
        let msg = serde_json::json!({"ev": [
            {"kind": "exec", "pid": 9, "path": "/usr/bin/curl", "args": ["curl", "-s", "x"]},
            {"kind": "deny", "ino": 3},
            {"kind": "exec", "pid": 9, "path": "", "args": []}
        ]});
        let l = entries(&msg, "s3-1", Path::new("/w"));
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].path, PathBuf::from("/usr/bin/curl"));
        assert_eq!(l[0].args, ["curl", "-s", "x"]);
        assert_eq!(
            (l[0].access.as_str(), l[0].via.as_deref()),
            ("exec", Some("kernel"))
        );
        assert_eq!(l[0].tag, "s3-1");
    }
}
