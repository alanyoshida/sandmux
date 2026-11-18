//! Montador de instruções BPF (eBPF), com rótulos.
//!
//! Só os opcodes que os programas do confinamento usam. Cada instrução tem 8
//! bytes: opcode, registradores (destino nos 4 bits de baixo, origem nos de
//! cima), deslocamento de 16 bits e imediato de 32 bits. O `ld_imm64` ocupa
//! duas.

use std::collections::HashMap;

use sandmux_spec::trf;

use crate::Error;

/// Registradores: `R0` é o retorno, `R1`..`R5` os argumentos das chamadas,
/// `R6`..`R9` sobrevivem às chamadas, `R10` é o ponteiro da pilha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reg(pub u8);

pub const R0: Reg = Reg(0);
pub const R1: Reg = Reg(1);
pub const R2: Reg = Reg(2);
pub const R3: Reg = Reg(3);
pub const R4: Reg = Reg(4);
pub const R5: Reg = Reg(5);
pub const R6: Reg = Reg(6);
pub const R7: Reg = Reg(7);
pub const R8: Reg = Reg(8);
pub const R9: Reg = Reg(9);
pub const R10: Reg = Reg(10);

/// Uma instrução já codificada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insn {
    pub code: u8,
    pub regs: u8,
    pub off: i16,
    pub imm: i32,
}

impl Insn {
    fn new(code: u8, dst: Reg, src: Reg, off: i16, imm: i32) -> Insn {
        Insn {
            code,
            regs: (src.0 << 4) | (dst.0 & 0xf),
            off,
            imm,
        }
    }

    /// Os 8 bytes da instrução, como o kernel lê.
    pub fn bytes(&self) -> [u8; 8] {
        let o = self.off.to_le_bytes();
        let i = self.imm.to_le_bytes();
        [self.code, self.regs, o[0], o[1], i[0], i[1], i[2], i[3]]
    }
}

/// Largura de uma leitura ou escrita na memória.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    B,
    H,
    W,
    DW,
}

impl Size {
    fn bits(self) -> u8 {
        match self {
            Size::W => 0x00,
            Size::H => 0x08,
            Size::B => 0x10,
            Size::DW => 0x18,
        }
    }

    /// A largura de um campo de `n` bytes.
    pub fn of(n: u32) -> Result<Size, Error> {
        Ok(match n {
            1 => Size::B,
            2 => Size::H,
            4 => Size::W,
            8 => Size::DW,
            _ => return Err(Error::Asm(format!("campo de {n} bytes"))),
        })
    }
}

const CLS_LDX: u8 = 0x01;
const CLS_ST: u8 = 0x02;
const CLS_STX: u8 = 0x03;
const CLS_JMP: u8 = 0x05;
const CLS_ALU64: u8 = 0x07;
const MODE_MEM: u8 = 0x60;
const MODE_ATOMIC: u8 = 0xc0;
const SRC_K: u8 = 0x00;
const SRC_X: u8 = 0x08;

/// Operações de comparação dos desvios.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Gt,
    Ge,
    Set,
}

impl Cond {
    fn op(self) -> u8 {
        match self {
            Cond::Eq => 0x10,
            Cond::Gt => 0x20,
            Cond::Ge => 0x30,
            Cond::Set => 0x40,
            Cond::Ne => 0x50,
        }
    }
}

/// Operações aritméticas de 64 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alu {
    Add,
    Sub,
    And,
    Or,
    Lsh,
    Rsh,
    Mov,
}

impl Alu {
    fn op(self) -> u8 {
        match self {
            Alu::Add => 0x00,
            Alu::Sub => 0x10,
            Alu::Or => 0x40,
            Alu::And => 0x50,
            Alu::Lsh => 0x60,
            Alu::Rsh => 0x70,
            Alu::Mov => 0xb0,
        }
    }
}

/// Um rótulo, criado antes e posto no lugar depois.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Label(usize);

/// O programa em montagem.
#[derive(Default)]
pub struct Asm {
    insns: Vec<Insn>,
    labels: Vec<Option<usize>>,
    /// Desvios pendentes: índice da instrução → rótulo.
    fixups: HashMap<usize, Label>,
}

impl Asm {
    pub fn new() -> Asm {
        Asm::default()
    }

    pub fn len(&self) -> usize {
        self.insns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.insns.is_empty()
    }

    pub fn label(&mut self) -> Label {
        self.labels.push(None);
        Label(self.labels.len() - 1)
    }

