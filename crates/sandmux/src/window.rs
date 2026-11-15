//! Janelas e áudio do desktop para o sandbox (`window`).
//!
//! No modo restrito, o sandmux pede ao compositor um socket próprio da aba
//! pelo protocolo `wp_security_context_v1`, para o compositor saber que os
//! clientes são de um sandbox, e põe na frente dele o repassador
//! ([`crate::wlproxy`]), que esconde os protocolos privilegiados. O sandbox
//! recebe o socket do repassador. No modo direto, entra o socket comum do
//! compositor. Em ambos, os sockets do PipeWire e do PulseAudio vêm junto.
//! Só nas abas do daemon, que mantém o repassador vivo.

use std::collections::BTreeMap;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use sandmux_policy::compose::WindowMounts;
use sandmux_spec::window::WindowMode;
use sandmux_spec::{tr, trf};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::security_context::v1::client::{
    wp_security_context_manager_v1::WpSecurityContextManagerV1,
    wp_security_context_v1::WpSecurityContextV1,
};

/// Motor e app_id informados ao compositor.
pub const ENGINE: &str = "sandmux";

/// Socket da aba registrado no compositor. Soltá-lo fecha a ponta que o
/// compositor observa (ele para de aceitar conexões novas) e apaga o arquivo.
#[derive(Debug)]
pub struct ContextSocket {
    pub path: PathBuf,
    _close: OwnedFd,
}

impl Drop for ContextSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// O que mantém a janela restrita de uma aba: o repassador (solto antes) e
/// o socket do security-context.
#[derive(Debug)]
pub struct WindowSocket {
    pub proxy: crate::wlproxy::Proxy,
    pub context: ContextSocket,
}

/// O que a opção `window` leva a um lançamento.
#[derive(Debug)]
pub struct Window {
    pub mounts: WindowMounts,
    pub socket: Option<WindowSocket>,
}

/// O aviso mostrado antes de lançar com a janela.
pub fn warning(mode: WindowMode) -> Option<String> {
    let base = tr!(
        "janela ligada: o agente pode abrir janelas no seu desktop, tocar som, gravar o microfone e ouvir os outros apps",
        "window on: the agent can open windows on your desktop, play sound, record the microphone and listen to other apps"
    );
    match mode {
        WindowMode::Off => None,
        WindowMode::On => Some(base.to_string()),
        WindowMode::Direct => Some(trf!(
            "{base}; no modo direto, também pode ler o clipboard e, dependendo do compositor, capturar a tela",
            "{base}; in direct mode, it can also read the clipboard and, depending on the compositor, capture the screen"
        )),
    }
}

