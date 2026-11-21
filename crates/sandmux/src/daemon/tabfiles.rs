//! Uma aba no kernel, pelo auxiliar root: os arquivos (change 78) e o escopo
//! do sandbox (change 79).
//!
//! O lançador pede, antes de criar o comando. O daemon prende ao escopo do
//! systemd da aba os programas BPF LSM do confinamento: os segredos e o
//! `file-deny` são negados pelo arquivo (`ENOENT` e `EACCES`), e as aberturas
//! das pastas do registro chegam pela fila do kernel. Sem o auxiliar, o BPF
//! LSM ou o escopo próprio, os arquivos ficam com o supervisor de aberturas.
//!
//! Nos kernels com o Landlock abaixo do ABI 6, o escopo do sandbox (sockets
//! abstratos, sinais e `ioctl`) também vem do auxiliar: o lançador o
//! registra, e o daemon guarda os parâmetros e o user namespace. Quando o
//! auxiliar cai, o daemon congela a aba, registra de novo o que ela tinha no
//! auxiliar seguinte e descongela; sem isso em 30 s, a aba morre.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sandmux_proto::msg::TabScope;
use sandmux_spec::{tr, trf};

use super::Shared;
use super::dockerfiles::Files;
use crate::helper::Client;

/// O que a aba tem no auxiliar, guardado na sessão: solto (as conexões
/// fecham, e as vigias param) quando a aba é relançada ou termina.
pub struct TabKernel {
    /// Os arquivos pelo kernel.
    pub files: Option<Arc<Files>>,
    /// O escopo do sandbox, quando o daemon o registrou (depois de uma queda
    /// do auxiliar; antes, a conexão do lançador o segura).
    _scope: Option<Client>,
    /// A conexão vigiada: a queda dela começa a recuperação.
    watch: Arc<Client>,
    stopping: Arc<AtomicBool>,
}

impl Drop for TabKernel {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(f) = &self.files {
            f.stop();
        }
        self.watch.close();
    }
}

/// O que o daemon respondeu ao lançador.
pub struct Confined {
    pub id: u32,
    /// O que guardar na sessão (os arquivos pelo kernel, o escopo, ou os
    /// dois).
    pub tab: Option<TabKernel>,
    /// Os arquivos pelo kernel, ou o motivo de ficar com o supervisor.
    pub files: Result<(), String>,
    /// O daemon vigia o escopo.
    pub scope: bool,
}

/// A sessão viva cuja etiqueta atual é `tag`: o id, a pasta de trabalho
/// (canônica) e o arquivo da política ao vivo.
fn session_of(shared: &Shared, tag: &str) -> Option<(u32, PathBuf, PathBuf)> {
    let st = shared.lock();
    st.sessions
        .values()
        .find(|s| s.tags.last().is_some_and(|t| t == tag))
        .map(|s| {
            let w =
                std::fs::canonicalize(&s.opts.workdir).unwrap_or_else(|_| s.opts.workdir.clone());
            (s.id, w, crate::launch::file_policy_path(tag))
        })
}

/// A aba `tag` fica com o supervisor: o motivo no resumo da política.
pub fn fallback(shared: &Shared, tag: &str, reason: &str) {
    let mut st = shared.lock();
    if let Some(p) = st
        .sessions
        .values_mut()
        .find(|s| s.tags.last().is_some_and(|t| t == tag))
        .and_then(|s| s.policy.as_mut())
    {
        p.files = Some(format!("supervisor: {reason}"));
    }
}

/// O escopo próprio do processo `pid`: um `.scope` que não é o do daemon.
fn own_scope(pid: u32) -> Result<PathBuf, String> {
    let cg = sandmux_policy::limits::cgroup_of(pid).ok_or_else(|| {
        tr!(
            "o cgroup do lançador não foi achado",
            "the launcher's cgroup was not found"
        )
        .to_string()
    })?;
    let mine = sandmux_policy::limits::cgroup_of(std::process::id());
    if cg.extension().is_none_or(|e| e != "scope") || Some(&cg) == mine.as_ref() {
        return Err(tr!(
            "a aba não roda num escopo próprio do systemd de usuário (sem os tetos)",
            "the tab does not run in its own user systemd scope (no caps)"
        )
        .into());
    }
    Ok(cg)
}

