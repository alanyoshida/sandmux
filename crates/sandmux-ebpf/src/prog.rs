//! Os programas BPF LSM do confinamento e a carga deles.
//!
//! Cada confinamento tem um mapa de armazenamento por inode só dele, com um
//! conjunto de bits por inode marcado ([`DENY`], [`PROTECT`], [`GITDIR`]), e
//! um programa por gancho. A decisão é pelo inode, e não pelo caminho:
//! desmontar uma cobertura ou tirar um somente-leitura não muda o resultado.
//!
//! O verificador só aceita no `bpf_inode_storage_get` ponteiros confiáveis:
//! os argumentos do gancho, o `f_inode` de um `file` e o `d_inode` de uma
//! `dentry` recebida. Subir pela árvore (`d_parent`) dá ponteiros não
//! confiáveis, recusados. Por isso cada programa olha o próprio inode e o
//! diretório que o gancho recebe, e quem pede o confinamento marca também
//! tudo o que existe dentro das pastas negadas e protegidas.

use std::os::fd::{AsRawFd, OwnedFd, RawFd};

use sandmux_spec::trf;

use crate::Error;
use crate::asm::{Alu, Asm, Cond, Insn, Label, R0, R1, R2, R3, R4, R6, R7, R8, R9, R10, Reg, Size};
use crate::btf::Btf;
use crate::sys;

/// Negado à leitura e à escrita; num diretório, nada é criado, apagado nem
/// renomeado dentro dele.
pub const DENY: u32 = 1;
/// Protegido contra escrita; num diretório, nada é criado, apagado nem
/// renomeado dentro dele.
pub const PROTECT: u32 = 2;
/// Diretório git: nomes reservados não podem ser criados nele, e ele mesmo
/// não pode ser apagado nem renomeado.
pub const GITDIR: u32 = 4;

/// Segredo de uma aba: a abertura devolve `ENOENT`, como o segredo escondido
/// do supervisor; num diretório, nada entra nem sai (`EACCES`).
pub const SECRET: u32 = 8;
/// Negado pela política de arquivos de uma aba (`file-deny`): `EACCES`.
pub const FDENY: u32 = 16;

/// Nomes que não podem ser criados num diretório git.
pub const GIT_RESERVED: [&str; 4] = ["commondir", "hooks", "config", "config.worktree"];

const FMODE_WRITE: i32 = 0x2;

/// Os ganchos usados e o parâmetro que cada um examina.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    FileOpen,
    InodeCreate,
    InodeMkdir,
    InodeMknod,
    InodeSymlink,
    InodeLink,
    InodeUnlink,
    InodeRmdir,
    InodeRename,
    InodeSetattr,
}

pub const HOOKS: [Hook; 10] = [
    Hook::FileOpen,
    Hook::InodeCreate,
    Hook::InodeMkdir,
    Hook::InodeMknod,
    Hook::InodeSymlink,
    Hook::InodeLink,
    Hook::InodeUnlink,
    Hook::InodeRmdir,
    Hook::InodeRename,
    Hook::InodeSetattr,
];

impl Hook {
    pub fn name(self) -> &'static str {
        match self {
            Hook::FileOpen => "file_open",
            Hook::InodeCreate => "inode_create",
            Hook::InodeMkdir => "inode_mkdir",
            Hook::InodeMknod => "inode_mknod",
            Hook::InodeSymlink => "inode_symlink",
            Hook::InodeLink => "inode_link",
            Hook::InodeUnlink => "inode_unlink",
            Hook::InodeRmdir => "inode_rmdir",
            Hook::InodeRename => "inode_rename",
            Hook::InodeSetattr => "inode_setattr",
        }
    }
}