    /// Põe o rótulo na próxima instrução.
    pub fn bind(&mut self, l: Label) {
        self.labels[l.0] = Some(self.insns.len());
    }

    fn push(&mut self, i: Insn) {
        self.insns.push(i);
    }

    pub fn alu_imm(&mut self, op: Alu, dst: Reg, imm: i32) {
        self.push(Insn::new(CLS_ALU64 | op.op() | SRC_K, dst, R0, 0, imm));
    }

    pub fn alu_reg(&mut self, op: Alu, dst: Reg, src: Reg) {
        self.push(Insn::new(CLS_ALU64 | op.op() | SRC_X, dst, src, 0, 0));
    }

    pub fn mov_imm(&mut self, dst: Reg, imm: i32) {
        self.alu_imm(Alu::Mov, dst, imm);
    }

    pub fn mov(&mut self, dst: Reg, src: Reg) {
        self.alu_reg(Alu::Mov, dst, src);
    }

    /// `dst = imm64`, em duas instruções.
    pub fn ld_imm64(&mut self, dst: Reg, v: u64) {
        self.push(Insn::new(0x18, dst, R0, 0, v as u32 as i32));
        self.push(Insn::new(0, R0, R0, 0, (v >> 32) as u32 as i32));
    }

    /// `dst = descritor de mapa` (`BPF_PSEUDO_MAP_FD`), em duas instruções.
    pub fn ld_map_fd(&mut self, dst: Reg, fd: i32) {
        self.push(Insn::new(0x18, dst, Reg(1), 0, fd));
        self.push(Insn::new(0, R0, R0, 0, 0));
    }

    /// `dst = *(size *)(src + off)`.
    pub fn ldx(&mut self, size: Size, dst: Reg, src: Reg, off: i16) {
        self.push(Insn::new(
            CLS_LDX | MODE_MEM | size.bits(),
            dst,
            src,
            off,
            0,
        ));
    }

    /// `*(size *)(dst + off) = src`.
    pub fn stx(&mut self, size: Size, dst: Reg, off: i16, src: Reg) {
        self.push(Insn::new(
            CLS_STX | MODE_MEM | size.bits(),
            dst,
            src,
            off,
            0,
        ));
    }

    /// `*(size *)(dst + off) = imm`.
    pub fn st(&mut self, size: Size, dst: Reg, off: i16, imm: i32) {
        self.push(Insn::new(
            CLS_ST | MODE_MEM | size.bits(),
            dst,
            R0,
            off,
            imm,
        ));
    }

    /// `*(u64 *)(dst + off) += src`, atômico (`BPF_STX | BPF_ATOMIC`, com
    /// `BPF_ADD` no imediato).
    pub fn atomic_add64(&mut self, dst: Reg, off: i16, src: Reg) {
        self.push(Insn::new(
            CLS_STX | MODE_ATOMIC | Size::DW.bits(),
            dst,
            src,
            off,
            0,
        ));
    }

    fn jump(&mut self, code: u8, dst: Reg, src: Reg, imm: i32, to: Label) {
        self.fixups.insert(self.insns.len(), to);
        self.push(Insn::new(code, dst, src, 0, imm));
    }

    pub fn ja(&mut self, to: Label) {
        self.jump(CLS_JMP, R0, R0, 0, to);
    }

    /// Desvia se `dst <cond> imm`.
    pub fn jmp_imm(&mut self, c: Cond, dst: Reg, imm: i32, to: Label) {
        self.jump(CLS_JMP | c.op() | SRC_K, dst, R0, imm, to);
    }

    /// Desvia se `dst <cond> src`.
    pub fn jmp_reg(&mut self, c: Cond, dst: Reg, src: Reg, to: Label) {
        self.jump(CLS_JMP | c.op() | SRC_X, dst, src, 0, to);
    }

    pub fn call(&mut self, helper: i32) {
        self.push(Insn::new(CLS_JMP | 0x80, R0, R0, 0, helper));
    }

    pub fn exit(&mut self) {
        self.push(Insn::new(CLS_JMP | 0x90, R0, R0, 0, 0));
    }

    /// Resolve os desvios e devolve as instruções.
    pub fn finish(self) -> Result<Vec<Insn>, Error> {
        let mut out = self.insns;
        for (at, l) in self.fixups {
            let to = self.labels[l.0].ok_or_else(|| {
                Error::Asm(trf!("rótulo {} sem lugar", "label {} not placed", l.0))
            })?;
            let rel = to as i64 - at as i64 - 1;
            out[at].off = i16::try_from(rel)
                .map_err(|_| Error::Asm(format!("desvio longo demais ({rel})")))?;
        }
        Ok(out)
    }
}

