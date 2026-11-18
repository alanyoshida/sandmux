//! Filtro seccomp por lista de negação.
//!
//! O programa BPF é montado aqui, sem biblioteca externa, para controlar três
//! detalhes que importam para a segurança: a arquitetura é conferida antes de
//! tudo (um binário de outra ABI tem todas as chamadas negadas); no x86_64,
//! números com o bit da ABI x32 são negados, porque senão a mesma chamada
//! passaria com outro número; e os comandos de `ioctl` são comparados só nos 32
//! bits de baixo, que é o que o kernel usa.

use std::io;

use sandmux_spec::SeccompRules;

const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JEQ_K: u16 = 0x15;
const BPF_JGE_K: u16 = 0x35;
const BPF_RET_K: u16 = 0x06;

const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;

const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
const OFF_ARG1_LO: u32 = 16 + 8;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7;

#[cfg(target_arch = "x86_64")]
const X32_SYSCALL_BIT: u32 = 0x4000_0000;

/// Número de uma chamada de sistema pelo nome, na arquitetura atual.
pub fn syscall_number(name: &str) -> Option<i64> {
    let n = match name {
        "mount" => libc::SYS_mount,
        "umount2" => libc::SYS_umount2,
        "pivot_root" => libc::SYS_pivot_root,
        "move_mount" => libc::SYS_move_mount,
        "open_tree" => libc::SYS_open_tree,
        "fsopen" => libc::SYS_fsopen,
        "fsconfig" => libc::SYS_fsconfig,
        "fsmount" => libc::SYS_fsmount,
        "fspick" => libc::SYS_fspick,
        "mount_setattr" => libc::SYS_mount_setattr,
        "bpf" => libc::SYS_bpf,
        "perf_event_open" => libc::SYS_perf_event_open,
        "init_module" => libc::SYS_init_module,
        "finit_module" => libc::SYS_finit_module,
        "delete_module" => libc::SYS_delete_module,
        "kexec_load" => libc::SYS_kexec_load,
        "kexec_file_load" => libc::SYS_kexec_file_load,
        "keyctl" => libc::SYS_keyctl,
        "add_key" => libc::SYS_add_key,
        "request_key" => libc::SYS_request_key,
        "userfaultfd" => libc::SYS_userfaultfd,
        "io_uring_setup" => libc::SYS_io_uring_setup,
        "io_uring_enter" => libc::SYS_io_uring_enter,
        "io_uring_register" => libc::SYS_io_uring_register,
        "ptrace" => libc::SYS_ptrace,
        "chroot" => libc::SYS_chroot,
        _ => return None,
    };
    Some(n)
}

/// Número de um comando de `ioctl` pelo nome.
pub fn ioctl_number(name: &str) -> Option<u32> {
    match name {
        "TIOCSTI" => Some(0x5412),
        "TIOCLINUX" => Some(0x541C),
        _ => None,
    }
}

fn stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

fn deny() -> libc::sock_filter {
    deny_with(libc::EPERM)
}

fn deny_with(errno: i32) -> libc::sock_filter {
    stmt(BPF_RET_K, SECCOMP_RET_ERRNO | errno as u32)
}

/// Monta o programa BPF. Erra se algum nome for desconhecido.
pub fn build(rules: &SeccompRules) -> Result<Vec<libc::sock_filter>, String> {
    let number = |name: &String| {
        syscall_number(name).map(|n| n as u32).ok_or_else(|| {
            sandmux_spec::trf!(
                "chamada de sistema desconhecida: {name}",
                "unknown system call: {name}"
            )
        })
    };
    // Cada chamada com o erro que devolve: as de `ENOSYS` primeiro, para que
    // um nome nas duas listas fique com `ENOSYS`.
    let mut syscalls = Vec::new();
    for name in &rules.enosys_syscalls {
        syscalls.push((number(name)?, libc::ENOSYS));
    }
    for name in &rules.deny_syscalls {
        syscalls.push((number(name)?, libc::EPERM));
    }
    let mut ioctls = Vec::new();
    for name in &rules.deny_ioctls {
        ioctls.push(ioctl_number(name).ok_or_else(|| {
            sandmux_spec::trf!(
                "comando de ioctl desconhecido: {name}",
                "unknown ioctl command: {name}"
            )
        })?);
    }

    let mut p = vec![
        stmt(BPF_LD_W_ABS, OFF_ARCH),
        jump(BPF_JEQ_K, AUDIT_ARCH, 1, 0),
        deny(),
        stmt(BPF_LD_W_ABS, OFF_NR),
    ];
    #[cfg(target_arch = "x86_64")]
    {
        p.push(jump(BPF_JGE_K, X32_SYSCALL_BIT, 0, 1));
        p.push(deny());
    }
    for (n, errno) in syscalls {
        p.push(jump(BPF_JEQ_K, n, 0, 1));
        p.push(deny_with(errno));
    }
    if !ioctls.is_empty() {
        // Se não for ioctl, pula o bloco inteiro: carga + 2 por comando + retorno.
        let skip = 1 + 2 * ioctls.len() + 1;
        let skip = u8::try_from(skip).map_err(|_| {
            sandmux_spec::tr!("comandos de ioctl demais", "too many ioctl commands").to_string()
        })?;
        p.push(jump(BPF_JEQ_K, libc::SYS_ioctl as u32, 0, skip));
        p.push(stmt(BPF_LD_W_ABS, OFF_ARG1_LO));
        for cmd in ioctls {
            p.push(jump(BPF_JEQ_K, cmd, 0, 1));
            p.push(deny());
        }
        p.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
    }
    p.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
    Ok(p)
}