/// O que os programas precisam saber do kernel, lido do BTF.
#[derive(Clone, Debug)]
pub struct Layout {
    pub prog_type_lsm: u32,
    pub attach_lsm_cgroup: u32,
    pub attach_lsm_mac: u32,
    pub map_inode_storage: u32,
    pub fn_inode_storage_get: i32,
    pub fn_probe_read_kernel: i32,
    pub fn_ancestor_cgroup_id: i32,
    pub fn_ringbuf_output: i32,
    pub fn_probe_read_kernel_str: i32,
    pub fn_map_lookup_elem: i32,
    pub fn_set_retval: i32,
    pub fn_d_path: i32,
    pub map_ringbuf: u32,
    pub map_array: u32,
    pub map_hash: u32,
    inode_ino: i16,
    inode_ino_size: Size,
    inode_sb: i16,
    sb_dev: i16,
    sb_dev_size: Size,
    file_inode: i16,
    file_mode: i16,
    file_mode_size: Size,
    file_path: i16,
    path_mnt: i16,
    /// Do `vfsmount` (dentro da `struct mount`) até o `mnt_id`.
    vfsmount_to_mnt_id: i32,
    d_inode: i16,
    d_name_name: i16,
    d_name_len: i16,
    d_name_len_size: Size,
    hooks: Vec<(Hook, u32, Vec<String>)>,
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

impl Layout {
    pub fn new(b: &Btf) -> Result<Layout, Error> {
        let mut hooks = Vec::new();
        for h in HOOKS {
            let (id, params) = b.func_params(&format!("bpf_lsm_{}", h.name()))?;
            hooks.push((h, id, params));
        }
        Ok(Layout {
            prog_type_lsm: uapi(b, "BPF_PROG_TYPE_LSM")? as u32,
            attach_lsm_cgroup: uapi(b, "BPF_LSM_CGROUP")? as u32,
            attach_lsm_mac: uapi(b, "BPF_LSM_MAC")? as u32,
            map_inode_storage: uapi(b, "BPF_MAP_TYPE_INODE_STORAGE")? as u32,
            fn_inode_storage_get: uapi(b, "BPF_FUNC_inode_storage_get")? as i32,
            fn_probe_read_kernel: uapi(b, "BPF_FUNC_probe_read_kernel")? as i32,
            fn_ancestor_cgroup_id: uapi(b, "BPF_FUNC_get_current_ancestor_cgroup_id")? as i32,
            fn_ringbuf_output: uapi(b, "BPF_FUNC_ringbuf_output")? as i32,
            fn_probe_read_kernel_str: uapi(b, "BPF_FUNC_probe_read_kernel_str")? as i32,
            fn_map_lookup_elem: uapi(b, "BPF_FUNC_map_lookup_elem")? as i32,
            fn_set_retval: uapi(b, "BPF_FUNC_set_retval")? as i32,
            fn_d_path: uapi(b, "BPF_FUNC_d_path")? as i32,
            map_ringbuf: uapi(b, "BPF_MAP_TYPE_RINGBUF")? as u32,
            map_array: uapi(b, "BPF_MAP_TYPE_ARRAY")? as u32,
            map_hash: uapi(b, "BPF_MAP_TYPE_HASH")? as u32,
            inode_ino: off(b, "inode", "i_ino")?,
            inode_ino_size: Size::of(b.field_size("inode", "i_ino")?)?,
            inode_sb: off(b, "inode", "i_sb")?,
            sb_dev: off(b, "super_block", "s_dev")?,
            sb_dev_size: Size::of(b.field_size("super_block", "s_dev")?)?,
            file_inode: off(b, "file", "f_inode")?,
            file_mode: off(b, "file", "f_mode")?,
            file_mode_size: Size::of(b.field_size("file", "f_mode")?)?,
            file_path: off(b, "file", "f_path")?,
            path_mnt: off(b, "path", "mnt")?,
            vfsmount_to_mnt_id: b.offset("mount", "mnt_id")? as i32
                - b.offset("mount", "mnt")? as i32,
            d_inode: off(b, "dentry", "d_inode")?,
            d_name_name: off(b, "dentry", "d_name.name")?,
            d_name_len: off(b, "dentry", "d_name.len")?,
            d_name_len_size: Size::of(b.field_size("dentry", "d_name.len")?)?,
            hooks,
        })
    }

    fn hook(&self, h: Hook) -> &(Hook, u32, Vec<String>) {
        self.hooks.iter().find(|x| x.0 == h).expect("gancho lido")
    }

    /// O id do BTF do gancho (o `attach_btf_id`).
    pub fn btf_id(&self, h: Hook) -> u32 {
        self.hook(h).1
    }

    /// A posição do parâmetro `name` do gancho.
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

/// Como os programas são presos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach {
    /// `BPF_LSM_CGROUP`: só os processos do cgroup e dos de baixo; o programa
    /// devolve 1 para seguir e 0 para negar.
    Cgroup,
    /// `BPF_LSM_MAC`, global: o programa confere se o processo está abaixo do
    /// cgroup (`nível`, `id`) e devolve 0 ou `-EPERM`.
    Global { level: i32, cgroup_id: u64 },
}

/// Os mapas dos eventos: a fila e o contador de descartados.
#[derive(Clone, Copy, Debug)]
pub struct EventMaps {
    pub ring_fd: i32,
    pub drops_fd: i32,
    /// O `s_dev` do superbloco da pasta de trabalho, no formato do kernel; 0
    /// manda as criações de qualquer sistema de arquivos.
    pub workdir_dev: u32,
    /// O mapa dos `mnt_id` cujas aberturas vão para a fila (o registro de
    /// arquivos de uma aba); `None`, sem eventos de abertura.
    pub open_mnts_fd: Option<i32>,
}

/// Onde o evento é montado na pilha, e a chave do contador.
const EV: i16 = -320;
const KEY: i16 = -328;
const MNT: i16 = -336;

struct Gen<'a> {
    a: Asm,
    l: &'a Layout,
    map_fd: i32,
    deny: Label,
    /// Negação com `ENOENT` (segredo) e com `EACCES` (`file-deny`).
    deny_noent: Label,
    deny_acces: Label,
    /// Os finais acima foram usados (o verificador recusa código
    /// inalcançável: só os usados são montados).
    used_noent: bool,
    used_acces: bool,
    ev: Option<EventMaps>,
    hook: i32,
}

