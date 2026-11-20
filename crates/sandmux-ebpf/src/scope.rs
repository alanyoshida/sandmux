//! O escopo de um sandbox por BPF LSM, nos kernels com o Landlock abaixo do
//! ABI 6 (o Ubuntu 24.04 com o kernel 6.8): o que o Landlock dá nos kernels
//! novos (o escopo de sockets abstratos e de sinais e, abaixo do ABI 5, a
//! restrição de `ioctl` em dispositivos).
//!
//! Os programas são globais (`BPF_LSM_MAC`). Um processo está num sandbox
//! quando o user namespace dele, ou um ancestral até [`DEPTH`] níveis, está no
//! mapa dos sandboxes registrados. O user namespace é criado por lançamento e
//! só o sandbox está nele; o cgroup não serve, porque uma aba sem os tetos
//! roda no cgroup do terminal.
//!
//! Os campos do kernel são lidos por carga direta (`ldx`) a partir de
//! ponteiros com tipo do BTF: o verificador converte cada uma numa leitura
//! protegida, que dá 0 quando o ponteiro não vale.

use std::os::fd::{AsRawFd, OwnedFd};

use sandmux_spec::trf;

use crate::Error;
use crate::asm::{Alu, Asm, Cond, Insn, Label, R0, R1, R2, R3, R6, R7, R8, R9, R10, Reg, Size};
use crate::btf::{Btf, Kind};
use crate::sys;

/// Níveis de user namespace subidos para achar o sandbox (um namespace
/// aninhado dentro do sandbox continua nele).
pub const DEPTH: usize = 8;

/// Dispositivos da lista de um sandbox, no máximo.
pub const MAX_DEVS: usize = 16;

/// O valor de um sandbox no mapa: as opções, o número de dispositivos, o
/// processo 1 do sandbox (o pid no host: está no user namespace, mas fora do
/// domínio do Landlock, e não recebe sinais do sandbox) e os pares (número
/// do dispositivo, máscara), no formato do kernel (`MAJOR << 20 | MINOR`).
pub const VALUE_SIZE: usize = DEVS_AT + MAX_DEVS * 8;
const INIT_AT: i16 = 8;
const DEVS_AT: usize = 16;

/// Opção: negar `ioctl` em dispositivo fora da lista (o `IOCTL_DEV` do
/// Landlock ABI 5, que falta no ABI 4).
pub const OPT_IOCTL: u32 = 1;

const EPERM: i32 = 1;
const EACCES: i32 = 13;
const S_IFMT: i32 = 0o170000;
const S_IFCHR: i32 = 0o020000;
const S_IFBLK: i32 = 0o060000;

/// Os comandos de `ioctl` que o Landlock deixa passar em qualquer arquivo,
/// mesmo sem o `IOCTL_DEV` (`is_masked_device_ioctl` do kernel 6.12, que
/// define o ABI 5): são tratados pelo VFS, não pelo driver.
pub const IOCTL_ALLOWED: [u32; 14] = [
    0x5451,      // FIOCLEX
    0x5450,      // FIONCLEX
    0x5421,      // FIONBIO
    0x5452,      // FIOASYNC
    0x5460,      // FIOQSIZE
    0xC004_5877, // FIFREEZE
    0xC004_5878, // FITHAW
    0xC020_660B, // FS_IOC_FIEMAP
    0x0000_0002, // FIGETBSZ
    0x4004_9409, // FICLONE
    0x4020_940D, // FICLONERANGE
    0xC018_9436, // FIDEDUPERANGE
    0x8011_1500, // FS_IOC_GETFSUUID
    0x8081_1501, // FS_IOC_GETFSSYSFSPATH
];

/// Os ganchos usados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    UnixStreamConnect,
    UnixMaySend,
    TaskKill,
    FileIoctl,
}

pub const HOOKS: [Hook; 4] = [
    Hook::UnixStreamConnect,
    Hook::UnixMaySend,
    Hook::TaskKill,
    Hook::FileIoctl,
];

impl Hook {
    pub fn name(self) -> &'static str {
        match self {
            Hook::UnixStreamConnect => "unix_stream_connect",
            Hook::UnixMaySend => "unix_may_send",
            Hook::TaskKill => "task_kill",
            Hook::FileIoctl => "file_ioctl",
        }
    }
}

