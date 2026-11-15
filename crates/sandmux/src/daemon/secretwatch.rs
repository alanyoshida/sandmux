//! Vigia de chaves reais: acha as chaves gravadas pelos harnesses nos HOMEs
//! sintéticos, leva ao cofre e deixa os falsos no lugar, como o
//! `sandmux doctor --fix`, sem perguntar nada. Roda numa thread do daemon,
//! com inotify nas pastas dos arquivos de credencial e uma varredura
//! periódica de segurança.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use sandmux_policy::Layout;
use sandmux_proto::SessionState;
use sandmux_spec::trf;

use super::Shared;
use crate::secretdomains::{infer_domains_for, provider_of};

/// Espera de silêncio depois de uma gravação, para não pegar um arquivo pela
/// metade.
const QUIET: Duration = Duration::from_millis(500);

/// Chaves que esperam, num harness.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Waiting {
    /// O cofre (trancado ou ausente).
    pub vault: u32,
    /// Um domínio (`sandmux doctor --fix`).
    pub domain: u32,
}

/// Estado do vigia no daemon.
#[derive(Default)]
pub struct Watch {
    kick: Mutex<Option<mpsc::Sender<()>>>,
    waiting: Mutex<BTreeMap<String, Waiting>>,
}

impl Watch {
    /// Pede uma varredura agora (o cofre acabou de abrir).
    pub fn kick(&self) {
        if let Some(tx) = self.kick.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(());
        }
    }

    /// As chaves que esperam no harness.
    pub fn waiting(&self, harness: &str) -> Waiting {
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(harness)
            .copied()
            .unwrap_or_default()
    }
}