impl Gen<'_> {
    /// Nega se o inode em `inode` (ponteiro confiável, talvez nulo) tem algum
    /// bit de `mask`.
    fn check(&mut self, inode: Reg, mask: u32, write: bool) {
        let to = self.deny;
        self.check_to(inode, mask, write, to);
    }

    /// As marcas das abas primeiro (`SECRET` com `ENOENT`, quando `secret`, e
    /// `FDENY` com `EACCES`), depois as do Docker em `mask` (`EPERM`).
    fn check_all(&mut self, inode: Reg, mask: u32, write: bool, secret: bool) {
        if secret {
            let to = self.deny_noent;
            self.check_to(inode, SECRET, write, to);
        }
        let to = self.deny_acces;
        let tab = if secret { FDENY } else { FDENY | SECRET };
        self.check_to(inode, tab, write, to);
        self.check(inode, mask, write);
    }

    fn check_to(&mut self, inode: Reg, mask: u32, write: bool, to: Label) {
        self.used_noent |= to == self.deny_noent;
        self.used_acces |= to == self.deny_acces;
        let next = self.a.label();
        self.a.jmp_imm(Cond::Eq, inode, 0, next);
        self.a.mov(R2, inode);
        self.a.ld_map_fd(R1, self.map_fd);
        self.a.mov_imm(R3, 0);
        self.a.mov_imm(R4, 0);
        self.a.call(self.l.fn_inode_storage_get);
        self.a.jmp_imm(Cond::Eq, R0, 0, next);
        self.a.ldx(Size::W, R1, R0, 0);
        self.a.alu_imm(Alu::And, R1, mask as i32);
        self.a.jmp_imm(Cond::Eq, R1, 0, next);
        self.deny_event(inode, write);
        self.a.ja(to);
        self.a.bind(next);
    }

    /// Manda o evento de abertura do arquivo em R8 (confiável; o inode em R7),
    /// só quando a montagem dele está no mapa das registradas: o `mnt_id`, o
    /// caminho por `bpf_d_path` e se grava.
    fn open_event(&mut self, skip: Label) {
        let Some(ev) = self.ev else { return };
        let Some(mnts) = ev.open_mnts_fd else { return };
        let l = self.l;
        let a = &mut self.a;
        // O `mnt_id` da `struct mount` do arquivo (fora do `vfsmount` que o
        // verificador conhece: por `probe_read_kernel`).
        a.ldx(Size::DW, R3, R8, l.file_path + l.path_mnt);
        a.jmp_imm(Cond::Eq, R3, 0, skip);
        a.alu_imm(Alu::Add, R3, l.vfsmount_to_mnt_id);
        a.st(Size::DW, R10, MNT, 0);
        a.mov(R1, R10);
        a.alu_imm(Alu::Add, R1, i32::from(MNT));
        a.mov_imm(R2, 4);
        a.call(l.fn_probe_read_kernel);
        a.jmp_imm(Cond::Ne, R0, 0, skip);
        a.ld_map_fd(R1, mnts);
        a.mov(R2, R10);
        a.alu_imm(Alu::Add, R2, i32::from(MNT));
        a.call(l.fn_map_lookup_elem);
        a.jmp_imm(Cond::Eq, R0, 0, skip);
        a.st(Size::W, R10, EV, crate::ring::EV_OPEN as i32);
        a.st(Size::W, R10, EV + 4, self.hook);
        a.st(Size::W, R10, EV + 8, 0);
        a.ldx(l.file_mode_size, R1, R8, l.file_mode);
        a.alu_imm(Alu::And, R1, FMODE_WRITE);
        a.stx(Size::W, R10, EV + 12, R1);
        a.ldx(l.inode_ino_size, R2, R7, l.inode_ino);
        a.stx(Size::DW, R10, EV + 16, R2);
        a.ldx(Size::W, R1, R10, MNT);
        a.stx(Size::W, R10, EV + 24, R1);
        a.mov(R1, R8);
        a.alu_imm(Alu::Add, R1, i32::from(l.file_path));
        a.mov(R2, R10);
        a.alu_imm(Alu::Add, R2, i32::from(EV) + crate::ring::EV_HEADER as i32);
        a.mov_imm(R3, crate::ring::EV_NAME as i32);
        a.call(l.fn_d_path);
        a.stx(Size::W, R10, EV + 28, R0);
        self.output((crate::ring::EV_HEADER + crate::ring::EV_NAME) as i32);
    }

    /// Manda o evento de bloqueio: os bits em R1, o inode marcado em `inode`
    /// (confiável).
    fn deny_event(&mut self, inode: Reg, write: bool) {
        if self.ev.is_none() {
            return;
        }
        let a = &mut self.a;
        a.st(Size::W, R10, EV, crate::ring::EV_DENY as i32);
        a.st(Size::W, R10, EV + 4, self.hook);
        a.stx(Size::W, R10, EV + 8, R1);
        a.st(Size::W, R10, EV + 12, i32::from(write));
        a.ldx(self.l.inode_ino_size, R2, inode, self.l.inode_ino);
        a.stx(Size::DW, R10, EV + 16, R2);
        a.st(Size::W, R10, EV + 24, 0);
        a.st(Size::W, R10, EV + 28, 0);
        self.output(crate::ring::EV_HEADER as i32);
    }

    /// `bpf_ringbuf_output` do evento montado em `EV`, com `size` bytes; na
    /// falha (fila cheia), soma 1 ao contador.
    fn output(&mut self, size: i32) {
        let Some(ev) = self.ev else { return };
        let done = self.a.label();
        let a = &mut self.a;
        a.ld_map_fd(R1, ev.ring_fd);
        a.mov(R2, R10);
        a.alu_imm(Alu::Add, R2, i32::from(EV));
        a.mov_imm(R3, size);
        a.mov_imm(R4, 0);
        a.call(self.l.fn_ringbuf_output);
        a.jmp_imm(Cond::Eq, R0, 0, done);
        a.st(Size::W, R10, KEY, 0);
        a.ld_map_fd(R1, ev.drops_fd);
        a.mov(R2, R10);
        a.alu_imm(Alu::Add, R2, i32::from(KEY));
        a.call(self.l.fn_map_lookup_elem);
        a.jmp_imm(Cond::Eq, R0, 0, done);
        a.mov_imm(R1, 1);
        a.atomic_add64(R0, 0, R1);
        a.bind(done);
    }

    /// Manda o evento de criação: diretório em R8 (confiável), dentry nova em
    /// R9. Só do sistema de arquivos da pasta de trabalho, quando ele é
    /// conhecido.
    fn create_event(&mut self, skip: Label) {
        let Some(ev) = self.ev else { return };
        let l = self.l;
        let a = &mut self.a;
        a.ldx(Size::DW, R1, R8, l.inode_sb);
        a.jmp_imm(Cond::Eq, R1, 0, skip);
        a.ldx(l.sb_dev_size, R7, R1, l.sb_dev);
        if ev.workdir_dev != 0 {
            a.ld_imm64(R1, u64::from(ev.workdir_dev));
            a.jmp_reg(Cond::Ne, R7, R1, skip);
        }
        a.st(Size::W, R10, EV, crate::ring::EV_CREATE as i32);
        a.st(Size::W, R10, EV + 4, self.hook);
        a.st(Size::W, R10, EV + 8, 0);
        a.st(Size::W, R10, EV + 12, 0);
        a.ldx(l.inode_ino_size, R2, R8, l.inode_ino);
        a.stx(Size::DW, R10, EV + 16, R2);
        a.stx(Size::W, R10, EV + 24, R7);
        a.mov(R1, R10);
        a.alu_imm(Alu::Add, R1, i32::from(EV) + crate::ring::EV_HEADER as i32);
        a.mov_imm(R2, crate::ring::EV_NAME as i32);
        a.ldx(Size::DW, R3, R9, l.d_name_name);
        a.call(l.fn_probe_read_kernel_str);
        a.stx(Size::W, R10, EV + 28, R0);
        self.output((crate::ring::EV_HEADER + crate::ring::EV_NAME) as i32);
    }

    /// Carrega o argumento `name` do gancho (posição `off` no contexto em R6)
    /// em `dst`.
    fn arg(&mut self, dst: Reg, off: i16) {
        self.a.ldx(Size::DW, dst, R6, off);
    }

    /// `d_inode` da dentry em `dentry`, em `dst`.
    fn d_inode(&mut self, dst: Reg, dentry: Reg) {
        self.a.ldx(Size::DW, dst, dentry, self.l.d_inode);
    }

    /// Nega a criação de um nome reservado (dentry em R9) num diretório git
    /// (inode do diretório em R8, confiável).
    fn reserved_names(&mut self) {
        let done = self.a.label();
        self.a.jmp_imm(Cond::Eq, R8, 0, done);
        self.a.mov(R2, R8);
        self.a.ld_map_fd(R1, self.map_fd);
        self.a.mov_imm(R3, 0);
        self.a.mov_imm(R4, 0);
        self.a.call(self.l.fn_inode_storage_get);
        self.a.jmp_imm(Cond::Eq, R0, 0, done);
        self.a.ldx(Size::W, R1, R0, 0);
        self.a.alu_imm(Alu::And, R1, GITDIR as i32);
        self.a.jmp_imm(Cond::Eq, R1, 0, done);
        self.a
            .ldx(self.l.d_name_len_size, R7, R9, self.l.d_name_len);
        for name in GIT_RESERVED {
            let b = name.as_bytes();
            let next = self.a.label();
            let a = &mut self.a;
            a.jmp_imm(Cond::Ne, R7, b.len() as i32, next);
            a.st(Size::DW, R10, -16, 0);
            a.st(Size::DW, R10, -8, 0);
            a.mov(R1, R10);
            a.alu_imm(Alu::Add, R1, -16);
            a.mov_imm(R2, b.len() as i32);
            a.ldx(Size::DW, R3, R9, self.l.d_name_name);
            a.call(self.l.fn_probe_read_kernel);
            // Sem conseguir ler o nome, nega.
            a.jmp_imm(Cond::Ne, R0, 0, self.deny);
            let mut w = [0u8; 16];
            w[..b.len()].copy_from_slice(b);
            for (i, chunk) in w.chunks(8).enumerate() {
                let v = u64::from_le_bytes(chunk.try_into().expect("8 bytes"));
                a.ldx(Size::DW, R1, R10, -16 + 8 * i as i16);
                a.ld_imm64(R2, v);
                a.jmp_reg(Cond::Ne, R1, R2, next);
            }
            self.a.mov_imm(R1, GITDIR as i32);
            self.deny_event(R8, true);
            self.a.ja(self.deny);
            self.a.bind(next);
        }
        self.a.bind(done);
    }
}

