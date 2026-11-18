//! Observação das execuções: um programa `tp_btf` no tracepoint
//! `sched_process_exec`, filtrado por um cgroup, que manda pela fila de
//! eventos o caminho executado, o processo e os argumentos.
//!
//! O tracepoint vale para a máquina inteira: o programa começa comparando o
//! ancestral do cgroup do processo, no nível dado, com o id pedido, e sai
//! logo quando não é. No tracepoint, o processo já está com a memória nova, e
//! os argumentos ficam entre `mm->arg_start` e `mm->arg_end`.

use std::os::fd::{AsRawFd, OwnedFd};

use sandmux_spec::trf;

use crate::Error;
use crate::asm::{Alu, Asm, Cond, Insn, R0, R1, R2, R3, R6, R7, R8, R10, Size};
use crate::btf::{Btf, Kind};
use crate::ring::{EV_EXEC, EV_HEADER, EV_NAME, EXEC_ARGS};
use crate::sys;

/// Tamanho do evento de execução: cabeçalho, caminho e argumentos.
pub const EXEC_EVENT: usize = EV_HEADER + EV_NAME + EXEC_ARGS;

/// O que o programa precisa saber do kernel, lido do BTF.
#[derive(Clone, Debug)]
pub struct ExecLayout {
    prog_type_tracing: u32,
    attach_raw_tp: u32,
    btf_id: u32,
    fn_ringbuf_reserve: i32,
    fn_ringbuf_submit: i32,
    fn_probe_read_kernel_str: i32,
    fn_probe_read_user: i32,
    fn_ancestor_cgroup_id: i32,
    fn_pid_tgid: i32,
    fn_map_lookup_elem: i32,
    map_ringbuf: u32,
    map_array: u32,
    bprm_filename: i16,
    task_mm: i16,
    mm_arg_start: i16,
    mm_arg_end: i16,
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

impl ExecLayout {
    pub fn new(b: &Btf) -> Result<ExecLayout, Error> {
        let btf_id = b
            .find(Kind::Typedef, "btf_trace_sched_process_exec")
            .ok_or_else(|| {
                Error::Btf(trf!(
                    "o kernel não tem o tracepoint sched_process_exec no BTF",
                    "the kernel has no sched_process_exec tracepoint in BTF"
                ))
            })?;
        Ok(ExecLayout {
            prog_type_tracing: uapi(b, "BPF_PROG_TYPE_TRACING")? as u32,
            attach_raw_tp: uapi(b, "BPF_TRACE_RAW_TP")? as u32,
            btf_id,
            fn_ringbuf_reserve: uapi(b, "BPF_FUNC_ringbuf_reserve")? as i32,
            fn_ringbuf_submit: uapi(b, "BPF_FUNC_ringbuf_submit")? as i32,
            fn_probe_read_kernel_str: uapi(b, "BPF_FUNC_probe_read_kernel_str")? as i32,
            fn_probe_read_user: uapi(b, "BPF_FUNC_probe_read_user")? as i32,
            fn_ancestor_cgroup_id: uapi(b, "BPF_FUNC_get_current_ancestor_cgroup_id")? as i32,
            fn_pid_tgid: uapi(b, "BPF_FUNC_get_current_pid_tgid")? as i32,
            fn_map_lookup_elem: uapi(b, "BPF_FUNC_map_lookup_elem")? as i32,
            map_ringbuf: uapi(b, "BPF_MAP_TYPE_RINGBUF")? as u32,
            map_array: uapi(b, "BPF_MAP_TYPE_ARRAY")? as u32,
            bprm_filename: off(b, "linux_binprm", "filename")?,
            task_mm: off(b, "task_struct", "mm")?,
            mm_arg_start: off(b, "mm_struct", "arg_start")?,
            mm_arg_end: off(b, "mm_struct", "arg_end")?,
        })
    }
}

/// Monta o programa. `level` e `cgroup_id` dizem qual subárvore é
/// observada; `ring_fd` e `drops_fd` são a fila e o contador de descartados.
pub fn build(
    l: &ExecLayout,
    level: i32,
    cgroup_id: u64,
    ring_fd: i32,
    drops_fd: i32,
) -> Result<Vec<Insn>, Error> {
    let mut a = Asm::new();
    let out = a.label();
    let drop = a.label();
    let no_args = a.label();
    let args_len = a.label();
    // R6 = contexto: a tarefa, o pid antigo e o `linux_binprm`.
    a.mov(R6, R1);
    a.mov_imm(R1, level);
    a.call(l.fn_ancestor_cgroup_id);
    a.ld_imm64(R1, cgroup_id);
    a.jmp_reg(Cond::Ne, R0, R1, out);
    // O evento, reservado na própria fila (não cabe na pilha).
    a.ld_map_fd(R1, ring_fd);
    a.mov_imm(R2, EXEC_EVENT as i32);
    a.mov_imm(R3, 0);
    a.call(l.fn_ringbuf_reserve);
    a.jmp_imm(Cond::Eq, R0, 0, drop);
    a.mov(R7, R0);
    a.st(Size::W, R7, 0, EV_EXEC as i32);
    a.st(Size::W, R7, 4, 0);
    a.st(Size::W, R7, 8, 0);
    a.st(Size::W, R7, 12, 0);
    a.st(Size::W, R7, 24, 0);
    a.st(Size::W, R7, 28, 0);
    // O processo (o tgid).
    a.call(l.fn_pid_tgid);
    a.alu_imm(Alu::Rsh, R0, 32);
    a.stx(Size::DW, R7, 16, R0);
    // O caminho executado.
    a.mov(R1, R7);
    a.alu_imm(Alu::Add, R1, EV_HEADER as i32);
    a.mov_imm(R2, EV_NAME as i32);
    a.ldx(Size::DW, R3, R6, 16);
    a.ldx(Size::DW, R3, R3, l.bprm_filename);
    a.call(l.fn_probe_read_kernel_str);
    a.stx(Size::W, R7, 28, R0);
    // Os argumentos: de `mm->arg_start` a `mm->arg_end`, até EXEC_ARGS.
    a.ldx(Size::DW, R1, R6, 0);
    a.ldx(Size::DW, R1, R1, l.task_mm);
    a.jmp_imm(Cond::Eq, R1, 0, no_args);
    a.ldx(Size::DW, R3, R1, l.mm_arg_start);
    a.ldx(Size::DW, R8, R1, l.mm_arg_end);
    a.alu_reg(Alu::Sub, R8, R3);
    a.jmp_imm(Cond::Gt, R8, EXEC_ARGS as i32, args_len);
    let have_len = a.label();
    a.ja(have_len);
    a.bind(args_len);
    a.mov_imm(R8, EXEC_ARGS as i32);
    a.bind(have_len);
    a.mov(R1, R7);
    a.alu_imm(Alu::Add, R1, (EV_HEADER + EV_NAME) as i32);
    a.mov(R2, R8);
    a.call(l.fn_probe_read_user);
    a.jmp_imm(Cond::Ne, R0, 0, no_args);
    a.stx(Size::W, R7, 24, R8);
    a.bind(no_args);
    a.mov(R1, R7);
    a.mov_imm(R2, 0);
    a.call(l.fn_ringbuf_submit);
    a.ja(out);
    // Fila cheia: soma 1 ao contador.
    a.bind(drop);
    a.st(Size::W, R10, -8, 0);
    a.ld_map_fd(R1, drops_fd);
    a.mov(R2, R10);
    a.alu_imm(Alu::Add, R2, -8);
    a.call(l.fn_map_lookup_elem);
    a.jmp_imm(Cond::Eq, R0, 0, out);
    a.mov_imm(R1, 1);
    a.atomic_add64(R0, 0, R1);
    a.bind(out);
    a.mov_imm(R0, 0);
    a.exit();
    a.finish()
}

/// Uma observação carregada: a fila, o contador e o programa preso.
pub struct Observation {
    ring: OwnedFd,
    drops: OwnedFd,
    _prog: OwnedFd,
    _link: OwnedFd,
}

impl Observation {
    /// Carrega e prende o programa, com uma fila e um contador próprios.
    pub fn load(l: &ExecLayout, level: i32, cgroup_id: u64) -> Result<Observation, Error> {
        let ring = sys::map_create(&sys::MapSpec {
            map_type: l.map_ringbuf,
            key_size: 0,
            value_size: 0,
            max_entries: crate::ring::RING_SIZE,
            flags: 0,
            name: "sandmux_exec",
            btf: None,
        })?;
        let drops = sys::map_create(&sys::MapSpec {
            map_type: l.map_array,
            key_size: 4,
            value_size: 8,
            max_entries: 1,
            flags: 0,
            name: "sandmux_xdrops",
            btf: None,
        })?;
        let (prog, link) = Observation::attach(l, level, cgroup_id, &ring, &drops)?;
        Ok(Observation {
            ring,
            drops,
            _prog: prog,
            _link: link,
        })
    }

