//! Os arquivos do Docker confinado por eBPF, ao vivo: as marcas em vigor, a
//! vigia da pasta de trabalho (os arquivos criados com o Docker rodando), a
//! política de arquivos ao vivo e o registro dos bloqueios e das criações
//! dos contêineres.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use sandmux_policy::{DockerMarks, MarkSet};
use sandmux_spec::FileEntry;
use sandmux_spec::files::{FilePolicy, FileVerdict};
use sandmux_spec::{tr, trf};
use serde_json::Value;

use crate::helper::{Client, MarkKind};

/// Espera de silêncio depois de uma criação.
const QUIET: Duration = Duration::from_millis(200);
/// Intervalo da varredura de recuo, sem vigias de inotify.
const POLL: Duration = Duration::from_secs(10);

const DENY: u32 = 1;
const PROTECT: u32 = 2;
const GITDIR: u32 = 4;
const SECRET: u32 = 8;
const FDENY: u32 = 16;

/// O estado dos arquivos de um Docker confinado.
pub struct Files {
    workdir: PathBuf,
    tag: String,
    /// Os arquivos de uma aba confinada pelo kernel (e não do Docker): as
    /// marcas de segredo e de `file-deny` separadas, e o registro das
    /// aberturas.
    tab: bool,
    /// As pastas do registro e as de fora dele, na visão do agente: só as
    /// aberturas dentro delas entram no registro da aba.
    roots: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
    /// A pasta de estado do registro (os caminhos conhecidos).
    state_dir: PathBuf,
    client: Client,
    state: Mutex<State>,
    /// A vigia caiu na varredura a cada 10 s.
    pub poll: AtomicBool,
    /// O Docker está parando: a queda da conexão não é um erro.
    pub stopping: AtomicBool,
}

struct State {
    policy: FilePolicy,
    /// As marcas em vigor.
    marks: MarkSet,
    /// As marcas que não dependem da política: as coberturas fora da pasta
    /// (`/etc`) e a proteção do `.git`.
    fixed: DockerMarks,
    /// Inode → caminhos marcados (para traduzir os bloqueios).
    by_ino: HashMap<u64, Vec<PathBuf>>,
    /// Inode → pasta da pasta de trabalho (para traduzir as criações).
    dirs: HashMap<u64, PathBuf>,
    /// Caminhos criados por contêineres já registrados nesta subida.
    created_logged: HashSet<PathBuf>,
    /// As aberturas já registradas neste lançamento (caminho e acesso),
    /// como o supervisor: uma linha por par.
    opened: HashSet<(PathBuf, &'static str)>,
    /// Os caminhos já vistos nesta pasta de trabalho, em qualquer
    /// lançamento (o campo `new`), lidos na primeira abertura.
    known: Option<Known>,
    /// Já avisou do teto de marcas.
    warned_cap: bool,
}

fn state_dir() -> PathBuf {
    crate::paths::state_dir().join("files")
}

/// Os caminhos já vistos numa pasta de trabalho, o mesmo arquivo do
/// supervisor (`known/<hash>.txt`).
struct Known {
    paths: HashSet<PathBuf>,
    file: Option<std::fs::File>,
}

impl Known {
    fn load(state: &Path, workdir: &Path) -> Known {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::OpenOptionsExt;
        let f = state
            .join("known")
            .join(format!("{}.txt", sandmux_spec::files::dir_hash(workdir)));
        let paths = std::fs::read(&f)
            .map(|b| {
                b.split(|&c| c == b'\n')
                    .filter(|l| !l.is_empty())
                    .map(|l| PathBuf::from(std::ffi::OsStr::from_bytes(l)))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&f)
            .ok();
        Known { paths, file }
    }

    /// Guarda `path`; verdadeiro na primeira vez.
    fn insert(&mut self, path: &Path) -> bool {
        use std::io::Write;
        use std::os::unix::ffi::OsStrExt;
        if !self.paths.insert(path.to_path_buf()) {
            return false;
        }
        if let Some(f) = self.file.as_mut() {
            let mut line = path.as_os_str().as_bytes().to_vec();
            line.push(b'\n');
            let _ = f.write_all(&line);
        }
        true
    }
}

fn created(workdir: &Path) -> HashSet<PathBuf> {
    sandmux_spec::files::read_created(&state_dir(), workdir)
}

/// As marcas que não dependem da política, de um conjunto calculado na
/// subida: as negadas fora da pasta de trabalho e a proteção do `.git`.
pub fn fixed_marks(m: &DockerMarks, workdir: &Path) -> DockerMarks {
    DockerMarks {
        deny: m
            .deny
            .iter()
            .filter(|p| !p.starts_with(workdir))
            .cloned()
            .collect(),
        protect: m.protect.clone(),
        gitdir: m.gitdir.clone(),
        ..Default::default()
    }
}

/// As marcas que valem para a política `policy`: as coberturas da varredura
/// da pasta, as fixas e o conteúdo das pastas negadas e protegidas. `Err`
/// com a contagem acima do teto.
pub fn desired(
    policy: &FilePolicy,
    created: &HashSet<PathBuf>,
    workdir: &Path,
    fixed: &DockerMarks,
) -> Result<MarkSet, usize> {
    let covers = sandmux_policy::docker_covers(policy, created, workdir)?;
    let mut m = fixed.clone();
    m.deny.extend(covers);
    m.deny.sort();
    m.deny.dedup();
    m.expand(crate::helper::MAX_MARKS)?;
    Ok(sandmux_policy::mark_set(&m))
}

/// As marcas de uma aba confinada pelo kernel: os segredos (`SECRET`) e o
/// `file-deny` (`FDENY`) da pasta de trabalho, com o conteúdo das pastas
/// negadas, e as fixas (a proteção do `.git`).
pub fn desired_tab(
    policy: &FilePolicy,
    created: &HashSet<PathBuf>,
    workdir: &Path,
    fixed: &DockerMarks,
) -> Result<MarkSet, usize> {
    let max = crate::helper::MAX_MARKS;
    let (secret, fdeny) =
        sandmux_policy::tab_covers(policy, created, &[workdir.to_path_buf()], &[], max)?;
    let mut m = fixed.clone();
    m.secret.extend(secret);
    m.fdeny.extend(fdeny);
    m.expand(max)?;
    Ok(sandmux_policy::mark_set(&m))
}

/// A política cabe nas marcas de uma aba? As marcas cobrem a pasta de
/// trabalho: um `file-deny` que pode valer fora dela (um nome, ou um caminho
/// de fora) fica com o supervisor. `Err` com o motivo.
pub fn tab_policy_fits(policy: &FilePolicy, workdir: &Path) -> Result<(), String> {
    use sandmux_spec::files::FilePattern;
    for p in &policy.deny {
        match p {
            FilePattern::Path(x) if Path::new(x).starts_with(workdir) => {}
            _ => {
                return Err(trf!(
                    "file-deny `{p}` vale fora da pasta de trabalho",
                    "file-deny `{p}` applies outside the work folder"
                ));
            }
        }
    }
    Ok(())
}

fn kinds(bits: u32) -> impl Iterator<Item = MarkKind> {
    [
        (DENY, MarkKind::Deny),
        (PROTECT, MarkKind::Protect),
        (GITDIR, MarkKind::GitDir),
        (SECRET, MarkKind::Secret),
        (FDENY, MarkKind::FileDeny),
    ]
    .into_iter()
    .filter(move |(b, _)| bits & b != 0)
    .map(|(_, k)| k)
}

fn ino_of(p: &Path) -> Option<u64> {
    std::fs::symlink_metadata(p).ok().map(|m| m.ino())
}

impl Files {
    /// O estado de um confinamento recém-ligado, com as marcas `initial`
    /// já mandadas ao auxiliar.
    pub fn new(
        workdir: &Path,
        tag: String,
        client: Client,
        policy: FilePolicy,
        initial: &DockerMarks,
    ) -> Files {
        Files::with_mode(workdir, tag, client, policy, initial, false)
    }

