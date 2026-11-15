//! Repassador do protocolo do Wayland entre o sandbox e o compositor, que
//! esconde os protocolos privilegiados.
//!
//! Só o registro de globais é lido: o sandbox vê apenas os globais da
//! [`ALLOWED`]. Um `bind` num global escondido derruba a conexão. O resto das
//! mensagens, com os descritores que viajam junto (SCM_RIGHTS), passa como
//! veio.

use std::collections::HashSet;
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

/// Globais que o sandbox pode ver: os de janela, buffer, entrada com foco e
/// aparência. Ficam de fora o clipboard em segundo plano (data-control), a
/// captura de tela, teclado e mouse virtuais, métodos de entrada, a lista e o
/// controle das janelas dos outros programas, as camadas por cima do desktop,
/// a trava de tela e qualquer protocolo que não está aqui.
pub const ALLOWED: &[&str] = &[
    "wl_compositor",
    "wl_subcompositor",
    "wl_shm",
    "wl_seat",
    "wl_output",
    "wl_data_device_manager",
    "wl_fixes",
    "xdg_wm_base",
    "zxdg_output_manager_v1",
    "zxdg_decoration_manager_v1",
    "org_kde_kwin_server_decoration_manager",
    "org_kde_kwin_server_decoration_palette_manager",
    "org_kde_kwin_blur_manager",
    "org_kde_kwin_contrast_manager",
    "org_kde_kwin_shadow_manager",
    "org_kde_kwin_slide_manager",
    "org_kde_kwin_appmenu_manager",
    "zwp_linux_dmabuf_v1",
    "wp_linux_drm_syncobj_manager_v1",
    "wp_viewporter",
    "wp_fractional_scale_manager_v1",
    "wp_presentation",
    "wp_single_pixel_buffer_manager_v1",
    "wp_content_type_manager_v1",
    "wp_tearing_control_manager_v1",
    "wp_alpha_modifier_v1",
    "wp_color_manager_v1",
    "wp_color_representation_manager_v1",
    "wp_commit_timing_manager_v1",
    "wp_fifo_manager_v1",
    "wp_cursor_shape_manager_v1",
    "wp_pointer_warp_v1",
    "zwp_pointer_constraints_v1",
    "zwp_relative_pointer_manager_v1",
    "zwp_pointer_gestures_v1",
    "zwp_tablet_manager_v2",
    "zwp_text_input_manager_v1",
    "zwp_text_input_manager_v2",
    "zwp_text_input_manager_v3",
    "zwp_primary_selection_device_manager_v1",
    "zwp_idle_inhibit_manager_v1",
    "xdg_activation_v1",
    "xdg_wm_dialog_v1",
    "xdg_toplevel_icon_manager_v1",
    "xdg_toplevel_tag_manager_v1",
    "xdg_system_bell_v1",
    "xdg_toplevel_drag_manager_v1",
    "frog_color_management_factory_v1",
    "zxdg_exporter_v2",
    "zxdg_importer_v2",
];

pub fn allowed(interface: &str) -> bool {
    ALLOWED.contains(&interface)
}

/// Objeto e opcode do `wl_display` e do `wl_registry`.
const DISPLAY_ID: u32 = 1;
const GET_REGISTRY: u16 = 1;
const REGISTRY_GLOBAL: u16 = 0;
const REGISTRY_GLOBAL_REMOVE: u16 = 1;
const REGISTRY_BIND: u16 = 0;

/// Descritores por mensagem do `libwayland`.
const MAX_FDS: usize = 28;

#[derive(Default)]
struct Shared {
    registries: HashSet<u32>,
    hidden: HashSet<u32>,
}

/// Uma mensagem do Wayland: objeto, opcode e argumentos (sem o cabeçalho).
struct Message<'a> {
    object: u32,
    opcode: u16,
    args: &'a [u8],
}

/// Tamanho da primeira mensagem completa de `buf`. `Err` quando o
/// cabeçalho é inválido.
fn message_len(buf: &[u8]) -> Result<Option<usize>, ()> {
    if buf.len() < 8 {
        return Ok(None);
    }
    let word = u32::from_ne_bytes(buf[4..8].try_into().unwrap_or_default());
    let size = (word >> 16) as usize;
    if size < 8 || !size.is_multiple_of(4) {
        return Err(());
    }
    Ok((buf.len() >= size).then_some(size))
}

fn parse(m: &[u8]) -> Message<'_> {
    let object = u32::from_ne_bytes(m[0..4].try_into().unwrap_or_default());
    let word = u32::from_ne_bytes(m[4..8].try_into().unwrap_or_default());
    Message {
        object,
        opcode: (word & 0xffff) as u16,
        args: &m[8..],
    }
}

fn arg_u32(args: &[u8], at: usize) -> Option<u32> {
    args.get(at..at + 4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap_or_default()))
}

/// Texto do Wayland em `at`: tamanho (com o NUL) e os bytes, alinhados em 4.
/// Devolve o texto e a posição seguinte.
fn arg_str(args: &[u8], at: usize) -> Option<(&str, usize)> {
    let len = arg_u32(args, at)? as usize;
    let start = at + 4;
    let bytes = args.get(start..start + len)?;
    let text = std::str::from_utf8(bytes.strip_suffix(&[0])?).ok()?;
    Some((text, start + len.div_ceil(4) * 4))
}

