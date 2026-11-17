//! A chamada `bpf()`, só com os comandos que o confinamento usa.
//!
//! As estruturas `bpf_attr` seguem a UAPI (`include/uapi/linux/bpf.h`). Cada
//! comando recebe uma área zerada de 128 bytes com os campos dele; o kernel
//! confere que o resto está zerado.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use crate::Error;
use crate::asm::{Insn, encode};

const BPF_MAP_CREATE: i64 = 0;
const BPF_MAP_LOOKUP_ELEM: i64 = 1;
const BPF_MAP_UPDATE_ELEM: i64 = 2;
const BPF_MAP_DELETE_ELEM: i64 = 3;
const BPF_PROG_LOAD: i64 = 5;
const BPF_BTF_LOAD: i64 = 18;
const BPF_LINK_CREATE: i64 = 28;

/// `BPF_F_NO_PREALLOC`, obrigatório nos mapas de armazenamento local.
pub const BPF_F_NO_PREALLOC: u32 = 1;

const ATTR: usize = 128;
const LOG: usize = 1 << 20;

#[repr(C, align(8))]
struct Attr([u8; ATTR]);

impl Attr {
    fn new() -> Attr {
        Attr([0; ATTR])
    }
    fn u32(&mut self, off: usize, v: u32) -> &mut Attr {
        self.0[off..off + 4].copy_from_slice(&v.to_ne_bytes());
        self
    }
    fn u64(&mut self, off: usize, v: u64) -> &mut Attr {
        self.0[off..off + 8].copy_from_slice(&v.to_ne_bytes());
        self
    }
    fn name(&mut self, off: usize, n: &str) -> &mut Attr {
        let b = n.as_bytes();
        let k = b.len().min(15);
        self.0[off..off + k].copy_from_slice(&b[..k]);
        self
    }
}

fn bpf(cmd: i64, attr: &mut Attr) -> std::io::Result<i64> {
    // SAFETY: a área tem o tamanho passado e vive até a volta da chamada; os
    // ponteiros dentro dela apontam para buffers que também vivem até lá.
    let r = unsafe {
        libc::syscall(
            libc::SYS_bpf,
            cmd,
            attr.0.as_mut_ptr(),
            ATTR as libc::c_uint,
        )
    };
    if r < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

fn fd(r: i64) -> OwnedFd {
    // SAFETY: o kernel acabou de criar o descritor, e só nós o temos.
    unsafe { OwnedFd::from_raw_fd(r as RawFd) }
}

/// Carrega um BTF de tipos (para dar tipo à chave e ao valor de um mapa).
pub fn btf_load(btf: &[u8]) -> Result<OwnedFd, Error> {
    let mut log = vec![0u8; 64 * 1024];
    let mut a = Attr::new();
    a.u64(0, btf.as_ptr() as u64)
        .u64(8, log.as_mut_ptr() as u64)
        .u32(16, btf.len() as u32)
        .u32(20, log.len() as u32)
        .u32(24, 1);
    bpf(BPF_BTF_LOAD, &mut a).map(fd).map_err(|e| Error::Sys {
        what: "BTF_LOAD".into(),
        err: e,
        log: cstr(&log),
    })
}

/// Parâmetros de um mapa novo.
pub struct MapSpec<'a> {
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub flags: u32,
    pub name: &'a str,
    /// BTF, id do tipo da chave e id do tipo do valor.
    pub btf: Option<(RawFd, u32, u32)>,
}

pub fn map_create(m: &MapSpec) -> Result<OwnedFd, Error> {
    let mut a = Attr::new();
    a.u32(0, m.map_type)
        .u32(4, m.key_size)
        .u32(8, m.value_size)
        .u32(12, m.max_entries)
        .u32(16, m.flags)
        .name(28, m.name);
    if let Some((btf, k, v)) = m.btf {
        a.u32(48, btf as u32).u32(52, k).u32(56, v);
    }
    bpf(BPF_MAP_CREATE, &mut a).map(fd).map_err(|e| Error::Sys {
        what: format!("MAP_CREATE {}", m.name),
        err: e,
        log: String::new(),
    })
}

/// Grava `value` na chave `key` (os dois como bytes).
pub fn map_update(map: &impl AsRawFd, key: &[u8], value: &[u8], flags: u64) -> Result<(), Error> {
    let mut a = Attr::new();
    a.u32(0, map.as_raw_fd() as u32)
        .u64(8, key.as_ptr() as u64)
        .u64(16, value.as_ptr() as u64)
        .u64(24, flags);
    bpf(BPF_MAP_UPDATE_ELEM, &mut a)
        .map(|_| ())
        .map_err(|e| Error::Sys {
            what: "MAP_UPDATE_ELEM".into(),
            err: e,
            log: String::new(),
        })
}