/// As instruções em bytes, como o `PROG_LOAD` recebe.
pub fn encode(insns: &[Insn]) -> Vec<u8> {
    insns.iter().flat_map(|i| i.bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codificacoes_conhecidas() {
        // Conferidas com o `bpftool prog dump xlated opcodes` de programas
        // equivalentes e com a tabela de opcodes do kernel
        // (Documentation/bpf/standardization/instruction-set.rst).
        let mut a = Asm::new();
        a.mov_imm(R0, 1); // b7 00 .. 01 00 00 00
        a.mov(R6, R1); // bf 16
        a.ldx(Size::DW, R1, R6, 8); // 79 61 08 00
        a.ldx(Size::W, R2, R1, 0x44); // 61 12 44 00
        a.st(Size::DW, R10, -8, 0); // 7a 0a f8 ff
        a.stx(Size::W, R10, -16, R2); // 63 2a f0 ff
        a.alu_imm(Alu::And, R2, 2); // 57 02
        a.call(145); // 85 00 .. 91 00 00 00
        a.exit(); // 95
        let i = a.finish().unwrap();
        let b: Vec<[u8; 8]> = i.iter().map(Insn::bytes).collect();
        assert_eq!(b[0], [0xb7, 0x00, 0, 0, 1, 0, 0, 0]);
        assert_eq!(b[1], [0xbf, 0x16, 0, 0, 0, 0, 0, 0]);
        assert_eq!(b[2], [0x79, 0x61, 8, 0, 0, 0, 0, 0]);
        assert_eq!(b[3], [0x61, 0x12, 0x44, 0, 0, 0, 0, 0]);
        assert_eq!(b[4], [0x7a, 0x0a, 0xf8, 0xff, 0, 0, 0, 0]);
        assert_eq!(b[5], [0x63, 0x2a, 0xf0, 0xff, 0, 0, 0, 0]);
        assert_eq!(b[6], [0x57, 0x02, 0, 0, 2, 0, 0, 0]);
        assert_eq!(b[7], [0x85, 0x00, 0, 0, 145, 0, 0, 0]);
        assert_eq!(b[8], [0x95, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn ld_imm64_e_mapa() {
        let mut a = Asm::new();
        a.ld_imm64(R1, 0x1122_3344_5566_7788);
        a.ld_map_fd(R2, 7);
        let i = a.finish().unwrap();
        assert_eq!(i.len(), 4);
        assert_eq!(i[0].bytes(), [0x18, 0x01, 0, 0, 0x88, 0x77, 0x66, 0x55]);
        assert_eq!(i[1].bytes(), [0, 0, 0, 0, 0x44, 0x33, 0x22, 0x11]);
        assert_eq!(i[2].bytes(), [0x18, 0x12, 0, 0, 7, 0, 0, 0]);
    }

    #[test]
    fn desvios_para_frente_e_para_tras() {
        let mut a = Asm::new();
        let topo = a.label();
        let fim = a.label();
        a.bind(topo);
        a.jmp_imm(Cond::Eq, R1, 0, fim); // 0 → 3: off 2
        a.jmp_reg(Cond::Ne, R1, R2, topo); // 1 → 0: off -2
        a.ja(fim); // 2 → 3: off 0
        a.bind(fim);
        a.exit();
        let i = a.finish().unwrap();
        assert_eq!((i[0].code, i[0].off), (0x15, 2));
        assert_eq!((i[1].code, i[1].regs, i[1].off), (0x5d, 0x21, -2));
        assert_eq!((i[2].code, i[2].off), (0x05, 0));
    }

    #[test]
    fn rotulo_sem_lugar_da_erro() {
        let mut a = Asm::new();
        let l = a.label();
        a.ja(l);
        assert!(a.finish().is_err());
    }

    #[test]
    fn soma_atomica() {
        let mut a = Asm::new();
        a.atomic_add64(R0, 0, R1);
        // BPF_STX | BPF_ATOMIC | BPF_DW = 0xdb, origem R1, imediato BPF_ADD.
        assert_eq!(
            a.finish().unwrap()[0].bytes(),
            [0xdb, 0x10, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn largura_dos_campos() {
        assert_eq!(Size::of(8).unwrap(), Size::DW);
        assert_eq!(Size::of(1).unwrap(), Size::B);
        assert!(Size::of(3).is_err());
    }
}
