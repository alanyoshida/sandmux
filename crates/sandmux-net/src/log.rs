//! Registro de rede: uma linha JSON por conexão, fluxo ou consulta, num
//! arquivo por dia. Nada do conteúdo das conexões é gravado.

use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Domínio de uma conexão sem domínio conhecido.
pub const NO_DOMAIN: &str = "sem domínio";

/// Uma linha do registro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub ts: String,
    pub tag: String,
    pub workdir: PathBuf,
    pub proto: String,
    pub domain: String,
    pub ip: String,
    pub port: u16,
    pub up: u64,
    pub down: u64,
    pub ms: u64,
    pub blocked: Option<String>,
    /// O domínio apareceu pela primeira vez nesta pasta de trabalho.
    #[serde(default)]
    pub new: bool,
    /// Pedido decifrado: método, caminho sem a query e status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Nome do segredo trocado neste pedido (nunca o valor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// Protocolo da linha de uma assinatura do agente SSH.
pub const SSH_SIGN: &str = "ssh-sign";

impl Entry {
    /// Uma assinatura do agente SSH: o domínio é a impressão digital da
    /// chave, e `secret` o nome dela.
    pub fn ssh_sign(
        tag: &str,
        workdir: &Path,
        name: &str,
        fingerprint: &str,
        blocked: Option<&str>,
    ) -> Entry {
        Entry {
            ts: now_local().1,
            tag: tag.into(),
            workdir: workdir.into(),
            proto: SSH_SIGN.into(),
            domain: fingerprint.into(),
            ip: String::new(),
            port: 0,
            up: 0,
            down: 0,
            ms: 0,
            blocked: blocked.map(String::from),
            new: false,
            method: None,
            path: None,
            status: None,
            secret: (!name.is_empty()).then(|| name.to_string()),
        }
    }
}

/// Acrescenta uma linha ao arquivo do dia em `dir` (pasta 700, arquivo 600).
pub fn append(dir: &Path, e: &Entry) {
    let day = e.ts.split('T').next().unwrap_or_default().to_string();
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join(format!("{day}.jsonl")))
        && let Ok(mut line) = serde_json::to_string(e)
    {
        line.push('\n');
        let _ = f.write_all(line.as_bytes());
    }
}

/// Um pedido HTTP decifrado.
pub struct Http<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub status: u16,
    pub secret: Option<&'a str>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Data e hora locais: (`AAAA-MM-DD`, `AAAA-MM-DDTHH:MM:SS`).
pub fn now_local() -> (String, String) {
    local(now_secs())
}

/// Dia local (`AAAA-MM-DD`) de `days` dias atrás.
pub fn day_ago(days: u32) -> String {
    local(now_secs() - days as i64 * 86_400).0
}

fn local(secs: i64) -> (String, String) {
    let t = secs as libc::time_t;
    // SAFETY: localtime_r com ponteiros válidos.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    let day = format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    );
    let ts = format!("{day}T{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
    (day, ts)
}

/// Hash FNV-1a de 64 bits, para o nome do arquivo de domínios conhecidos.
fn fnv(p: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in p.as_os_str().as_encoded_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Grava as linhas de um sandbox.
pub struct Log {
    dir: PathBuf,
    tag: String,
    workdir: PathBuf,
    known: Mutex<Option<HashSet<String>>>,
}

impl Log {
    pub fn new(dir: &Path, tag: &str, workdir: &Path) -> Log {
        Log {
            dir: dir.into(),
            tag: tag.into(),
            workdir: workdir.into(),
            known: Mutex::new(None),
        }
    }

    fn known_file(&self) -> PathBuf {
        self.dir
            .join("known")
            .join(format!("{}.txt", fnv(&self.workdir)))
    }

    /// Marca o domínio como conhecido; devolve se ele é novo para a pasta.
    fn is_new(&self, domain: &str) -> bool {
        if domain == NO_DOMAIN || domain.is_empty() {
            return false;
        }
        let mut g = self.known.lock().unwrap_or_else(|e| e.into_inner());
        let set = g.get_or_insert_with(|| {
            std::fs::read_to_string(self.known_file())
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        });
        if set.contains(domain) {
            return false;
        }
        set.insert(domain.to_string());
        let f = self.known_file();
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&f)
        {
            let _ = writeln!(file, "{domain}");
        }
        true
    }

    /// Grava uma linha. Erros de disco não derrubam o gateway.
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &self,
        proto: &str,
        domain: &str,
        ip: &str,
        port: u16,
        up: u64,
        down: u64,
        ms: u64,
        blocked: Option<&str>,
    ) -> Entry {
        self.write_full(proto, domain, ip, port, up, down, ms, blocked, None)
    }

    /// Grava a linha de um pedido decifrado.
    #[allow(clippy::too_many_arguments)]
    pub fn write_http(
        &self,
        domain: &str,
        ip: &str,
        port: u16,
        up: u64,
        down: u64,
        ms: u64,
        http: Http<'_>,
    ) -> Entry {
        self.write_full("https", domain, ip, port, up, down, ms, None, Some(http))
    }

    #[allow(clippy::too_many_arguments)]
    fn write_full(
        &self,
        proto: &str,
        domain: &str,
        ip: &str,
        port: u16,
        up: u64,
        down: u64,
        ms: u64,
        blocked: Option<&str>,
        http: Option<Http<'_>>,
    ) -> Entry {
        let (day, ts) = now_local();
        let e = Entry {
            ts,
            tag: self.tag.clone(),
            workdir: self.workdir.clone(),
            proto: proto.into(),
            domain: domain.into(),
            ip: ip.into(),
            port,
            up,
            down,
            ms,
            blocked: blocked.map(String::from),
            // Uma conexão de entrada não é um destino novo.
            new: blocked.is_none() && proto != "inbound" && self.is_new(domain),
            method: http.as_ref().map(|h| h.method.to_string()),
            path: http.as_ref().map(|h| h.path.to_string()),
            status: http.as_ref().map(|h| h.status),
            secret: http.as_ref().and_then(|h| h.secret.map(String::from)),
        };
        if std::fs::create_dir_all(&self.dir).is_ok() {
            let _ = std::fs::set_permissions(
                &self.dir,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            );
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(self.dir.join(format!("{day}.jsonl")))
                && let Ok(mut line) = serde_json::to_string(&e)
            {
                line.push('\n');
                let _ = f.write_all(line.as_bytes());
            }
        }
        e
    }
}