/// Lê o valor da chave `key` para `value`; `false` se a chave não existe.
pub fn map_lookup(map: &impl AsRawFd, key: &[u8], value: &mut [u8]) -> Result<bool, Error> {
    let mut a = Attr::new();
    a.u32(0, map.as_raw_fd() as u32)
        .u64(8, key.as_ptr() as u64)
        .u64(16, value.as_mut_ptr() as u64);
    match bpf(BPF_MAP_LOOKUP_ELEM, &mut a) {
        Ok(_) => Ok(true),
        Err(e) if e.raw_os_error() == Some(libc::ENOENT) => Ok(false),
        Err(e) => Err(Error::Sys {
            what: "MAP_LOOKUP_ELEM".into(),
            err: e,
            log: String::new(),
        }),
    }
}

pub fn map_delete(map: &impl AsRawFd, key: &[u8]) -> Result<(), Error> {
    let mut a = Attr::new();
    a.u32(0, map.as_raw_fd() as u32).u64(8, key.as_ptr() as u64);
    bpf(BPF_MAP_DELETE_ELEM, &mut a)
        .map(|_| ())
        .map_err(|e| Error::Sys {
            what: "MAP_DELETE_ELEM".into(),
            err: e,
            log: String::new(),
        })
}

/// Parâmetros de um programa novo.
pub struct ProgSpec<'a> {
    pub prog_type: u32,
    pub expected_attach_type: u32,
    pub attach_btf_id: u32,
    pub name: &'a str,
    pub license: &'a str,
    pub insns: &'a [Insn],
}

/// Carrega o programa. No erro, devolve o fim do log do verificador.
pub fn prog_load(p: &ProgSpec) -> Result<OwnedFd, Error> {
    let code = encode(p.insns);
    let license = std::ffi::CString::new(p.license).expect("licença sem NUL");
    let mut log = vec![0u8; LOG];
    let mut a = Attr::new();
    a.u32(0, p.prog_type)
        .u32(4, p.insns.len() as u32)
        .u64(8, code.as_ptr() as u64)
        .u64(16, license.as_ptr() as u64)
        .u32(24, 1)
        .u32(28, log.len() as u32)
        .u64(32, log.as_mut_ptr() as u64)
        .name(48, p.name)
        .u32(68, p.expected_attach_type)
        .u32(108, p.attach_btf_id);
    bpf(BPF_PROG_LOAD, &mut a).map(fd).map_err(|e| Error::Sys {
        what: format!("PROG_LOAD {}", p.name),
        err: e,
        log: tail(&cstr(&log), 40),
    })
}

/// Prende o programa a um alvo (para o `BPF_LSM_CGROUP`, o cgroup). O link
/// some quando o descritor devolvido fecha.
pub fn link_create(prog: &impl AsRawFd, target: RawFd, attach_type: u32) -> Result<OwnedFd, Error> {
    let mut a = Attr::new();
    a.u32(0, prog.as_raw_fd() as u32)
        .u32(4, target as u32)
        .u32(8, attach_type);
    bpf(BPF_LINK_CREATE, &mut a)
        .map(fd)
        .map_err(|e| Error::Sys {
            what: "LINK_CREATE".into(),
            err: e,
            log: String::new(),
        })
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_zerada_com_os_campos() {
        let mut a = Attr::new();
        a.u32(4, 7)
            .u64(8, 0x0102_0304_0506_0708)
            .name(28, "nome-comprido-demais-aqui");
        assert_eq!(&a.0[4..8], &7u32.to_ne_bytes());
        assert_eq!(&a.0[8..16], &0x0102_0304_0506_0708u64.to_ne_bytes());
        // O nome é cortado em 15 bytes, com o NUL no 16º.
        assert_eq!(&a.0[28..43], b"nome-comprido-d");
        assert_eq!(a.0[43], 0);
        assert!(a.0[44..].iter().all(|&b| b == 0));
    }

    #[test]
    fn sem_privilegio_da_erro_e_nao_trava() {
        // Sem CAP_BPF, o kernel recusa; o erro chega com o nome do comando.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let e = btf_load(&crate::btf::int_btf()).unwrap_err();
        assert!(e.to_string().contains("BTF_LOAD"), "{e}");
    }
}
