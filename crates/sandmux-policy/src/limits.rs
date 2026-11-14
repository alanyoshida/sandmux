//! Tetos de memória e de processos por sandbox: os tipos, os padrões, o
//! comando do escopo transitório do systemd e a detecção de suporte.

use std::path::{Path, PathBuf};

/// Teto de memória padrão: 75% da RAM.
pub const DEFAULT_MEMORY: MemoryLimit = MemoryLimit::Percent(75);
/// Teto de processos e threads padrão.
pub const DEFAULT_PROCESSES: ProcessLimit = ProcessLimit::Count(8192);
/// Menor teto de memória aceito.
pub const MIN_MEMORY: u64 = 128 * 1024 * 1024;
/// Menor teto de processos aceito.
pub const MIN_PROCESSES: u32 = 32;

/// Teto de memória.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLimit {
    /// Porcentagem da RAM total, de 1 a 100.
    Percent(u8),
    Bytes(u64),
    /// Sem teto.
    Max,
}

/// Teto de processos e threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLimit {
    Count(u32),
    /// Sem teto.
    Max,
}

/// Formatos aceitos, para mensagens.
pub const MEMORY_FORMATS: &str =
    "uma porcentagem da RAM (\"50%\"), um tamanho com K, M, G ou T (\"16G\") ou \"max\"";
pub const PROCESS_FORMATS: &str = "um número de processos (512) ou \"max\"";

impl MemoryLimit {
    pub fn parse(s: &str) -> Result<MemoryLimit, String> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("max") {
            return Ok(MemoryLimit::Max);
        }
        if let Some(p) = t.strip_suffix('%') {
            return match p.trim().parse::<u8>() {
                Ok(n) if (1..=100).contains(&n) => Ok(MemoryLimit::Percent(n)),
                _ => Err(format!(
                    "teto de memória inválido: {s:?}; a porcentagem vai de 1% a 100%"
                )),
            };
        }
        let (num, mult) = match t.chars().last().map(|c| c.to_ascii_uppercase()) {
            Some('K') => (&t[..t.len() - 1], 1u64 << 10),
            Some('M') => (&t[..t.len() - 1], 1 << 20),
            Some('G') => (&t[..t.len() - 1], 1 << 30),
            Some('T') => (&t[..t.len() - 1], 1 << 40),
            _ => {
                return Err(format!(
                    "teto de memória inválido: {s:?}; use {MEMORY_FORMATS}"
                ));
            }
        };
        let n: u64 = num
            .trim()
            .parse()
            .map_err(|_| format!("teto de memória inválido: {s:?}; use {MEMORY_FORMATS}"))?;
        let bytes = n
            .checked_mul(mult)
            .ok_or_else(|| format!("teto de memória grande demais: {s:?}"))?;
        if bytes < MIN_MEMORY {
            return Err(format!(
                "teto de memória pequeno demais: {s:?}; o mínimo é 128M"
            ));
        }
        Ok(MemoryLimit::Bytes(bytes))
    }

    /// O teto em bytes, para uma máquina com `mem_total` bytes de RAM.
    /// `None` é sem teto.
    pub fn bytes(&self, mem_total: u64) -> Option<u64> {
        match *self {
            MemoryLimit::Percent(p) => {
                Some((mem_total / 100).saturating_mul(p as u64).max(MIN_MEMORY))
            }
            MemoryLimit::Bytes(b) => Some(b),
            MemoryLimit::Max => None,
        }
    }
}

impl std::fmt::Display for MemoryLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            MemoryLimit::Percent(p) => write!(f, "{p}%"),
            MemoryLimit::Bytes(b) => {
                for (suf, m) in [
                    ("T", 1u64 << 40),
                    ("G", 1 << 30),
                    ("M", 1 << 20),
                    ("K", 1 << 10),
                ] {
                    if b % m == 0 {
                        return write!(f, "{}{suf}", b / m);
                    }
                }
                write!(f, "{b}")
            }
            MemoryLimit::Max => write!(f, "max"),
        }
    }
}

impl ProcessLimit {
    pub fn parse(s: &str) -> Result<ProcessLimit, String> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("max") {
            return Ok(ProcessLimit::Max);
        }
        let n: u32 = t
            .parse()
            .map_err(|_| format!("teto de processos inválido: {s:?}; use {PROCESS_FORMATS}"))?;
        ProcessLimit::count(n)
    }

    pub fn count(n: u32) -> Result<ProcessLimit, String> {
        if n < MIN_PROCESSES {
            return Err(format!(
                "teto de processos pequeno demais: {n}; o mínimo é {MIN_PROCESSES}"
            ));
        }
        Ok(ProcessLimit::Count(n))
    }

    /// `None` é sem teto.
    pub fn value(&self) -> Option<u64> {
        match *self {
            ProcessLimit::Count(n) => Some(n as u64),
            ProcessLimit::Max => None,
        }
    }
}

