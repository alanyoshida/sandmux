//! Leitura do BTF do kernel (`/sys/kernel/btf/vmlinux`).
//!
//! Faz o papel do CO-RE sem o `clang`: os programas do confinamento são
//! montados na hora da carga, com os ids dos ganchos, a posição dos
//! parâmetros e os deslocamentos dos campos lidos do kernel que está rodando.

use std::collections::HashMap;
use std::path::Path;

use sandmux_spec::trf;

use crate::Error;

/// Caminho do BTF do kernel.
pub const VMLINUX: &str = "/sys/kernel/btf/vmlinux";

const MAGIC: u16 = 0xeb9f;

/// Tipos do BTF que importam aqui (os outros só são pulados).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Int,
    Ptr,
    Array,
    Struct,
    Union,
    Enum,
    Fwd,
    Typedef,
    Volatile,
    Const,
    Restrict,
    Func,
    FuncProto,
    Var,
    Datasec,
    Float,
    DeclTag,
    TypeTag,
    Enum64,
}

impl Kind {
    fn from(n: u32) -> Option<Kind> {
        Some(match n {
            1 => Kind::Int,
            2 => Kind::Ptr,
            3 => Kind::Array,
            4 => Kind::Struct,
            5 => Kind::Union,
            6 => Kind::Enum,
            7 => Kind::Fwd,
            8 => Kind::Typedef,
            9 => Kind::Volatile,
            10 => Kind::Const,
            11 => Kind::Restrict,
            12 => Kind::Func,
            13 => Kind::FuncProto,
            14 => Kind::Var,
            15 => Kind::Datasec,
            16 => Kind::Float,
            17 => Kind::DeclTag,
            18 => Kind::TypeTag,
            19 => Kind::Enum64,
            _ => return None,
        })
    }
}

/// Membro de struct ou union, parâmetro de função ou valor de enum.
#[derive(Clone, Debug)]
struct Member {
    name: u32,
    ty: u32,
    /// Deslocamento em bits (membros); valor (enums).
    val: i64,
}

#[derive(Clone, Debug)]
struct Type {
    name: u32,
    kind: Kind,
    kind_flag: bool,
    /// `size` ou `type`, conforme o tipo.
    size_or_type: u32,
    members: Vec<Member>,
}

/// O BTF lido, com os tipos indexados por id (o 0 é o `void`).
pub struct Btf {
    strings: Vec<u8>,
    types: Vec<Type>,
    by_name: HashMap<(Kind, String), u32>,
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, Error> {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| Error::Btf("BTF truncado".into()))
}

fn u32_at(b: &[u8], o: usize) -> Result<u32, Error> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| Error::Btf("BTF truncado".into()))
}

impl Btf {
    /// Lê o BTF do kernel.
    pub fn vmlinux() -> Result<Btf, Error> {
        Btf::load(Path::new(VMLINUX))
    }

    pub fn load(p: &Path) -> Result<Btf, Error> {
        let b = std::fs::read(p).map_err(|e| Error::Btf(format!("{}: {e}", p.display())))?;
        Btf::parse(&b)
    }