/// O que é preciso para confinar (e reconfinar) uma aba.
struct Ctx {
    shared: Arc<Shared>,
    id: u32,
    tag: String,
    cg: PathBuf,
    workdir: PathBuf,
    policy_file: PathBuf,
    mounts: Vec<u32>,
    view: (Vec<PathBuf>, Vec<PathBuf>),
    /// Os arquivos estão pelo kernel (e voltam numa recuperação).
    files: bool,
    /// O escopo do sandbox e o user namespace dele.
    scope: Option<(TabScope, OwnedFd)>,
}

/// Quanto a aba espera congelada pelo auxiliar de volta.
const REATTACH: std::time::Duration = std::time::Duration::from_secs(30);

/// O pedido de uma aba.
pub struct Request {
    pub tag: String,
    pub pid: u32,
    pub mounts: Vec<u32>,
    pub view: (Vec<PathBuf>, Vec<PathBuf>),
    pub scope: Option<(TabScope, OwnedFd)>,
}

/// Confina os arquivos da aba e guarda o escopo dela. Sem sessão ou sem o
/// escopo próprio, nada fica com o daemon.
pub fn confine(shared: &Arc<Shared>, req: Request) -> Result<Confined, String> {
    let (id, workdir, policy_file) = session_of(shared, &req.tag).ok_or_else(|| {
        trf!(
            "nenhuma sessão viva com a etiqueta {}",
            "no live session with tag {}",
            req.tag
        )
    })?;
    let cg = own_scope(req.pid)?;
    let mut ctx = Ctx {
        shared: shared.clone(),
        id,
        tag: req.tag,
        cg,
        workdir,
        policy_file,
        mounts: req.mounts,
        view: req.view,
        files: false,
        scope: req.scope,
    };
    let files = attach_files(&ctx);
    ctx.files = files.is_ok();
    let reason = files.as_ref().err().cloned();
    let files = files.ok();
    if files.is_none() && ctx.scope.is_none() {
        return Ok(Confined {
            id,
            tab: None,
            files: Err(reason.unwrap_or_default()),
            scope: false,
        });
    }
    let watch = match watch_client() {
        Ok(w) => w,
        Err(e) => {
            // Sem a vigia, nada fica com o daemon: os arquivos voltam ao
            // supervisor, e o lançador vigia o escopo.
            if let Some(f) = &files {
                f.stop();
            }
            return Ok(Confined {
                id,
                tab: None,
                files: Err(reason.unwrap_or(e)),
                scope: false,
            });
        }
    };
    let scope = ctx.scope.is_some();
    let tab = TabKernel {
        files,
        _scope: None,
        watch: Arc::new(watch),
        stopping: Arc::new(AtomicBool::new(false)),
    };
    let ctx = Arc::new(ctx);
    spawn_watch(&ctx, &tab)?;
    Ok(Confined {
        id,
        tab: Some(tab),
        files: match reason {
            None => Ok(()),
            Some(r) => Err(r),
        },
        scope,
    })
}

/// Uma conexão com o auxiliar só para saber da queda dele.
fn watch_client() -> Result<Client, String> {
    let path = crate::helper::socket_path();
    let c = Client::connect(&path).map_err(|e| {
        trf!(
            "o auxiliar root não responde em {}: {e}",
            "the root helper does not answer at {}: {e}",
            path.display()
        )
    })?;
    c.status()?;
    Ok(c)
}

