//! Leitura da fila de eventos (`BPF_MAP_TYPE_RINGBUF`) e o formato dos
//! eventos que os programas do confinamento mandam.
//!
//! A fila é lida por `mmap`: a página do consumidor (com escrita, onde fica a
//! posição lida), a página do produtor e os dados, mapeados duas vezes em
//! seguida para um registro que dá a volta caber inteiro. Cada registro tem
//! um cabeçalho de 8 bytes (o tamanho, com os bits de ocupado e descartado, e
//! o deslocamento de página) e é alinhado em 8 bytes.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};

use sandmux_spec::trf;

use crate::Error;

const BUSY: u32 = 1 << 31;
const DISCARD: u32 = 1 << 30;
const HDR: usize = 8;

/// Tamanho dos dados da fila de cada confinamento.
pub const RING_SIZE: u32 = 256 * 1024;
/// A fila de uma aba confinada pelo kernel, com os eventos de abertura.
pub const RING_SIZE_TAB: u32 = 1024 * 1024;

/// Tipos de evento.
pub const EV_DENY: u32 = 1;
pub const EV_CREATE: u32 = 2;
pub const EV_EXEC: u32 = 3;
pub const EV_CONN: u32 = 4;
/// Uma abertura de arquivo numa montagem registrada: o caminho no nome, o
/// `mnt_id` no lugar do `dev`.
pub const EV_OPEN: u32 = 5;

/// Tamanho do evento de conexão.
pub const CONN_EVENT: usize = 48;

/// Uma conexão TCP aberta por um processo observado (no `SYN_SENT`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    pub pid: u32,
    pub sport: u16,
    pub dport: u16,
    pub ip: std::net::IpAddr,
}

impl Conn {
    /// Lê o evento: `kind`, a família, a porta de origem (ordem da máquina),
    /// a de destino (ordem da rede), o pid, o IPv4 e o IPv6.
    pub fn decode(b: &[u8]) -> Option<Conn> {
        if b.len() < CONN_EVENT || u32_at(b, 0) != EV_CONN {
            return None;
        }
        let family = u32_at(b, 4) as i32;
        let ip = if family == libc::AF_INET6 {
            let mut a = [0u8; 16];
            a.copy_from_slice(&b[32..48]);
            std::net::IpAddr::from(a)
        } else {
            std::net::IpAddr::from([b[24], b[25], b[26], b[27]])
        };
        let dport_be = u32_at(b, 12) as u16;
        Some(Conn {
            pid: u64::from_le_bytes(b[16..24].try_into().ok()?) as u32,
            sport: u32_at(b, 8) as u16,
            dport: u16::from_be(dport_be),
            ip,
        })
    }
}

/// Bytes dos argumentos de uma execução, depois do caminho.
pub const EXEC_ARGS: usize = 1024;

/// Tamanho do cabeçalho de um evento e do nome que vem depois dele.
pub const EV_HEADER: usize = 32;
pub const EV_NAME: usize = 256;

/// Um evento de um programa do confinamento.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// [`EV_DENY`] ou [`EV_CREATE`].
    pub kind: u32,
    /// Posição do gancho em [`crate::prog::HOOKS`].
    pub hook: u32,
    /// Os bits da marca que negaram (bloqueios).
    pub bits: u32,
    /// A abertura negada era para escrita.
    pub write: bool,
    /// O inode marcado que negou, ou o diretório da criação.
    pub ino: u64,
    /// O `s_dev` do superbloco do diretório (criações).
    pub dev: u32,
    /// O nome do arquivo criado (criações) ou o caminho executado
    /// (execuções).
    pub name: String,
    /// Os argumentos de uma execução.
    pub args: Vec<String>,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"))
}