/// Instala o filtro no processo atual. Exige `no_new_privs` já ligado, ou
/// `CAP_SYS_ADMIN` no user namespace do processo (modo de visão).
pub fn install(rules: &SeccompRules) -> io::Result<()> {
    let prog = build(rules).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let fprog = libc::sock_fprog {
        len: prog.len() as u16,
        filter: prog.as_ptr() as *mut libc::sock_filter,
    };
    // SAFETY: `fprog` aponta para `prog`, que vive durante a chamada.
    let r = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &fprog as *const libc::sock_fprog,
            0,
            0,
        )
    };
    if r != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

const SECCOMP_RET_USER_NOTIF: u32 = 0x7fc0_0000;
const SECCOMP_SET_MODE_FILTER: libc::c_ulong = 1;
const SECCOMP_FILTER_FLAG_NEW_LISTENER: libc::c_ulong = 1 << 3;
/// Recebida a notificação, só um sinal fatal interrompe a espera: sem isto,
/// runtimes que preemptam com sinais (o Go, com SIGURG) recomeçam a abertura
/// sem parar.
const SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV: libc::c_ulong = 1 << 5;

/// Chamadas de abertura de arquivo que vão ao supervisor.
#[cfg(target_arch = "x86_64")]
pub const OPEN_SYSCALLS: &[i64] = &[
    libc::SYS_open,
    libc::SYS_openat,
    libc::SYS_openat2,
    libc::SYS_creat,
    libc::SYS_rename,
    libc::SYS_renameat,
    libc::SYS_renameat2,
    libc::SYS_link,
    libc::SYS_linkat,
    libc::SYS_symlink,
    libc::SYS_symlinkat,
];
#[cfg(target_arch = "aarch64")]
pub const OPEN_SYSCALLS: &[i64] = &[
    libc::SYS_openat,
    libc::SYS_openat2,
    libc::SYS_renameat,
    libc::SYS_renameat2,
    libc::SYS_linkat,
    libc::SYS_symlinkat,
];

/// Chamadas de execução que vão ao supervisor, com o registro de execuções
/// pelo supervisor ligado.
pub const EXEC_SYSCALLS: &[i64] = &[libc::SYS_execve, libc::SYS_execveat];

/// O filtro das aberturas: `SECCOMP_RET_USER_NOTIF` para cada chamada de
/// [`OPEN_SYSCALLS`] (e de [`EXEC_SYSCALLS`], com `exec`), o resto passa (o
/// filtro de negação decide antes).
pub fn build_notify(exec: bool) -> Vec<libc::sock_filter> {
    let calls: Vec<i64> = OPEN_SYSCALLS
        .iter()
        .chain(if exec { EXEC_SYSCALLS } else { &[] })
        .copied()
        .collect();
    let n = calls.len();
    let mut p = vec![
        stmt(BPF_LD_W_ABS, OFF_ARCH),
        jump(BPF_JEQ_K, AUDIT_ARCH, 1, 0),
        stmt(BPF_RET_K, SECCOMP_RET_ALLOW),
        stmt(BPF_LD_W_ABS, OFF_NR),
    ];
    for (i, nr) in calls.iter().enumerate() {
        // Achou: pula até o retorno da notificação, depois do retorno comum.
        p.push(jump(BPF_JEQ_K, *nr as u32, (n - i) as u8, 0));
    }
    p.push(stmt(BPF_RET_K, SECCOMP_RET_ALLOW));
    p.push(stmt(BPF_RET_K, SECCOMP_RET_USER_NOTIF));
    p
}