    /// Carrega o programa numa fila e num contador que já existem (os do
    /// confinamento do Docker) e o prende. Devolve o programa e o link.
    pub fn attach(
        l: &ExecLayout,
        level: i32,
        cgroup_id: u64,
        ring: &impl AsRawFd,
        drops: &impl AsRawFd,
    ) -> Result<(OwnedFd, OwnedFd), Error> {
        let insns = build(l, level, cgroup_id, ring.as_raw_fd(), drops.as_raw_fd())?;
        let prog = sys::prog_load(&sys::ProgSpec {
            prog_type: l.prog_type_tracing,
            expected_attach_type: l.attach_raw_tp,
            attach_btf_id: l.btf_id,
            name: "sm_exec",
            license: crate::prog::LICENSE,
            insns: &insns,
        })?;
        let link = match sys::link_create(&prog, 0, l.attach_raw_tp) {
            Ok(k) => k,
            Err(_) => sys::raw_tracepoint_open(&prog)?,
        };
        Ok((prog, link))
    }

    pub fn ring(&self) -> std::io::Result<OwnedFd> {
        self.ring.try_clone()
    }

    pub fn drops(&self) -> std::io::Result<OwnedFd> {
        self.drops.try_clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programa_de_execucao_monta() {
        if !std::path::Path::new(crate::btf::VMLINUX).exists() {
            return;
        }
        let b = Btf::vmlinux().unwrap();
        let l = ExecLayout::new(&b).unwrap();
        let p = build(&l, 5, 1234, 4, 5).unwrap();
        assert_eq!(p.last().unwrap().code, 0x95);
        // Começa pelo filtro do cgroup, com o id como constante.
        assert!(
            p.iter()
                .take(6)
                .any(|i| i.code == 0x85 && i.imm == l.fn_ancestor_cgroup_id)
        );
        for f in [
            l.fn_ringbuf_reserve,
            l.fn_ringbuf_submit,
            l.fn_probe_read_kernel_str,
            l.fn_probe_read_user,
        ] {
            assert!(
                p.iter().any(|i| i.code == 0x85 && i.imm == f),
                "falta a chamada {f}"
            );
        }
        assert!(
            p.iter()
                .any(|i| i.code == 0x18 && i.regs == 0x11 && i.imm == 4)
        );
        assert!(p.iter().any(|i| i.code == 0xdb));
        assert_eq!(EXEC_EVENT, 32 + 256 + 1024);
    }
}
