//! Entrada da TUI, lida crua: tecla de prefixo (byte de controle ou tecla do
//! kitty), a tecla da ação depois dela, eventos de mouse SGR e, com um modal
//! aberto, teclas de edição. O resto vai cru para a aba ativa.

use super::modes::Mouse;

/// O que a entrada pede.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Bytes para a aba ativa.
    Send(Vec<u8>),
    /// Uma ação depois do prefixo, pelo nome (`new`, `next`…).
    Action(String),
    /// Prefixo e um dígito: ir para a aba naquela posição.
    Goto(usize),
    Mouse(Mouse),
    Key(Key),
    /// O terminal ganhou (`true`) ou perdeu o foco (`CSI I` / `CSI O`).
    Focus(bool),
}

/// Teclas que os modais entendem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Up,
    Down,
    Left,
    Right,
    Tab,
    /// Ctrl com uma letra minúscula (só as que a TUI usa).
    Ctrl(char),
}

/// Uma unidade da entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    bytes: Vec<u8>,
    /// Tecla do kitty: código, modificadores (1 = nenhum), evento (3 = soltar).
    kitty: Option<(u32, u32, u32)>,
    mouse: Option<Mouse>,
}

fn parse_kitty(seq: &[u8]) -> Option<(u32, u32, u32)> {
    let body = seq.strip_prefix(b"\x1b[")?.strip_suffix(b"u")?;
    let text = std::str::from_utf8(body).ok()?;
    let mut fields = text.split(';');
    let code: u32 = fields.next()?.split(':').next()?.parse().ok()?;
    let (mods, ev) = match fields.next() {
        Some(f) => {
            let mut p = f.split(':');
            let mods = p
                .next()
                .filter(|s| !s.is_empty())
                .map_or(Some(1), |s| s.parse().ok())?;
            (mods, p.next().map_or(Some(1), |s| s.parse().ok())?)
        }
        None => (1, 1),
    };
    Some((code, mods, ev))
}

fn parse_mouse(seq: &[u8]) -> Option<Mouse> {
    let body = seq.strip_prefix(b"\x1b[<")?;
    let (body, release) = match body.last()? {
        b'M' => (&body[..body.len() - 1], false),
        b'm' => (&body[..body.len() - 1], true),
        _ => return None,
    };
    let text = std::str::from_utf8(body).ok()?;
    let mut f = text.split(';').map(|s| s.parse::<u32>().ok());
    let b = f.next()??;
    let x = f.next()?? as u16;
    let y = f.next()?? as u16;
    Some(Mouse { b, x, y, release })
}

impl Token {
    fn release(&self) -> bool {
        matches!(self.kitty, Some((_, _, 3)))
    }

    /// Ctrl com a letra `letter`.
    fn is_ctrl(&self, letter: char) -> bool {
        match self.kitty {
            Some((code, mods, ev)) => {
                ev != 3 && code == letter as u32 && (mods.saturating_sub(1) & 4) != 0
            }
            None => self.bytes == [(letter as u8) & 0x1f],
        }
    }

    /// O caractere simples da tecla (sem Ctrl nem Alt), se houver.
    fn plain_char(&self) -> Option<char> {
        match self.kitty {
            Some((code, mods, ev)) => {
                let m = mods.saturating_sub(1);
                (ev != 3 && m & !1 == 0)
                    .then(|| char::from_u32(code))
                    .flatten()
            }
            None => std::str::from_utf8(&self.bytes)
                .ok()
                .and_then(|s| {
                    let mut c = s.chars();
                    c.next().filter(|_| c.next().is_none())
                })
                .filter(|c| !c.is_control()),
        }
    }

