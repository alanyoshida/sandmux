//! Os arquivos de uma aba confinados pelo kernel: o lançador pede, antes de
//! liberar o comando, e o daemon prende ao escopo da aba os programas BPF LSM
//! do confinamento (pelo auxiliar root). Os segredos e o `file-deny` são
//! negados pelo arquivo (`ENOENT` e `EACCES`), e as aberturas das pastas do
//! registro chegam pela fila do kernel. Sem o auxiliar, o BPF LSM ou o
//! escopo próprio, a aba fica com o supervisor de aberturas.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sandmux_spec::{tr, trf};

use super::Shared;
use super::dockerfiles::Files;

/// O confinamento de uma aba, guardado na sessão: solto (a conexão com o
/// auxiliar fecha, e a vigia para) quando a aba é relançada ou termina.
pub struct TabFiles {
    pub files: Arc<Files>,
}

impl Drop for TabFiles {
    fn drop(&mut self) {
        self.files.stop();
    }
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
}

/// Quanto a aba espera congelada pelo auxiliar de volta.
const REATTACH: std::time::Duration = std::time::Duration::from_secs(10);

/// Confina os arquivos da aba `tag`, cujo lançador é o processo `pid`.
/// `Err` com o motivo para ficar com o supervisor.
pub fn confine(
    shared: &Arc<Shared>,
    tag: &str,
    pid: u32,
    mounts: Vec<u32>,
    view: (Vec<PathBuf>, Vec<PathBuf>),
) -> Result<(u32, TabFiles), String> {
    let (id, workdir, policy_file) = session_of(shared, tag).ok_or_else(|| {
        trf!(
            "nenhuma sessão viva com a etiqueta {tag}",
            "no live session with tag {tag}"
        )
    })?;
    let cg = own_scope(pid)?;
    let ctx = Arc::new(Ctx {
        shared: shared.clone(),
        id,
        tag: tag.to_string(),
        cg,
        workdir,
        policy_file,
        mounts,
        view,
    });
    Ok((id, attach(&ctx)?))
}

/// Liga o confinamento do escopo da aba no auxiliar, com as marcas da
/// política atual, e as threads dos eventos e da vigia.
fn attach(ctx: &Arc<Ctx>) -> Result<TabFiles, String> {
    use crate::helper::{Client, MarkKind, socket_path};
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
    // Sem as marcas da política, a aba fica com o supervisor: soltar o
    // `TabFiles` fecha a conexão, e o auxiliar tira o confinamento.
    let tab = TabFiles {
        files: files.clone(),
    };
    files.refresh(None).map_err(|e| {
        trf!(
            "as marcas da política não foram aplicadas: {e}",
            "the policy marks were not applied: {e}"
        )
    })?;
    let f = files.clone();
    std::thread::Builder::new()
        .name("aba-arquivos".into())
        .spawn(move || f.events_loop(|| {}))
        .map_err(|e| e.to_string())?;
    // A queda da conexão pela própria thread: a fila de eventos de uma aba
    // pode ter muitas aberturas por gravar, e a aba não espera por elas
    // sem as marcas.
    let (f, ctx2) = (files.clone(), ctx.clone());
    std::thread::Builder::new()
        .name("aba-queda".into())
        .spawn(move || {
            f.wait_closed();
            if !f.stopping.load(std::sync::atomic::Ordering::SeqCst) {
                reattach(&ctx2, &f);
            }
        })
        .map_err(|e| e.to_string())?;
    let f = files.clone();
    std::thread::Builder::new()
        .name("aba-pasta".into())
        .spawn(move || f.watch_loop())
        .map_err(|e| e.to_string())?;
    Ok(tab)
}

/// A conexão com o auxiliar caiu: sem as marcas, nada roda na aba. O escopo
/// fica congelado até o auxiliar voltar (o systemd o sobe na conexão
/// seguinte) e o confinamento ser refeito, com a política atual; sem isso
/// em [`REATTACH`], tudo no escopo morre.
fn reattach(ctx: &Arc<Ctx>, old: &Arc<Files>) {
    let id = ctx.id;
    // Congela antes de soltar a conexão antiga: na saída limpa do auxiliar,
    // as regras ficam até ela fechar.
    let frozen = freeze(&ctx.cg, true);
    // A vigia do confinamento que caiu para.
    old.stop();
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
        match attach(ctx) {
            Ok(tab) => {
                let mut st = ctx.shared.lock();
                let same = st.sessions.get_mut(&id).filter(|s| {
                    s.tab_files
                        .as_ref()
                        .is_some_and(|t| Arc::ptr_eq(&t.files, old))
                });
                match same {
                    Some(s) => {
                        s.tab_files = Some(tab);
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
}