fn off(b: &Btf, s: &str, p: &str) -> Result<i16, Error> {
    let o = b.offset(s, p)?;
    i16::try_from(o).map_err(|_| Error::Btf(format!("{s}.{p} longe demais ({o})")))
}

fn uapi(b: &Btf, name: &str) -> Result<i64, Error> {
    b.enum_value(name).ok_or_else(|| {
        Error::Btf(trf!(
            "o kernel não tem {name} no BTF",
            "the kernel has no {name} in BTF"
        ))
    })
}

/// O que os programas precisam saber do kernel, lido do BTF.
#[derive(Clone, Debug)]
pub struct ScopeLayout {
    prog_type_lsm: u32,
    attach_lsm_mac: u32,
    map_hash: u32,
    fn_current_task_btf: i32,
    fn_map_lookup_elem: i32,
    fn_probe_read_kernel: i32,
    hooks: Vec<(Hook, u32, Vec<String>)>,
    task_cred: i16,
    task_tgid: i16,
    cred_user_ns: i16,
    userns_inum: i16,
    userns_parent: i16,
    unix_addr: i16,
    /// `unix_address.name` mais o `sun_family`: o primeiro byte do caminho.
    unix_path0: i16,
    sock_socket: i16,
    socket_file: i16,
    socket_sk: i16,
    file_cred: i16,
    file_inode: i16,
    inode_mode: i16,
    inode_mode_size: Size,
    inode_rdev: i16,
}

impl ScopeLayout {
    pub fn new(b: &Btf) -> Result<ScopeLayout, Error> {
        let mut hooks = Vec::new();
        for h in HOOKS {
            let (id, params) = b.func_params(&format!("bpf_lsm_{}", h.name()))?;
            hooks.push((h, id, params));
        }
        Ok(ScopeLayout {
            prog_type_lsm: uapi(b, "BPF_PROG_TYPE_LSM")? as u32,
            attach_lsm_mac: uapi(b, "BPF_LSM_MAC")? as u32,
            map_hash: uapi(b, "BPF_MAP_TYPE_HASH")? as u32,
            fn_current_task_btf: uapi(b, "BPF_FUNC_get_current_task_btf")? as i32,
            fn_map_lookup_elem: uapi(b, "BPF_FUNC_map_lookup_elem")? as i32,
            fn_probe_read_kernel: uapi(b, "BPF_FUNC_probe_read_kernel")? as i32,
            hooks,
            task_cred: off(b, "task_struct", "cred")?,
            task_tgid: off(b, "task_struct", "tgid")?,
            cred_user_ns: off(b, "cred", "user_ns")?,
            userns_inum: off(b, "user_namespace", "ns.inum")?,
            userns_parent: off(b, "user_namespace", "parent")?,
            unix_addr: off(b, "unix_sock", "addr")?,
            unix_path0: off(b, "unix_address", "name")? + 2,
            sock_socket: off(b, "sock", "sk_socket")?,
            socket_file: off(b, "socket", "file")?,
            socket_sk: off(b, "socket", "sk")?,
            file_cred: off(b, "file", "f_cred")?,
            file_inode: off(b, "file", "f_inode")?,
            inode_mode: off(b, "inode", "i_mode")?,
            inode_mode_size: Size::of(b.field_size("inode", "i_mode")?)?,
            inode_rdev: off(b, "inode", "i_rdev")?,
        })
    }

    fn hook(&self, h: Hook) -> &(Hook, u32, Vec<String>) {
        self.hooks.iter().find(|x| x.0 == h).expect("gancho lido")
    }

    pub fn btf_id(&self, h: Hook) -> u32 {
        self.hook(h).1
    }

    /// A posição, no contexto, do parâmetro `name` do gancho.
    fn arg(&self, h: Hook, name: &str) -> Result<i16, Error> {
        self.hook(h)
            .2
            .iter()
            .position(|p| p == name)
            .map(|i| (i * 8) as i16)
            .ok_or_else(|| {
                Error::Btf(trf!(
                    "o gancho {} não tem o parâmetro {name}",
                    "hook {} has no parameter {name}",
                    h.name()
                ))
            })
    }
}