/// O que fazer com uma mensagem do sandbox para o compositor.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Drop,
    Close,
}

fn from_client(m: &Message, s: &mut Shared) -> Verdict {
    if m.object == DISPLAY_ID && m.opcode == GET_REGISTRY {
        if let Some(id) = arg_u32(m.args, 0) {
            s.registries.insert(id);
        }
        return Verdict::Pass;
    }
    if s.registries.contains(&m.object) && m.opcode == REGISTRY_BIND {
        let ok = arg_u32(m.args, 0).is_some_and(|name| !s.hidden.contains(&name))
            && arg_str(m.args, 4).is_some_and(|(i, _)| allowed(i));
        return if ok { Verdict::Pass } else { Verdict::Close };
    }
    Verdict::Pass
}

fn from_server(m: &Message, s: &mut Shared) -> Verdict {
    if !s.registries.contains(&m.object) {
        return Verdict::Pass;
    }
    match m.opcode {
        REGISTRY_GLOBAL => {
            let (Some(name), Some((iface, _))) = (arg_u32(m.args, 0), arg_str(m.args, 4)) else {
                return Verdict::Close;
            };
            if allowed(iface) {
                Verdict::Pass
            } else {
                s.hidden.insert(name);
                Verdict::Drop
            }
        }
        REGISTRY_GLOBAL_REMOVE => match arg_u32(m.args, 0) {
            Some(name) if s.hidden.contains(&name) => Verdict::Drop,
            _ => Verdict::Pass,
        },
        _ => Verdict::Pass,
    }
}

/// Filtra as mensagens completas de `buf`, deixando o resto (uma mensagem
/// incompleta) em `buf`. `None`: fechar a conexão.
fn filter(buf: &mut Vec<u8>, s: &Mutex<Shared>, client: bool) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut st = s.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match message_len(&buf[pos..]) {
            Err(()) => return None,
            Ok(None) => break,
            Ok(Some(n)) => {
                let raw = &buf[pos..pos + n];
                let m = parse(raw);
                let v = if client {
                    from_client(&m, &mut st)
                } else {
                    from_server(&m, &mut st)
                };
                match v {
                    Verdict::Pass => out.extend_from_slice(raw),
                    Verdict::Drop => {}
                    Verdict::Close => return None,
                }
                pos += n;
            }
        }
    }
    buf.drain(..pos);
    Some(out)
}

/// Repassa uma direção até uma das pontas fechar.
fn pump(from: &UnixStream, to: &UnixStream, s: &Mutex<Shared>, client: bool) {
    let mut buf = Vec::new();
    let mut pending: Vec<OwnedFd> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut cmsg = nix::cmsg_space!([RawFd; MAX_FDS]);
        let mut iov = [IoSliceMut::new(&mut chunk)];
        let n = match recvmsg::<()>(
            from.as_raw_fd(),
            &mut iov,
            Some(&mut cmsg),
            MsgFlags::MSG_CMSG_CLOEXEC,
        ) {
            Ok(r) => {
                if let Ok(cmsgs) = r.cmsgs() {
                    for c in cmsgs {
                        if let ControlMessageOwned::ScmRights(fds) = c {
                            // SAFETY: descritores recém-recebidos, nossos.
                            pending.extend(
                                fds.into_iter()
                                    .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) }),
                            );
                        }
                    }
                }
                r.bytes
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let Some(out) = filter(&mut buf, s, client) else {
            break;
        };
        if out.is_empty() {
            continue;
        }
        // Os descritores vão com os primeiros bytes repassados depois deles:
        // o `libwayland` os enfileira até a mensagem que os usa.
        let raw: Vec<RawFd> = pending.iter().map(|f| f.as_raw_fd()).collect();
        let mut sent = 0;
        let mut ok = true;
        while sent < out.len() {
            let iov = [IoSlice::new(&out[sent..])];
            let cm = [ControlMessage::ScmRights(&raw)];
            let cms: &[ControlMessage] = if sent == 0 && !raw.is_empty() {
                &cm
            } else {
                &[]
            };
            match sendmsg::<()>(to.as_raw_fd(), &iov, cms, MsgFlags::MSG_NOSIGNAL, None) {
                Ok(k) => sent += k,
                Err(nix::errno::Errno::EINTR) => {}
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        pending.clear();
        if !ok {
            break;
        }
    }
    let _ = from.shutdown(std::net::Shutdown::Both);
    let _ = to.shutdown(std::net::Shutdown::Both);
}

/// Atende uma conexão do sandbox, ligando-a a `upstream`.
fn serve(client: UnixStream, upstream: &Path) {
    let Ok(server) = UnixStream::connect(upstream) else {
        return;
    };
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (c2, s2, sh2) = match (client.try_clone(), server.try_clone()) {
        (Ok(c), Ok(s)) => (c, s, shared.clone()),
        _ => return,
    };
    let up = std::thread::spawn(move || pump(&c2, &s2, &sh2, true));
    pump(&server, &client, &shared, false);
    let _ = up.join();
}