/// Linhas dos últimos `days` dias (hoje incluído), das mais velhas às mais
/// novas.
pub fn read_recent(dir: &Path, days: u32) -> Vec<Entry> {
    (0..days.max(1))
        .rev()
        .flat_map(|d| read_file(&dir.join(format!("{}.jsonl", day_ago(d)))))
        .collect()
}

/// Últimas linhas de ontem e de hoje em memória, lendo de cada vez só o que
/// foi acrescentado ao arquivo de hoje.
pub struct Tail {
    dir: PathBuf,
    cap: usize,
    day: String,
    offset: u64,
    entries: std::collections::VecDeque<Entry>,
}

impl Tail {
    pub fn new(dir: &Path, cap: usize) -> Tail {
        Tail {
            dir: dir.into(),
            cap: cap.max(1),
            day: String::new(),
            offset: 0,
            entries: Default::default(),
        }
    }

    fn push(&mut self, e: Entry) {
        if self.entries.len() == self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
    }

    /// Lê o que há de novo e devolve as linhas guardadas, das mais velhas às
    /// mais novas.
    pub fn refresh(&mut self) -> &std::collections::VecDeque<Entry> {
        use std::io::{Read, Seek, SeekFrom};
        let today = day_ago(0);
        let path = self.dir.join(format!("{today}.jsonl"));
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if self.day != today || len < self.offset {
            // Dia novo, ou o arquivo foi apagado ou trocado: recomeça.
            self.entries.clear();
            self.day = today;
            self.offset = 0;
            for e in read_file(&self.dir.join(format!("{}.jsonl", day_ago(1)))) {
                self.push(e);
            }
        }
        if len > self.offset
            && let Ok(mut f) = std::fs::File::open(&path)
            && f.seek(SeekFrom::Start(self.offset)).is_ok()
        {
            let mut buf = Vec::new();
            let _ = f.read_to_end(&mut buf);
            // Só as linhas completas; o resto fica para a próxima vez.
            if let Some(end) = buf.iter().rposition(|b| *b == b'\n') {
                let text = String::from_utf8_lossy(&buf[..end]).into_owned();
                for e in text.lines().filter_map(|l| serde_json::from_str(l).ok()) {
                    self.push(e);
                }
                self.offset += end as u64 + 1;
            }
        }
        &self.entries
    }
}

/// Arquivos do registro (`AAAA-MM-DD.jsonl`), dos mais velhos aos mais novos,
/// com o tamanho.
fn day_files(dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let mut v: Vec<(String, PathBuf, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let day = name.strip_suffix(".jsonl")?;
            let ok = day.len() == 10
                && day.chars().enumerate().all(|(i, c)| {
                    if i == 4 || i == 7 {
                        c == '-'
                    } else {
                        c.is_ascii_digit()
                    }
                });
            let len = e.metadata().ok()?.len();
            ok.then(|| (day.to_string(), e.path(), len))
        })
        .collect();
    v.sort();
    v
}