    /// Como [`Files::new`]; com `tab`, os arquivos de uma aba confinada pelo
    /// kernel.
    pub fn with_mode(
        workdir: &Path,
        tag: String,
        client: Client,
        policy: FilePolicy,
        initial: &DockerMarks,
        tab: bool,
    ) -> Files {
        let marks = sandmux_policy::mark_set(initial);
        let mut by_ino: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for p in marks.keys() {
            if let Some(i) = ino_of(p) {
                by_ino.entry(i).or_default().push(p.clone());
            }
        }
        Files {
            workdir: workdir.to_path_buf(),
            tag,
            tab,
            roots: Vec::new(),
            exclude: Vec::new(),
            state_dir: state_dir(),
            client,
            state: Mutex::new(State {
                policy,
                marks,
                fixed: fixed_marks(initial, workdir),
                by_ino,
                dirs: HashMap::new(),
                created_logged: HashSet::new(),
                opened: HashSet::new(),
                known: None,
                warned_cap: false,
            }),
            poll: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
        }
    }

    /// As pastas do registro da aba (`roots`) e as de fora dele (`exclude`).
    pub fn with_view(mut self, (roots, exclude): (Vec<PathBuf>, Vec<PathBuf>)) -> Files {
        self.roots = roots;
        self.exclude = exclude;
        self
    }

    /// Para: a queda da conexão deixa de ser um erro, e a conexão com o
    /// auxiliar fecha (a thread dos eventos segura o `Files`).
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.client.close();
    }

    /// Espera a conexão com o auxiliar cair ou fechar.
    pub fn wait_closed(&self) {
        self.client.wait_closed();
    }

    /// Quem as marcas confinam, para o log.
    fn who(&self) -> &'static str {
        if self.tab {
            tr!("a aba", "tab")
        } else {
            "Docker"
        }
    }

    /// Outra pasta de estado (os testes).
    #[cfg(test)]
    fn with_state(mut self, state: PathBuf) -> Files {
        self.state_dir = state;
        self
    }