/// Intervalo da varredura periódica (`SANDMUX_SECRET_SCAN_SECS`, para os
/// testes; 60 segundos).
fn interval() -> Duration {
    let secs = std::env::var("SANDMUX_SECRET_SCAN_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(60);
    Duration::from_secs(secs)
}

pub fn start(shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel();
    *shared
        .secrets
        .kick
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(tx);
    let _ = std::thread::Builder::new()
        .name("chaves-reais".into())
        .spawn(move || run(&shared, &rx));
}

/// As pastas a observar: para cada pasta de credencial, ela e as pastas
/// acima dela até a pasta dos HOMEs sintéticos, as que existem. Assim a
/// criação de uma pasta nova também chega ao vigia.
fn targets(layout: &Layout) -> Vec<(PathBuf, &'static str)> {
    let mut out = BTreeMap::new();
    for &h in sandmux_import::SCANNED {
        let home = layout.home(h);
        let Some(top) = home.parent().map(Path::to_path_buf) else {
            continue;
        };
        let mut dirs = sandmux_import::watch_dirs(h, &home);
        dirs.push(home.clone());
        for d in dirs {
            let mut cur = Some(d.as_path());
            while let Some(c) = cur {
                if c.is_dir() && !c.is_symlink() {
                    out.entry(c.to_path_buf()).or_insert(h);
                }
                if c == top {
                    break;
                }
                cur = c.parent();
            }
        }
    }
    out.into_iter().collect()
}

fn run(shared: &Arc<Shared>, rx: &mpsc::Receiver<()>) {
    let layout = Layout::new(&shared.base.data_dir);
    // `SANDMUX_SECRET_INOTIFY=0` deixa só a varredura periódica (testes).
    let ino = if std::env::var("SANDMUX_SECRET_INOTIFY").as_deref() == Ok("0") {
        None
    } else {
        Inotify::init(InitFlags::IN_CLOEXEC | InitFlags::IN_NONBLOCK).ok()
    };
    let mut wds: HashMap<WatchDescriptor, &'static str> = HashMap::new();
    let refresh = |wds: &mut HashMap<WatchDescriptor, &'static str>| {
        let Some(ino) = &ino else { return };
        for (dir, h) in targets(&layout) {
            let flags = AddWatchFlags::IN_CLOSE_WRITE
                | AddWatchFlags::IN_MOVED_TO
                | AddWatchFlags::IN_CREATE;
            if let Ok(wd) = ino.add_watch(&dir, flags) {
                wds.insert(wd, h);
            }
        }
    };
    let every = interval();
    let mut next_full = Instant::now();
    let mut dirty: BTreeSet<&'static str> = BTreeSet::new();
    let mut last_event: Option<Instant> = None;
    loop {
        if rx.try_recv().is_ok() {
            while rx.try_recv().is_ok() {}
            next_full = Instant::now();
        }
        let now = Instant::now();
        if now >= next_full {
            refresh(&mut wds);
            for &h in sandmux_import::SCANNED {
                scan(shared, &layout, h);
            }
            dirty.clear();
            last_event = None;
            next_full = now + every;
        } else if let Some(t) = last_event
            && now >= t + QUIET
        {
            // Uma pasta nova pode ter aparecido: observa antes de varrer.
            refresh(&mut wds);
            for h in std::mem::take(&mut dirty) {
                scan(shared, &layout, h);
            }
            last_event = None;
        }
        match &ino {
            Some(ino) => {
                use std::os::fd::AsFd;
                let mut fds = [nix::poll::PollFd::new(
                    ino.as_fd(),
                    nix::poll::PollFlags::POLLIN,
                )];
                let _ = nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(200u16));
                if let Ok(evs) = ino.read_events() {
                    for e in evs {
                        if let Some(h) = wds.get(&e.wd) {
                            dirty.insert(h);
                            last_event = Some(Instant::now());
                        }
                    }
                }
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Varre o HOME sintético de um harness e move o que der.
fn scan(shared: &Arc<Shared>, layout: &Layout, h: &str) {
    let home = layout.home(h);
    let found = if home.is_dir() {
        sandmux_import::find_real_secrets(h, &home)
    } else {
        Vec::new()
    };
    let mut waiting = Waiting::default();
    let mut moved: u32 = 0;
    for r in &found {
        let n = r.found.len() as u32;
        let keys = r
            .found
            .iter()
            .map(|x| x.key.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let file = r.file.display().to_string();
        let mut entries = Vec::new();
        for x in &r.found {
            let provider = provider_of(&r.original, x.line);
            let name = match &provider {
                Some(p) => format!("{h}:{file}:{p}.{}", x.key),
                None => format!("{h}:{file}:{}", x.key),
            };
            let domains = infer_domains_for(
                &shared.base,
                h,
                &x.key,
                provider.as_deref(),
                Some(&r.original),
            );
            entries.push((name, x, domains));
        }
        if entries.iter().any(|(_, _, d)| d.is_empty()) {
            waiting.domain += n;
            continue;
        }
        let mut vault = shared.vault.lock().unwrap_or_else(|e| e.into_inner());
        if !vault.unlocked() {
            waiting.vault += n;
            continue;
        }
        // O harness pode ter regravado o arquivo depois da leitura: a próxima
        // varredura pega a versão nova.
        if std::fs::read_to_string(home.join(&r.file)).ok().as_deref() != Some(&r.original) {
            continue;
        }
        let mut ok = true;
        for (name, x, domains) in entries {
            if let Err(e) = vault.add(sandmux_vault::Secret {
                name: name.clone(),
                value: x.real.clone(),
                domains,
                fake: Some(x.fake.clone()),
                tools: Vec::new(),
                kind: String::new(),
            }) {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmuxd: chaves reais: {h} {file}: o cofre recusou {name} ({e}); o arquivo fica como estava",
                        "sandmuxd: real keys: {h} {file}: the vault refused {name} ({e}); the file stays as it was"
                    )
                );
                ok = false;
                break;
            }
        }
        drop(vault);
        if !ok {
            continue;
        }
        match sandmux_import::apply_fakes(h, &home, &layout.manifest(h), r) {
            Ok(()) => {
                moved += n;
                eprintln!(
                    "{}",
                    trf!(
                        "sandmuxd: chaves reais: {h} {file}: {n} levada(s) ao cofre ({keys}); o arquivo tem os falsos",
                        "sandmuxd: real keys: {h} {file}: {n} moved to the vault ({keys}); the file has the fakes"
                    )
                );
            }
            Err(e) => eprintln!(
                "{}",
                trf!(
                    "sandmuxd: chaves reais: {h} {file}: gravar os falsos: {e}",
                    "sandmuxd: real keys: {h} {file}: writing the fakes: {e}"
                )
            ),
        }
    }
    if moved > 0 {
        let s = shared.clone();
        std::thread::spawn(move || super::net::recompile_all(&s));
        let mut st = shared.lock();
        for s in st.sessions.values_mut() {
            if s.opts.harness == h && s.state == SessionState::Viva {
                s.keys_moved += moved;
            }
        }
        shared.changed.notify_all();
    }
    let before = shared
        .secrets
        .waiting
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(h.to_string(), waiting)
        .unwrap_or_default();
    if moved > 0 || (before != waiting && waiting != Waiting::default()) {
        shared.notify.send(super::notify::Event::Keys {
            harness: h.to_string(),
            note: sandmux_proto::msg::KeyNote {
                moved,
                waiting_vault: waiting.vault,
                no_domain: waiting.domain,
            },
        });
    }
    if before != waiting && waiting != Waiting::default() {
        eprintln!(
            "{}",
            trf!(
                "sandmuxd: chaves reais: {h}: {} esperando o cofre, {} esperando um domínio (`sandmux doctor --fix`)",
                "sandmuxd: real keys: {h}: {} waiting for the vault, {} waiting for a domain (`sandmux doctor --fix`)",
                waiting.vault,
                waiting.domain
            )
        );
    }
}
