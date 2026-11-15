//! Lançador do sandbox do sandmux.
//!
//! Uso: `sandmux-run --spec-fd <N>`, `sandmux-run --view-file <arquivo>` ou
//! `sandmux-run --check`
//!
//! Com `--view-file`, só monta a raiz da especificação no namespace atual e
//! executa o comando, sem namespaces novos, Landlock nem seccomp: é a visão de
//! arquivos do Docker privado, montada dentro do user namespace do
//! `rootlesskit`.
//!
//! Com `--check`, verifica o suporte da máquina neste processo (que é o que
//! recebe o perfil AppArmor) e escreve o resultado em linhas `chave:valor`.
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
    // O idioma vem do daemon por SANDMUX_LANG.
    sandmux_spec::lang::set(sandmux_spec::lang::resolve_env(None));
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--check"] {
        print!("{}", sandmux_launcher::support::check().to_wire());
        exit(0);
    }
    // Shell dentro do sandbox de uma aba: o terminal vai ao supervisor dela.
    if let [flag, path] = args.as_slice()
        && flag == "--enter"
    {
        exit(sandmux_launcher::enter::client(std::path::Path::new(path)));
    }
    if let [flag, path] = args.as_slice()
        && flag == "--view-file"
    {
        let bytes = std::fs::read(path).unwrap_or_else(|e| {
            fail(
                sandmux_spec::trf!("ler a visão {path}: {e}", "read the view {path}: {e}"),
                EXIT_SETUP,
            )
        });
        let spec = LaunchSpec::from_json(&bytes).unwrap_or_else(|e| fail(e, EXIT_SETUP));
        let e = sandmux_launcher::exec_view(&spec);
        fail(&e, e.exit_code());
    }
    let fd: i32 = match args.as_slice() {
        [flag, n] if flag == "--spec-fd" => n.parse().unwrap_or_else(|_| {
            fail(
                sandmux_spec::trf!("descritor inválido: {n}", "invalid descriptor: {n}"),
                EXIT_SETUP,
            )
        }),
        _ => fail(
            sandmux_spec::tr!(
                "uso: sandmux-run --spec-fd <N> | --view-file <arquivo> | --check",
                "usage: sandmux-run --spec-fd <N> | --view-file <file> | --check"
            ),
            EXIT_SETUP,
        ),
    };
    // SAFETY: fcntl só consulta o descritor.
    if fd < 3 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        fail(
            sandmux_spec::trf!(
                "descritor {fd} não está aberto",
                "descriptor {fd} is not open"
            ),
            EXIT_SETUP,
        );
    }
    // SAFETY: o descritor foi herdado para este fim e passa a ser deste processo.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut bytes = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytes) {
        fail(
            sandmux_spec::trf!("ler a especificação: {e}", "read the spec: {e}"),
            EXIT_SETUP,
        );
    }
    drop(file);
    let spec = LaunchSpec::from_json(&bytes).unwrap_or_else(|e| fail(e, EXIT_SETUP));
    drop(bytes);
    match launch(&spec) {
        Ok(code) => exit(code),
        Err(e) => fail(&e, e.exit_code()),
    }
}