/// Retenção: apaga os arquivos com mais de `days` dias e, se a soma passar de
/// `max_bytes`, os mais velhos até caber. Devolve os apagados.
pub fn prune(dir: &Path, days: u32, max_bytes: u64) -> Vec<PathBuf> {
    let cutoff = day_ago(days);
    let mut removed = Vec::new();
    let mut files = day_files(dir);
    files.retain(|(day, path, _)| {
        if *day < cutoff {
            if std::fs::remove_file(path).is_ok() {
                removed.push(path.clone());
            }
            false
        } else {
            true
        }
    });
    let mut total: u64 = files.iter().map(|f| f.2).sum();
    for (_, path, len) in &files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            removed.push(path.clone());
            total -= len;
        }
    }
    removed
}

/// Lê as linhas de um arquivo do registro (as inválidas são puladas).
pub fn read_file(p: &Path) -> Vec<Entry> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linhas_e_dominio_novo() {
        let d = tempfile::tempdir().unwrap();
        let log = Log::new(d.path(), "run-1", Path::new("/w"));
        let a = log.write("tcp", "exemplo.test", "1.2.3.4", 443, 10, 20, 5, None);
        assert!(a.new);
        let b = log.write("tcp", "exemplo.test", "1.2.3.4", 443, 1, 2, 3, None);
        assert!(!b.new);
        let c = log.write("tcp", NO_DOMAIN, "1.2.3.4", 80, 0, 0, 1, None);
        assert!(!c.new);
        let x = log.write("udp", "q.test", "1.2.3.4", 443, 0, 0, 0, Some("QUIC"));
        assert!(!x.new);
        // Outro sandbox na mesma pasta já conhece o domínio.
        let log2 = Log::new(d.path(), "run-2", Path::new("/w"));
        assert!(
            !log2
                .write("tcp", "exemplo.test", "1.2.3.4", 443, 0, 0, 0, None)
                .new
        );
        let day = now_local().0;
        let all = read_file(&d.path().join(format!("{day}.jsonl")));
        assert_eq!(all.len(), 5);
        assert_eq!(all[3].blocked.as_deref(), Some("QUIC"));
        assert_eq!(all[0].up, 10);
        assert_eq!(read_recent(d.path(), 2).len(), 5);
    }

    #[test]
    fn tail_le_so_o_acrescentado() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join(format!("{}.jsonl", day_ago(1))),
            "{\"ts\":\"x\",\"tag\":\"a\",\"workdir\":\"/w\",\"proto\":\"tcp\",\"domain\":\"ontem.test\",\"ip\":\"\",\"port\":1,\"up\":0,\"down\":0,\"ms\":0,\"blocked\":null}\n",
        )
        .unwrap();
        let mut t = Tail::new(d.path(), 3);
        assert_eq!(t.refresh().len(), 1);
        let log = Log::new(d.path(), "b", Path::new("/w"));
        log.write("tcp", "a.test", "1.1.1.1", 1, 0, 0, 0, None);
        log.write("tcp", "b.test", "1.1.1.1", 1, 0, 0, 0, None);
        let v: Vec<String> = t.refresh().iter().map(|e| e.domain.clone()).collect();
        assert_eq!(v, ["ontem.test", "a.test", "b.test"]);
        log.write("tcp", "c.test", "1.1.1.1", 1, 0, 0, 0, None);
        let v: Vec<String> = t.refresh().iter().map(|e| e.domain.clone()).collect();
        assert_eq!(v, ["a.test", "b.test", "c.test"]);
        // Arquivo apagado: recomeça.
        std::fs::remove_file(d.path().join(format!("{}.jsonl", day_ago(0)))).unwrap();
        log.write("tcp", "d.test", "1.1.1.1", 1, 0, 0, 0, None);
        let v: Vec<String> = t.refresh().iter().map(|e| e.domain.clone()).collect();
        assert_eq!(v, ["ontem.test", "d.test"]);
    }

    #[test]
    fn retencao_por_idade_e_por_tamanho() {
        let d = tempfile::tempdir().unwrap();
        let p = |n: u32| d.path().join(format!("{}.jsonl", day_ago(n)));
        std::fs::write(p(40), "x").unwrap();
        std::fs::write(p(3), vec![b'x'; 100]).unwrap();
        std::fs::write(p(1), vec![b'x'; 100]).unwrap();
        std::fs::write(p(0), vec![b'x'; 100]).unwrap();
        std::fs::write(d.path().join("outro.txt"), "x").unwrap();
        let r = prune(d.path(), 30, 10_000);
        assert_eq!(r, vec![p(40)]);
        let r = prune(d.path(), 30, 250);
        assert_eq!(r, vec![p(3)]);
        assert!(p(1).exists() && p(0).exists());
        assert!(d.path().join("outro.txt").exists());
    }
}
