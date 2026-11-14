//! Colar imagem: lê o clipboard do sistema (X11 ou Wayland) e grava um PNG na
//! pasta de entrada.

use std::path::{Path, PathBuf};

/// Lê a imagem do clipboard: largura, altura e RGBA.
pub fn clipboard_image() -> Result<(usize, usize, Vec<u8>), String> {
    let mut cb =
        arboard::Clipboard::new().map_err(|e| format!("não foi possível ler o clipboard: {e}"))?;
    match cb.get_image() {
        Ok(img) => Ok((img.width, img.height, img.bytes.into_owned())),
        Err(arboard::Error::ContentNotAvailable) => Err("o clipboard não tem imagem".into()),
        Err(e) => Err(format!("não foi possível ler o clipboard: {e}")),
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
        return Err("a imagem do clipboard está vazia".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("pasta de entrada: {e}"))?;
    let mut path = dir.join(format!("colado-{stamp}.png"));
    let mut n = 1;
    while path.exists() {
        n += 1;
        path = dir.join(format!("colado-{stamp}-{n}.png"));
    }
    let f = std::fs::File::create(&path).map_err(|e| format!("gravar {}: {e}", path.display()))?;
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
