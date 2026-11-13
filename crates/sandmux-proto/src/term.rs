//! Utilitários de fluxo de terminal: o filtro de OSC 52 e o rastreador dos
//! modos que o emulador em memória não acompanha.

/// Tamanho máximo de um OSC; acima disso a sequência inteira é descartada.
pub const MAX_OSC: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FState {
    Ground,
    /// Viu ESC fora de OSC.
    Esc,
    /// Viu 0xC2 fora de OSC (talvez o C1 OSC codificado em UTF-8).
    C2,
    Osc,
    /// Viu ESC dentro de OSC.
    OscEsc,
    /// Viu 0xC2 dentro de OSC (talvez o C1 ST codificado em UTF-8).
    OscC2,
}

/// Retira da saída as sequências OSC 52: a consulta do clipboard sempre, e a
/// gravação quando a sessão não a libera. Reconhece OSC iniciado por `ESC ]`
/// ou pelo C1 codificado em UTF-8 (`C2 9D`) e terminado por BEL, `ESC \` ou
/// `C2 9C`, mesmo dividido entre chamadas.
#[derive(Debug)]
pub struct OscFilter {
    allow_clipboard: bool,
    state: FState,
    buf: Vec<u8>,
    discarding: bool,
}

impl OscFilter {
    pub fn new(allow_clipboard: bool) -> Self {
        Self {
            allow_clipboard,
            state: FState::Ground,
            buf: Vec::new(),
            discarding: false,
        }
    }

    /// Filtra um pedaço do fluxo. Bytes de uma sequência ainda incompleta
    /// ficam guardados para a próxima chamada.
    pub fn filter(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        for &b in input {
            self.step(b, &mut out);
        }
        out
    }

    fn step(&mut self, b: u8, out: &mut Vec<u8>) {
        match self.state {
            FState::Ground => match b {
                0x1b => self.state = FState::Esc,
                0xc2 => self.state = FState::C2,
                _ => out.push(b),
            },
            FState::Esc => {
                if b == b']' {
                    self.start_osc(&[0x1b, b']']);
                } else {
                    out.push(0x1b);
                    self.state = FState::Ground;
                    self.step(b, out);
                }
            }
            FState::C2 => {
                if b == 0x9d {
                    self.start_osc(&[0xc2, 0x9d]);
                } else {
                    out.push(0xc2);
                    self.state = FState::Ground;
                    self.step(b, out);
                }
            }
            FState::Osc => match b {
                0x07 => {
                    self.append(&[0x07]);
                    self.finish(out);
                }
                0x1b => self.state = FState::OscEsc,
                0xc2 => self.state = FState::OscC2,
                _ => self.append(&[b]),
            },
            FState::OscEsc => {
                if b == b'\\' {
                    self.append(&[0x1b, b'\\']);
                    self.finish(out);
                } else {
                    // Um ESC que não é ST interrompe o OSC.
                    self.finish(out);
                    self.state = FState::Esc;
                    self.step(b, out);
                }
            }
            FState::OscC2 => {
                if b == 0x9c {
                    self.append(&[0xc2, 0x9c]);
                    self.finish(out);
                } else {
                    self.append(&[0xc2]);
                    self.state = FState::Osc;
                    self.step(b, out);
                }
            }
        }
    }

    fn start_osc(&mut self, intro: &[u8]) {
        self.buf.clear();
        self.buf.extend_from_slice(intro);
        self.discarding = false;
        self.state = FState::Osc;
    }

    fn append(&mut self, bytes: &[u8]) {
        if self.discarding {
            return;
        }
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > MAX_OSC {
            self.discarding = true;
            self.buf.clear();
        }
    }

    fn finish(&mut self, out: &mut Vec<u8>) {
        self.state = FState::Ground;
        if self.discarding {
            self.discarding = false;
            return;
        }
        if self.keep(&self.buf) {
            out.extend_from_slice(&self.buf);
        }
        self.buf.clear();
    }

