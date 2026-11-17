//! Os programas BPF LSM do confinamento e a carga deles.
//!
//! Cada confinamento tem um mapa de armazenamento por inode só dele, com um
//! conjunto de bits por inode marcado ([`DENY`], [`PROTECT`], [`GITDIR`]), e
//! um programa por gancho. A decisão segue a árvore real do sistema de
//! arquivos (`d_parent`), e não as montagens: desmontar uma cobertura ou
//! tirar um somente-leitura não muda o resultado.

use std::os::fd::{AsRawFd, OwnedFd, RawFd};

use sandmux_spec::trf;

use crate::Error;
use crate::asm::{Alu, Asm, Cond, Insn, Label, R0, R1, R2, R3, R4, R6, R7, R8, R9, R10, Reg, Size};
use crate::btf::Btf;
use crate::sys;

/// Negado à leitura e à escrita, com tudo o que está abaixo.
pub const DENY: u32 = 1;
/// Protegido contra escrita, com tudo o que está abaixo.
pub const PROTECT: u32 = 2;
/// Diretório git: nomes reservados não podem ser criados nele, e ele mesmo
/// não pode ser apagado nem renomeado.
pub const GITDIR: u32 = 4;

/// Nomes que não podem ser criados num diretório git.
pub const GIT_RESERVED: [&str; 4] = ["commondir", "hooks", "config", "config.worktree"];

/// Teto de níveis da subida pela árvore; mais fundo que isso é negado.
pub const MAX_DEPTH: usize = 64;

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
    file_dentry: i16,
    file_mode: i16,
    file_mode_size: Size,
    d_parent: i16,
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
            file_dentry: off(b, "file", "f_path.dentry")?,
            file_mode: off(b, "file", "f_mode")?,
            file_mode_size: Size::of(b.field_size("file", "f_mode")?)?,
            d_parent: off(b, "dentry", "d_parent")?,
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

struct Gen<'a> {
    a: Asm,
    l: &'a Layout,
    map_fd: i32,
    allow: Label,
    deny: Label,
}

impl Gen<'_> {
    /// Marca de `inode` (registrador com o ponteiro, já sem nulo) em R0
    /// (ponteiro para o valor, ou 0).
    fn storage(&mut self, inode: Reg) {
        self.a.mov(R2, inode);
        self.a.ld_map_fd(R1, self.map_fd);
        self.a.mov_imm(R3, 0);
        self.a.mov_imm(R4, 0);
        self.a.call(self.l.fn_inode_storage_get);
    }

    /// Sobe de `dentry` (em R7, destruído) pela árvore. No próprio inode,
    /// nega se a marca tem algum bit de `own`; nos de cima, de `above`.
    fn walk(&mut self, own: u32, above: u32) {
        let done = self.a.label();
        for level in 0..MAX_DEPTH {
            let mask = if level == 0 { own } else { above };
            let next = self.a.label();
            if mask != 0 {
                self.a.ldx(Size::DW, R1, R7, self.l.d_inode);
                self.a.jmp_imm(Cond::Eq, R1, 0, next);
                self.storage(R1);
                self.a.jmp_imm(Cond::Eq, R0, 0, next);
                self.a.ldx(Size::W, R1, R0, 0);
                self.a.alu_imm(Alu::And, R1, mask as i32);
                self.a.jmp_imm(Cond::Ne, R1, 0, self.deny);
            }
            self.a.bind(next);
            self.a.ldx(Size::DW, R1, R7, self.l.d_parent);
            self.a.jmp_imm(Cond::Eq, R1, 0, done);
            self.a.jmp_reg(Cond::Eq, R1, R7, done);
            self.a.mov(R7, R1);
        }
        // Fundo demais: nega, por segurança.
        self.a.ja(self.deny);
        self.a.bind(done);
    }

    /// Nega a criação de um nome reservado (`dentry` em R9) num diretório
    /// git. Termina em `allow` quando não é o caso.
    fn reserved_names(&mut self) {
        let a = &mut self.a;
        a.ldx(Size::DW, R1, R9, self.l.d_parent);
        a.jmp_imm(Cond::Eq, R1, 0, self.allow);
        a.ldx(Size::DW, R1, R1, self.l.d_inode);
        a.jmp_imm(Cond::Eq, R1, 0, self.allow);
        self.storage(R1);
        let a = &mut self.a;
        a.jmp_imm(Cond::Eq, R0, 0, self.allow);
        a.ldx(Size::W, R1, R0, 0);
        a.alu_imm(Alu::And, R1, GITDIR as i32);
        a.jmp_imm(Cond::Eq, R1, 0, self.allow);
        a.ldx(self.l.d_name_len_size, R6, R9, self.l.d_name_len);
        for name in GIT_RESERVED {
            let b = name.as_bytes();
            let next = self.a.label();
            let a = &mut self.a;
            a.jmp_imm(Cond::Ne, R6, b.len() as i32, next);
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
            a.ja(self.deny);
            a.bind(next);
        }
        self.a.ja(self.allow);
    }
}