    /// A abertura de `path` entra no registro da aba.
    fn in_view(&self, path: &Path) -> bool {
        (self.roots.is_empty() || self.roots.iter().any(|r| path.starts_with(r)))
            && !self.exclude.iter().any(|e| path.starts_with(e))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Manda ao auxiliar a diferença entre as marcas em vigor e `new`.
    fn apply(&self, st: &mut State, new: MarkSet) -> Result<(), String> {
        let (add, del) = sandmux_policy::mark_diff(&st.marks, &new);
        for (paths, bit) in group(&add) {
            for k in kinds(bit) {
                self.client.mark(k, &paths)?;
            }
        }
        for (paths, bit) in group(&del) {
            for k in kinds(bit) {
                self.client.unmark(k, &paths)?;
            }
        }
        for p in del.keys() {
            if !new.contains_key(p)
                && let Some(i) = ino_of(p)
                && let Some(v) = st.by_ino.get_mut(&i)
            {
                v.retain(|x| x != p);
            }
        }
        for p in add.keys() {
            if !st.marks.contains_key(p)
                && let Some(i) = ino_of(p)
            {
                st.by_ino.entry(i).or_default().push(p.clone());
            }
        }
        st.marks = new;
        Ok(())
    }

    /// Recalcula as marcas com a política `policy` (ou a atual) e manda a
    /// diferença.
    /// `Err` quando as marcas não ficaram como a política pede (acima do teto
    /// ou recusadas pelo auxiliar); o motivo também vai ao log.
    pub fn refresh(&self, policy: Option<FilePolicy>) -> Result<(), String> {
        let mut st = self.lock();
        if let Some(p) = policy {
            st.policy = p;
        }
        let calc = if self.tab { desired_tab } else { desired };
        let new = match calc(
            &st.policy,
            &created(&self.workdir),
            &self.workdir,
            &st.fixed,
        ) {
            Ok(n) => n,
            Err(n) => {
                let max = crate::helper::MAX_MARKS;
                if !st.warned_cap {
                    st.warned_cap = true;
                    eprintln!(
                        "{}",
                        trf!(
                            "sandmuxd: {} de {}: {n} arquivos a marcar, acima do teto de {max}; as marcas ficam como estão",
                            "sandmuxd: {} for {}: {n} files to mark, above the cap of {max}; marks stay as they are",
                            self.who(),
                            self.workdir.display()
                        )
                    );
                }
                return Err(trf!(
                    "{n} arquivos a marcar, acima do teto de {max}",
                    "{n} files to mark, above the cap of {max}"
                ));
            }
        };
        self.apply(&mut st, new).inspect_err(|e| {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: {} de {}: marcas ao vivo: {e}",
                    "sandmuxd: {} for {}: live marks: {e}",
                    self.who(),
                    self.workdir.display()
                )
            );
        })
    }

    /// Marca os caminhos novos da vigia (criados com o Docker rodando) que a
    /// política nega, com o que está dentro das pastas novas.
    fn mark_new(&self, paths: &BTreeSet<PathBuf>) {
        let created = created(&self.workdir);
        let mut st = self.lock();
        let dir_bit = if self.tab { FDENY } else { DENY };
        let mut denied: Vec<PathBuf> = st
            .marks
            .iter()
            .filter(|(p, b)| *b & dir_bit != 0 && p.is_dir())
            .map(|(p, _)| p.clone())
            .collect();
        let mut new = st.marks.clone();
        for p in paths {
            if self.tab {
                // Dentro de uma pasta negada, ou negado ou segredo pela
                // política, cada um com a marca dele.
                let bit = if denied.iter().any(|d| p.starts_with(d)) {
                    FDENY
                } else {
                    match st.policy.decide(p, |q| created.contains(q)) {
                        FileVerdict::Deny(_) => FDENY,
                        FileVerdict::Secret => SECRET,
                        FileVerdict::Pass => 0,
                    }
                };
                if bit != 0 {
                    *new.entry(p.clone()).or_default() |= bit;
                    if bit == FDENY && p.is_dir() {
                        denied.push(p.clone());
                    }
                }
            } else if sandmux_policy::docker_mark_for(
                &st.policy,
                |q| created.contains(q),
                &denied,
                p,
            ) {
                *new.entry(p.clone()).or_default() |= DENY;
                if p.is_dir() {
                    denied.push(p.clone());
                }
            }
        }
        if new != st.marks
            && let Err(e) = self.apply(&mut st, new)
        {
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: Docker de {}: marcar arquivo novo: {e}",
                    "sandmuxd: Docker for {}: mark new file: {e}",
                    self.workdir.display()
                )
            );
        }
    }

    /// Lê os eventos do auxiliar até a conexão terminar, gravando as linhas
    /// no registro. Quando ela termina sem o Docker estar parando, chama
    /// `on_drop`.
    pub fn events_loop(self: &Arc<Files>, on_drop: impl FnOnce()) {
        let Some(rx) = self.client.take_events() else {
            return;
        };
        for msg in rx {
            for e in self.entries(&msg) {
                write_entry(&e);
            }
        }
        if !self.stopping.load(Ordering::SeqCst) {
            on_drop();
        }
    }

    /// As linhas do registro de uma mensagem de eventos.
    pub fn entries(&self, msg: &Value) -> Vec<FileEntry> {
        let (_, ts) = sandmux_net::log::now_local();
        let mut out = Vec::new();
        let mut st = self.lock();
        let entry = |path: PathBuf, access: &str, blocked: Option<String>| FileEntry {
            ts: ts.clone(),
            tag: self.tag.clone(),
            workdir: self.workdir.clone(),
            path,
            access: access.into(),
            blocked,
            new: false,
            args: Vec::new(),
            via: None,
        };
        if let Some(n) = msg["dropped"].as_u64().filter(|n| *n > 0) {
            out.push(entry(
                self.workdir.clone(),
                "?",
                Some(trf!(
                    "{n} eventos descartados (fila cheia)",
                    "{n} events dropped (queue full)"
                )),
            ));
        }
        // As execuções de uma aba vêm pela observação dela (`ExecObs`); o
        // confinamento também as manda, e seriam linhas dobradas.
        if !self.tab {
            out.extend(super::execlog::entries(msg, &self.tag, &self.workdir));
        }
        for e in msg["ev"].as_array().into_iter().flatten() {
            let ino = e["ino"].as_u64().unwrap_or(0);
            match e["kind"].as_str() {
                Some("deny") => {
                    let Some(path) = st.by_ino.get(&ino).and_then(|v| v.first()).cloned() else {
                        continue;
                    };
                    let bits = e["bits"].as_u64().unwrap_or(0) as u32;
                    let hook = e["hook"].as_str().unwrap_or("");
                    let access = match hook {
                        "file_open" if e["write"] == true => "w",
                        "file_open" => "r",
                        "inode_create" | "inode_mkdir" | "inode_mknod" | "inode_symlink" => {
                            "create"
                        }
                        _ => "w",
                    };
                    let why = reason(&st.policy, &self.workdir, &path, bits);
                    out.push(entry(path, access, Some(why)));
                }
                Some("open") if self.tab => {
                    let path = e["path"].as_str().unwrap_or("");
                    if path.is_empty() || !self.in_view(Path::new(path)) {
                        continue;
                    }
                    let access = if e["write"] == true { "w" } else { "r" };
                    let path = PathBuf::from(path);
                    if !st.opened.insert((path.clone(), access)) {
                        continue;
                    }
                    let new = st
                        .known
                        .get_or_insert_with(|| Known::load(&self.state_dir, &self.workdir))
                        .insert(&path);
                    out.push(FileEntry {
                        new,
                        ..entry(path, access, None)
                    });
                }
                Some("create") => {
                    let Some(dir) = st.dirs.get(&ino).cloned() else {
                        continue;
                    };
                    let name = e["name"].as_str().unwrap_or("");
                    if name.is_empty() || name.contains('/') {
                        continue;
                    }
                    let path = dir.join(name);
                    // Numa aba, o segredo que o agente cria continua dele: vai
                    // para a lista dos criados (que o supervisor fazia).
                    if self.tab && st.policy.is_secret(&path) {
                        let _ =
                            sandmux_spec::files::add_created(&state_dir(), &self.workdir, &path);
                    }
                    if st.created_logged.insert(path.clone()) {
                        let new = self.tab
                            && st
                                .known
                                .get_or_insert_with(|| Known::load(&self.state_dir, &self.workdir))
                                .insert(&path);
                        out.push(FileEntry {
                            new,
                            ..entry(path, "create", None)
                        });
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Guarda o inode de cada pasta da pasta de trabalho.
    fn index_dirs(&self, dirs: &[PathBuf]) {
        let mut st = self.lock();
        for d in dirs {
            if let Some(i) = ino_of(d) {
                st.dirs.insert(i, d.clone());
            }
        }
    }

    /// A vigia da pasta, até `stopping`. Com inotify, marca o que aparece
    /// depois de 200 ms de silêncio; sem vigias, refaz a varredura a cada
    /// 10 s. Também recalcula tudo quando os segredos do agente mudam.
    pub fn watch_loop(self: &Arc<Files>) {
        self.watch(false, POLL);
    }

    /// A vigia, com o recuo para a varredura forçado (`force_poll`) e o
    /// intervalo dela (testes).
    fn watch(self: &Arc<Files>, force_poll: bool, poll_every: Duration) {
        let created_file = sandmux_spec::files::created_file(&state_dir(), &self.workdir);
        let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut created_stamp: Option<SystemTime> = stamp(&created_file);
        let ino = Inotify::init(InitFlags::IN_CLOEXEC | InitFlags::IN_NONBLOCK).ok();
        let mut wds: HashMap<WatchDescriptor, PathBuf> = HashMap::new();
        let flags = AddWatchFlags::IN_CREATE
            | AddWatchFlags::IN_MOVED_TO
            | AddWatchFlags::IN_ONLYDIR
            | AddWatchFlags::IN_DONT_FOLLOW;
        let mut polling = force_poll
            || match &ino {
                Some(i) => add_tree(i, &mut wds, flags, &self.workdir, self).is_none(),
                None => true,
            };
        let enter_poll = |me: &Files| {
            me.poll.store(true, Ordering::SeqCst);
            eprintln!(
                "{}",
                trf!(
                    "sandmuxd: Docker de {}: sem vigias de inotify (fs.inotify.max_user_watches); arquivos novos por varredura a cada 10 s",
                    "sandmuxd: Docker for {}: no inotify watches left (fs.inotify.max_user_watches); new files by a scan every 10 s",
                    me.workdir.display()
                )
            );
        };
        if polling {
            enter_poll(self);
        }
        let mut pending: BTreeSet<PathBuf> = BTreeSet::new();
        let mut last_event = Instant::now();
        let mut last_poll = Instant::now();
        while !self.stopping.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
            // Os segredos do agente mudaram: recalcula tudo.
            let s = stamp(&created_file);
            if s != created_stamp {
                created_stamp = s;
                let _ = self.refresh(None);
            }
            if polling {
                if last_poll.elapsed() >= poll_every {
                    last_poll = Instant::now();
                    self.index_dirs(&all_dirs(&self.workdir));
                    let _ = self.refresh(None);
                }
                continue;
            }
            let Some(i) = ino.as_ref() else { continue };
            if let Ok(evs) = i.read_events() {
                for ev in evs {
                    let (Some(dir), Some(name)) = (wds.get(&ev.wd).cloned(), ev.name) else {
                        continue;
                    };
                    let p = dir.join(name);
                    if p.is_dir() && !p.is_symlink() {
                        match add_tree(i, &mut wds, flags, &p, self) {
                            Some(found) => pending.extend(found),
                            None => {
                                polling = true;
                                enter_poll(self);
                            }
                        }
                    }
                    pending.insert(p);
                    last_event = Instant::now();
                }
            }
            if !pending.is_empty() && last_event.elapsed() >= QUIET {
                let batch = std::mem::take(&mut pending);
                self.mark_new(&batch);
            }
        }
    }
}

/// Percorre `root`: vigia as pastas e devolve tudo o que achou. `None` sem
/// vigias disponíveis (`ENOSPC`).
fn add_tree(
    ino: &Inotify,
    wds: &mut HashMap<WatchDescriptor, PathBuf>,
    flags: AddWatchFlags,
    root: &Path,
    files: &Files,
) -> Option<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut dirs = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        match ino.add_watch(&d, flags) {
            Ok(wd) => {
                wds.insert(wd, d.clone());
            }
            Err(nix::errno::Errno::ENOSPC) => return None,
            Err(_) => {}
        }
        dirs.push(d.clone());
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(p.clone());
                }
                found.push(p);
            }
        }
    }
    files.index_dirs(&dirs);
    Some(found)
}