/// A thread que espera a vigia cair e recupera a aba.
fn spawn_watch(ctx: &Arc<Ctx>, tab: &TabKernel) -> Result<(), String> {
    let (ctx, watch, stopping, files) = (
        ctx.clone(),
        tab.watch.clone(),
        tab.stopping.clone(),
        tab.files.clone(),
    );
    std::thread::Builder::new()
        .name("aba-queda".into())
        .spawn(move || {
            watch.wait_closed();
            if !stopping.load(Ordering::SeqCst) {
                recover(&ctx, &watch, files.as_ref());
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Liga o confinamento dos arquivos no auxiliar, com as marcas da política
/// atual, e a thread dos eventos e a vigia da pasta.
fn attach_files(ctx: &Ctx) -> Result<Arc<Files>, String> {
    use crate::helper::{MarkKind, socket_path};
    if ctx.mounts.is_empty() {
        return Err(tr!(
            "sem as montagens das pastas do registro",
            "no mounts for the logged folders"
        )
        .into());
    }
    let policy: sandmux_spec::files::FilePolicy = std::fs::read(&ctx.policy_file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| {
            tr!(
                "sem a política de arquivos da aba",
                "no file policy for the tab"
            )
            .to_string()
        })?;
    super::dockerfiles::tab_policy_fits(&policy, &ctx.workdir)?;
    if !sandmux_ebpf::lsm_active()? {
        return Err(tr!(
            "o BPF LSM não está ativo no kernel (falta `bpf` no parâmetro lsm=)",
            "the BPF LSM is not active in the kernel (`bpf` missing from the lsm= parameter)"
        )
        .into());
    }
    let path = socket_path();
    let c = Client::connect(&path).map_err(|e| {
        trf!(
            "o auxiliar root não responde em {}: {e}",
            "the root helper does not answer at {}: {e}",
            path.display()
        )
    })?;
    let st = c.status()?;
    if !st.lsm {
        return Err(st.reason.unwrap_or_else(|| {
            tr!(
                "o auxiliar diz que o BPF LSM não está ativo",
                "the helper says the BPF LSM is not active"
            )
            .into()
        }));
    }
    if !st.tab {
        return Err(trf!(
            "o auxiliar root ({}) não confina os arquivos de uma aba; atualize o auxiliar",
            "the root helper ({}) does not confine a tab's files; update the helper",
            st.version
        ));
    }
    c.confine_with(
        &ctx.cg,
        crate::helper::kernel_dev(&ctx.workdir),
        &ctx.mounts,
    )
    .map_err(|e| {
        trf!(
            "o auxiliar recusou o confinamento: {e}",
            "the helper refused the confinement: {e}"
        )
    })?;
    // As marcas fixas (o `.git`) agora; as da política pelo `refresh`.
    let fixed = sandmux_policy::git_marks(&ctx.workdir);
    for (kind, paths) in [
        (MarkKind::Protect, &fixed.protect),
        (MarkKind::GitDir, &fixed.gitdir),
    ] {
        c.mark(kind, paths)?;
    }
    let files = Arc::new(
        Files::with_mode(&ctx.workdir, ctx.tag.clone(), c, policy, &fixed, true)
            .with_view(ctx.view.clone()),
    );
    // Sem as marcas da política, os arquivos ficam com o supervisor: soltar
    // a conexão tira o confinamento.
    if let Err(e) = files.refresh(None) {
        files.stop();
        return Err(trf!(
            "as marcas da política não foram aplicadas: {e}",
            "the policy marks were not applied: {e}"
        ));
    }
    let f = files.clone();
    let spawned = std::thread::Builder::new()
        .name("aba-arquivos".into())
        .spawn(move || f.events_loop(|| {}))
        .and_then(|_| {
            let f = files.clone();
            std::thread::Builder::new()
                .name("aba-pasta".into())
                .spawn(move || f.watch_loop())
        });
    if let Err(e) = spawned {
        files.stop();
        return Err(e.to_string());
    }
    Ok(files)
}

/// Registra o escopo do sandbox numa conexão própria.
fn attach_scope(sc: &TabScope, ns: &OwnedFd) -> Result<Client, String> {
    let c = watch_client()?;
    c.scope_with(ns, sc.ioctl, sc.init_pid, &sc.devs)?;
    Ok(c)
}

/// Tudo o que a aba tinha, num auxiliar novo.
fn reattach(ctx: &Ctx) -> Result<TabKernel, String> {
    let files = if ctx.files {
        Some(attach_files(ctx)?)
    } else {
        None
    };
    // Daqui em diante, um erro solta o que já voltou (o `Drop`).
    let mut tab = TabKernel {
        files,
        _scope: None,
        watch: Arc::new(watch_client()?),
        stopping: Arc::new(AtomicBool::new(false)),
    };
    if let Some((sc, ns)) = &ctx.scope {
        tab._scope = Some(attach_scope(sc, ns)?);
    }
    Ok(tab)
}

/// A conexão com o auxiliar caiu: sem as regras, nada roda na aba. O escopo
/// fica congelado até o auxiliar voltar (o systemd o sobe na conexão
/// seguinte) e o que a aba tinha ser refeito; sem isso em [`REATTACH`],
/// tudo no escopo morre.
fn recover(ctx: &Arc<Ctx>, old: &Arc<Client>, old_files: Option<&Arc<Files>>) {
    let id = ctx.id;
    // Congela antes de soltar a conexão antiga: na saída limpa do auxiliar,
    // as regras ficam até ela fechar.
    let frozen = freeze(&ctx.cg, true);
    if let Some(f) = old_files {
        f.stop();
    }
    if !frozen {
        eprintln!(
            "{}",
            trf!(
                "sandmuxd: a conexão com o auxiliar root caiu e a aba {id} não congelou; ela para",
                "sandmuxd: the connection to the root helper dropped and tab {id} did not freeze; it stops"
            )
        );
        kill_scope(&ctx.cg);
        return;
    }
    eprintln!(
        "{}",
        trf!(
            "sandmuxd: a conexão com o auxiliar root caiu; a aba {id} fica congelada até o confinamento voltar",
            "sandmuxd: the connection to the root helper dropped; tab {id} stays frozen until the confinement is back"
        )
    );
    let deadline = std::time::Instant::now() + REATTACH;
    loop {
        match reattach(ctx) {
            Ok(tab) => {
                let mut st = ctx.shared.lock();
                let same = st.sessions.get_mut(&id).filter(|s| {
                    s.tab_kernel
                        .as_ref()
                        .is_some_and(|t| Arc::ptr_eq(&t.watch, old))
                });
                match same {
                    Some(s) => {
                        if spawn_watch(ctx, &tab).is_err() {
                            drop(st);
                            drop(tab);
                            kill_scope(&ctx.cg);
                            return;
                        }
                        s.tab_kernel = Some(tab);
                        drop(st);
                        freeze(&ctx.cg, false);
                        eprintln!(
                            "{}",
                            trf!(
                                "sandmuxd: a aba {id} foi confinada de novo",
                                "sandmuxd: tab {id} is confined again"
                            )
                        );
                    }
                    // A aba terminou ou foi relançada nesse meio tempo: o
                    // lançamento novo no mesmo escopo tem o confinamento
                    // dele; o que sobrou de um escopo antigo morre.
                    None => {
                        let here = st
                            .sessions
                            .get(&id)
                            .and_then(|s| s.pid())
                            .and_then(|p| sandmux_policy::limits::cgroup_of(p as u32))
                            .is_some_and(|c| c == ctx.cg);
                        drop(st);
                        drop(tab);
                        if here {
                            freeze(&ctx.cg, false);
                        } else {
                            kill_scope(&ctx.cg);
                        }
                    }
                }
                return;
            }
            Err(e) if std::time::Instant::now() >= deadline => {
                eprintln!(
                    "{}",
                    trf!(
                        "sandmuxd: a aba {id} não foi confinada de novo ({e}); ela para",
                        "sandmuxd: tab {id} was not confined again ({e}); it stops"
                    )
                );
                kill_scope(&ctx.cg);
                return;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(300)),
        }
    }
}

/// Congela (ou descongela) tudo no escopo (`cgroup.freeze`).
fn freeze(cg: &Path, on: bool) -> bool {
    std::fs::write(cg.join("cgroup.freeze"), if on { "1" } else { "0" }).is_ok()
}

/// Mata todos os processos do escopo (`cgroup.kill`); `cg` é o caminho
/// absoluto do cgroup (`/sys/fs/cgroup/...`, como `cgroup_of` devolve).
fn kill_scope(cg: &Path) {
    let _ = std::fs::write(cg.join("cgroup.kill"), "1");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mata_pelo_caminho_absoluto_do_cgroup() {
        let d = tempfile::tempdir().unwrap();
        kill_scope(d.path());
        assert_eq!(
            std::fs::read_to_string(d.path().join("cgroup.kill")).unwrap(),
            "1"
        );
    }

    #[test]
    fn congela_e_descongela_pelo_arquivo_do_cgroup() {
        let d = tempfile::tempdir().unwrap();
        assert!(freeze(d.path(), true));
        assert_eq!(
            std::fs::read_to_string(d.path().join("cgroup.freeze")).unwrap(),
            "1"
        );
        assert!(freeze(d.path(), false));
        assert_eq!(
            std::fs::read_to_string(d.path().join("cgroup.freeze")).unwrap(),
            "0"
        );
        assert!(!freeze(&d.path().join("nao-existe"), true));
    }
}