    fn keep(&self, seq: &[u8]) -> bool {
        let body = &seq[2..];
        let mut parts = body.splitn(3, |&c| c == b';');
        if parts.next() != Some(b"52") {
            return true;
        }
        let _selection = parts.next();
        let data = parts.next().unwrap_or(b"");
        // Tira o terminador do fim dos dados.
        let data = data
            .strip_suffix(b"\x07")
            .or_else(|| data.strip_suffix(b"\x1b\\"))
            .or_else(|| data.strip_suffix(b"\xc2\x9c"))
            .unwrap_or(data);
        if data == b"?" {
            return false;
        }
        self.allow_clipboard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TState {
    Ground,
    Esc,
    Csi,
}

/// Acompanha, no fluxo de saída, os modos de terminal que precisam ser
/// reenviados ao anexar ou desfeitos ao desanexar.
#[derive(Debug, Clone)]
pub struct ModeTracker {
    state: TState,
    csi: Vec<u8>,
    /// Pilha do protocolo de teclado do kitty.
    pub kitty: Vec<u32>,
    /// Bandeiras do kitty definidas com `CSI = … u` com a pilha vazia.
    pub kitty_base: u32,
    pub focus: bool,
    pub alt_screen: bool,
}

impl Default for ModeTracker {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_CSI: usize = 64;
const MAX_KITTY: usize = 64;

impl ModeTracker {
    pub fn new() -> Self {
        Self {
            state: TState::Ground,
            csi: Vec::new(),
            kitty: Vec::new(),
            kitty_base: 0,
            focus: false,
            alt_screen: false,
        }
    }

    pub fn feed(&mut self, input: &[u8]) {
        for &b in input {
            match self.state {
                TState::Ground => {
                    if b == 0x1b {
                        self.state = TState::Esc;
                    }
                }
                TState::Esc => match b {
                    b'[' => {
                        self.csi.clear();
                        self.state = TState::Csi;
                    }
                    b'c' => {
                        self.reset_state();
                        self.state = TState::Ground;
                    }
                    0x1b => {}
                    _ => self.state = TState::Ground,
                },
                TState::Csi => {
                    if (0x40..=0x7e).contains(&b) {
                        self.apply(b);
                        self.state = TState::Ground;
                    } else if (0x20..=0x3f).contains(&b) && self.csi.len() < MAX_CSI {
                        self.csi.push(b);
                    } else {
                        self.state = if b == 0x1b {
                            TState::Esc
                        } else {
                            TState::Ground
                        };
                    }
                }
            }
        }
    }

    fn reset_state(&mut self) {
        self.kitty.clear();
        self.kitty_base = 0;
        self.focus = false;
        self.alt_screen = false;
    }

    fn params(body: &[u8]) -> Vec<u32> {
        body.split(|&c| c == b';')
            .map(|p| {
                let p = p.split(|&c| c == b':').next().unwrap_or(b"");
                std::str::from_utf8(p)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            })
            .collect()
    }

    fn apply(&mut self, fin: u8) {
        let csi = std::mem::take(&mut self.csi);
        let (prefix, body) = match csi.first() {
            Some(&p @ (b'?' | b'>' | b'<' | b'=')) => (Some(p), &csi[1..]),
            _ => (None, &csi[..]),
        };
        match (prefix, fin) {
            (Some(b'?'), b'h' | b'l') => {
                let on = fin == b'h';
                for m in Self::params(body) {
                    match m {
                        47 | 1047 | 1049 => self.alt_screen = on,
                        1004 => self.focus = on,
                        _ => {}
                    }
                }
            }
            (Some(b'>'), b'u') => {
                if self.kitty.len() < MAX_KITTY {
                    let flags = Self::params(body).first().copied().unwrap_or(0);
                    self.kitty.push(flags);
                }
            }
            (Some(b'<'), b'u') => {
                let n = Self::params(body).first().copied().unwrap_or(1).max(1) as usize;
                let keep = self.kitty.len().saturating_sub(n);
                self.kitty.truncate(keep);
            }
            (Some(b'='), b'u') => {
                let p = Self::params(body);
                let flags = p.first().copied().unwrap_or(0);
                let mode = p.get(1).copied().unwrap_or(1);
                let cur = self.kitty.last_mut().unwrap_or(&mut self.kitty_base);
                *cur = match mode {
                    2 => *cur | flags,
                    3 => *cur & !flags,
                    _ => flags,
                };
            }
            _ => {}
        }
    }

    /// Sequências que reativam, num terminal novo, os modos que o emulador em
    /// memória não reproduz: o teclado do kitty e os eventos de foco.
    pub fn replay(&self) -> Vec<u8> {
        let mut out = String::new();
        if self.kitty_base != 0 {
            out.push_str(&format!("\x1b[={};1u", self.kitty_base));
        }
        for f in &self.kitty {
            out.push_str(&format!("\x1b[>{f}u"));
        }
        if self.focus {
            out.push_str("\x1b[?1004h");
        }
        out.into_bytes()
    }

    /// Sequências que devolvem o terminal do usuário ao estado normal depois
    /// de anexado a uma sessão.
    pub fn reset(&self) -> Vec<u8> {
        let mut out = String::new();
        if self.alt_screen {
            out.push_str("\x1b[?1049l");
        }
        if !self.kitty.is_empty() {
            out.push_str(&format!("\x1b[<{}u", self.kitty.len()));
        }
        if self.kitty_base != 0 {
            out.push_str("\x1b[=0;1u");
        }
        out.push_str(concat!(
            "\x1b[0m",
            "\x1b[?25h",
            "\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l",
            "\x1b[?1005l\x1b[?1006l\x1b[?1015l",
            "\x1b[?1004l",
            "\x1b[?2004l",
            "\x1b[?1l\x1b>",
            "\x1b[0 q",
        ));
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(allow: bool, input: &[u8]) -> Vec<u8> {
        OscFilter::new(allow).filter(input)
    }

    /// Filtra o mesmo fluxo cortado em todos os pontos possíveis.
    fn every_split(allow: bool, input: &[u8]) -> Vec<u8> {
        let whole = run(allow, input);
        for cut in 0..=input.len() {
            let mut f = OscFilter::new(allow);
            let mut out = f.filter(&input[..cut]);
            out.extend(f.filter(&input[cut..]));
            assert_eq!(out, whole, "corte em {cut}");
        }
        whole
    }

    const WRITE_BEL: &[u8] = b"a\x1b]52;c;b2xh\x07b";
    const WRITE_ST: &[u8] = b"a\x1b]52;c;b2xh\x1b\\b";
    const WRITE_C1: &[u8] = b"a\xc2\x9d52;c;b2xh\xc2\x9cb";
    const QUERY: &[u8] = b"a\x1b]52;c;?\x07b";

    #[test]
    fn gravacao_retirada_por_padrao_em_toda_forma() {
        for seq in [WRITE_BEL, WRITE_ST, WRITE_C1] {
            assert_eq!(every_split(false, seq), b"ab", "{seq:?}");
        }
    }

    #[test]
    fn gravacao_passa_quando_liberada() {
        for seq in [WRITE_BEL, WRITE_ST, WRITE_C1] {
            assert_eq!(every_split(true, seq), seq, "{seq:?}");
        }
    }

    #[test]
    fn consulta_sempre_retirada() {
        assert_eq!(every_split(true, QUERY), b"ab");
        assert_eq!(every_split(false, QUERY), b"ab");
        assert_eq!(every_split(true, b"\x1b]52;p;?\x1b\\"), b"");
    }

    #[test]
    fn outras_sequencias_passam_iguais() {
        let s: &[u8] = b"ol\xc3\xa1 \xc2\xa0\x1b[31mx\x1b]0;titulo\x07\x1b]8;;http://x\x1b\\y\x1b]8;;\x1b\\\x1b[0m \xe2\x9d\x9d \x1bc\x1b";
        // O ESC final fica guardado até o próximo byte.
        let mut f = OscFilter::new(false);
        let mut out = f.filter(s);
        out.extend(f.filter(b"[m"));
        let mut expected = s.to_vec();
        expected.extend_from_slice(b"[m");
        assert_eq!(out, expected);
    }

    #[test]
    fn osc_interrompido_por_outro_esc() {
        assert_eq!(every_split(false, b"\x1b]52;c;abc\x1b[1mx"), b"\x1b[1mx");
        assert_eq!(every_split(false, b"\x1b]0;t\x1b[1mx"), b"\x1b]0;t\x1b[1mx");
    }

    #[test]
    fn osc_gigante_descartado() {
        let mut s = b"a\x1b]0;".to_vec();
        s.extend(std::iter::repeat_n(b'x', MAX_OSC + 10));
        s.extend_from_slice(b"\x07b\x1b]0;ok\x07");
        let mut f = OscFilter::new(true);
        let mut out = Vec::new();
        for chunk in s.chunks(4096) {
            out.extend(f.filter(chunk));
        }
        assert_eq!(out, b"ab\x1b]0;ok\x07");
    }

    #[test]
    fn rastreia_pilha_do_kitty() {
        let mut t = ModeTracker::new();
        t.feed(b"\x1b[>1u\x1b[>3u");
        assert_eq!(t.kitty, vec![1, 3]);
        t.feed(b"\x1b[=5;2u");
        assert_eq!(t.kitty, vec![1, 7]);
        t.feed(b"\x1b[<u");
        assert_eq!(t.kitty, vec![1]);
        t.feed(b"\x1b[<9u");
        assert!(t.kitty.is_empty());
        t.feed(b"\x1b[=4;1u");
        assert_eq!(t.kitty_base, 4);
        assert_eq!(t.replay(), b"\x1b[=4;1u");
    }

    #[test]
    fn rastreia_foco_e_tela_alternativa_divididos() {
        let mut t = ModeTracker::new();
        for b in b"\x1b[?1049;1004h\x1b[>1u" {
            t.feed(&[*b]);
        }
        assert!(t.alt_screen && t.focus);
        assert_eq!(t.replay(), b"\x1b[>1u\x1b[?1004h");
        let reset = String::from_utf8(t.reset()).unwrap();
        assert!(reset.starts_with("\x1b[?1049l\x1b[<1u"), "{reset:?}");
        assert!(reset.contains("\x1b[?1004l") && reset.contains("\x1b[?25h"));
        t.feed(b"\x1b[?1004l\x1b[?1049l");
        assert!(!t.alt_screen && !t.focus);
        t.feed(b"\x1b[>1u\x1bc");
        assert!(t.kitty.is_empty());
    }

    #[test]
    fn sem_modos_reset_nao_mexe_na_tela() {
        let t = ModeTracker::new();
        let reset = String::from_utf8(t.reset()).unwrap();
        assert!(!reset.contains("1049") && !reset.contains("<"));
        assert!(t.replay().is_empty());
    }
}