impl std::fmt::Display for ProcessLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ProcessLimit::Count(n) => write!(f, "{n}"),
            ProcessLimit::Max => write!(f, "max"),
        }
    }
}

/// Os tetos de um lançamento, já em bytes e em número.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    pub memory: Option<u64>,
    pub processes: Option<u64>,
}

/// RAM total da máquina, de `/proc/meminfo`.
pub fn mem_total() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_meminfo(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// Argumentos do `systemd-run` que envolvem o lançador num escopo com os
/// tetos. O comando do lançador vem depois de `--`.
pub fn scope_args(limits: &Limits, description: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "--user",
        "--scope",
        "--quiet",
        "--collect",
        "--expand-environment=no",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(format!("--description={description}"));
    // Sem isto, o systemd para o escopo inteiro na primeira morte por falta
    // de memória; o kernel já matou o processo que estourou.
    a.push("-p".into());
    a.push("OOMPolicy=continue".into());
    if let Some(m) = limits.memory {
        a.push("-p".into());
        a.push(format!("MemoryMax={m}"));
        a.push("-p".into());
        a.push("MemorySwapMax=0".into());
    }
    if let Some(p) = limits.processes {
        a.push("-p".into());
        a.push(format!("TasksMax={p}"));
    }
    a.push("--".into());
    a
}

/// Pasta do `user@<uid>.service` a partir do `/proc/self/cgroup`.
pub fn user_service_cgroup(proc_cgroup: &str, uid: u32) -> Option<PathBuf> {
    let path = proc_cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))?
        .trim();
    let unit = format!("user@{uid}.service");
    let mut out = PathBuf::from("/sys/fs/cgroup");
    for c in Path::new(path).components().skip(1) {
        out.push(c);
        if c.as_os_str() == unit.as_str() {
            return Some(out);
        }
    }
    None
}

/// Os controladores `memory` e `pids` estão delegados?
pub fn delegated(subtree_control: &str) -> Result<(), String> {
    let have: Vec<&str> = subtree_control.split_whitespace().collect();
    let missing: Vec<&str> = ["memory", "pids"]
        .into_iter()
        .filter(|c| !have.contains(c))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "o systemd de usuário não delega os controladores {} do cgroup",
            missing.join(" e ")
        ))
    }
}

/// `XDG_RUNTIME_DIR` para o `systemd-run`: ele fala com o systemd de usuário
/// pelo socket privado `$XDG_RUNTIME_DIR/systemd/private`. Quando a pasta do
/// ambiente não tem esse socket (o sandmux usa outra pasta de runtime), vale
/// a do systemd, `/run/user/<uid>`.
pub fn systemd_runtime_dir() -> Option<PathBuf> {
    let has = |d: &Path| d.join("systemd/private").exists();
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        && has(&d)
    {
        return None;
    }
    // SAFETY: getuid não falha.
    let d = PathBuf::from(format!("/run/user/{}", unsafe { getuid() }));
    has(&d).then_some(d)
}

/// Um `systemd-run` com o `XDG_RUNTIME_DIR` certo.
pub fn systemd_run() -> std::process::Command {
    let mut c = std::process::Command::new("systemd-run");
    if let Some(d) = systemd_runtime_dir() {
        c.env("XDG_RUNTIME_DIR", d);
    }
    c
}

/// O sandbox pode receber tetos nesta máquina? `Err` traz o motivo.
pub fn support() -> Result<(), String> {
    static CACHE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(detect).clone()
}

fn detect() -> Result<(), String> {
    let in_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("systemd-run").is_file()))
        .unwrap_or(false);
    if !in_path {
        return Err("o systemd-run não está no PATH".into());
    }
    // O mesmo caminho do lançamento: um escopo transitório pelo barramento
    // do usuário (DBUS_SESSION_BUS_ADDRESS ou $XDG_RUNTIME_DIR/bus).
    let ok = systemd_run()
        .args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--expand-environment=no",
            "--",
            "true",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err(
            "o systemd de usuário não respondeu (`systemd-run --user --scope` falhou)".into(),
        );
    }
    // SAFETY: getuid não falha.
    let uid = unsafe { getuid() };
    let own = std::fs::read_to_string("/proc/self/cgroup")
        .map_err(|e| format!("ler /proc/self/cgroup: {e}"))?;
    let svc = user_service_cgroup(&own, uid)
        .ok_or_else(|| format!("este processo não está sob o user@{uid}.service do systemd"))?;
    let ctl = std::fs::read_to_string(svc.join("cgroup.subtree_control"))
        .map_err(|e| format!("ler {}: {e}", svc.join("cgroup.subtree_control").display()))?;
    delegated(&ctl)
}

