//! Notificações do desktop, pelo `org.freedesktop.Notifications` do
//! barramento de sessão: bloqueios de rede, chaves reais, abas encerradas e
//! abas esperando o cofre. Agrupadas por (aba, tipo) em 60 segundos, e
//! suprimidas para a aba que uma TUI em foco está mostrando.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use sandmux_proto::SessionState;
use sandmux_spec::{tr, trf};

use super::Shared;

/// Janela do agrupamento.
const GROUP: Duration = Duration::from_secs(60);

/// Um evento a avisar.
pub enum Event {
    Block {
        tag: String,
        domain: String,
        reason: String,
    },
    /// Chaves reais de um harness (movidas, esperando o cofre, esperando um
    /// domínio).
    Keys {
        harness: String,
        note: sandmux_proto::msg::KeyNote,
    },
    Ended {
        session: u32,
        code: i32,
    },
    /// Confere se há abas esperando o cofre.
    WaitingVault,
    /// O real de uma identidade de cloud falta (a primeira vez).
    Cloud {
        name: String,
        why: String,
    },
}

#[derive(Clone, Copy)]
struct Focus {
    focused: bool,
    active: Option<u32>,
}

/// Estado das notificações no daemon.
#[derive(Default)]
pub struct Notifier {
    tx: Mutex<Option<mpsc::Sender<Event>>>,
    focus: Mutex<HashMap<i32, Focus>>,
}

impl Notifier {
    pub fn send(&self, e: Event) {
        if let Some(tx) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(e);
        }
    }

    /// O foco de uma TUI, pelo PID do par do socket.
    pub fn set_focus(&self, pid: i32, focused: bool, active: Option<u32>) {
        self.focus
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pid, Focus { focused, active });
    }

    /// Uma TUI viva e em foco mostra uma destas abas.
    fn suppressed(&self, sessions: &[u32]) -> bool {
        let mut f = self.focus.lock().unwrap_or_else(|e| e.into_inner());
        f.retain(|pid, _| std::path::Path::new(&format!("/proc/{pid}")).exists());
        f.values()
            .any(|x| x.focused && x.active.is_some_and(|a| sessions.contains(&a)))
    }
}

pub fn start(shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel();
    *shared.notify.tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    let _ = std::thread::Builder::new()
        .name("notificacoes".into())
        .spawn(move || run(&shared, &rx));
}

/// Uma notificação em andamento: o id dado pelo serviço, a contagem e os
/// itens (domínios), para a próxima do grupo a substituir.
struct Group {
    id: u32,
    count: u32,
    items: Vec<String>,
    last: Instant,
}

/// Conexão com o barramento de sessão, refeita depois de uma falha.
struct Bus {
    rt: tokio::runtime::Runtime,
    conn: Option<zbus::Connection>,
    warned: bool,
}

impl Bus {
    fn new() -> Option<Bus> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        Some(Bus {
            rt,
            conn: None,
            warned: false,
        })
    }

    /// Manda e devolve o id da notificação.
    fn notify(&mut self, replaces: u32, summary: &str, body: &str) -> Option<u32> {
        let conn = self.conn.take();
        let r: zbus::Result<(zbus::Connection, u32)> = self.rt.block_on(async {
            let conn = match conn {
                Some(c) => c,
                None => {
                    let b = match std::env::var("SANDMUX_NOTIFY_BUS") {
                        Ok(a) => zbus::connection::Builder::address(a.as_str())?,
                        Err(_) => zbus::connection::Builder::session()?,
                    };
                    b.build().await?
                }
            };
            let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
            let reply = conn
                .call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "Notify",
                    &(
                        "sandmux",
                        replaces,
                        "dialog-warning",
                        summary,
                        body,
                        Vec::<&str>::new(),
                        hints,
                        -1i32,
                    ),
                )
                .await?;
            let id: u32 = reply.body().deserialize()?;
            Ok((conn, id))
        });
        match r {
            Ok((conn, id)) => {
                self.conn = Some(conn);
                self.warned = false;
                Some(id)
            }
            Err(e) => {
                if !self.warned {
                    eprintln!(
                        "{}",
                        trf!(
                            "sandmuxd: notificação do desktop não mandada: {e}",
                            "sandmuxd: desktop notification not sent: {e}"
                        )
                    );
                    self.warned = true;
                }
                None
            }
        }
    }
}

fn run(shared: &Arc<Shared>, rx: &mpsc::Receiver<Event>) {
    let Some(mut bus) = Bus::new() else { return };
    let mut follow = sandmux_net::log::Follow::from_end(&super::net::log_dir());
    let mut groups: HashMap<String, Group> = HashMap::new();
    loop {
        let mut events = Vec::new();
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(e) => events.push(e),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        for e in follow.read_new() {
            if let Some(reason) = e.blocked {
                events.push(Event::Block {
                    tag: e.tag,
                    domain: e.domain,
                    reason,
                });
            }
        }
        if events.is_empty() {
            continue;
        }
        let path = shared.base.config_dir.join(sandmux_policy::config::FILE);
        if !sandmux_policy::config::notify_enabled(&path) {
            continue;
        }
        for e in events {
            if let Some((key, summary, item, body)) = describe(shared, e) {
                show(&mut bus, &mut groups, key, &summary, item, body);
            }
        }
        groups.retain(|_, g| g.last.elapsed() < GROUP);
    }
}