/// Monta o programa de um gancho.
pub fn build(l: &Layout, h: Hook, map_fd: i32, attach: Attach) -> Result<Vec<Insn>, Error> {
    let mut a = Asm::new();
    let allow = a.label();
    let deny = a.label();
    let mut g = Gen {
        a,
        l,
        map_fd,
        allow,
        deny,
    };
    // R6 = contexto (vetor de argumentos de 64 bits).
    g.a.mov(R6, R1);
    if let Attach::Global { level, cgroup_id } = attach {
        g.a.mov_imm(R1, level);
        g.a.call(l.fn_ancestor_cgroup_id);
        g.a.ld_imm64(R1, cgroup_id);
        g.a.jmp_reg(Cond::Ne, R0, R1, allow);
    }
    let all = DENY | PROTECT | GITDIR;
    let tree = DENY | PROTECT;
    match h {
        Hook::FileOpen => {
            let file = l.arg(h, "file")?;
            g.a.ldx(Size::DW, R8, R6, file);
            g.a.ldx(Size::DW, R7, R8, l.file_dentry);
            g.a.ldx(l.file_mode_size, R1, R8, l.file_mode);
            let write = g.a.label();
            g.a.jmp_imm(Cond::Set, R1, FMODE_WRITE, write);
            g.walk(DENY, DENY);
            g.a.ja(allow);
            g.a.bind(write);
            g.walk(tree, tree);
            g.a.ja(allow);
        }
        Hook::InodeCreate | Hook::InodeMkdir | Hook::InodeMknod | Hook::InodeSymlink => {
            let d = l.arg(h, "dentry")?;
            g.a.ldx(Size::DW, R9, R6, d);
            g.a.mov(R7, R9);
            g.walk(all, tree);
            g.reserved_names();
        }
        Hook::InodeLink => {
            let old = l.arg(h, "old_dentry")?;
            let new = l.arg(h, "new_dentry")?;
            g.a.ldx(Size::DW, R7, R6, old);
            g.walk(all, tree);
            g.a.ldx(Size::DW, R9, R6, new);
            g.a.mov(R7, R9);
            g.walk(all, tree);
            g.reserved_names();
        }
        Hook::InodeRename => {
            let old = l.arg(h, "old_dentry")?;
            let new = l.arg(h, "new_dentry")?;
            g.a.ldx(Size::DW, R7, R6, old);
            g.walk(all, tree);
            g.a.ldx(Size::DW, R9, R6, new);
            g.a.mov(R7, R9);
            g.walk(all, tree);
            g.reserved_names();
        }
        Hook::InodeUnlink | Hook::InodeRmdir | Hook::InodeSetattr => {
            let d = l.arg(h, "dentry")?;
            g.a.ldx(Size::DW, R7, R6, d);
            g.walk(all, tree);
            g.a.ja(allow);
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
    g.a.finish()
}

/// Licença declarada ao kernel: compatível com a GPL, que alguns auxiliares
/// do BPF exigem (`bpf_probe_read_kernel`).
pub const LICENSE: &str = "Dual MIT/GPL";

/// Um confinamento carregado: o mapa, os programas presos e os arquivos
/// marcados, mantidos abertos para a marca não se perder.
pub struct Confinement {
    map: OwnedFd,
    _btf: OwnedFd,
    _progs: Vec<OwnedFd>,
    _links: Vec<OwnedFd>,
    marked: Vec<OwnedFd>,
    pub attach: Attach,
}

impl Confinement {
    /// Carrega os programas e os prende ao cgroup `cgroup`. Com
    /// `Attach::Global`, o descritor do cgroup não é usado.
    pub fn load(l: &Layout, cgroup: RawFd, attach: Attach) -> Result<Confinement, Error> {
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
        let mut progs = Vec::new();
        let mut links = Vec::new();
        for h in HOOKS {
            let insns = build(l, h, map.as_raw_fd(), attach)?;
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
            _btf: btf,
            _progs: progs,
            _links: links,
            marked: Vec::new(),
            attach,
        })
    }

    /// Marca o arquivo do descritor com `bits` (somados aos que ele já tem),
    /// e guarda o descritor.
    pub fn mark(&mut self, fd: OwnedFd, bits: u32) -> Result<(), Error> {
        let key = fd.as_raw_fd().to_ne_bytes();
        let mut cur = [0u8; 4];
        let had = sys::map_lookup(&self.map, &key, &mut cur)?;
        let v = if had { u32::from_ne_bytes(cur) } else { 0 } | bits;
        sys::map_update(&self.map, &key, &v.to_ne_bytes(), 0)?;
        self.marked.push(fd);
        Ok(())
    }

    /// Quantos arquivos estão marcados.
    pub fn marks(&self) -> usize {
        self.marked.len()
    }

    /// As marcas de um arquivo (para os testes).
    pub fn bits(&self, fd: &impl AsRawFd) -> Result<u32, Error> {
        let key = fd.as_raw_fd().to_ne_bytes();
        let mut cur = [0u8; 4];
        Ok(if sys::map_lookup(&self.map, &key, &mut cur)? {
            u32::from_ne_bytes(cur)
        } else {
            0
        })
    }
}

impl Drop for Confinement {
    fn drop(&mut self) {
        // As marcas saem antes dos descritores, para não sobrar nada no
        // armazenamento dos inodes que continuam na memória.
        for f in &self.marked {
            let _ = sys::map_delete(&self.map, &f.as_raw_fd().to_ne_bytes());
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
                let p = build(&l, h, 3, at).unwrap_or_else(|e| panic!("{h:?}: {e}"));
                assert_eq!(p.last().unwrap().code, 0x95, "{h:?} termina em exit");
                assert!(p.len() < 8000, "{h:?}: {} instruções", p.len());
                // O mapa entra pelo descritor 3, como BPF_PSEUDO_MAP_FD.
                assert!(
                    p.iter()
                        .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 3),
                    "{h:?} usa o mapa"
                );
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
        let c = build(&l, Hook::InodeUnlink, 3, Attach::Cgroup).unwrap();
        assert_eq!(movs(&c), [1, 0]);
        let g = build(
            &l,
            Hook::InodeUnlink,
            3,
            Attach::Global {
                level: 1,
                cgroup_id: 9,
            },
        )
        .unwrap();
        assert_eq!(movs(&g), [0, -libc::EPERM]);
    }

    #[test]
    fn nomes_reservados_so_na_criacao() {
        let Some(l) = layout() else { return };
        let reads = |h| {
            build(&l, h, 3, Attach::Cgroup)
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
        let e = build(&l, Hook::InodeCreate, 3, Attach::Cgroup).unwrap_err();
        assert!(e.to_string().contains("dentry"), "{e}");
    }
}