/// Monta o programa de um gancho.
pub fn build(
    l: &Layout,
    h: Hook,
    map_fd: i32,
    attach: Attach,
    ev: Option<EventMaps>,
) -> Result<Vec<Insn>, Error> {
    let mut a = Asm::new();
    let allow = a.label();
    let deny = a.label();
    let deny_noent = a.label();
    let deny_acces = a.label();
    let hook = HOOKS.iter().position(|x| *x == h).unwrap_or(0) as i32;
    let mut g = Gen {
        a,
        l,
        map_fd,
        deny,
        deny_noent,
        deny_acces,
        used_noent: false,
        used_acces: false,
        ev,
        hook,
    };
    // R6 = contexto (vetor de argumentos de 64 bits).
    g.a.mov(R6, R1);
    if let Attach::Global { level, cgroup_id } = attach {
        g.a.mov_imm(R1, level);
        g.a.call(l.fn_ancestor_cgroup_id);
        g.a.ld_imm64(R1, cgroup_id);
        g.a.jmp_reg(Cond::Ne, R0, R1, allow);
    }
    // O próprio alvo: negado, protegido ou diretório git. O diretório: negado
    // ou protegido (nada entra nem sai dele).
    let own = DENY | PROTECT | GITDIR;
    let dir = DENY | PROTECT;
    match h {
        Hook::FileOpen => {
            let file = l.arg(h, "file")?;
            g.arg(R8, file);
            g.a.ldx(Size::DW, R7, R8, l.file_inode);
            g.a.ldx(l.file_mode_size, R1, R8, l.file_mode);
            let write = g.a.label();
            let opened = g.a.label();
            g.a.jmp_imm(Cond::Set, R1, FMODE_WRITE, write);
            g.check_all(R7, DENY, false, true);
            g.a.ja(opened);
            g.a.bind(write);
            g.check_all(R7, DENY | PROTECT, true, true);
            g.a.bind(opened);
            g.open_event(allow);
        }
        Hook::InodeCreate | Hook::InodeMkdir | Hook::InodeMknod | Hook::InodeSymlink => {
            g.arg(R8, l.arg(h, "dir")?);
            g.check_all(R8, dir, true, false);
            g.arg(R9, l.arg(h, "dentry")?);
            g.reserved_names();
            if h == Hook::InodeCreate {
                g.create_event(allow);
            }
        }
        Hook::InodeLink => {
            g.arg(R1, l.arg(h, "old_dentry")?);
            g.d_inode(R7, R1);
            g.check_all(R7, own, true, true);
            g.arg(R8, l.arg(h, "dir")?);
            g.check_all(R8, dir, true, false);
            g.arg(R9, l.arg(h, "new_dentry")?);
            g.reserved_names();
        }
        Hook::InodeRename => {
            g.arg(R1, l.arg(h, "old_dentry")?);
            g.d_inode(R7, R1);
            g.check_all(R7, own, true, true);
            g.arg(R7, l.arg(h, "old_dir")?);
            g.check_all(R7, dir, true, false);
            g.arg(R1, l.arg(h, "new_dentry")?);
            g.d_inode(R7, R1);
            g.check_all(R7, own, true, true);
            g.arg(R8, l.arg(h, "new_dir")?);
            g.check_all(R8, dir, true, false);
            g.arg(R9, l.arg(h, "new_dentry")?);
            g.reserved_names();
        }
        Hook::InodeUnlink | Hook::InodeRmdir => {
            g.arg(R7, l.arg(h, "dir")?);
            g.check_all(R7, dir, true, false);
            g.arg(R1, l.arg(h, "dentry")?);
            g.d_inode(R7, R1);
            g.check_all(R7, own, true, true);
        }
        Hook::InodeSetattr => {
            g.arg(R1, l.arg(h, "dentry")?);
            g.d_inode(R7, R1);
            g.check_all(R7, own, true, true);
        }
    }
    let (ok, no) = match attach {
        Attach::Cgroup => (1, 0),
        Attach::Global { .. } => (0, -libc::EPERM),
    };
    g.a.bind(allow);
    g.a.mov_imm(R0, ok);
    g.a.exit();
    g.a.bind(deny);
    g.a.mov_imm(R0, no);
    g.a.exit();
    // Os outros erros: preso a um cgroup, o programa só aceita ou nega, e o
    // erro vem do `bpf_set_retval`; no anexo global, do retorno.
    for (label, errno, used) in [
        (deny_noent, libc::ENOENT, g.used_noent),
        (deny_acces, libc::EACCES, g.used_acces),
    ] {
        if !used {
            continue;
        }
        g.a.bind(label);
        match attach {
            Attach::Cgroup => {
                g.a.mov_imm(R1, -errno);
                g.a.call(l.fn_set_retval);
                g.a.mov_imm(R0, 0);
            }
            Attach::Global { .. } => g.a.mov_imm(R0, -errno),
        }
        g.a.exit();
    }
    g.a.finish()
}