/// Acha o sandbox do user namespace em `R9`: sobe até [`DEPTH`] níveis
/// procurando no mapa. Achando, desvia para `found` com o valor em `R7` e o
/// inum do sandbox em `R8`; senão, segue. Usa `R1`, `R2`, `R0` e `R9`.
fn find_sandbox(a: &mut Asm, l: &ScopeLayout, map_fd: i32, found: Label) {
    let miss = a.label();
    let hit = a.label();
    for _ in 0..DEPTH {
        a.jmp_imm(Cond::Eq, R9, 0, miss);
        a.ldx(Size::W, R1, R9, l.userns_inum);
        a.stx(Size::W, R10, -4, R1);
        a.ld_map_fd(R1, map_fd);
        a.mov(R2, R10);
        a.alu_imm(Alu::Add, R2, -4);
        a.call(l.fn_map_lookup_elem);
        a.jmp_imm(Cond::Ne, R0, 0, hit);
        a.ldx(Size::DW, R9, R9, l.userns_parent);
    }
    a.ja(miss);
    a.bind(hit);
    a.mov(R7, R0);
    a.ldx(Size::W, R8, R10, -4);
    a.ja(found);
    a.bind(miss);
}

/// O user namespace em `ns` é o do sandbox (`R8`) ou um de baixo? Desvia
/// para `inside` quando é; senão, segue. Usa `R1` e `ns`.
fn in_sandbox(a: &mut Asm, l: &ScopeLayout, ns: Reg, inside: Label) {
    let outside = a.label();
    for _ in 0..DEPTH {
        a.jmp_imm(Cond::Eq, ns, 0, outside);
        a.ldx(Size::W, R1, ns, l.userns_inum);
        a.jmp_reg(Cond::Eq, R1, R8, inside);
        a.ldx(Size::DW, ns, ns, l.userns_parent);
    }
    a.bind(outside);
}

/// `R9` = o user namespace do processo atual.
fn current_ns(a: &mut Asm, l: &ScopeLayout) {
    a.call(l.fn_current_task_btf);
    a.ldx(Size::DW, R1, R0, l.task_cred);
    a.ldx(Size::DW, R9, R1, l.cred_user_ns);
}