    fn key(&self) -> Option<Key> {
        if let Some((code, mods, ev)) = self.kitty {
            if ev == 3 {
                return None;
            }
            let m = mods.saturating_sub(1);
            return match code {
                13 => Some(Key::Enter),
                27 => Some(Key::Esc),
                127 | 8 => Some(Key::Backspace),
                9 => Some(Key::Tab),
                c @ 97..=122 if m == 4 => char::from_u32(c).map(Key::Ctrl),
                _ if m & !1 == 0 => char::from_u32(code).map(Key::Char),
                _ => None,
            };
        }
        match self.bytes.as_slice() {
            b"\r" | b"\n" => Some(Key::Enter),
            b"\x1b" => Some(Key::Esc),
            b"\x7f" | b"\x08" => Some(Key::Backspace),
            b"\t" => Some(Key::Tab),
            b"\x12" => Some(Key::Ctrl('r')),
            b"\x1b[A" | b"\x1bOA" => Some(Key::Up),
            b"\x1b[B" | b"\x1bOB" => Some(Key::Down),
            b"\x1b[C" | b"\x1bOC" => Some(Key::Right),
            b"\x1b[D" | b"\x1bOD" => Some(Key::Left),
            _ => self.plain_char().map(Key::Char),
        }
    }
}

const MAX_HELD: usize = 64;

/// Estado do reconhecedor entre leituras.
#[derive(Debug)]
pub struct Parser {
    prefix: char,
    keys: Vec<(char, String)>,
    pending: bool,
    held: Vec<u8>,
    /// Com um modal aberto, as teclas viram [`Input::Key`].
    pub modal: bool,
}

impl Parser {
    pub fn new(prefix: char, keys: Vec<(char, String)>) -> Self {
        Parser {
            prefix,
            keys,
            pending: false,
            held: Vec::new(),
            modal: false,
        }
    }