    pub fn parse(b: &[u8]) -> Result<Btf, Error> {
        if u16_at(b, 0)? != MAGIC {
            return Err(Error::Btf("assinatura do BTF errada".into()));
        }
        let hdr_len = u32_at(b, 4)? as usize;
        let type_off = u32_at(b, 8)? as usize;
        let type_len = u32_at(b, 12)? as usize;
        let str_off = u32_at(b, 16)? as usize;
        let str_len = u32_at(b, 20)? as usize;
        let tstart = hdr_len + type_off;
        let sstart = hdr_len + str_off;
        let strings = b
            .get(sstart..sstart + str_len)
            .ok_or_else(|| Error::Btf("strings do BTF fora do arquivo".into()))?
            .to_vec();
        let tb = b
            .get(tstart..tstart + type_len)
            .ok_or_else(|| Error::Btf("tipos do BTF fora do arquivo".into()))?;
        let mut types = vec![Type {
            name: 0,
            kind: Kind::Int,
            kind_flag: false,
            size_or_type: 0,
            members: Vec::new(),
        }];
        let mut o = 0;
        while o < tb.len() {
            let name = u32_at(tb, o)?;
            let info = u32_at(tb, o + 4)?;
            let st = u32_at(tb, o + 8)?;
            o += 12;
            let vlen = (info & 0xffff) as usize;
            let kind_n = (info >> 24) & 0x1f;
            let kind = Kind::from(kind_n)
                .ok_or_else(|| Error::Btf(format!("tipo de BTF desconhecido: {kind_n}")))?;
            let kind_flag = info >> 31 == 1;
            let mut members = Vec::new();
            match kind {
                Kind::Int | Kind::Var | Kind::DeclTag => o += 4,
                Kind::Array => o += 12,
                Kind::Struct | Kind::Union => {
                    for _ in 0..vlen {
                        members.push(Member {
                            name: u32_at(tb, o)?,
                            ty: u32_at(tb, o + 4)?,
                            val: i64::from(u32_at(tb, o + 8)?),
                        });
                        o += 12;
                    }
                }
                Kind::Enum => {
                    for _ in 0..vlen {
                        members.push(Member {
                            name: u32_at(tb, o)?,
                            ty: 0,
                            val: i64::from(u32_at(tb, o + 4)? as i32),
                        });
                        o += 8;
                    }
                }
                Kind::Enum64 => {
                    for _ in 0..vlen {
                        let lo = u64::from(u32_at(tb, o + 4)?);
                        let hi = u64::from(u32_at(tb, o + 8)?);
                        members.push(Member {
                            name: u32_at(tb, o)?,
                            ty: 0,
                            val: (hi << 32 | lo) as i64,
                        });
                        o += 12;
                    }
                }
                Kind::FuncProto => {
                    for _ in 0..vlen {
                        members.push(Member {
                            name: u32_at(tb, o)?,
                            ty: u32_at(tb, o + 4)?,
                            val: 0,
                        });
                        o += 8;
                    }
                }
                Kind::Datasec => o += 12 * vlen,
                _ => {}
            }
            types.push(Type {
                name,
                kind,
                kind_flag,
                size_or_type: st,
                members,
            });
        }
        let mut btf = Btf {
            strings,
            types,
            by_name: HashMap::new(),
        };
        for (id, t) in btf.types.iter().enumerate().skip(1) {
            if t.name == 0 {
                continue;
            }
            let n = btf.str(t.name).to_string();
            // O primeiro com o nome vence: o vmlinux não repete os que usamos.
            btf.by_name.entry((t.kind, n)).or_insert(id as u32);
        }
        Ok(btf)
    }

    fn str(&self, off: u32) -> &str {
        let s = self.strings.get(off as usize..).unwrap_or(&[]);
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        std::str::from_utf8(&s[..end]).unwrap_or("")
    }

    fn ty(&self, id: u32) -> Result<&Type, Error> {
        self.types
            .get(id as usize)
            .ok_or_else(|| Error::Btf(format!("tipo {id} fora do BTF")))
    }

    /// Id de um tipo pelo tipo e pelo nome.
    pub fn find(&self, kind: Kind, name: &str) -> Option<u32> {
        self.by_name.get(&(kind, name.to_string())).copied()
    }

    /// Pula `typedef`, `const`, `volatile`, `restrict` e marcas de tipo.
    fn resolve(&self, mut id: u32) -> Result<u32, Error> {
        for _ in 0..32 {
            let t = self.ty(id)?;
            match t.kind {
                Kind::Typedef | Kind::Const | Kind::Volatile | Kind::Restrict | Kind::TypeTag => {
                    id = t.size_or_type
                }
                _ => return Ok(id),
            }
        }
        Err(Error::Btf(format!(
            "cadeia de tipos longa demais a partir de {id}"
        )))
    }

    /// Os parâmetros de uma função (`FUNC`), em ordem: nome e tipo.
    pub fn func_params(&self, func: &str) -> Result<(u32, Vec<String>), Error> {
        let id = self.find(Kind::Func, func).ok_or_else(|| {
            Error::Btf(trf!(
                "o kernel não tem a função {func} no BTF",
                "the kernel has no function {func} in BTF"
            ))
        })?;
        let proto = self.ty(self.ty(id)?.size_or_type)?;
        if proto.kind != Kind::FuncProto {
            return Err(Error::Btf(trf!(
                "{func} sem protótipo no BTF",
                "{func} has no prototype in BTF"
            )));
        }
        let names = proto
            .members
            .iter()
            .map(|m| self.str(m.name).to_string())
            .collect();
        Ok((id, names))
    }

