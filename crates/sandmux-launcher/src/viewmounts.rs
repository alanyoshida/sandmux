//! As montagens das pastas do registro na visão do agente, pelo `mountinfo`
//! do processo 1: o kernel só manda as aberturas feitas nelas (os arquivos
//! da aba pelo kernel, change 78).

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

/// O máximo de montagens que o mapa do kernel guarda; com mais, a aba fica
/// com o supervisor.
pub const MAX_MOUNTS: usize = 256;

/// Desfaz os escapes octais do `mountinfo` (`\040` é o espaço).
fn unescape(s: &str) -> PathBuf {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = (b[i + 1] - b'0') as u32 * 64
                + (b[i + 2] - b'0') as u32 * 8
                + (b[i + 3] - b'0') as u32;
            out.push(v as u8);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    PathBuf::from(OsString::from_vec(out))
}

/// As montagens (`mnt_id`, ponto de montagem) de um `mountinfo`.
fn parse(info: &str) -> Vec<(u32, PathBuf)> {
    info.lines()
        .filter_map(|l| {
            let mut f = l.split(' ');
            let id = f.next()?.parse().ok()?;
            let point = f.nth(3)?;
            Some((id, unescape(point)))
        })
        .collect()
}

/// Os ids das montagens que guardam as pastas `roots`: a que contém cada
/// pasta e as montadas dentro dela, menos as que ficam em `exclude`.
pub fn view_mounts(info: &str, roots: &[PathBuf], exclude: &[PathBuf]) -> Vec<u32> {
    let mounts = parse(info);
    let out_of = |p: &Path| exclude.iter().any(|e| p.starts_with(e));
    let mut ids = BTreeSet::new();
    for r in roots {
        // A de cima da pilha: entre os pontos mais longos, a última da lista.
        if let Some((id, _)) = mounts
            .iter()
            .filter(|(_, p)| r.starts_with(p))
            .max_by_key(|(_, p)| p.components().count())
        {
            ids.insert(*id);
        }
        for (id, p) in &mounts {
            if p != r && p.starts_with(r) && !out_of(p) {
                ids.insert(*id);
            }
        }
    }
    ids.into_iter().collect()
}

/// O mesmo, lido do processo `pid`.
pub fn of_pid(pid: i32, roots: &[PathBuf], exclude: &[PathBuf]) -> Vec<u32> {
    std::fs::read_to_string(format!("/proc/{pid}/mountinfo"))
        .map(|i| view_mounts(&i, roots, exclude))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "\
20 19 0:40 / / rw,relatime - tmpfs tmpfs rw
21 20 259:2 /usr /usr ro,relatime - ext4 /dev/x ro
22 20 259:2 /home/u/projeto /home/u/projeto rw,relatime - ext4 /dev/x rw
23 22 0:41 / /home/u/projeto/node_modules rw - tmpfs tmpfs rw
24 20 0:42 / /home/u rw - tmpfs tmpfs rw
25 24 259:2 /home/u/.cache /home/u/.cache rw - ext4 /dev/x rw
26 24 259:2 /home/u/.config/minha\\040pasta /home/u/.config/minha\\040pasta rw - ext4 /dev/x rw
27 22 259:2 /home/u/projeto /home/u/projeto rw,relatime - ext4 /dev/x rw
";

    #[test]
    fn montagens_das_pastas_do_registro() {
        let roots = [PathBuf::from("/home/u/projeto"), PathBuf::from("/home/u")];
        let exclude = [PathBuf::from("/home/u/.cache")];
        let ids = view_mounts(INFO, &roots, &exclude);
        // A pasta de trabalho (as duas da pilha: 27 está por cima), o que
        // está montado nela, o HOME e a pasta com espaço; sem o cache, a
        // raiz e o /usr.
        assert_eq!(ids, vec![22, 23, 24, 26, 27]);
    }

    #[test]
    fn a_pasta_sem_montagem_propria_fica_com_a_de_cima() {
        let ids = view_mounts(INFO, &[PathBuf::from("/home/u/projeto/src")], &[]);
        assert_eq!(ids, vec![27]);
    }

    #[test]
    fn escapes_octais() {
        assert_eq!(unescape("a\\040b"), PathBuf::from("a b"));
        assert_eq!(unescape("a\\04"), PathBuf::from("a\\04"));
    }
}