    fn tokens(&mut self, input: &[u8]) -> Vec<Token> {
        let mut buf = std::mem::take(&mut self.held);
        buf.extend_from_slice(input);
        let mut out = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            let len = if b == 0x1b && matches!(buf.get(i + 1), Some(b'[') | Some(b'O')) {
                if buf[i + 1] == b'O' {
                    if i + 2 < buf.len() { 3 } else { 0 }
                } else {
                    let mut j = i + 2;
                    while j < buf.len() && (0x20..=0x3f).contains(&buf[j]) {
                        j += 1;
                    }
                    if j >= buf.len() { 0 } else { j - i + 1 }
                }
            } else if b >= 0xc0 {
                // Começo de um caractere UTF-8.
                let n = if b >= 0xf0 {
                    4
                } else if b >= 0xe0 {
                    3
                } else {
                    2
                };
                if i + n <= buf.len() { n } else { 0 }
            } else {
                1
            };
            if len == 0 {
                if buf.len() - i <= MAX_HELD {
                    // Sequência cortada no fim da leitura: espera o resto.
                    self.held = buf[i..].to_vec();
                    break;
                }
                out.push(Token {
                    bytes: vec![b],
                    kitty: None,
                    mouse: None,
                });
                i += 1;
                continue;
            }
            let bytes = buf[i..i + len].to_vec();
            out.push(Token {
                kitty: parse_kitty(&bytes),
                mouse: parse_mouse(&bytes),
                bytes,
            });
            i += len;
        }
        out
    }

    /// Processa um pedaço da entrada.
    pub fn feed(&mut self, input: &[u8]) -> Vec<Input> {
        let mut out = Vec::new();
        let mut send = Vec::new();
        let flush = |send: &mut Vec<u8>, out: &mut Vec<Input>| {
            if !send.is_empty() {
                out.push(Input::Send(std::mem::take(send)));
            }
        };
        for t in self.tokens(input) {
            if let Some(m) = t.mouse {
                flush(&mut send, &mut out);
                out.push(Input::Mouse(m));
                continue;
            }
            if t.bytes == b"\x1b[I" || t.bytes == b"\x1b[O" {
                flush(&mut send, &mut out);
                out.push(Input::Focus(t.bytes == b"\x1b[I"));
                continue;
            }
            if self.pending {
                if t.release() {
                    continue;
                }
                self.pending = false;
                if t.is_ctrl(self.prefix) {
                    if self.modal {
                        continue;
                    }
                    send.extend_from_slice(&t.bytes);
                    continue;
                }
                if let Some(c) = t.plain_char() {
                    flush(&mut send, &mut out);
                    if let Some(d) = c.to_digit(10) {
                        out.push(Input::Goto(d as usize));
                    } else if let Some((_, a)) = self.keys.iter().find(|(k, _)| *k == c) {
                        out.push(Input::Action(a.clone()));
                    }
                }
                continue;
            }
            if t.is_ctrl(self.prefix) {
                self.pending = true;
                continue;
            }
            if self.modal {
                if let Some(k) = t.key() {
                    out.push(Input::Key(k));
                }
                continue;
            }
            send.extend_from_slice(&t.bytes);
        }
        flush(&mut send, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> Parser {
        Parser::new(
            'b',
            vec![
                ('c', "new".into()),
                ('n', "next".into()),
                ('d', "detach".into()),
            ],
        )
    }

    #[test]
    fn aviso_de_foco_nao_vai_para_a_aba() {
        let mut p = parser();
        assert_eq!(
            all(&mut p, &[b"ab\x1b[Icd\x1b[O"]),
            vec![
                Input::Send(b"ab".to_vec()),
                Input::Focus(true),
                Input::Send(b"cd".to_vec()),
                Input::Focus(false),
            ]
        );
        // Cortado no meio da leitura: espera o resto.
        assert_eq!(all(&mut p, &[b"\x1b[", b"O"]), vec![Input::Focus(false)]);
        // Entre o prefixo e a tecla da ação, não atrapalha a ação.
        assert_eq!(
            all(&mut p, &[b"\x02\x1b[Ic"]),
            vec![Input::Focus(true), Input::Action("new".into())]
        );
    }

    fn all(p: &mut Parser, chunks: &[&[u8]]) -> Vec<Input> {
        chunks.iter().flat_map(|c| p.feed(c)).collect()
    }

    #[test]
    fn texto_cru_e_acoes() {
        let mut p = parser();
        assert_eq!(
            all(&mut p, &[b"ls\r\x02n\x02c"]),
            vec![
                Input::Send(b"ls\r".to_vec()),
                Input::Action("next".into()),
                Input::Action("new".into())
            ]
        );
        assert_eq!(all(&mut p, &[b"\x02", b"3"]), vec![Input::Goto(3)]);
        assert_eq!(all(&mut p, &[b"\x02\x02"]), vec![Input::Send(vec![2])]);
        // Tecla desconhecida depois do prefixo é descartada.
        assert_eq!(all(&mut p, &[b"\x02zq"]), vec![Input::Send(b"q".to_vec())]);
    }

    #[test]
    fn prefixo_do_kitty_e_trocado() {
        let mut p = parser();
        assert_eq!(
            all(&mut p, &[b"\x1b[98;5u\x1b[98;5:3u\x1b[110u"]),
            vec![Input::Action("next".into())]
        );
        let mut a = Parser::new('a', vec![('n', "next".into())]);
        assert_eq!(
            all(&mut a, &[b"\x02\x01n"]),
            vec![Input::Send(vec![2]), Input::Action("next".into())]
        );
    }

    #[test]
    fn mouse_separado() {
        let mut p = parser();
        assert_eq!(
            all(&mut p, &[b"x\x1b[<0;12;1M", b"\x1b[<0;12", b";1my"]),
            vec![
                Input::Send(b"x".to_vec()),
                Input::Mouse(Mouse {
                    b: 0,
                    x: 12,
                    y: 1,
                    release: false
                }),
                Input::Mouse(Mouse {
                    b: 0,
                    x: 12,
                    y: 1,
                    release: true
                }),
                Input::Send(b"y".to_vec()),
            ]
        );
    }

    #[test]
    fn teclas_do_modal() {
        let mut p = parser();
        p.modal = true;
        assert_eq!(
            all(&mut p, &[b"ab\x7f\x1b[B\r", "é".as_bytes(), b"\x1b"]),
            vec![
                Input::Key(Key::Char('a')),
                Input::Key(Key::Char('b')),
                Input::Key(Key::Backspace),
                Input::Key(Key::Down),
                Input::Key(Key::Enter),
                Input::Key(Key::Char('é')),
                Input::Key(Key::Esc),
            ]
        );
        // O prefixo ainda funciona com o modal aberto.
        assert_eq!(
            all(&mut p, &[b"\x02d"]),
            vec![Input::Action("detach".into())]
        );
    }
}