impl Event {
    /// Lê um evento do formato que os programas gravam: `kind`, `hook`,
    /// `bits`, `write` (u32 cada), `ino` (u64), `dev`, o tamanho do nome com o
    /// NUL (u32) e, nas criações, o nome.
    pub fn decode(b: &[u8]) -> Option<Event> {
        if b.len() < EV_HEADER {
            return None;
        }
        let kind = u32_at(b, 0);
        if kind != EV_DENY && kind != EV_CREATE && kind != EV_EXEC && kind != EV_OPEN {
            return None;
        }
        let len = u32_at(b, 28) as i32;
        let name = if kind != EV_DENY && len > 1 {
            let end = (EV_HEADER + len as usize - 1).min(b.len());
            String::from_utf8_lossy(&b[EV_HEADER..end]).into_owned()
        } else {
            String::new()
        };
        let args = if kind == EV_EXEC {
            let n = (u32_at(b, 24) as usize).min(EXEC_ARGS);
            let start = EV_HEADER + EV_NAME;
            let end = (start + n).min(b.len());
            b.get(start..end)
                .unwrap_or(&[])
                .split(|&c| c == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect()
        } else {
            Vec::new()
        };
        Some(Event {
            kind,
            hook: u32_at(b, 4),
            bits: u32_at(b, 8),
            write: u32_at(b, 12) != 0,
            ino: u64::from_le_bytes(b[16..24].try_into().expect("8 bytes")),
            dev: u32_at(b, 24),
            name,
            args,
        })
    }
}

/// Lê os registros entre `cons` e `prod` em `data` (os dados mapeados duas
/// vezes, com `mask` = tamanho - 1). Para num registro ainda ocupado. Devolve
/// os registros e a nova posição do consumidor.
pub fn consume(data: &[u8], mask: u64, mut cons: u64, prod: u64) -> (Vec<Vec<u8>>, u64) {
    let mut out = Vec::new();
    while cons < prod {
        let at = (cons & mask) as usize;
        if at + HDR > data.len() {
            break;
        }
        let len_word = u32_at(data, at);
        if len_word & BUSY != 0 {
            break;
        }
        let len = (len_word & !(BUSY | DISCARD)) as usize;
        if len_word & DISCARD == 0 && at + HDR + len <= data.len() {
            out.push(data[at + HDR..at + HDR + len].to_vec());
        }
        cons += ((len + HDR + 7) & !7) as u64;
    }
    (out, cons)
}

/// Uma fila aberta para leitura.
pub struct Reader {
    fd: OwnedFd,
    epoll: OwnedFd,
    cons: *mut u8,
    prod: *mut u8,
    page: usize,
    size: usize,
}

// SAFETY: os ponteiros são de mapeamentos deste leitor, usados por uma thread
// de cada vez (o leitor não é Sync).
unsafe impl Send for Reader {}

impl Reader {
    /// Mapeia a fila `fd`, de `size` bytes de dados.
    pub fn new(fd: OwnedFd, size: u32) -> Result<Reader, Error> {
        let size = size as usize;
        // SAFETY: sysconf não falha para _SC_PAGESIZE.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let err = |what: &str| Error::Sys {
            what: what.into(),
            err: std::io::Error::last_os_error(),
            log: String::new(),
        };
        // SAFETY: mapeamentos do descritor da fila, nos tamanhos que o kernel
        // define para ela; conferidos contra MAP_FAILED.
        let (cons, prod) = unsafe {
            let cons = libc::mmap(
                std::ptr::null_mut(),
                page,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            );
            if cons == libc::MAP_FAILED {
                return Err(err("mmap da fila (consumidor)"));
            }
            let prod = libc::mmap(
                std::ptr::null_mut(),
                page + 2 * size,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                page as libc::off_t,
            );
            if prod == libc::MAP_FAILED {
                libc::munmap(cons, page);
                return Err(err("mmap da fila (produtor)"));
            }
            (cons.cast::<u8>(), prod.cast::<u8>())
        };
        // SAFETY: epoll_create1 devolve um descritor novo ou -1.
        let ep = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if ep < 0 {
            return Err(err("epoll_create1"));
        }
        // SAFETY: descritor recém-criado.
        let epoll = unsafe { std::os::fd::FromRawFd::from_raw_fd(ep) };
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: 0,
        };
        // SAFETY: registro do descritor da fila no epoll.
        if unsafe { libc::epoll_ctl(ep, libc::EPOLL_CTL_ADD, fd.as_raw_fd(), &mut ev) } < 0 {
            return Err(err("epoll_ctl"));
        }
        Ok(Reader {
            fd,
            epoll,
            cons,
            prod,
            page,
            size,
        })
    }

    /// Espera até `timeout_ms` por registros e devolve os que chegaram.
    pub fn poll(&mut self, timeout_ms: i32) -> Vec<Vec<u8>> {
        let mut ev = libc::epoll_event { events: 0, u64: 0 };
        // SAFETY: espera num epoll nosso, com um evento de saída.
        unsafe { libc::epoll_wait(self.epoll.as_raw_fd(), &mut ev, 1, timeout_ms) };
        // SAFETY: as posições ficam no começo das páginas mapeadas, alinhadas
        // em 8; os dados (2 * size) vêm logo depois da página do produtor.
        let (cons_pos, prod_pos, data) = unsafe {
            let c = &*(self.cons as *const AtomicU64);
            let p = &*(self.prod as *const AtomicU64);
            let data = std::slice::from_raw_parts(self.prod.add(self.page), 2 * self.size);
            (c, p.load(Ordering::Acquire), data)
        };
        let (out, next) = consume(
            data,
            self.size as u64 - 1,
            cons_pos.load(Ordering::Relaxed),
            prod_pos,
        );
        cons_pos.store(next, Ordering::Release);
        out
    }

    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        // SAFETY: os mapeamentos feitos em `new`, nos mesmos tamanhos.
        unsafe {
            libc::munmap(self.cons.cast(), self.page);
            libc::munmap(self.prod.cast(), self.page + 2 * self.size);
        }
    }
}