/// Pasta de runtime do usuário no host: a do pedido, ou a deste processo.
fn host_runtime(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    env.get("XDG_RUNTIME_DIR")
        .cloned()
        .or_else(|| std::env::var("XDG_RUNTIME_DIR").ok())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// O socket do compositor do usuário, pelo `WAYLAND_DISPLAY` do pedido (ou
/// deste processo).
pub fn compositor_socket(env: &BTreeMap<String, String>) -> Result<PathBuf> {
    let display = env
        .get("WAYLAND_DISPLAY")
        .cloned()
        .or_else(|| std::env::var("WAYLAND_DISPLAY").ok())
        .filter(|d| !d.is_empty())
        .ok_or_else(|| {
            anyhow!(tr!(
                "a janela precisa de uma sessão Wayland, e WAYLAND_DISPLAY não está definida (o X11 não é suportado)",
                "the window needs a Wayland session, and WAYLAND_DISPLAY is not set (X11 is not supported)"
            ))
        })?;
    let path = if Path::new(&display).is_absolute() {
        PathBuf::from(&display)
    } else {
        host_runtime(env)
            .ok_or_else(|| {
                anyhow!(tr!(
                    "a janela precisa de XDG_RUNTIME_DIR",
                    "the window needs XDG_RUNTIME_DIR"
                ))
            })?
            .join(&display)
    };
    let is_socket = std::fs::metadata(&path)
        .map(|m| m.file_type().is_socket())
        .unwrap_or(false);
    if !is_socket {
        bail!(trf!(
            "o socket do compositor não existe: {}",
            "the compositor socket does not exist: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn socket_at(p: PathBuf) -> Option<PathBuf> {
    std::fs::metadata(&p)
        .ok()
        .filter(|m| m.file_type().is_socket())
        .map(|_| p)
}

/// Prepara a janela de um lançamento. `name` identifica a aba para o
/// compositor e nomeia o socket.
pub fn open(
    mode: WindowMode,
    env: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<Window>> {
    if !mode.is_on() {
        return Ok(None);
    }
    let compositor = compositor_socket(env)?;
    let (wayland, socket) = match mode {
        WindowMode::Direct => (compositor, None),
        _ => {
            let dir = crate::paths::runtime_dir().join("window");
            private_dir(&dir)?;
            let context = security_context(&compositor, &dir.join(format!("{name}.ctx")), name)?;
            let proxy =
                crate::wlproxy::Proxy::start(&dir.join(format!("{name}.sock")), &context.path)
                    .map_err(|e| {
                        anyhow!(trf!("repassador do Wayland: {e}", "Wayland relay: {e}"))
                    })?;
            (proxy.path.clone(), Some(WindowSocket { proxy, context }))
        }
    };
    let rt = host_runtime(env);
    Ok(Some(Window {
        mounts: WindowMounts {
            wayland,
            pipewire: rt.as_ref().and_then(|r| socket_at(r.join("pipewire-0"))),
            pulse: rt.as_ref().and_then(|r| socket_at(r.join("pulse/native"))),
        },
        socket,
    }))
}

fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = dir.parent() {
        crate::paths::ensure_runtime_dir(parent)?;
    }
    match std::fs::create_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(anyhow!("{}: {e}", dir.display())),
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpSecurityContextManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpSecurityContextManagerV1,
        _: <WpSecurityContextManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpSecurityContextV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpSecurityContextV1,
        _: <WpSecurityContextV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Registra no compositor um socket de escuta em `path`, com o motor
/// `sandmux` e o identificador `instance`.
pub fn security_context(compositor: &Path, path: &Path, instance: &str) -> Result<ContextSocket> {
    let fail = |e: &dyn std::fmt::Display| {
        anyhow!(trf!("compositor Wayland: {e}", "Wayland compositor: {e}"))
    };
    let stream = UnixStream::connect(compositor).map_err(|e| fail(&e))?;
    let conn = Connection::from_socket(stream).map_err(|e| fail(&e))?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).map_err(|e| fail(&e))?;
    let qh = queue.handle();
    let manager: WpSecurityContextManagerV1 = globals.bind(&qh, 1..=1, ()).map_err(|_| {
        anyhow!(tr!(
            "o compositor não tem o protocolo wp_security_context_v1, que isola as janelas do sandbox; use `window = \"direct\"` (ou --window-direct) para o socket comum, que deixa o agente ler o clipboard e, dependendo do compositor, capturar a tela",
            "the compositor lacks the wp_security_context_v1 protocol, which isolates the sandbox windows; use `window = \"direct\"` (or --window-direct) for the plain socket, which lets the agent read the clipboard and, depending on the compositor, capture the screen"
        ))
    })?;
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).map_err(|e| anyhow!("{}: {e}", path.display()))?;
    let (close_r, close_w) = nix::unistd::pipe().map_err(|e| anyhow!("pipe: {e}"))?;
    // A ponta que fica conosco não pode vazar para os filhos.
    nix::fcntl::fcntl(
        &close_w,
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
    )
    .map_err(|e| anyhow!("fcntl: {e}"))?;
    let ctx = manager.create_listener(listener.as_fd(), close_r.as_fd(), &qh, ());
    ctx.set_sandbox_engine(ENGINE.into());
    ctx.set_app_id(ENGINE.into());
    ctx.set_instance_id(instance.into());
    ctx.commit();
    ctx.destroy();
    manager.destroy();
    queue.roundtrip(&mut State).map_err(|e| fail(&e))?;
    let socket = ContextSocket {
        path: path.to_path_buf(),
        _close: close_w,
    };
    // O compositor ficou com as suas cópias do socket de escuta e da ponta
    // de leitura; as nossas fecham aqui.
    drop(listener);
    drop(close_r);
    Ok(socket)
}