/// Texto do corpo de um grupo: o primeiro evento, ou a contagem com os
/// itens.
type Body = Box<dyn Fn(u32, &[String]) -> String>;

/// A chave do grupo, o título, o item (para a lista) e o corpo; `None`
/// quando não há o que avisar.
fn describe(shared: &Arc<Shared>, e: Event) -> Option<(String, String, Option<String>, Body)> {
    let n = &shared.notify;
    match e {
        Event::Block {
            tag,
            domain,
            reason,
        } => {
            let (id, name) = {
                let st = shared.lock();
                st.sessions
                    .values()
                    .find(|s| s.tags.contains(&tag))
                    .map(|s| (s.id, s.name.clone()))?
            };
            if n.suppressed(&[id]) {
                return None;
            }
            let reason = sandmux_spec::net::reason_text(&reason);
            let first = domain.clone();
            Some((
                format!("bloqueio:{id}"),
                trf!("sandmux · {name}", "sandmux · {name}"),
                Some(domain),
                Box::new(move |count, items| {
                    if count == 1 {
                        trf!(
                            "Bloqueado: {first} ({reason})",
                            "Blocked: {first} ({reason})"
                        )
                    } else {
                        trf!("{count} bloqueios: {}", "{count} blocks: {}", list(items))
                    }
                }),
            ))
        }
        Event::Keys { harness, note } => {
            let ids: Vec<u32> = {
                let st = shared.lock();
                st.sessions
                    .values()
                    .filter(|s| s.opts.harness == harness)
                    .map(|s| s.id)
                    .collect()
            };
            if !note.any() || n.suppressed(&ids) {
                return None;
            }
            let text = note.to_string();
            Some((
                format!("chaves:{harness}"),
                trf!(
                    "sandmux · chaves reais do {harness}",
                    "sandmux · real keys of {harness}"
                ),
                None,
                Box::new(move |_, _| text.clone()),
            ))
        }
        Event::Cloud { name, why } => Some((
            format!("cloud:{name}"),
            trf!("sandmux · {name}", "sandmux · {name}"),
            None,
            Box::new(move |_, _| why.clone()),
        )),
        Event::Ended { session, code } => {
            let name = shared.lock().sessions.get(&session)?.name.clone();
            if n.suppressed(&[session]) {
                return None;
            }
            Some((
                format!("fim:{session}"),
                trf!("sandmux · {name}", "sandmux · {name}"),
                None,
                Box::new(move |_, _| {
                    trf!(
                        "O harness terminou (código {code}). Ctrl+b r reinicia a aba.",
                        "The harness ended (code {code}). Ctrl+b r restarts the tab."
                    )
                }),
            ))
        }
        Event::WaitingVault => {
            let ids: Vec<u32> = {
                let st = shared.lock();
                st.sessions
                    .values()
                    .filter(|s| s.state == SessionState::AguardandoCofre)
                    .map(|s| s.id)
                    .collect()
            };
            if ids.is_empty() || n.suppressed(&ids) {
                return None;
            }
            let k = ids.len();
            Some((
                "cofre".into(),
                tr!("sandmux · cofre trancado", "sandmux · vault locked").into(),
                None,
                Box::new(move |_, _| {
                    trf!(
                        "{k} aba(s) esperando o cofre: abra a TUI ou rode `sandmux vault unlock`.",
                        "{k} tab(s) waiting for the vault: open the TUI or run `sandmux vault unlock`."
                    )
                }),
            ))
        }
    }
}

/// Até três itens, com "e mais N".
fn list(items: &[String]) -> String {
    let shown = items.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > 3 {
        trf!("{shown} e mais {}", "{shown} and {} more", items.len() - 3)
    } else {
        shown
    }
}

fn show(
    bus: &mut Bus,
    groups: &mut HashMap<String, Group>,
    key: String,
    summary: &str,
    item: Option<String>,
    body: Body,
) {
    let g = groups
        .entry(key)
        .and_modify(|g| {
            if g.last.elapsed() >= GROUP {
                g.count = 0;
                g.items.clear();
                g.id = 0;
            }
        })
        .or_insert(Group {
            id: 0,
            count: 0,
            items: Vec::new(),
            last: Instant::now(),
        });
    g.count += 1;
    g.last = Instant::now();
    if let Some(i) = item
        && !g.items.contains(&i)
    {
        g.items.push(i);
    }
    let text = body(g.count, &g.items);
    if let Some(id) = bus.notify(g.id, summary, &text) {
        g.id = id;
    }
}