/// Monta o programa de um gancho. `map_fd` é o mapa dos sandboxes.
pub fn build(l: &ScopeLayout, h: Hook, map_fd: i32) -> Result<Vec<Insn>, Error> {
    let mut a = Asm::new();
    let allow = a.label();
    let deny = a.label();
    let found = a.label();
    a.mov(R6, R1);
    let errno = match h {
        Hook::UnixStreamConnect | Hook::UnixMaySend => {
            current_ns(&mut a, l);
            find_sandbox(&mut a, l, map_fd, found);
            a.ja(allow);
            a.bind(found);
            // R1 = o `sock` do outro lado.
            a.ldx(Size::DW, R1, R6, l.arg(h, "other")?);
            if h == Hook::UnixMaySend {
                a.ldx(Size::DW, R1, R1, l.socket_sk);
            }
            a.jmp_imm(Cond::Eq, R1, 0, allow);
            a.mov(R7, R1);
            // Só os endereços abstratos: sem endereço (par de sockets, socket
            // sem nome) ou por caminho, passa. O `unix_sock` vai além do
            // `sock` que o verificador conhece: lido por `probe_read_kernel`.
            a.st(Size::DW, R10, -16, 0);
            a.mov(R1, R10);
            a.alu_imm(Alu::Add, R1, -16);
            a.mov_imm(R2, 8);
            a.mov(R3, R7);
            a.alu_imm(Alu::Add, R3, l.unix_addr as i32);
            a.call(l.fn_probe_read_kernel);
            a.ldx(Size::DW, R3, R10, -16);
            a.jmp_imm(Cond::Eq, R3, 0, allow);
            a.st(Size::DW, R10, -24, 1);
            a.mov(R1, R10);
            a.alu_imm(Alu::Add, R1, -24);
            a.mov_imm(R2, 1);
            a.alu_imm(Alu::Add, R3, l.unix_path0 as i32);
            a.call(l.fn_probe_read_kernel);
            a.jmp_imm(Cond::Ne, R0, 0, allow);
            a.ldx(Size::B, R2, R10, -24);
            a.jmp_imm(Cond::Ne, R2, 0, allow);
            // Quem criou o socket: o user namespace das credenciais do
            // arquivo dele.
            a.ldx(Size::DW, R1, R7, l.sock_socket);
            a.jmp_imm(Cond::Eq, R1, 0, deny);
            a.ldx(Size::DW, R1, R1, l.socket_file);
            a.jmp_imm(Cond::Eq, R1, 0, deny);
            a.ldx(Size::DW, R1, R1, l.file_cred);
            a.jmp_imm(Cond::Eq, R1, 0, deny);
            a.ldx(Size::DW, R9, R1, l.cred_user_ns);
            in_sandbox(&mut a, l, R9, allow);
            a.ja(deny);
            EPERM
        }
        Hook::TaskKill => {
            // As credenciais do gancho só existem nos sinais de E/S
            // assíncrona do USB (`kill_pid_usb`), que não vêm do processo
            // atual: esses passam. Nos outros, o argumento é nulo. O
            // verificador trata os argumentos do gancho como ponteiros que
            // nunca são nulos e apaga a conferência `!= 0` (no kernel 6.8 e
            // no 7.0 a leitura de `cred->user_ns` com `cred` nulo derrubou o
            // kernel): o valor é lido do contexto por `probe_read_kernel`,
            // como número.
            a.st(Size::DW, R10, -16, 0);
            a.mov(R1, R10);
            a.alu_imm(Alu::Add, R1, -16);
            a.mov_imm(R2, 8);
            a.mov(R3, R6);
            a.alu_imm(Alu::Add, R3, l.arg(h, "cred")? as i32);
            a.call(l.fn_probe_read_kernel);
            a.ldx(Size::DW, R1, R10, -16);
            a.jmp_imm(Cond::Ne, R1, 0, allow);
            current_ns(&mut a, l);
            find_sandbox(&mut a, l, map_fd, found);
            a.ja(allow);
            a.bind(found);
            // O alvo: o processo 1 do sandbox nunca; os outros, só dentro.
            a.ldx(Size::DW, R1, R6, l.arg(h, "p")?);
            a.ldx(Size::W, R2, R1, l.task_tgid);
            a.ldx(Size::W, R3, R7, INIT_AT);
            a.jmp_reg(Cond::Eq, R2, R3, deny);
            a.ldx(Size::DW, R1, R6, l.arg(h, "p")?);
            a.ldx(Size::DW, R1, R1, l.task_cred);
            a.jmp_imm(Cond::Eq, R1, 0, deny);
            a.ldx(Size::DW, R9, R1, l.cred_user_ns);
            in_sandbox(&mut a, l, R9, allow);
            a.ja(deny);
            EPERM
        }
        Hook::FileIoctl => {
            current_ns(&mut a, l);
            find_sandbox(&mut a, l, map_fd, found);
            a.ja(allow);
            a.bind(found);
            a.ldx(Size::W, R1, R7, 0);
            a.alu_imm(Alu::And, R1, OPT_IOCTL as i32);
            a.jmp_imm(Cond::Eq, R1, 0, allow);
            // Só dispositivos de caractere e de bloco.
            a.ldx(Size::DW, R1, R6, l.arg(h, "file")?);
            a.ldx(Size::DW, R9, R1, l.file_inode);
            a.jmp_imm(Cond::Eq, R9, 0, allow);
            a.ldx(l.inode_mode_size, R1, R9, l.inode_mode);
            a.alu_imm(Alu::And, R1, S_IFMT);
            let device = a.label();
            a.jmp_imm(Cond::Eq, R1, S_IFCHR, device);
            a.jmp_imm(Cond::Eq, R1, S_IFBLK, device);
            a.ja(allow);
            a.bind(device);
            // Os comandos que o VFS trata (os 32 bits de baixo do argumento).
            a.ldx(Size::DW, R8, R6, l.arg(h, "cmd")?);
            a.alu_imm(Alu::Lsh, R8, 32);
            a.alu_imm(Alu::Rsh, R8, 32);
            for c in IOCTL_ALLOWED {
                a.ld_imm64(R1, c as u64);
                a.jmp_reg(Cond::Eq, R8, R1, allow);
            }
            // A lista do sandbox: `(rdev & máscara) == dispositivo`.
            a.ldx(Size::W, R8, R9, l.inode_rdev);
            a.ldx(Size::W, R9, R7, 4);
            for i in 0..MAX_DEVS {
                a.jmp_imm(Cond::Eq, R9, i as i32, deny);
                a.ldx(Size::W, R1, R7, (DEVS_AT + i * 8) as i16);
                a.ldx(Size::W, R2, R7, (DEVS_AT + 4 + i * 8) as i16);
                a.alu_reg(Alu::And, R2, R8);
                a.jmp_reg(Cond::Eq, R2, R1, allow);
            }
            a.ja(deny);
            EACCES
        }
    };
    a.bind(deny);
    a.mov_imm(R0, -errno);
    a.exit();
    a.bind(allow);
    a.mov_imm(R0, 0);
    a.exit();
    a.finish()
}

