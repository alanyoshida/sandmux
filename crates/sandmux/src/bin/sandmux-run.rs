//! Lançador do sandbox do sandmux.
//!
//! Uso: `sandmux-run --spec-fd <N>`
//!
//! Lê uma especificação de lançamento em JSON do descritor herdado `N`, monta
//! o confinamento que ela descreve e executa o comando dentro dele. Termina com
//! o código de saída do comando, com 125 quando o confinamento não pôde ser
//! montado e com 127 quando o comando não pôde ser executado.

use std::io::Read;
use std::os::fd::FromRawFd;
use std::process::exit;

use sandmux_launcher::{EXIT_SETUP, launch};
use sandmux_spec::LaunchSpec;

fn fail(msg: impl std::fmt::Display, code: i32) -> ! {
    eprintln!("sandmux-run: {msg}");
    exit(code);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fd: i32 = match args.as_slice() {
        [flag, n] if flag == "--spec-fd" => n
            .parse()
            .unwrap_or_else(|_| fail(format!("descritor inválido: {n}"), EXIT_SETUP)),
        _ => fail("uso: sandmux-run --spec-fd <N>", EXIT_SETUP),
    };
    // SAFETY: fcntl só consulta o descritor.
    if fd < 3 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        fail(format!("descritor {fd} não está aberto"), EXIT_SETUP);
    }
    // SAFETY: o descritor foi herdado para este fim e passa a ser deste processo.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut bytes = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytes) {
        fail(format!("ler a especificação: {e}"), EXIT_SETUP);
    }
    drop(file);
    let spec = LaunchSpec::from_json(&bytes).unwrap_or_else(|e| fail(e, EXIT_SETUP));
    drop(bytes);
    match launch(&spec) {
        Ok(code) => exit(code),
        Err(e) => fail(&e, e.exit_code()),
    }
}