/// Licença declarada ao kernel: compatível com a GPL, que alguns auxiliares
/// do BPF exigem (`bpf_probe_read_kernel`).
pub const LICENSE: &str = "Dual MIT/GPL";

/// Um confinamento carregado: o mapa, os programas presos e os arquivos
/// marcados, mantidos abertos para a marca não se perder.
pub struct Confinement {
    map: OwnedFd,
    ring: OwnedFd,
    drops: OwnedFd,
    ring_size: u32,
    _mnts: Option<OwnedFd>,
    _btf: OwnedFd,
    _progs: Vec<OwnedFd>,
    _links: Vec<OwnedFd>,
    /// Dispositivo, inode e o descritor guardado de cada arquivo marcado.
    marked: Vec<(u64, u64, OwnedFd)>,
    pub attach: Attach,
}

impl Confinement {
    /// Carrega os programas e os prende ao cgroup `cgroup`. Com
    /// `Attach::Global`, o descritor do cgroup não é usado.
    pub fn load(
        l: &Layout,
        cgroup: RawFd,
        attach: Attach,
        workdir_dev: u32,
    ) -> Result<Confinement, Error> {
        Confinement::load_with(l, cgroup, attach, workdir_dev, &[])
    }

    /// Como [`Confinement::load`], com as montagens (`mnt_id`) cujas
    /// aberturas vão para a fila: o registro de arquivos de uma aba. Com
    /// montagens, a fila é maior ([`crate::ring::RING_SIZE_TAB`]).
    pub fn load_with(
        l: &Layout,
        cgroup: RawFd,
        attach: Attach,
        workdir_dev: u32,
        open_mnts: &[u32],
    ) -> Result<Confinement, Error> {
        let ring_size = if open_mnts.is_empty() {
            crate::ring::RING_SIZE
        } else {
            crate::ring::RING_SIZE_TAB
        };
        let btf = sys::btf_load(&crate::btf::int_btf())?;
        let map = sys::map_create(&sys::MapSpec {
            map_type: l.map_inode_storage,
            key_size: 4,
            value_size: 4,
            max_entries: 0,
            flags: sys::BPF_F_NO_PREALLOC,
            name: "sandmux_marks",
            btf: Some((btf.as_raw_fd(), 1, 1)),
        })?;
        let ring = sys::map_create(&sys::MapSpec {
            map_type: l.map_ringbuf,
            key_size: 0,
            value_size: 0,
            max_entries: ring_size,
            flags: 0,
            name: "sandmux_events",
            btf: None,
        })?;
        let mnts = if open_mnts.is_empty() {
            None
        } else {
            let m = sys::map_create(&sys::MapSpec {
                map_type: l.map_hash,
                key_size: 4,
                value_size: 1,
                max_entries: 256,
                flags: 0,
                name: "sandmux_mnts",
                btf: None,
            })?;
            for id in open_mnts.iter().take(256) {
                sys::map_update(&m, &id.to_ne_bytes(), &[1], 0)?;
            }
            Some(m)
        };
        let drops = sys::map_create(&sys::MapSpec {
            map_type: l.map_array,
            key_size: 4,
            value_size: 8,
            max_entries: 1,
            flags: 0,
            name: "sandmux_drops",
            btf: None,
        })?;
        let ev = EventMaps {
            ring_fd: ring.as_raw_fd(),
            drops_fd: drops.as_raw_fd(),
            workdir_dev,
            open_mnts_fd: mnts.as_ref().map(|m| m.as_raw_fd()),
        };
        let mut progs = Vec::new();
        let mut links = Vec::new();
        for h in HOOKS {
            let insns = build(l, h, map.as_raw_fd(), attach, Some(ev))?;
            let (exp, target) = match attach {
                Attach::Cgroup => (l.attach_lsm_cgroup, cgroup),
                Attach::Global { .. } => (l.attach_lsm_mac, 0),
            };
            let name = format!("sm_{}", h.name());
            let prog = sys::prog_load(&sys::ProgSpec {
                prog_type: l.prog_type_lsm,
                expected_attach_type: exp,
                attach_btf_id: l.btf_id(h),
                name: &name,
                license: LICENSE,
                insns: &insns,
            })?;
            let link = sys::link_create(&prog, target, exp)?;
            progs.push(prog);
            links.push(link);
        }
        Ok(Confinement {
            map,
            ring,
            drops,
            ring_size,
            _mnts: mnts,
            _btf: btf,
            _progs: progs,
            _links: links,
            marked: Vec::new(),
            attach,
        })
    }