/// Todas as pastas da pasta de trabalho, sem seguir links.
fn all_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    stack.push(e.path());
                }
            }
        }
        out.push(d);
    }
    out
}

/// Agrupa as marcas pelos bits, para mandar cada tipo de uma vez.
fn group(m: &MarkSet) -> Vec<(Vec<PathBuf>, u32)> {
    let mut by: HashMap<u32, Vec<PathBuf>> = HashMap::new();
    for (p, b) in m {
        by.entry(*b).or_default().push(p.clone());
    }
    let mut v: Vec<_> = by.into_iter().map(|(b, p)| (p, b)).collect();
    v.sort_by_key(|x| x.1);
    v
}

/// O motivo de um bloqueio, como no registro do agente.
pub fn reason(policy: &FilePolicy, workdir: &Path, path: &Path, bits: u32) -> String {
    if !path.starts_with(workdir) {
        return tr!("cobertura", "cover").into();
    }
    if bits & (PROTECT | GITDIR) != 0 && bits & (DENY | SECRET | FDENY) == 0 {
        return ".git".into();
    }
    if bits & SECRET != 0 {
        return tr!("segredo", "secret").into();
    }
    match policy.decide(path, |_| false) {
        FileVerdict::Deny(p) => format!("file-deny: {p}"),
        FileVerdict::Secret => tr!("segredo", "secret").into(),
        FileVerdict::Pass => "file-deny".into(),
    }
}