/// Instala o filtro das aberturas e devolve o descritor de notificação.
/// Exige `no_new_privs` já ligado.
pub fn install_notify(exec: bool) -> io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    let prog = build_notify(exec);
    let fprog = libc::sock_fprog {
        len: prog.len() as u16,
        filter: prog.as_ptr() as *mut libc::sock_filter,
    };
    // SAFETY: `fprog` aponta para `prog`, que vive durante a chamada.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            SECCOMP_SET_MODE_FILTER,
            SECCOMP_FILTER_FLAG_NEW_LISTENER | SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV,
            &fprog as *const libc::sock_fprog,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descritor novo, de posse exclusiva.
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd as i32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Roda o programa BPF sobre um `seccomp_data` mínimo (nr e arch).
    fn eval(p: &[libc::sock_filter], nr: u32, arch: u32) -> u32 {
        let mut pc = 0usize;
        let mut acc = 0u32;
        loop {
            let i = p[pc];
            match i.code {
                BPF_LD_W_ABS => acc = if i.k == OFF_NR { nr } else { arch },
                BPF_JEQ_K => {
                    pc += if acc == i.k { i.jt } else { i.jf } as usize;
                }
                BPF_JGE_K => {
                    pc += if acc >= i.k { i.jt } else { i.jf } as usize;
                }
                BPF_RET_K => return i.k,
                _ => panic!("instrução inesperada"),
            }
            pc += 1;
        }
    }

    #[test]
    fn filtro_das_aberturas() {
        let p = build_notify(false);
        for nr in OPEN_SYSCALLS {
            assert_eq!(eval(&p, *nr as u32, AUDIT_ARCH), SECCOMP_RET_USER_NOTIF);
        }
        assert_eq!(
            eval(&p, libc::SYS_execve as u32, AUDIT_ARCH),
            SECCOMP_RET_ALLOW
        );
        let x = build_notify(true);
        for nr in OPEN_SYSCALLS.iter().chain(EXEC_SYSCALLS) {
            assert_eq!(eval(&x, *nr as u32, AUDIT_ARCH), SECCOMP_RET_USER_NOTIF);
        }
        assert_eq!(
            eval(&x, libc::SYS_read as u32, AUDIT_ARCH),
            SECCOMP_RET_ALLOW
        );
        assert_eq!(
            eval(&p, libc::SYS_read as u32, AUDIT_ARCH),
            SECCOMP_RET_ALLOW
        );
        assert_eq!(eval(&p, libc::SYS_openat as u32, 1), SECCOMP_RET_ALLOW);
    }

    #[test]
    fn nome_desconhecido_e_recusado() {
        let r = SeccompRules {
            deny_syscalls: vec!["nao_existe".into()],
            ..Default::default()
        };
        assert!(build(&r).unwrap_err().contains("nao_existe"));
        let r = SeccompRules {
            enosys_syscalls: vec!["outro".into()],
            ..Default::default()
        };
        assert!(build(&r).unwrap_err().contains("outro"));
    }

    #[test]
    fn erro_certo_por_chamada() {
        let r = SeccompRules {
            deny_syscalls: vec!["bpf".into(), "keyctl".into()],
            enosys_syscalls: vec!["keyctl".into(), "add_key".into()],
            ..Default::default()
        };
        let p = build(&r).unwrap();
        let eperm = SECCOMP_RET_ERRNO | libc::EPERM as u32;
        let enosys = SECCOMP_RET_ERRNO | libc::ENOSYS as u32;
        let nr = |n: i64| n as u32;
        assert_eq!(eval(&p, nr(libc::SYS_bpf), AUDIT_ARCH), eperm);
        // Nas duas listas, vale o ENOSYS.
        assert_eq!(eval(&p, nr(libc::SYS_keyctl), AUDIT_ARCH), enosys);
        assert_eq!(eval(&p, nr(libc::SYS_add_key), AUDIT_ARCH), enosys);
        assert_eq!(eval(&p, nr(libc::SYS_read), AUDIT_ARCH), SECCOMP_RET_ALLOW);
        // Outra arquitetura: tudo negado.
        assert_eq!(eval(&p, nr(libc::SYS_read), 1), eperm);
        #[cfg(target_arch = "x86_64")]
        {
            // O número x32 da mesma chamada também é negado.
            assert_eq!(
                eval(&p, nr(libc::SYS_keyctl) | X32_SYSCALL_BIT, AUDIT_ARCH),
                eperm
            );
            assert_eq!(
                eval(&p, nr(libc::SYS_read) | X32_SYSCALL_BIT, AUDIT_ARCH),
                eperm
            );
        }
    }

    #[test]
    fn saltos_do_bloco_de_ioctl_apontam_para_o_retorno() {
        let r = SeccompRules {
            deny_syscalls: vec!["mount".into()],
            deny_ioctls: vec!["TIOCSTI".into(), "TIOCLINUX".into()],
            enosys_syscalls: vec!["keyctl".into()],
        };
        let p = build(&r).unwrap();
        let i = p
            .iter()
            .position(|f| f.code == BPF_JEQ_K && f.k == libc::SYS_ioctl as u32)
            .unwrap();
        let target = i + 1 + p[i].jf as usize;
        assert_eq!(
            target,
            p.len() - 1,
            "salto de não-ioctl deve cair no ALLOW final"
        );
        assert_eq!(p[target - 1].k, SECCOMP_RET_ALLOW);
    }
}