    /// Marca o arquivo do descritor com `bits` (somados aos que ele já tem).
    /// Guarda o descritor, ou só a marca quando o arquivo já tem um guardado.
    pub fn mark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), Error> {
        let id = file_id(&fd)?;
        let held = self.marked.iter().position(|m| (m.0, m.1) == id);
        let key = match held {
            Some(i) => self.marked[i].2.as_raw_fd(),
            None => fd.as_raw_fd(),
        }
        .to_ne_bytes();
        let mut cur = [0u8; 4];
        let had = sys::map_lookup(&self.map, &key, &mut cur)?;
        let v = if had { u32::from_ne_bytes(cur) } else { 0 } | bits;
        sys::map_update(&self.map, &key, &v.to_ne_bytes(), 0)?;
        if held.is_none() {
            self.marked.push((id.0, id.1, fd));
        }
        Ok(())
    }

    /// Tira `bits` da marca do arquivo do descritor. Sem bit nenhum, apaga a
    /// marca e fecha o descritor guardado. Um arquivo sem marca não é erro.
    pub fn unmark(&mut self, fd: &impl AsRawFd, bits: u32) -> Result<(), Error> {
        let id = file_id(fd)?;
        let Some(i) = self.marked.iter().position(|m| (m.0, m.1) == id) else {
            return Ok(());
        };
        let key = self.marked[i].2.as_raw_fd().to_ne_bytes();
        let mut cur = [0u8; 4];
        let had = sys::map_lookup(&self.map, &key, &mut cur)?;
        let v = if had { u32::from_ne_bytes(cur) } else { 0 } & !bits;
        if v == 0 {
            let _ = sys::map_delete(&self.map, &key);
            self.marked.swap_remove(i);
        } else {
            sys::map_update(&self.map, &key, &v.to_ne_bytes(), 0)?;
        }
        Ok(())
    }

    /// Quantos arquivos estão marcados.
    pub fn marks(&self) -> usize {
        self.marked.len()
    }

    /// As marcas de um arquivo (para os testes).
    pub fn bits(&self, fd: &impl AsRawFd) -> Result<u32, Error> {
        let id = file_id(fd)?;
        let Some(m) = self.marked.iter().find(|m| (m.0, m.1) == id) else {
            return Ok(0);
        };
        let key = m.2.as_raw_fd().to_ne_bytes();
        let mut cur = [0u8; 4];
        Ok(if sys::map_lookup(&self.map, &key, &mut cur)? {
            u32::from_ne_bytes(cur)
        } else {
            0
        })
    }

    /// Uma cópia do descritor da fila de eventos, para a thread que a lê.
    pub fn ring(&self) -> std::io::Result<OwnedFd> {
        self.ring.try_clone()
    }

    /// O tamanho da fila (para o leitor).
    pub fn ring_size(&self) -> u32 {
        self.ring_size
    }

    /// Uma cópia do descritor do contador de eventos descartados.
    pub fn drops(&self) -> std::io::Result<OwnedFd> {
        self.drops.try_clone()
    }
}