/// Um dispositivo da lista: o número no formato do kernel e a máscara.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dev {
    pub dev: u32,
    pub mask: u32,
}

impl Dev {
    /// Um dispositivo exato, pelo número principal e o secundário.
    pub fn exact(major: u32, minor: u32) -> Dev {
        Dev {
            dev: kdev(major, minor),
            mask: u32::MAX,
        }
    }

    /// Todos os dispositivos de um número principal.
    pub fn major(major: u32) -> Dev {
        Dev {
            dev: kdev(major, 0),
            mask: 0xfff0_0000,
        }
    }

    /// O dispositivo de um `st_rdev` do espaço de usuário (`makedev` da
    /// glibc).
    pub fn from_rdev(rdev: u64) -> Dev {
        let major = (((rdev >> 32) & 0xffff_f000) | ((rdev >> 8) & 0xfff)) as u32;
        let minor = (((rdev >> 12) & 0xffff_ff00) | (rdev & 0xff)) as u32;
        Dev::exact(major, minor)
    }
}

/// `MKDEV` do kernel: 12 bits de número principal e 20 de secundário.
pub fn kdev(major: u32, minor: u32) -> u32 {
    (major << 20) | (minor & 0xf_ffff)
}

/// O valor de um sandbox no mapa.
pub fn value(opts: u32, init_pid: u32, devs: &[Dev]) -> Result<[u8; VALUE_SIZE], Error> {
    if devs.len() > MAX_DEVS {
        return Err(Error::Asm(trf!(
            "dispositivos demais na lista ({}, o máximo é {MAX_DEVS})",
            "too many devices in the list ({}, the maximum is {MAX_DEVS})",
            devs.len()
        )));
    }
    let mut v = [0u8; VALUE_SIZE];
    v[0..4].copy_from_slice(&opts.to_ne_bytes());
    v[4..8].copy_from_slice(&(devs.len() as u32).to_ne_bytes());
    v[8..12].copy_from_slice(&init_pid.to_ne_bytes());
    for (i, d) in devs.iter().enumerate() {
        let at = DEVS_AT + i * 8;
        v[at..at + 4].copy_from_slice(&d.dev.to_ne_bytes());
        v[at + 4..at + 8].copy_from_slice(&d.mask.to_ne_bytes());
    }
    Ok(v)
}

/// Os programas carregados e o mapa dos sandboxes registrados.
pub struct Scopes {
    map: OwnedFd,
    _progs: Vec<(OwnedFd, OwnedFd)>,
}

/// Sandboxes no mapa, no máximo (todos os usuários).
pub const MAX_SCOPES: u32 = 4096;

impl Scopes {
    /// Cria o mapa e carrega e prende os quatro programas.
    pub fn load(l: &ScopeLayout) -> Result<Scopes, Error> {
        let map = sys::map_create(&sys::MapSpec {
            map_type: l.map_hash,
            key_size: 4,
            value_size: VALUE_SIZE as u32,
            max_entries: MAX_SCOPES,
            flags: 0,
            name: "sandmux_scopes",
            btf: None,
        })?;
        let mut progs = Vec::new();
        for h in HOOKS {
            let insns = build(l, h, map.as_raw_fd())?;
            let name = format!("sm_{}", h.name());
            let prog = sys::prog_load(&sys::ProgSpec {
                prog_type: l.prog_type_lsm,
                expected_attach_type: l.attach_lsm_mac,
                attach_btf_id: l.btf_id(h),
                name: &name,
                license: crate::prog::LICENSE,
                insns: &insns,
            })?;
            let link = match sys::link_create(&prog, 0, l.attach_lsm_mac) {
                Ok(k) => k,
                Err(_) => sys::raw_tracepoint_open(&prog)?,
            };
            progs.push((prog, link));
        }
        Ok(Scopes { map, _progs: progs })
    }

