//! Colar imagem: lê o clipboard do sistema (X11 ou Wayland) e grava um PNG na
//! pasta de entrada.

use std::path::{Path, PathBuf};

use sandmux_spec::{tr, trf};

/// Lê a imagem do clipboard: largura, altura e RGBA.
pub fn clipboard_image() -> Result<(usize, usize, Vec<u8>), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| {
        trf!(
            "não foi possível ler o clipboard: {e}",
            "could not read the clipboard: {e}"
        )
    })?;
    match cb.get_image() {
        Ok(img) => Ok((img.width, img.height, img.bytes.into_owned())),
        Err(arboard::Error::ContentNotAvailable) => {
            Err(tr!("o clipboard não tem imagem", "the clipboard has no image").into())
        }
        Err(e) => Err(trf!(
            "não foi possível ler o clipboard: {e}",
            "could not read the clipboard: {e}"
        )),
    }
}

/// Grava RGBA como PNG em `dir`, com nome pela data e hora. Devolve o caminho.
pub fn save_png(
    dir: &Path,
    w: usize,
    h: usize,
    rgba: &[u8],
    stamp: &str,
) -> Result<PathBuf, String> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Err(tr!(
            "a imagem do clipboard está vazia",
            "the clipboard image is empty"
        )
        .into());
    }
    std::fs::create_dir_all(dir).map_err(|e| trf!("pasta de entrada: {e}", "inbox folder: {e}"))?;
    let mut path = dir.join(format!("colado-{stamp}.png"));
    let mut n = 1;
    while path.exists() {
        n += 1;
        path = dir.join(format!("colado-{stamp}-{n}.png"));
    }
    let f = std::fs::File::create(&path)
        .map_err(|e| trf!("gravar {}: {e}", "write {}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| format!("PNG: {e}"))?;
    wr.write_image_data(&rgba[..w * h * 4])
        .map_err(|e| format!("PNG: {e}"))?;
    Ok(path)
}

/// Data e hora local no formato `AAAAMMDD-HHMMSS`.
pub fn stamp() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r com ponteiros válidos.
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grava_png_valido_sem_sobrescrever() {
        let d = tempfile::tempdir().unwrap();
        let rgba = vec![255u8; 2 * 3 * 4];
        let a = save_png(d.path(), 2, 3, &rgba, "20261113-120000").unwrap();
        let b = save_png(d.path(), 2, 3, &rgba, "20261113-120000").unwrap();
        assert_ne!(a, b);
        assert!(
            a.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("colado-20261113")
        );
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&a).unwrap()));
        let r = dec.read_info().unwrap();
        assert_eq!((r.info().width, r.info().height), (2, 3));
        assert!(save_png(d.path(), 0, 0, &[], "x").is_err());
        assert_eq!(stamp().len(), 15);
    }
}

/// Maior pedido de cópia aceito do harness (base64).
pub const MAX_COPY: usize = 1 << 20;

/// Callbacks do emulador da aba: guarda os textos que o harness pediu para
/// copiar (OSC 52). Só chegam os pedidos que o daemon deixou passar.
#[derive(Debug, Default)]
pub struct Clip {
    pub pending: Vec<String>,
}

impl vt100::Callbacks for Clip {
    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, _ty: &[u8], data: &[u8]) {
        if data.len() > MAX_COPY {
            return;
        }
        if let Some(t) = decode_base64(data).and_then(|b| String::from_utf8(b).ok()) {
            self.pending.push(t);
        }
    }
}

/// Base64 padrão, sem dependência; `None` num caractere inválido.
pub fn decode_base64(data: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in data {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// O clipboard do sistema (Wayland ou X11), aberto na primeira cópia e
/// guardado: no X11 o conteúdo é servido pelo processo enquanto ele vive.
#[derive(Default)]
pub struct System {
    cb: Option<arboard::Clipboard>,
    tried: bool,
}

impl System {
    /// Grava o texto; sem acesso ao clipboard, não faz nada.
    pub fn set_text(&mut self, text: &str) -> bool {
        if !self.tried {
            self.tried = true;
            self.cb = arboard::Clipboard::new().ok();
        }
        match self.cb.as_mut() {
            Some(cb) => cb.set_text(text.to_string()).is_ok(),
            None => false,
        }
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    #[test]
    fn base64_decodifica() {
        assert_eq!(decode_base64(b"b2zDoQ==").unwrap(), "olá".as_bytes());
        assert_eq!(decode_base64(b"YWJj").unwrap(), b"abc");
        assert!(decode_base64(b"a*b").is_none());
    }

    #[test]
    fn copia_do_harness_na_fila() {
        let mut p = vt100::Parser::new_with_callbacks(
            std::num::NonZeroU16::new(5).unwrap(),
            std::num::NonZeroU16::new(20).unwrap(),
            0,
            Clip::default(),
        );
        p.process(b"antes\x1b]52;c;b2zDoQ==\x07depois");
        assert_eq!(p.callbacks().pending, vec!["olá"]);
        assert!(p.screen().contents().contains("antesdepois"));
        let big = format!("\x1b]52;c;{}\x07", "A".repeat(MAX_COPY + 4));
        p.callbacks_mut().pending.clear();
        p.process(big.as_bytes());
        assert!(p.callbacks().pending.is_empty());
    }
}