/// Lê e zera o contador de eventos descartados.
pub fn take_drops(drops: &impl AsRawFd) -> u64 {
    let key = 0u32.to_ne_bytes();
    let mut cur = [0u8; 8];
    match sys::map_lookup(drops, &key, &mut cur) {
        Ok(true) => {
            let n = u64::from_ne_bytes(cur);
            if n > 0 {
                let _ = sys::map_update(drops, &key, &0u64.to_ne_bytes(), 0);
            }
            n
        }
        _ => 0,
    }
}

/// O dispositivo e o inode do arquivo de um descritor.
fn file_id(fd: &impl AsRawFd) -> Result<(u64, u64), Error> {
    // SAFETY: fstat escreve na struct zerada.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } < 0 {
        return Err(Error::Sys {
            what: "fstat".into(),
            err: std::io::Error::last_os_error(),
            log: String::new(),
        });
    }
    Ok((st.st_dev, st.st_ino))
}

impl Drop for Confinement {
    fn drop(&mut self) {
        // As marcas saem antes dos descritores, para não sobrar nada no
        // armazenamento dos inodes que continuam na memória.
        for m in &self.marked {
            let _ = sys::map_delete(&self.map, &m.2.as_raw_fd().to_ne_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Option<Layout> {
        if !std::path::Path::new(crate::btf::VMLINUX).exists() {
            return None;
        }
        Some(Layout::new(&Btf::vmlinux().unwrap()).unwrap())
    }

    #[test]
    fn todos_os_ganchos_montam() {
        let Some(l) = layout() else { return };
        for h in HOOKS {
            for at in [
                Attach::Cgroup,
                Attach::Global {
                    level: 5,
                    cgroup_id: 1234,
                },
            ] {
                let p = build(
                    &l,
                    h,
                    3,
                    at,
                    Some(EventMaps {
                        ring_fd: 4,
                        drops_fd: 5,
                        workdir_dev: 0x80_0002,
                        open_mnts_fd: Some(6),
                    }),
                )
                .unwrap_or_else(|e| panic!("{h:?}: {e}"));
                assert_eq!(p.last().unwrap().code, 0x95, "{h:?} termina em exit");
                assert!(p.len() < 8000, "{h:?}: {} instruções", p.len());
                // O mapa entra pelo descritor 3, como BPF_PSEUDO_MAP_FD.
                assert!(
                    p.iter()
                        .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 3),
                    "{h:?} usa o mapa"
                );
                // A fila entra pelo descritor 4, e o contador pela soma atômica.
                assert!(
                    p.iter()
                        .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 4),
                    "{h:?} usa a fila"
                );
                assert!(p.iter().any(|i| i.code == 0xdb), "{h:?} conta descartes");
                let le_nome = p
                    .iter()
                    .any(|i| i.code == 0x85 && i.imm == l.fn_probe_read_kernel_str);
                assert_eq!(le_nome, h == Hook::InodeCreate, "{h:?}");
            }
        }
    }

    #[test]
    fn retornos_conforme_o_anexo() {
        let Some(l) = layout() else { return };
        let movs = |p: &[Insn]| -> Vec<i32> {
            p.iter()
                .filter(|i| i.code == 0xb7 && i.regs & 0xf == 0)
                .map(|i| i.imm)
                .collect()
        };
        let c = build(&l, Hook::InodeUnlink, 3, Attach::Cgroup, None).unwrap();
        // Aceita, nega (EPERM), e as negações por `bpf_set_retval` (ENOENT e
        // EACCES), que devolvem 0 depois de pôr o erro.
        assert_eq!(movs(&c), [1, 0, 0, 0]);
        let retval: Vec<i32> = c
            .windows(2)
            .filter(|w| w[1].code == 0x85 && w[1].imm == l.fn_set_retval)
            .map(|w| w[0].imm)
            .collect();
        assert_eq!(retval, [-libc::ENOENT, -libc::EACCES]);
        let g = build(
            &l,
            Hook::InodeUnlink,
            3,
            Attach::Global {
                level: 1,
                cgroup_id: 9,
            },
            None,
        )
        .unwrap();
        assert_eq!(movs(&g), [0, -libc::EPERM, -libc::ENOENT, -libc::EACCES]);
    }

    #[test]
    fn evento_de_abertura_so_com_montagens() {
        let Some(l) = layout() else { return };
        let ev = |mnts| {
            Some(EventMaps {
                ring_fd: 4,
                drops_fd: 5,
                workdir_dev: 0,
                open_mnts_fd: mnts,
            })
        };
        let com = build(&l, Hook::FileOpen, 3, Attach::Cgroup, ev(Some(6))).unwrap();
        assert!(com.iter().any(|i| i.code == 0x85 && i.imm == l.fn_d_path));
        assert!(
            com.iter()
                .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 6)
        );
        let sem = build(&l, Hook::FileOpen, 3, Attach::Cgroup, ev(None)).unwrap();
        assert!(!sem.iter().any(|i| i.code == 0x85 && i.imm == l.fn_d_path));
        // Só a abertura tem o evento.
        let outro = build(&l, Hook::InodeUnlink, 3, Attach::Cgroup, ev(Some(6))).unwrap();
        assert!(!outro.iter().any(|i| i.code == 0x85 && i.imm == l.fn_d_path));
    }

    #[test]
    fn nomes_reservados_so_na_criacao() {
        let Some(l) = layout() else { return };
        let reads = |h| {
            build(&l, h, 3, Attach::Cgroup, None)
                .unwrap()
                .iter()
                .filter(|i| i.code == 0x85 && i.imm == l.fn_probe_read_kernel)
                .count()
        };
        assert_eq!(reads(Hook::InodeCreate), GIT_RESERVED.len());
        assert_eq!(reads(Hook::InodeRename), GIT_RESERVED.len());
        assert_eq!(reads(Hook::FileOpen), 0);
        assert_eq!(reads(Hook::InodeSetattr), 0);
    }

    #[test]
    fn gancho_sem_parametro_da_erro() {
        let Some(mut l) = layout() else { return };
        for x in &mut l.hooks {
            if x.0 == Hook::InodeCreate {
                x.2 = vec!["dir".into()];
            }
        }
        let e = build(&l, Hook::InodeCreate, 3, Attach::Cgroup, None).unwrap_err();
        assert!(e.to_string().contains("dentry"), "{e}");
    }
}