unsafe extern "C" {
    fn getuid() -> u32;
}

/// Pasta do cgroup de um processo, em `/sys/fs/cgroup`.
pub fn cgroup_of(pid: u32) -> Option<PathBuf> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    cgroup_path(&text)
}

fn cgroup_path(proc_cgroup: &str) -> Option<PathBuf> {
    let rel = proc_cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))?
        .trim();
    Some(Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/')))
}

/// Contadores de um cgroup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    pub memory: u64,
    pub memory_peak: u64,
    /// `None` é sem teto.
    pub memory_max: Option<u64>,
    pub processes: u64,
    pub processes_peak: u64,
    pub processes_max: Option<u64>,
    pub cpu_usec: u64,
    /// Processos mortos por falta de memória (`memory.events`, `oom_kill`).
    pub oom_kills: u64,
    /// Criações de processo recusadas pelo teto (`pids.events`, `max`).
    pub fork_refusals: u64,
}

fn read_u64(dir: &Path, f: &str) -> Option<u64> {
    std::fs::read_to_string(dir.join(f))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn read_max(dir: &Path, f: &str) -> Option<Option<u64>> {
    let s = std::fs::read_to_string(dir.join(f)).ok()?;
    match s.trim() {
        "max" => Some(None),
        v => v.parse().ok().map(Some),
    }
}

fn read_key(dir: &Path, f: &str, key: &str) -> Option<u64> {
    std::fs::read_to_string(dir.join(f))
        .ok()?
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            (it.next() == Some(key)).then(|| it.next()?.parse().ok())?
        })
}

/// Lê os contadores de um cgroup. `None` quando a pasta não existe mais.
pub fn read_counters(dir: &Path) -> Option<Counters> {
    let memory = read_u64(dir, "memory.current")?;
    Some(Counters {
        memory,
        memory_peak: read_u64(dir, "memory.peak").unwrap_or(memory),
        memory_max: read_max(dir, "memory.max").unwrap_or(None),
        processes: read_u64(dir, "pids.current").unwrap_or(0),
        processes_peak: read_u64(dir, "pids.peak").unwrap_or(0),
        processes_max: read_max(dir, "pids.max").unwrap_or(None),
        cpu_usec: read_key(dir, "cpu.stat", "usage_usec").unwrap_or(0),
        oom_kills: read_key(dir, "memory.events", "oom_kill").unwrap_or(0),
        fork_refusals: read_key(dir, "pids.events", "max").unwrap_or(0),
    })
}