    /// Procura o membro `name` em `id`, entrando nos membros anônimos.
    /// Devolve o deslocamento em bits e o tipo do membro.
    fn member(&self, id: u32, name: &str) -> Result<Option<(u32, u32)>, Error> {
        let t = self.ty(self.resolve(id)?)?;
        if !matches!(t.kind, Kind::Struct | Kind::Union) {
            return Ok(None);
        }
        for m in &t.members {
            // Com o kind_flag, os 8 bits de cima são o tamanho do campo de bits.
            let off = if t.kind_flag {
                (m.val as u32) & 0x00ff_ffff
            } else {
                m.val as u32
            };
            if m.name == 0 {
                if let Some((o, ty)) = self.member(m.ty, name)? {
                    return Ok(Some((off + o, ty)));
                }
            } else if self.str(m.name) == name {
                return Ok(Some((off, m.ty)));
            }
        }
        Ok(None)
    }

    /// Deslocamento em bytes de um caminho de campos (`f_path.dentry`) dentro
    /// da struct `strukt`.
    pub fn offset(&self, strukt: &str, path: &str) -> Result<u32, Error> {
        let mut id = self.find(Kind::Struct, strukt).ok_or_else(|| {
            Error::Btf(trf!(
                "o kernel não tem a struct {strukt} no BTF",
                "the kernel has no struct {strukt} in BTF"
            ))
        })?;
        let mut bits = 0;
        for part in path.split('.') {
            let (o, ty) = self.member(id, part)?.ok_or_else(|| {
                Error::Btf(trf!(
                    "a struct {strukt} não tem o campo {path} no BTF",
                    "struct {strukt} has no field {path} in BTF"
                ))
            })?;
            bits += o;
            id = ty;
        }
        if bits % 8 != 0 {
            return Err(Error::Btf(trf!(
                "{strukt}.{path} é um campo de bits",
                "{strukt}.{path} is a bit field"
            )));
        }
        Ok(bits / 8)
    }

    /// Tamanho em bytes do tipo de um campo (para escolher a largura da
    /// leitura).
    pub fn field_size(&self, strukt: &str, path: &str) -> Result<u32, Error> {
        let mut id = self.find(Kind::Struct, strukt).ok_or_else(|| {
            Error::Btf(trf!(
                "o kernel não tem a struct {strukt} no BTF",
                "the kernel has no struct {strukt} in BTF"
            ))
        })?;
        for part in path.split('.') {
            let (_, ty) = self.member(id, part)?.ok_or_else(|| {
                Error::Btf(trf!(
                    "a struct {strukt} não tem o campo {path} no BTF",
                    "struct {strukt} has no field {path} in BTF"
                ))
            })?;
            id = ty;
        }
        let t = self.ty(self.resolve(id)?)?;
        Ok(match t.kind {
            Kind::Ptr => 8,
            Kind::Int | Kind::Enum | Kind::Struct | Kind::Union | Kind::Float | Kind::Enum64 => {
                t.size_or_type
            }
            _ => return Err(Error::Btf(format!("{strukt}.{path}: tamanho desconhecido"))),
        })
    }

    /// Valor de um item de enum (`BPF_FUNC_inode_storage_get`).
    pub fn enum_value(&self, name: &str) -> Option<i64> {
        self.types
            .iter()
            .filter(|t| matches!(t.kind, Kind::Enum | Kind::Enum64))
            .flat_map(|t| t.members.iter())
            .find(|m| self.str(m.name) == name)
            .map(|m| m.val)
    }
}