/// Repassador de uma aba: o socket de escuta (montado no sandbox) e o
/// compositor de destino. Soltá-lo para de aceitar e apaga o arquivo.
#[derive(Debug)]
pub struct Proxy {
    pub path: PathBuf,
    listener: Arc<UnixListener>,
}

impl Proxy {
    pub fn start(path: &Path, upstream: &Path) -> std::io::Result<Proxy> {
        let _ = std::fs::remove_file(path);
        let listener = Arc::new(UnixListener::bind(path)?);
        let l = listener.clone();
        let up = upstream.to_path_buf();
        std::thread::Builder::new()
            .name("wayland-proxy".into())
            .spawn(move || {
                for c in l.incoming() {
                    match c {
                        Ok(c) => {
                            let up = up.clone();
                            let _ = std::thread::Builder::new()
                                .name("wayland-conn".into())
                                .spawn(move || serve(c, &up));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Proxy {
            path: path.to_path_buf(),
            listener,
        })
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        // Acorda o `accept` com erro, e a thread termina.
        // SAFETY: shutdown num descritor válido do socket de escuta.
        unsafe { libc::shutdown(self.listener.as_raw_fd(), libc::SHUT_RDWR) };
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(object: u32, opcode: u16, args: &[u8]) -> Vec<u8> {
        let size = 8 + args.len() as u32;
        let mut v = object.to_ne_bytes().to_vec();
        v.extend((size << 16 | opcode as u32).to_ne_bytes());
        v.extend_from_slice(args);
        v
    }

    fn string(s: &str) -> Vec<u8> {
        let mut v = ((s.len() + 1) as u32).to_ne_bytes().to_vec();
        v.extend(s.as_bytes());
        v.push(0);
        while !v.len().is_multiple_of(4) {
            v.push(0);
        }
        v
    }

    fn global(reg: u32, name: u32, iface: &str) -> Vec<u8> {
        let mut a = name.to_ne_bytes().to_vec();
        a.extend(string(iface));
        a.extend(1u32.to_ne_bytes());
        msg(reg, REGISTRY_GLOBAL, &a)
    }

    fn bind(reg: u32, name: u32, iface: &str, id: u32) -> Vec<u8> {
        let mut a = name.to_ne_bytes().to_vec();
        a.extend(string(iface));
        a.extend(1u32.to_ne_bytes());
        a.extend(id.to_ne_bytes());
        msg(reg, REGISTRY_BIND, &a)
    }

    #[test]
    fn esconde_os_globais_privilegiados() {
        let s = Mutex::new(Shared::default());
        let mut c = msg(DISPLAY_ID, GET_REGISTRY, &2u32.to_ne_bytes());
        assert_eq!(filter(&mut c, &s, true).unwrap().len(), 12);
        let mut ev = global(2, 1, "wl_compositor");
        ev.extend(global(2, 7, "ext_data_control_manager_v1"));
        ev.extend(global(2, 9, "zwlr_screencopy_manager_v1"));
        ev.extend(msg(2, REGISTRY_GLOBAL_REMOVE, &7u32.to_ne_bytes()));
        // Uma mensagem incompleta fica para a próxima leitura.
        let partial = global(2, 3, "wl_shm");
        ev.extend_from_slice(&partial[..10]);
        let out = filter(&mut ev, &s, false).unwrap();
        assert_eq!(out, global(2, 1, "wl_compositor"));
        assert_eq!(ev, partial[..10]);
        ev.extend_from_slice(&partial[10..]);
        assert_eq!(filter(&mut ev, &s, false).unwrap(), partial);
        // Outro objeto com o mesmo opcode passa.
        let mut other = global(5, 7, "ext_data_control_manager_v1");
        assert!(!filter(&mut other, &s, false).unwrap().is_empty());
    }

    #[test]
    fn bind_escondido_fecha_a_conexao() {
        let s = Mutex::new(Shared::default());
        let mut c = msg(DISPLAY_ID, GET_REGISTRY, &2u32.to_ne_bytes());
        filter(&mut c, &s, true).unwrap();
        let mut ev = global(2, 7, "ext_data_control_manager_v1");
        filter(&mut ev, &s, false).unwrap();
        let mut ok = bind(2, 1, "wl_compositor", 3);
        assert!(filter(&mut ok, &s, true).is_some());
        let mut bad = bind(2, 7, "ext_data_control_manager_v1", 4);
        assert!(filter(&mut bad, &s, true).is_none());
        // Mesmo com o nome de um global liberado, a interface decide.
        let mut sneaky = bind(2, 1, "zwlr_screencopy_manager_v1", 5);
        assert!(filter(&mut sneaky, &s, true).is_none());
    }

    #[test]
    fn cabecalho_invalido_fecha() {
        let s = Mutex::new(Shared::default());
        let mut bad = 1u32.to_ne_bytes().to_vec();
        bad.extend((4u32 << 16).to_ne_bytes());
        assert!(filter(&mut bad, &s, true).is_none());
    }
}