/// Formata bytes em unidades binárias, com uma casa e vírgula.
pub fn human(b: u64) -> String {
    let units = [
        ("T", 1u64 << 40),
        ("G", 1 << 30),
        ("M", 1 << 20),
        ("K", 1 << 10),
    ];
    for (s, m) in units {
        if b >= m {
            let v = b as f64 / m as f64;
            let txt = if v >= 100.0 || (v - v.round()).abs() < 0.05 {
                format!("{:.0}", v.round())
            } else {
                format!("{v:.1}").replace('.', ",")
            };
            return format!("{txt}{s}");
        }
    }
    format!("{b}B")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatos_de_memoria() {
        assert_eq!(MemoryLimit::parse("75%"), Ok(MemoryLimit::Percent(75)));
        assert_eq!(MemoryLimit::parse("16G"), Ok(MemoryLimit::Bytes(16 << 30)));
        assert_eq!(
            MemoryLimit::parse("512m"),
            Ok(MemoryLimit::Bytes(512 << 20))
        );
        assert_eq!(MemoryLimit::parse("1t"), Ok(MemoryLimit::Bytes(1 << 40)));
        assert_eq!(MemoryLimit::parse("MAX"), Ok(MemoryLimit::Max));
        assert_eq!(
            MemoryLimit::parse("128M"),
            Ok(MemoryLimit::Bytes(128 << 20))
        );
        for bad in ["0%", "101%", "64M", "muito", "16", "-1G", "", "1.5G"] {
            assert!(MemoryLimit::parse(bad).is_err(), "{bad}");
        }
        assert!(MemoryLimit::parse("64M").unwrap_err().contains("128M"));
        assert_eq!(MemoryLimit::Bytes(16 << 30).to_string(), "16G");
        assert_eq!(MemoryLimit::Bytes(1536 << 20).to_string(), "1536M");
        assert_eq!(MemoryLimit::Percent(50).to_string(), "50%");
    }

    #[test]
    fn porcentagem_pela_ram() {
        let total = 46 * (1u64 << 30);
        assert_eq!(
            MemoryLimit::Percent(75).bytes(total),
            Some(total / 100 * 75)
        );
        assert_eq!(MemoryLimit::Max.bytes(total), None);
        assert_eq!(MemoryLimit::Bytes(1 << 30).bytes(total), Some(1 << 30));
    }

    #[test]
    fn formatos_de_processos() {
        assert_eq!(ProcessLimit::parse("8192"), Ok(ProcessLimit::Count(8192)));
        assert_eq!(ProcessLimit::parse("max"), Ok(ProcessLimit::Max));
        for bad in ["0", "31", "-5", "muitos", ""] {
            assert!(ProcessLimit::parse(bad).is_err(), "{bad}");
        }
        assert!(ProcessLimit::parse("10").unwrap_err().contains("32"));
    }

    #[test]
    fn meminfo() {
        let t = "MemTotal:       48234516 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_meminfo(t), Some(48234516 * 1024));
        assert_eq!(parse_meminfo("nada"), None);
    }

    #[test]
    fn comando_do_escopo() {
        let a = scope_args(
            &Limits {
                memory: Some(1 << 30),
                processes: Some(64),
            },
            "sandmux: shell:app",
        );
        let s = a.join(" ");
        assert!(s.starts_with("--user --scope --quiet --collect --expand-environment=no"));
        assert!(
            s.contains("-p MemoryMax=1073741824 -p MemorySwapMax=0 -p TasksMax=64"),
            "{s}"
        );
        assert!(s.ends_with("--"));
        let s = scope_args(
            &Limits {
                memory: None,
                processes: Some(64),
            },
            "x",
        )
        .join(" ");
        assert!(!s.contains("Memory") && s.contains("TasksMax=64"), "{s}");
        let s = scope_args(&Limits::default(), "x").join(" ");
        assert!(s.contains("-p OOMPolicy=continue"), "{s}");
        assert!(!s.contains("Max="), "{s}");
    }

    #[test]
    fn caminho_do_user_service() {
        let c = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-x.scope\n";
        assert_eq!(
            user_service_cgroup(c, 1000),
            Some(PathBuf::from(
                "/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service"
            ))
        );
        assert_eq!(
            user_service_cgroup("0::/system.slice/x.service\n", 1000),
            None
        );
        assert_eq!(user_service_cgroup(c, 1001), None);
    }

    #[test]
    fn delegacao() {
        assert!(delegated("cpu memory pids\n").is_ok());
        let e = delegated("cpu\n").unwrap_err();
        assert!(e.contains("memory e pids"), "{e}");
        assert!(delegated("memory").unwrap_err().contains("pids"));
    }

    #[test]
    fn contadores_de_um_cgroup() {
        let d = tempfile::tempdir().unwrap();
        let w = |f: &str, v: &str| std::fs::write(d.path().join(f), v).unwrap();
        w("memory.current", "1048576\n");
        w("memory.peak", "2097152\n");
        w("memory.max", "max\n");
        w("pids.current", "37\n");
        w("pids.peak", "40\n");
        w("pids.max", "8192\n");
        w("cpu.stat", "usage_usec 1500000\nuser_usec 1000000\n");
        w(
            "memory.events",
            "low 0\nhigh 0\nmax 3\noom 1\noom_kill 1\noom_group_kill 0\n",
        );
        w("pids.events", "max 5\n");
        let c = read_counters(d.path()).unwrap();
        assert_eq!(
            c,
            Counters {
                memory: 1 << 20,
                memory_peak: 2 << 20,
                memory_max: None,
                processes: 37,
                processes_peak: 40,
                processes_max: Some(8192),
                cpu_usec: 1_500_000,
                oom_kills: 1,
                fork_refusals: 5,
            }
        );
        assert_eq!(read_counters(&d.path().join("sumiu")), None);
        assert_eq!(
            cgroup_path("0::/user.slice/x.scope\n"),
            Some(PathBuf::from("/sys/fs/cgroup/user.slice/x.scope"))
        );
    }

    #[test]
    fn formato_humano() {
        assert_eq!(human(1288490188), "1,2G");
        assert_eq!(human(46 * (1 << 30) / 100 * 75), "34,5G");
        assert_eq!(human(512 << 20), "512M");
        assert_eq!(human(1 << 30), "1G");
        assert_eq!(human(900), "900B");
    }
}