/// Grava uma linha no registro de arquivos do dia (pasta 700, arquivo 600).
pub fn write_entry(e: &FileEntry) {
    let (day, _) = sandmux_net::log::now_local();
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join(format!("{day}.jsonl")))
    else {
        return;
    };
    if let Ok(mut line) = serde_json::to_string(e) {
        line.push('\n');
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(w: &Path) -> FilePolicy {
        let (secrets, secret_exceptions) = sandmux_spec::files::default_secrets();
        FilePolicy {
            secrets,
            secret_exceptions,
            workdir: w.to_path_buf(),
            ..Default::default()
        }
    }

    #[test]
    fn motivo_do_bloqueio() {
        let w = Path::new("/w");
        let p = policy(w);
        assert_eq!(
            reason(&p, w, Path::new("/etc/x"), DENY),
            tr!("cobertura", "cover")
        );
        assert_eq!(reason(&p, w, Path::new("/w/.git/hooks"), PROTECT), ".git");
        assert_eq!(
            reason(&p, w, Path::new("/w/.env"), DENY),
            tr!("segredo", "secret")
        );
        let mut q = p.clone();
        q.deny = vec![sandmux_spec::files::FilePattern::parse("*.sql", None).unwrap()];
        assert_eq!(
            reason(&q, w, Path::new("/w/a.sql"), DENY),
            "file-deny: *.sql"
        );
    }

    #[test]
    fn marcas_desejadas_seguem_a_politica() {
        let d = tempfile::tempdir().unwrap();
        let w = d.path().canonicalize().unwrap();
        std::fs::write(w.join(".env"), "").unwrap();
        std::fs::create_dir_all(w.join("privado/sub")).unwrap();
        std::fs::write(w.join("privado/sub/a"), "").unwrap();
        let fixed = DockerMarks {
            deny: vec![PathBuf::from("/etc/shadow")],
            ..Default::default()
        };
        let mut p = policy(&w);
        let m = desired(&p, &HashSet::new(), &w, &fixed).unwrap();
        assert_eq!(m.get(&w.join(".env")), Some(&DENY));
        assert!(m.contains_key(Path::new("/etc/shadow")));
        assert!(!m.contains_key(&w.join("privado")));
        p.deny = vec![
            sandmux_spec::files::FilePattern::parse(&w.join("privado").display().to_string(), None)
                .unwrap(),
        ];
        let m2 = desired(&p, &HashSet::new(), &w, &fixed).unwrap();
        for x in ["privado", "privado/sub", "privado/sub/a"] {
            assert_eq!(m2.get(&w.join(x)), Some(&DENY), "{x}");
        }
        // O segredo do agente sai.
        let created: HashSet<PathBuf> = [w.join(".env")].into();
        let m3 = desired(&p, &created, &w, &fixed).unwrap();
        assert!(!m3.contains_key(&w.join(".env")));
    }

    #[test]
    fn marcas_da_aba_separam_segredo_e_file_deny() {
        let d = tempfile::tempdir().unwrap();
        let w = d.path().canonicalize().unwrap();
        std::fs::write(w.join(".env"), "").unwrap();
        std::fs::write(w.join("a.txt"), "").unwrap();
        std::fs::create_dir_all(w.join("privado/sub")).unwrap();
        std::fs::write(w.join("privado/sub/a"), "").unwrap();
        let mut p = policy(&w);
        p.deny = vec![
            sandmux_spec::files::FilePattern::parse(&w.join("privado").display().to_string(), None)
                .unwrap(),
        ];
        let fixed = DockerMarks::default();
        let m = desired_tab(&p, &HashSet::new(), &w, &fixed).unwrap();
        assert_eq!(m.get(&w.join(".env")), Some(&SECRET));
        for x in ["privado", "privado/sub", "privado/sub/a"] {
            assert_eq!(m.get(&w.join(x)), Some(&FDENY), "{x}");
        }
        assert!(!m.contains_key(&w.join("a.txt")));
        // Nada de `DENY`: o Docker não está nesta conta.
        assert!(m.values().all(|b| b & DENY == 0));
        // O segredo criado pelo agente fica dele.
        let created: HashSet<PathBuf> = [w.join(".env")].into();
        let m2 = desired_tab(&p, &created, &w, &fixed).unwrap();
        assert!(!m2.contains_key(&w.join(".env")));
    }

    #[test]
    fn politica_que_cabe_na_aba() {
        let w = Path::new("/w");
        let mut p = policy(w);
        assert!(tab_policy_fits(&p, w).is_ok());
        p.deny = vec![sandmux_spec::files::FilePattern::parse("/w/privado", None).unwrap()];
        assert!(tab_policy_fits(&p, w).is_ok());
        // Um nome vale em qualquer pasta, e um caminho de fora também fica
        // com o supervisor.
        for fora in ["*.sql", "/etc/passwd"] {
            p.deny = vec![sandmux_spec::files::FilePattern::parse(fora, None).unwrap()];
            let e = tab_policy_fits(&p, w).unwrap_err();
            assert!(e.contains(fora), "{e}");
        }
    }

    use crate::helper::{Backend, Confined, Events, Server};

    /// Auxiliar falso: guarda (caminho, bits, marca ou desmarca).
    type Log = Arc<Mutex<Vec<(PathBuf, u32, bool)>>>;
    struct FakeBackend(Log);
    struct FakeConfined(Log, usize);
    fn path_of(fd: &std::os::fd::OwnedFd) -> PathBuf {
        use std::os::fd::AsRawFd;
        std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).unwrap()
    }
    impl Confined for FakeConfined {
        fn mark(&mut self, fd: std::os::fd::OwnedFd, bits: u32) -> Result<(), String> {
            self.0.lock().unwrap().push((path_of(&fd), bits, true));
            self.1 += 1;
            Ok(())
        }
        fn unmark(&mut self, fd: std::os::fd::OwnedFd, bits: u32) -> Result<(), String> {
            self.0.lock().unwrap().push((path_of(&fd), bits, false));
            Ok(())
        }
        fn marks(&self) -> usize {
            self.1
        }
        fn attach(&self) -> &'static str {
            "cgroup"
        }
        fn events(&self) -> Option<Box<dyn Events>> {
            None
        }
    }
    impl Backend for FakeBackend {
        fn lsm(&self) -> Result<bool, String> {
            Ok(true)
        }
        fn observe(&self, _: &std::os::fd::OwnedFd) -> Result<Box<dyn Events>, String> {
            Err("sem observação".into())
        }
        fn confine(
            &self,
            _: &std::os::fd::OwnedFd,
            _: u32,
            _: &[u32],
        ) -> Result<Box<dyn Confined>, String> {
            Ok(Box::new(FakeConfined(self.0.clone(), 0)))
        }
        fn check_cgroup(&self, _: &std::os::fd::OwnedFd, _: u32) -> Result<(), String> {
            Ok(())
        }
        fn scope_supported(&self) -> Result<(), String> {
            Err("sem escopo".into())
        }
        fn scope(
            &self,
            _: &std::os::fd::OwnedFd,
            _: u32,
            _: u32,
            _: &[(u32, u32)],
        ) -> Result<Box<dyn crate::helper::Scoped>, String> {
            Err("sem escopo".into())
        }
    }

    /// Uma pasta de trabalho, um auxiliar falso e o `Files` ligado a ele.
    fn setup(deny_dir: bool) -> (tempfile::TempDir, PathBuf, Arc<Files>, Log) {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let w = base.join("w");
        std::fs::create_dir_all(w.join("privado")).unwrap();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let sock = base.join("h.sock");
        let server = Server::new(Box::new(FakeBackend(log.clone())));
        let l = crate::helper::listen(&sock).unwrap();
        std::thread::spawn(move || crate::helper::accept_loop(server, l));
        let c = Client::connect(&sock).unwrap();
        c.confine(&w, 0).unwrap();
        let mut p = policy(&w);
        let mut initial = DockerMarks::default();
        if deny_dir {
            p.deny = vec![
                sandmux_spec::files::FilePattern::parse(
                    &w.join("privado").display().to_string(),
                    None,
                )
                .unwrap(),
            ];
            initial.deny.push(w.join("privado"));
        }
        let f = Arc::new(Files::new(&w, "docker-t".into(), c, p, &initial));
        (d, w, f, log)
    }

    fn wait_mark(log: &Log, p: &Path, mark: bool) {
        let fim = Instant::now() + Duration::from_secs(5);
        while !log
            .lock()
            .unwrap()
            .iter()
            .any(|x| x.0 == p && x.2 == mark && x.1 & DENY != 0)
        {
            assert!(
                Instant::now() < fim,
                "{} não foi marcado: {:?}",
                p.display(),
                log.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn vigia_marca_os_arquivos_novos() {
        let (d, w, f, log) = setup(true);
        let g = f.clone();
        let t = std::thread::spawn(move || g.watch(false, POLL));
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(w.join(".env"), "x").unwrap();
        std::fs::write(w.join("a.txt"), "x").unwrap();
        std::fs::create_dir_all(w.join("privado/sub")).unwrap();
        std::fs::write(w.join("privado/sub/b"), "x").unwrap();
        // Renomeado para dentro da pasta, com nome de segredo.
        let fora = d.path().join("fora");
        std::fs::write(&fora, "x").unwrap();
        std::fs::rename(&fora, w.join(".env.local")).unwrap();
        wait_mark(&log, &w.join(".env"), true);
        wait_mark(&log, &w.join("privado/sub"), true);
        wait_mark(&log, &w.join("privado/sub/b"), true);
        wait_mark(&log, &w.join(".env.local"), true);
        assert!(!log.lock().unwrap().iter().any(|x| x.0 == w.join("a.txt")));
        f.stopping.store(true, Ordering::SeqCst);
        t.join().unwrap();
    }

    #[test]
    fn varredura_de_recuo() {
        let (_d, w, f, log) = setup(false);
        let g = f.clone();
        let t = std::thread::spawn(move || g.watch(true, Duration::from_millis(300)));
        std::fs::write(w.join("id_rsa"), "x").unwrap();
        wait_mark(&log, &w.join("id_rsa"), true);
        assert!(f.poll.load(Ordering::SeqCst));
        f.stopping.store(true, Ordering::SeqCst);
        t.join().unwrap();
    }

    #[test]
    fn politica_ao_vivo_manda_a_diferenca() {
        let (_d, w, f, log) = setup(false);
        std::fs::write(w.join("dados.csv"), "x").unwrap();
        std::fs::write(w.join(".env"), "x").unwrap();
        f.refresh(None).unwrap();
        wait_mark(&log, &w.join(".env"), true);
        let mut p = policy(&w);
        p.deny = vec![sandmux_spec::files::FilePattern::parse("*.csv", None).unwrap()];
        p.allow = vec![
            sandmux_spec::files::FilePattern::parse(&w.join(".env").display().to_string(), None)
                .unwrap(),
        ];
        f.refresh(Some(p)).unwrap();
        wait_mark(&log, &w.join("dados.csv"), true);
        wait_mark(&log, &w.join(".env"), false);
    }

    #[test]
    fn eventos_viram_linhas() {
        let (_d, w, f, _log) = setup(false);
        std::fs::write(w.join(".env"), "x").unwrap();
        f.refresh(None).unwrap();
        f.index_dirs(std::slice::from_ref(&w));
        let env_ino = ino_of(&w.join(".env")).unwrap();
        let w_ino = ino_of(&w).unwrap();
        let msg = serde_json::json!({
            "dropped": 3,
            "ev": [
                {"kind": "deny", "hook": "file_open", "bits": 1, "write": false, "ino": env_ino},
                {"kind": "create", "hook": "inode_create", "ino": w_ino, "name": "saida.txt"},
                {"kind": "create", "hook": "inode_create", "ino": w_ino, "name": "saida.txt"},
                {"kind": "create", "hook": "inode_create", "ino": 1, "name": "x"},
                {"kind": "create", "hook": "inode_create", "ino": w_ino, "name": "../fuga"}
            ]
        });
        let l = f.entries(&msg);
        assert_eq!(l.len(), 3, "{l:?}");
        assert!(l[0].blocked.as_deref().unwrap().contains('3'));
        assert_eq!(l[1].path, w.join(".env"));
        assert_eq!(
            (l[1].access.as_str(), l[1].blocked.as_deref()),
            ("r", Some(tr!("segredo", "secret")))
        );
        assert_eq!(
            (l[2].path.clone(), l[2].access.as_str()),
            (w.join("saida.txt"), "create")
        );
        assert!(l.iter().all(|x| x.tag == "docker-t"));
    }

    #[test]
    fn fixas_tiram_a_pasta() {
        let w = Path::new("/w");
        let m = DockerMarks {
            deny: vec![PathBuf::from("/w/.env"), PathBuf::from("/etc/a")],
            protect: vec![PathBuf::from("/w/.git/hooks")],
            gitdir: vec![PathBuf::from("/w/.git")],
            ..Default::default()
        };
        let f = fixed_marks(&m, w);
        assert_eq!(f.deny, vec![PathBuf::from("/etc/a")]);
        assert_eq!(f.protect.len(), 1);
        assert_eq!(f.gitdir.len(), 1);
    }

    #[test]
    fn aberturas_da_aba_entram_no_registro() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let w = base.join("w");
        std::fs::create_dir_all(&w).unwrap();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let sock = base.join("h.sock");
        let server = Server::new(Box::new(FakeBackend(log.clone())));
        let l = crate::helper::listen(&sock).unwrap();
        std::thread::spawn(move || crate::helper::accept_loop(server, l));
        let c = Client::connect(&sock).unwrap();
        c.confine_with(&w, 0, &[7]).unwrap();
        let p = policy(&w);
        let f = Files::with_mode(
            &w,
            "aba-t".into(),
            c,
            p.clone(),
            &DockerMarks::default(),
            true,
        )
        .with_view((
            vec![w.clone(), PathBuf::from("/home/u")],
            vec![PathBuf::from("/home/u/.cache")],
        ))
        .with_state(base.join("estado"));
        std::fs::write(w.join(".env"), "x").unwrap();
        f.refresh(None).unwrap();
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|x| x.0 == w.join(".env") && x.1 == SECRET && x.2)
        );
        let msg = serde_json::json!({"ev": [
            {"kind": "open", "write": false, "ino": 1, "mnt": 7, "path": w.join("a.txt")},
            {"kind": "open", "write": true, "ino": 2, "mnt": 7, "path": "/home/u/notas.md"},
            {"kind": "open", "write": false, "ino": 3, "mnt": 7, "path": "/home/u/.cache/x"},
            {"kind": "open", "write": false, "ino": 4, "mnt": 7, "path": "/tmp/fora"},
            {"kind": "open", "write": false, "ino": 5, "mnt": 7, "path": ""},
            {"kind": "exec", "pid": 9, "path": "/usr/bin/ls", "args": ["ls"]},
        ]});
        let got = |f: &Files| -> Vec<(PathBuf, String, bool)> {
            f.entries(&msg)
                .into_iter()
                .map(|e| (e.path, e.access, e.new))
                .collect()
        };
        assert_eq!(
            got(&f),
            vec![
                (w.join("a.txt"), "r".to_string(), true),
                (PathBuf::from("/home/u/notas.md"), "w".to_string(), true),
            ]
        );
        // Uma linha por caminho e acesso no lançamento, como o supervisor.
        assert!(got(&f).is_empty());
        // Num lançamento novo, os caminhos já são conhecidos.
        let c2 = Client::connect(&sock).unwrap();
        c2.confine_with(&w, 0, &[7]).unwrap();
        let f2 = Files::with_mode(&w, "aba-u".into(), c2, p, &DockerMarks::default(), true)
            .with_view((vec![w.clone(), PathBuf::from("/home/u")], vec![]))
            .with_state(base.join("estado"));
        assert_eq!(
            got(&f2),
            vec![
                (w.join("a.txt"), "r".to_string(), false),
                (PathBuf::from("/home/u/notas.md"), "w".to_string(), false),
                (PathBuf::from("/home/u/.cache/x"), "r".to_string(), true),
            ]
        );
        // O Docker não registra aberturas (os eventos nem chegam, mas a
        // conta é da aba); as execuções, sim (na aba, vêm pela observação).
        let (_d2, _w2, docker, _) = setup(false);
        let d: Vec<String> = docker.entries(&msg).into_iter().map(|e| e.access).collect();
        assert_eq!(d, vec!["exec".to_string()]);
    }

    #[test]
    fn negacao_de_segredo_na_aba() {
        let w = Path::new("/w");
        let p = policy(w);
        assert_eq!(
            reason(&p, w, &w.join(".env"), SECRET),
            tr!("segredo", "secret")
        );
        assert_eq!(
            reason(&p, w, &w.join("a.sql"), FDENY | PROTECT),
            "file-deny"
        );
    }
}