/// Mensagem de erro de um evento que não se lê (para o registro do
/// auxiliar).
pub fn bad_event(len: usize) -> String {
    trf!(
        "evento do BPF ilegível ({len} bytes)",
        "unreadable BPF event ({len} bytes)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(buf: &mut Vec<u8>, payload: &[u8], flags: u32) {
        buf.extend_from_slice(&(payload.len() as u32 | flags).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(payload);
        while !buf.len().is_multiple_of(8) {
            buf.push(0);
        }
    }

    #[test]
    fn le_registros_e_para_no_ocupado() {
        let mut d = Vec::new();
        rec(&mut d, b"abc", 0);
        rec(&mut d, b"descartado", DISCARD);
        rec(&mut d, b"defgh", 0);
        let busy_at = d.len() as u64;
        rec(&mut d, b"ocupado", BUSY);
        let prod = d.len() as u64;
        d.resize(1024, 0);
        let (out, cons) = consume(&d, 1023, 0, prod);
        assert_eq!(out, vec![b"abc".to_vec(), b"defgh".to_vec()]);
        assert_eq!(cons, busy_at);
        // Nada novo: nada lido, posição igual.
        assert_eq!(consume(&d, 1023, cons, cons), (vec![], cons));
    }

    #[test]
    fn evento_de_criacao_e_de_bloqueio() {
        let mut b = vec![0u8; EV_HEADER + EV_NAME];
        b[0..4].copy_from_slice(&EV_CREATE.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[16..24].copy_from_slice(&4242u64.to_le_bytes());
        b[24..28].copy_from_slice(&0x0080_0002u32.to_le_bytes());
        b[28..32].copy_from_slice(&10u32.to_le_bytes());
        b[32..41].copy_from_slice(b"saida.txt");
        let e = Event::decode(&b).unwrap();
        assert_eq!((e.kind, e.ino, e.dev), (EV_CREATE, 4242, 0x0080_0002));
        assert_eq!(e.name, "saida.txt");
        let mut d = vec![0u8; EV_HEADER];
        d[0..4].copy_from_slice(&EV_DENY.to_le_bytes());
        d[8..12].copy_from_slice(&1u32.to_le_bytes());
        d[12..16].copy_from_slice(&1u32.to_le_bytes());
        d[16..24].copy_from_slice(&7u64.to_le_bytes());
        let e = Event::decode(&d).unwrap();
        assert!(e.write && e.bits == 1 && e.ino == 7 && e.name.is_empty());
        assert!(Event::decode(&[0; 8]).is_none());
        let mut c = vec![0u8; CONN_EVENT];
        c[0..4].copy_from_slice(&EV_CONN.to_le_bytes());
        c[4..8].copy_from_slice(&(libc::AF_INET as u32).to_le_bytes());
        c[8..12].copy_from_slice(&40000u32.to_le_bytes());
        c[12..16].copy_from_slice(&u32::from(443u16.to_be()).to_le_bytes());
        c[16..24].copy_from_slice(&321u64.to_le_bytes());
        c[24..28].copy_from_slice(&[1, 1, 1, 1]);
        let k = Conn::decode(&c).unwrap();
        assert_eq!((k.pid, k.sport, k.dport), (321, 40000, 443));
        assert_eq!(k.ip, std::net::IpAddr::from([1, 1, 1, 1]));
        assert!(Event::decode(&c).is_none());
        // Execução: o pid no lugar do inode, o tamanho dos argumentos no do
        // dispositivo.
        let mut x = vec![0u8; EV_HEADER + EV_NAME + EXEC_ARGS];
        x[0..4].copy_from_slice(&EV_EXEC.to_le_bytes());
        x[16..24].copy_from_slice(&777u64.to_le_bytes());
        let argv = b"ls\0-la\0/tmp\0";
        x[24..28].copy_from_slice(&(argv.len() as u32).to_le_bytes());
        x[28..32].copy_from_slice(&12u32.to_le_bytes());
        x[32..43].copy_from_slice(b"/usr/bin/ls");
        x[EV_HEADER + EV_NAME..EV_HEADER + EV_NAME + argv.len()].copy_from_slice(argv);
        let e = Event::decode(&x).unwrap();
        assert_eq!(
            (e.kind, e.ino, e.name.as_str()),
            (EV_EXEC, 777, "/usr/bin/ls")
        );
        assert_eq!(e.args, ["ls", "-la", "/tmp"]);
        assert!(Event::decode(&[9; EV_HEADER]).is_none());
    }
}
