//! Modos da aba ativa no terminal do usuário e recodificação do mouse para o
//! harness.

use sandmux_proto::term::ModeTracker;
use vt100::{MouseProtocolEncoding, MouseProtocolMode, Screen};

/// Liga no terminal do usuário os modos que o harness da aba ligou e que
/// mudam o que o terminal envia: teclado do kitty, foco, colagem entre
/// marcadores, cursor e teclado de aplicação. Sem tela alternativa nem mouse,
/// que são da TUI.
pub fn host_modes(screen: &Screen, tracker: &ModeTracker) -> Vec<u8> {
    let mut out = tracker.replay();
    if screen.bracketed_paste() {
        out.extend_from_slice(b"\x1b[?2004h");
    }
    if screen.application_cursor() {
        out.extend_from_slice(b"\x1b[?1h");
    }
    if screen.application_keypad() {
        out.extend_from_slice(b"\x1b=");
    }
    out
}

/// Desfaz o que [`host_modes`] ligou.
pub fn host_reset(tracker: &ModeTracker) -> Vec<u8> {
    let mut out = String::new();
    if !tracker.kitty.is_empty() {
        out.push_str(&format!("\x1b[<{}u", tracker.kitty.len()));
    }
    if tracker.kitty_base != 0 {
        out.push_str("\x1b[=0;1u");
    }
    out.push_str("\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b>");
    out.into_bytes()
}

/// Um evento de mouse SGR do terminal do usuário (`CSI < b ; x ; y M/m`),
/// com coordenadas a partir de 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    pub b: u32,
    pub x: u16,
    pub y: u16,
    /// `m`: botão solto.
    pub release: bool,
}

impl Mouse {
    pub fn is_wheel(&self) -> bool {
        self.b & 64 != 0
    }

    pub fn wheel_up(&self) -> bool {
        self.is_wheel() && self.b & 1 == 0
    }

    pub fn is_motion(&self) -> bool {
        self.b & 32 != 0
    }

    /// Botão 0, 1 ou 2; 3 é "nenhum" (movimento sem botão).
    pub fn button(&self) -> u32 {
        self.b & 3
    }
}

/// Recodifica um evento para o harness da aba, com a linha deslocada por
/// `dy` (a barra de abas), no modo e na codificação que ele pediu. `None`
/// quando o harness não quer esse evento.
pub fn encode_for(screen: &Screen, m: &Mouse, dy: u16) -> Option<Vec<u8>> {
    let mode = screen.mouse_protocol_mode();
    let wants = match mode {
        MouseProtocolMode::None => false,
        MouseProtocolMode::Press => !m.release && !m.is_motion(),
        MouseProtocolMode::PressRelease => !m.is_motion(),
        MouseProtocolMode::ButtonMotion => !m.is_motion() || m.button() != 3,
        MouseProtocolMode::AnyMotion => true,
    };
    if !wants || m.y <= dy {
        return None;
    }
    let (x, y) = (m.x, m.y - dy);
    match screen.mouse_protocol_encoding() {
        MouseProtocolEncoding::Sgr => Some(
            format!(
                "\x1b[<{};{};{}{}",
                m.b,
                x,
                y,
                if m.release { 'm' } else { 'M' }
            )
            .into_bytes(),
        ),
        enc => {
            // Formato X10: soltar é o botão 3; coordenadas + 32.
            let b = if m.release && !m.is_wheel() {
                (m.b & !3) | 3
            } else {
                m.b
            };
            let mut out = b"\x1b[M".to_vec();
            let push = |out: &mut Vec<u8>, v: u32| {
                let c = v + 32;
                if enc == MouseProtocolEncoding::Utf8 {
                    let mut buf = [0u8; 4];
                    let ch = char::from_u32(c).unwrap_or(' ');
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                } else {
                    out.push(c.min(255) as u8);
                }
            };
            push(&mut out, b);
            push(&mut out, x as u32);
            push(&mut out, y as u32);
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(feed: &[u8]) -> vt100::Parser {
        let mut p = vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        );
        p.process(feed);
        p
    }

    #[test]
    fn modos_para_o_terminal() {
        let p = screen(b"\x1b[?2004h\x1b[?1h\x1b=\x1b[?1049h\x1b[?1000h");
        let mut t = ModeTracker::new();
        t.feed(b"\x1b[>1u\x1b[?1004h");
        let s = String::from_utf8(host_modes(p.screen(), &t)).unwrap();
        assert!(s.contains("\x1b[>1u") && s.contains("\x1b[?1004h"), "{s:?}");
        assert!(s.contains("\x1b[?2004h") && s.contains("\x1b[?1h") && s.contains("\x1b="));
        assert!(!s.contains("1049") && !s.contains("1000"), "{s:?}");
        let r = String::from_utf8(host_reset(&t)).unwrap();
        assert!(r.contains("\x1b[<1u") && r.contains("\x1b[?2004l") && r.contains("\x1b>"));
    }

    #[test]
    fn mouse_sgr_com_deslocamento() {
        let p = screen(b"\x1b[?1000h\x1b[?1006h");
        let m = Mouse {
            b: 0,
            x: 10,
            y: 6,
            release: false,
        };
        assert_eq!(encode_for(p.screen(), &m, 1).unwrap(), b"\x1b[<0;10;5M");
        let r = Mouse { release: true, ..m };
        assert_eq!(encode_for(p.screen(), &r, 1).unwrap(), b"\x1b[<0;10;5m");
        // Na barra de abas, nunca vai para o harness.
        let bar = Mouse { y: 1, ..m };
        assert_eq!(encode_for(p.screen(), &bar, 1), None);
        // Movimento sem botão só com 1003.
        let mv = Mouse { b: 35, ..m };
        assert_eq!(encode_for(p.screen(), &mv, 1), None);
        let p3 = screen(b"\x1b[?1003h\x1b[?1006h");
        assert!(encode_for(p3.screen(), &mv, 1).is_some());
    }

    #[test]
    fn mouse_x10_e_sem_mouse() {
        let none = screen(b"");
        let m = Mouse {
            b: 0,
            x: 1,
            y: 2,
            release: false,
        };
        assert_eq!(encode_for(none.screen(), &m, 1), None);
        let p = screen(b"\x1b[?1000h");
        assert_eq!(
            encode_for(p.screen(), &m, 1).unwrap(),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        let r = Mouse { release: true, ..m };
        assert_eq!(
            encode_for(p.screen(), &r, 1).unwrap(),
            vec![0x1b, b'[', b'M', 35, 33, 33]
        );
        // Modo só de apertar: soltar não vai.
        let p9 = screen(b"\x1b[?9h");
        assert_eq!(encode_for(p9.screen(), &r, 1), None);
    }
}