/// Um BTF mínimo com um só tipo, `int` (id 1), que o kernel exige para criar
/// um mapa de armazenamento por inode (as chaves e os valores precisam de
/// tipo).
pub fn int_btf() -> Vec<u8> {
    let strings = b"\0int\0";
    let mut types = Vec::new();
    // name_off = 1 ("int"), info = INT (1) << 24, size = 4.
    types.extend_from_slice(&1u32.to_le_bytes());
    types.extend_from_slice(&(1u32 << 24).to_le_bytes());
    types.extend_from_slice(&4u32.to_le_bytes());
    // Codificação do INT: sem sinal especial, deslocamento 0, 32 bits.
    types.extend_from_slice(&32u32.to_le_bytes());
    let mut b = Vec::new();
    b.extend_from_slice(&MAGIC.to_le_bytes());
    b.push(1); // versão
    b.push(0); // flags
    b.extend_from_slice(&24u32.to_le_bytes()); // hdr_len
    b.extend_from_slice(&0u32.to_le_bytes()); // type_off
    b.extend_from_slice(&(types.len() as u32).to_le_bytes());
    b.extend_from_slice(&(types.len() as u32).to_le_bytes()); // str_off
    b.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    b.extend_from_slice(&types);
    b.extend_from_slice(strings);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel() -> Option<Btf> {
        if !Path::new(VMLINUX).exists() {
            eprintln!("sem {VMLINUX}: pulado");
            return None;
        }
        Some(Btf::vmlinux().expect("BTF do kernel"))
    }

    #[test]
    fn ganchos_do_kernel() {
        let Some(b) = kernel() else { return };
        for h in [
            "file_open",
            "inode_create",
            "inode_link",
            "inode_unlink",
            "inode_symlink",
            "inode_mkdir",
            "inode_rmdir",
            "inode_mknod",
            "inode_rename",
            "inode_setattr",
        ] {
            let (id, params) = b.func_params(&format!("bpf_lsm_{h}")).expect(h);
            assert!(id > 0, "{h}");
            assert!(!params.is_empty(), "{h}");
        }
        let (_, p) = b.func_params("bpf_lsm_inode_create").unwrap();
        assert_eq!(&p[..2], ["dir", "dentry"]);
        let (_, p) = b.func_params("bpf_lsm_inode_rename").unwrap();
        assert_eq!(p, ["old_dir", "old_dentry", "new_dir", "new_dentry"]);
        assert!(b.func_params("bpf_lsm_nao_existe").is_err());
    }

    #[test]
    fn deslocamentos_plausiveis() {
        let Some(b) = kernel() else { return };
        let f_inode = b.offset("file", "f_inode").unwrap();
        let dentry = b.offset("file", "f_path.dentry").unwrap();
        let mnt = b.offset("file", "f_path.mnt").unwrap();
        assert!(f_inode < 512 && dentry < 512);
        assert_eq!(dentry, mnt + 8, "struct path é {{mnt, dentry}}");
        let parent = b.offset("dentry", "d_parent").unwrap();
        let name = b.offset("dentry", "d_name.name").unwrap();
        let len = b.offset("dentry", "d_name.len").unwrap();
        let qstr = b.offset("dentry", "d_name").unwrap();
        assert!(parent < qstr && qstr <= len && len < name);
        assert_eq!(b.field_size("dentry", "d_name.len").unwrap(), 4);
        assert_eq!(b.field_size("dentry", "d_inode").unwrap(), 8);
        assert_eq!(b.field_size("file", "f_mode").unwrap(), 4);
        assert!(b.offset("file", "nao_existe").is_err());
    }

    #[test]
    fn enums_da_uapi() {
        let Some(b) = kernel() else { return };
        assert_eq!(b.enum_value("BPF_MAP_TYPE_INODE_STORAGE"), Some(28));
        assert_eq!(b.enum_value("BPF_PROG_TYPE_LSM"), Some(29));
        assert_eq!(b.enum_value("BPF_FUNC_map_lookup_elem"), Some(1));
        assert!(b.enum_value("BPF_FUNC_inode_storage_get").is_some());
        assert!(b.enum_value("BPF_LSM_CGROUP").is_some());
    }

    #[test]
    fn btf_minimo_le_de_volta() {
        let b = Btf::parse(&int_btf()).unwrap();
        assert_eq!(b.find(Kind::Int, "int"), Some(1));
    }

    #[test]
    fn btf_truncado_da_erro() {
        assert!(Btf::parse(&[0x9f, 0xeb, 1]).is_err());
        assert!(Btf::parse(&[0; 24]).is_err());
    }
}
