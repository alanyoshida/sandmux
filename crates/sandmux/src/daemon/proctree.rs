//! A árvore de processos de uma aba pelo kernel: os eventos `exec` e `exit`
//! da observação das execuções montam um mapa pid → nó, e o pedido da
//! árvore confere cada nó no `/proc` (tira quem sumiu, mesmo sem o `exit`) e
//! completa os campos. Os pids são os do host.

use std::collections::HashMap;
use std::path::Path;

use sandmux_proto::msg::ProcNode;
use serde_json::Value;

/// Teto de nós por aba.
pub const MAX_NODES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Node {
    ppid: u32,
    /// O `starttime` do `/proc/<pid>/stat`: um pid reaproveitado tem outro.
    start: u64,
}

#[derive(Clone, Default)]
pub struct ProcTree {
    nodes: HashMap<u32, Node>,
}

/// Os campos do `/proc/<pid>/stat` que a árvore usa.
struct Stat {
    name: String,
    state: char,
    ppid: u32,
    ticks: u64,
    start: u64,
    rss_pages: u64,
}

fn stat(proc_root: &Path, pid: u32) -> Option<Stat> {
    let s = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    // O nome fica entre o primeiro `(` e o último `)` e pode ter espaços.
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    let name = s.get(open + 1..close)?.to_string();
    let f: Vec<&str> = s.get(close + 1..)?.split_whitespace().collect();
    // f[0] é o campo 3 do stat (o estado).
    let n = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok());
    Some(Stat {
        name,
        state: f.first()?.chars().next()?,
        ppid: n(1)? as u32,
        ticks: n(11)? + n(12)?,
        start: n(19)?,
        rss_pages: n(21).unwrap_or(0),
    })
}

/// Os pids do cgroup `cg` e dos de baixo.
fn cgroup_pids(cg: &Path) -> Vec<u32> {
    let mut out: Vec<u32> = std::fs::read_to_string(cg.join("cgroup.procs"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    for d in std::fs::read_dir(cg).into_iter().flatten().flatten() {
        if d.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(cgroup_pids(&d.path()));
        }
    }
    out
}

/// Uma mudança da árvore vinda de um evento da observação.
pub enum Change {
    Exec { pid: u32, ppid: u32, start: u64 },
    Exit(u32),
}

/// As mudanças de uma mensagem da observação, com o `starttime` lido no
/// `/proc` (fora do lock do daemon). Um auxiliar antigo manda a execução sem
/// `ppid`: o nó fica como raiz.
pub fn changes(msg: &Value, proc_root: &Path) -> Vec<Change> {
    msg["ev"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let pid = e["pid"].as_u64().and_then(|p| u32::try_from(p).ok())?;
            match e["kind"].as_str() {
                Some("exec") => Some(Change::Exec {
                    pid,
                    ppid: e["ppid"].as_u64().unwrap_or(0) as u32,
                    start: stat(proc_root, pid).map(|s| s.start).unwrap_or(0),
                }),
                Some("exit") => Some(Change::Exit(pid)),
                _ => None,
            }
        })
        .collect()
}

fn sysconf(name: libc::c_int, default: u64) -> u64 {
    // SAFETY: sysconf só lê uma configuração do sistema.
    let v = unsafe { libc::sysconf(name) };
    if v > 0 { v as u64 } else { default }
}

impl ProcTree {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn insert(&mut self, pid: u32, ppid: u32, start: u64) {
        if self.nodes.len() >= MAX_NODES && !self.nodes.contains_key(&pid) {
            return;
        }
        self.nodes.insert(pid, Node { ppid, start });
    }

    /// Tira `pid`; os filhos passam a ter como pai o pai dele.
    pub fn exit(&mut self, pid: u32) {
        let Some(n) = self.nodes.remove(&pid) else {
            return;
        };
        for c in self.nodes.values_mut() {
            if c.ppid == pid {
                c.ppid = n.ppid;
            }
        }
    }

    /// Aplica os eventos `exec` e `exit` de uma mensagem da observação.
    pub fn apply(&mut self, msg: &Value, proc_root: &Path) {
        self.commit(changes(msg, proc_root));
    }

    /// Aplica as mudanças lidas por [`changes`].
    pub fn commit(&mut self, changes: Vec<Change>) {
        for c in changes {
            match c {
                Change::Exec { pid, ppid, start } => self.insert(pid, ppid, start),
                Change::Exit(pid) => self.exit(pid),
            }
        }
    }

