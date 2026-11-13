//! Tecla de prefixo do modo anexado: Ctrl+b, como byte 0x02 ou na forma do
//! protocolo de teclado do kitty (`CSI 98 ; 5 u`).

/// O que fazer com um pedaço da entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Send(Vec<u8>),
    Detach,
}

/// Uma unidade da entrada: um byte ou uma sequência de tecla do kitty.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    bytes: Vec<u8>,
    /// Código, modificadores (1 = nenhum) e tipo de evento (1 = apertar,
    /// 2 = repetir, 3 = soltar), quando é uma tecla `CSI … u`.
    key: Option<(u32, u32, u32)>,
}

impl Token {
    fn byte(b: u8) -> Self {
        Token {
            bytes: vec![b],
            key: None,
        }
    }

    fn is_release(&self) -> bool {
        matches!(self.key, Some((_, _, 3)))
    }

    fn is_ctrl_b(&self) -> bool {
        match self.key {
            Some((98, mods, ev)) => ev != 3 && (mods.saturating_sub(1) & 4) != 0,
            Some(_) => false,
            None => self.bytes == [0x02],
        }
    }

    fn is_d(&self) -> bool {
        match self.key {
            Some((100, mods, ev)) => ev != 3 && (mods.saturating_sub(1) & !1) == 0,
            Some(_) => false,
            None => self.bytes == *b"d",
        }
    }
}

/// Interpreta `CSI params u` como tecla do kitty.
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
            let ev = p.next().map_or(Some(1), |s| s.parse().ok())?;
            (mods, ev)
        }
        None => (1, 1),
    };
    Some((code, mods, ev))
}

const MAX_HELD: usize = 32;

/// Estado da tecla de prefixo entre leituras.
#[derive(Debug, Default)]
pub struct Prefix {
    pending: bool,
    held: Vec<u8>,
}

impl Prefix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Processa um pedaço da entrada. Depois de [`Action::Detach`], o resto
    /// do pedaço é descartado.
    pub fn feed(&mut self, input: &[u8]) -> Vec<Action> {
        let mut buf = std::mem::take(&mut self.held);
        buf.extend_from_slice(input);
        let mut actions = Vec::new();
        let mut send = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            let token = if buf[i] == 0x1b && buf.get(i + 1) == Some(&b'[') {
                let mut j = i + 2;
                while j < buf.len() && (0x30..=0x3f).contains(&buf[j]) {
                    j += 1;
                }
                if j >= buf.len() {
                    if buf.len() - i <= MAX_HELD {
                        // Sequência cortada no fim da leitura: espera o resto.
                        self.held = buf[i..].to_vec();
                        break;
                    }
                    Token::byte(buf[i])
                } else {
                    let seq = &buf[i..=j];
                    Token {
                        bytes: seq.to_vec(),
                        key: parse_kitty(seq),
                    }
                }
            } else {
                Token::byte(buf[i])
            };
            i += token.bytes.len();
            if !self.pending {
                if token.is_ctrl_b() {
                    self.pending = true;
                } else {
                    send.extend_from_slice(&token.bytes);
                }
                continue;
            }
            if token.is_release() {
                continue;
            }
            self.pending = false;
            if token.is_d() {
                if !send.is_empty() {
                    actions.push(Action::Send(std::mem::take(&mut send)));
                }
                actions.push(Action::Detach);
                self.held.clear();
                return actions;
            }
            if token.is_ctrl_b() {
                send.extend_from_slice(&token.bytes);
            }
        }
        if !send.is_empty() {
            actions.push(Action::Send(send));
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(chunks: &[&[u8]]) -> Vec<Action> {
        let mut p = Prefix::new();
        chunks.iter().flat_map(|c| p.feed(c)).collect()
    }

    fn sent(actions: &[Action]) -> Vec<u8> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Send(b) => Some(b.clone()),
                Action::Detach => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn texto_passa_igual() {
        let a = feed_all(&[b"ls -la\r", b"\x1b[A\x1b"]);
        assert_eq!(sent(&a), b"ls -la\r\x1b[A\x1b");
    }

    #[test]
    fn ctrl_b_d_desanexa() {
        let a = feed_all(&[b"ab\x02dXYZ"]);
        assert_eq!(a, vec![Action::Send(b"ab".to_vec()), Action::Detach]);
        let a = feed_all(&[b"\x02", b"d"]);
        assert_eq!(a, vec![Action::Detach]);
    }

    #[test]
    fn ctrl_b_duas_vezes_envia_um() {
        assert_eq!(sent(&feed_all(&[b"\x02\x02x"])), b"\x02x");
        assert_eq!(sent(&feed_all(&[b"\x02", b"\x02"])), b"\x02");
    }

    #[test]
    fn outra_tecla_depois_do_prefixo_descartada() {
        assert_eq!(sent(&feed_all(&[b"\x02xy"])), b"y");
        assert!(!feed_all(&[b"\x02x"]).contains(&Action::Detach));
    }

    #[test]
    fn forma_do_kitty() {
        let a = feed_all(&[b"\x1b[98;5u\x1b[100u"]);
        assert_eq!(a, vec![Action::Detach]);
        // Com tipos de evento: apertar, soltar, e `d` como texto.
        let a = feed_all(&[b"\x1b[98;5:1u\x1b[98;5:3u", b"d"]);
        assert_eq!(a, vec![Action::Detach]);
        // Ctrl+b duas vezes envia um Ctrl+b na mesma forma.
        assert_eq!(sent(&feed_all(&[b"\x1b[98;5u\x1b[98;5u"])), b"\x1b[98;5u");
        // Outras teclas do kitty passam iguais.
        assert_eq!(
            sent(&feed_all(&[b"\x1b[97;5u\x1b[13u"])),
            b"\x1b[97;5u\x1b[13u"
        );
    }

    #[test]
    fn sequencia_cortada_entre_leituras() {
        let a = feed_all(&[b"x\x1b[98", b";5u", b"d"]);
        assert_eq!(a, vec![Action::Send(b"x".to_vec()), Action::Detach]);
        assert_eq!(sent(&feed_all(&[b"\x1b[1", b"5~"])), b"\x1b[15~");
    }
}