    /// Registra o user namespace `inum` com as opções e os dispositivos.
    pub fn register(&self, inum: u32, opts: u32, init_pid: u32, devs: &[Dev]) -> Result<(), Error> {
        sys::map_update(
            &self.map,
            &inum.to_ne_bytes(),
            &value(opts, init_pid, devs)?,
            0,
        )
    }

    /// Tira o namespace do registro.
    pub fn unregister(&self, inum: u32) -> Result<(), Error> {
        sys::map_delete(&self.map, &inum.to_ne_bytes())
    }
}

/// Os programas existem no BTF deste kernel? (Os ganchos e os campos.)
pub fn supported(b: &Btf) -> Result<(), Error> {
    ScopeLayout::new(b).map(|_| ())?;
    if b.find(Kind::Struct, "unix_sock").is_none() {
        return Err(Error::Btf("unix_sock".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programas_montam() {
        if !std::path::Path::new(crate::btf::VMLINUX).exists() {
            return;
        }
        let b = Btf::vmlinux().unwrap();
        let l = ScopeLayout::new(&b).unwrap();
        for h in HOOKS {
            let p = build(&l, h, 7).unwrap();
            assert_eq!(p.last().unwrap().code, 0x95, "{h:?}");
            // O mapa dos sandboxes e a procura nele.
            assert!(
                p.iter()
                    .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 7),
                "{h:?}"
            );
            assert!(
                p.iter()
                    .any(|i| i.code == 0x85 && i.imm == l.fn_map_lookup_elem),
                "{h:?}"
            );
            assert!(p.len() < 4096, "{h:?}: {}", p.len());
        }
        // O `ioctl` confere os comandos do VFS e a lista de dispositivos.
        let p = build(&l, Hook::FileIoctl, 7).unwrap();
        assert!(
            p.iter()
                .any(|i| i.code == 0x18 && i.imm == 0xC004_5877u32 as i32)
        );
    }

    #[test]
    fn comandos_de_ioctl_do_landlock() {
        // _IO, _IOR, _IOW e _IOWR do x86_64: direção nos bits 30-31, tamanho
        // nos 16-29, tipo nos 8-15 e número nos 0-7.
        let io =
            |dir: u32, ty: u32, nr: u32, size: u32| (dir << 30) | (size << 16) | (ty << 8) | nr;
        let (w, r) = (1, 2);
        let rw = w | r;
        let want = [
            0x5451,
            0x5450,
            0x5421,
            0x5452,
            0x5460,
            io(rw, b'X' as u32, 119, 4),
            io(rw, b'X' as u32, 120, 4),
            io(rw, b'f' as u32, 11, 32),
            io(0, 0, 2, 0),
            io(w, 0x94, 9, 4),
            io(w, 0x94, 13, 32),
            io(rw, 0x94, 54, 24),
            io(r, 0x15, 0, 17),
            io(r, 0x15, 1, 129),
        ];
        assert_eq!(IOCTL_ALLOWED, want);
    }

    #[test]
    fn numeros_de_dispositivo() {
        assert_eq!(kdev(136, 3), (136 << 20) | 3);
        // makedev(5, 2) da glibc é 0x502; makedev(259, 70000).
        assert_eq!(Dev::from_rdev(0x502), Dev::exact(5, 2));
        let (ma, mi) = (std::hint::black_box(259u64), std::hint::black_box(70000u64));
        let big = ((ma & 0xfff) << 8) | ((ma & !0xfff) << 32) | (mi & 0xff) | ((mi & !0xff) << 12);
        assert_eq!(Dev::from_rdev(big), Dev::exact(259, 70000));
        let m = Dev::major(136);
        assert_eq!(kdev(136, 9) & m.mask, m.dev);
        assert_ne!(kdev(137, 9) & m.mask, m.dev);
        let v = value(OPT_IOCTL, 4321, &[Dev::exact(1, 3), m]).unwrap();
        assert_eq!(&v[0..8], &[1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(&v[8..12], &4321u32.to_ne_bytes());
        assert_eq!(&v[16..20], &kdev(1, 3).to_ne_bytes());
        assert!(value(0, 0, &[Dev::exact(1, 3); 17]).is_err());
    }
}