    /// Os processos que já estão no escopo `cg` (e nos de baixo), para a
    /// árvore não começar vazia quando a observação liga com a aba rodando.
    pub fn seed(&mut self, cg: &Path, proc_root: &Path) {
        for pid in cgroup_pids(cg) {
            if self.nodes.contains_key(&pid) {
                continue;
            }
            if let Some(s) = stat(proc_root, pid) {
                self.insert(pid, s.ppid, s.start);
            }
        }
    }

    /// Os processos vivos, conferidos no `/proc` (ver [`render`]), tirando da
    /// árvore os que terminaram.
    pub fn snapshot(&mut self, proc_root: &Path) -> Vec<ProcNode> {
        let (nodes, dead) = render(self.clone(), proc_root);
        for pid in dead {
            self.exit(pid);
        }
        nodes
    }
}

/// Os processos vivos de uma cópia da árvore, conferidos no `/proc` (fora do
/// lock do daemon), e os pids que terminaram: quem sumiu, virou zumbi ou
/// mudou de `starttime`. O pai de cada nó é 0 quando ele não está entre os
/// vivos.
pub fn render(mut tree: ProcTree, proc_root: &Path) -> (Vec<ProcNode>, Vec<u32>) {
    tree.render(proc_root)
}

impl ProcTree {
    fn render(&mut self, proc_root: &Path) -> (Vec<ProcNode>, Vec<u32>) {
        let hz = sysconf(libc::_SC_CLK_TCK, 100);
        let page = sysconf(libc::_SC_PAGESIZE, 4096);
        let uptime = std::fs::read_to_string(proc_root.join("uptime"))
            .ok()
            .and_then(|u| u.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let mut live = Vec::new();
        let mut dead = Vec::new();
        for (&pid, n) in &self.nodes {
            match stat(proc_root, pid) {
                Some(s)
                    if !matches!(s.state, 'Z' | 'X') && (n.start == 0 || n.start == s.start) =>
                {
                    live.push((pid, s));
                }
                _ => dead.push(pid),
            }
        }
        for &pid in &dead {
            self.exit(pid);
        }
        let alive: std::collections::HashSet<u32> = live.iter().map(|(p, _)| *p).collect();
        let mut out: Vec<ProcNode> = live
            .into_iter()
            .map(|(pid, s)| {
                // O pai depois das saídas acima.
                let ppid = self.nodes.get(&pid).map(|n| n.ppid).unwrap_or(0);
                let dir = proc_root.join(pid.to_string());
                let cmdline = std::fs::read(dir.join("cmdline"))
                    .map(|b| {
                        b.split(|&c| c == 0)
                            .filter(|a| !a.is_empty())
                            .map(|a| String::from_utf8_lossy(a).into_owned())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                let cgroup = std::fs::read_to_string(dir.join("cgroup"))
                    .ok()
                    .and_then(|c| {
                        c.lines()
                            .find_map(|l| l.strip_prefix("0::").map(str::to_string))
                    })
                    .unwrap_or_default();
                let started = s.start as f64 / hz as f64;
                ProcNode {
                    pid,
                    ppid: if alive.contains(&ppid) { ppid } else { 0 },
                    cmdline: if cmdline.is_empty() {
                        s.name.clone()
                    } else {
                        cmdline
                    },
                    name: s.name,
                    rss: s.rss_pages * page,
                    cpu_ms: s.ticks * 1000 / hz,
                    age_secs: (uptime - started).max(0.0) as u64,
                    cgroup,
                }
            })
            .collect();
        out.sort_by_key(|n| n.pid);
        (out, dead)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Um `/proc` falso com os processos `(pid, ppid, nome, starttime)`.
    fn fake_proc(procs: &[(u32, u32, &str, u64)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("uptime"), "1000.00 50.00\n").unwrap();
        for &(pid, ppid, name, start) in procs {
            let p = d.path().join(pid.to_string());
            std::fs::create_dir_all(&p).unwrap();
            // Campos 3 a 24: estado, ppid, ..., utime (14) e stime (15) em
            // 100 tiques cada, starttime (22), vsize e rss (24) em 2 páginas.
            let mut f = vec!["0".to_string(); 22];
            f[0] = "S".into();
            f[1] = ppid.to_string();
            f[11] = "100".into();
            f[12] = "100".into();
            f[19] = start.to_string();
            f[21] = "2".into();
            std::fs::write(p.join("stat"), format!("{pid} ({name}) {}\n", f.join(" "))).unwrap();
            std::fs::write(p.join("cmdline"), format!("{name}\0--x\0")).unwrap();
            std::fs::write(p.join("cgroup"), "0::/user.slice/aba.scope\n").unwrap();
        }
        d
    }

    fn pais(t: &mut ProcTree, root: &Path) -> Vec<(u32, u32)> {
        t.snapshot(root).iter().map(|n| (n.pid, n.ppid)).collect()
    }

    #[test]
    fn eventos_montam_tres_niveis() {
        let p = fake_proc(&[
            (10, 1, "bash", 5),
            (11, 10, "node", 6),
            (12, 11, "npm (x)", 7),
        ]);
        let mut t = ProcTree::default();
        t.apply(
            &json!({"ev": [
                {"kind": "exec", "pid": 10, "ppid": 1, "path": "/bin/bash"},
                {"kind": "exec", "pid": 11, "ppid": 10, "path": "/usr/bin/node"},
                {"kind": "conn", "pid": 11},
                {"kind": "exec", "pid": 12, "ppid": 11, "path": "/usr/bin/npm"}
            ]}),
            p.path(),
        );
        // O 1 não está na árvore: o bash é raiz.
        assert_eq!(pais(&mut t, p.path()), [(10, 0), (11, 10), (12, 11)]);
        let n = &t.snapshot(p.path())[2];
        assert_eq!(n.name, "npm (x)");
        assert_eq!(n.cmdline, "npm (x) --x");
        assert_eq!(n.cgroup, "/user.slice/aba.scope");
        assert_eq!(n.cpu_ms, 200 * 1000 / sysconf(libc::_SC_CLK_TCK, 100));
        assert_eq!(n.rss, 2 * sysconf(libc::_SC_PAGESIZE, 4096));
    }

    #[test]
    fn saida_do_meio_passa_os_filhos_ao_avo() {
        let p = fake_proc(&[(10, 1, "bash", 5), (12, 11, "npm", 7), (13, 11, "sh", 8)]);
        let mut t = ProcTree::default();
        t.apply(
            &json!({"ev": [
                {"kind": "exec", "pid": 10, "ppid": 1},
                {"kind": "exec", "pid": 11, "ppid": 10},
                {"kind": "exec", "pid": 12, "ppid": 11},
                {"kind": "exec", "pid": 13, "ppid": 11},
                {"kind": "exit", "pid": 11}
            ]}),
            p.path(),
        );
        assert_eq!(pais(&mut t, p.path()), [(10, 0), (12, 10), (13, 10)]);
        t.apply(&json!({"ev": [{"kind": "exit", "pid": 10}]}), p.path());
        assert_eq!(pais(&mut t, p.path()), [(12, 0), (13, 0)]);
    }

    #[test]
    fn saida_perdida_e_pid_reusado_saem_no_pedido() {
        let p = fake_proc(&[(10, 1, "bash", 5), (11, 10, "node", 6)]);
        let mut t = ProcTree::default();
        t.apply(
            &json!({"ev": [
                {"kind": "exec", "pid": 10, "ppid": 1},
                {"kind": "exec", "pid": 11, "ppid": 10},
                {"kind": "exec", "pid": 20, "ppid": 10}
            ]}),
            p.path(),
        );
        // O 20 não existe no /proc: sai. O 11 passa a outro processo.
        let mut f = vec!["0".to_string(); 22];
        f[0] = "S".into();
        f[1] = "1".into();
        f[19] = "999".into();
        std::fs::write(
            p.path().join("11/stat"),
            format!("11 (outro) {}\n", f.join(" ")),
        )
        .unwrap();
        assert_eq!(pais(&mut t, p.path()), [(10, 0)]);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn semente_pelo_cgroup() {
        let p = fake_proc(&[(10, 1, "bash", 5), (11, 10, "node", 6)]);
        let cg = tempfile::tempdir().unwrap();
        std::fs::write(cg.path().join("cgroup.procs"), "10\n").unwrap();
        std::fs::create_dir(cg.path().join("filho")).unwrap();
        std::fs::write(cg.path().join("filho/cgroup.procs"), "11\n99\n").unwrap();
        let mut t = ProcTree::default();
        t.seed(cg.path(), p.path());
        assert_eq!(pais(&mut t, p.path()), [(10, 0), (11, 10)]);
    }

    #[test]
    fn processo_de_verdade_no_proc() {
        let me = std::process::id();
        let mut t = ProcTree::default();
        t.apply(
            &json!({"ev": [{"kind": "exec", "pid": me, "ppid": 0}, {"kind": "exec", "pid": 4_000_000_000u32}]}),
            Path::new("/proc"),
        );
        let v = t.snapshot(Path::new("/proc"));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pid, me);
        assert!(v[0].rss > 0 && !v[0].cmdline.is_empty(), "{:?}", v[0]);
    }

    #[test]
    fn teto_de_nos() {
        let mut t = ProcTree::default();
        for pid in 0..(MAX_NODES as u32 + 10) {
            t.insert(pid + 1, 0, 0);
        }
        assert_eq!(t.len(), MAX_NODES);
    }
}
