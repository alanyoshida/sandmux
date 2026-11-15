//! `sandmux setup`: o passo de configuração depois de instalar, igual para o
//! script, os pacotes e o `cargo install`. Instala o perfil do AppArmor do
//! lançador (com `sudo`, depois de perguntar), garante o daemon e mostra o
//! diagnóstico.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use sandmux_spec::{tr, trf};

/// Onde fica o perfil do lançador.
pub const PROFILE_PATH: &str = "/etc/apparmor.d/sandmux-run";
/// O controle da restrição de user namespace pelo AppArmor.
const RESTRICT_KNOB: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";

/// O que fazer com o perfil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStep {
    /// O kernel não tem a restrição: o perfil não serve.
    NotNeeded,
    /// A restrição existe, mas falta o `apparmor_parser`.
    NoParser,
    /// O perfil gravado já é o certo.
    UpToDate,
    /// Gravar e carregar o perfil.
    Install,
}

pub fn profile_step(knob: bool, parser: bool, installed: Option<&str>, want: &str) -> ProfileStep {
    if !knob {
        ProfileStep::NotNeeded
    } else if !parser {
        ProfileStep::NoParser
    } else if installed == Some(want) {
        ProfileStep::UpToDate
    } else {
        ProfileStep::Install
    }
}

fn find_parser() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec!["/usr/sbin".into(), "/sbin".into()];
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    dirs.into_iter()
        .map(|d| d.join("apparmor_parser"))
        .find(|p| p.is_file())
}

fn is_root() -> bool {
    // SAFETY: geteuid não falha.
    unsafe { libc::geteuid() == 0 }
}

fn has_sudo() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("sudo").is_file()))
        .unwrap_or(false)
}

/// Os comandos que instalam o perfil, para quem roda como root.
pub fn manual_commands(profile: &str) -> String {
    format!(
        "sudo tee {PROFILE_PATH} >/dev/null <<'EOF'\n{profile}EOF\nsudo apparmor_parser -r {PROFILE_PATH}\n"
    )
}

fn ask(question: &str) -> Result<bool> {
    eprint!("{question} {} ", tr!("[S/n]", "[Y/n]"));
    std::io::stderr().flush()?;
    let mut l = String::new();
    std::io::stdin().read_line(&mut l)?;
    let t = l.trim().to_lowercase();
    Ok(t.is_empty() || t == "s" || t == "sim" || t == "y" || t == "yes")
}

/// Grava e carrega o perfil, direto (root) ou por `sudo`.
fn install_profile(profile: &str, parser: &Path, sudo: bool) -> Result<()> {
    let run = |args: &[&str], input: Option<&str>| -> Result<()> {
        let mut c = if sudo {
            let mut c = Command::new("sudo");
            c.args(args);
            c
        } else {
            let mut c = Command::new(args[0]);
            c.args(&args[1..]);
            c
        };
        c.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stdout(Stdio::null());
        let mut child = c
            .spawn()
            .with_context(|| trf!("executar {}", "run {}", args[0]))?;
        if let Some(i) = input {
            child
                .stdin
                .take()
                .context(tr!("entrada do processo", "process input"))?
                .write_all(i.as_bytes())?;
        }
        if !child.wait()?.success() {
            bail!(trf!("`{}` falhou", "`{}` failed", args.join(" ")));
        }
        Ok(())
    };
    run(&["tee", PROFILE_PATH], Some(profile))?;
    run(&[&parser.to_string_lossy(), "-r", PROFILE_PATH], None)
}

fn installed_msg() -> String {
    trf!(
        "AppArmor: perfil do lançador gravado em {PROFILE_PATH} e carregado.",
        "AppArmor: launcher profile written to {PROFILE_PATH} and loaded."
    )
}

pub fn run(no_sudo: bool, no_daemon: bool) -> Result<i32> {
    let root = is_root();
    let want = crate::doctor::apparmor_profile();
    let installed = std::fs::read_to_string(PROFILE_PATH).ok();
    let parser = find_parser();
    let step = profile_step(
        Path::new(RESTRICT_KNOB).exists(),
        parser.is_some(),
        installed.as_deref(),
        &want,
    );
    let mut ok = true;
    match step {
        ProfileStep::NotNeeded => {
            println!(
                "{}",
                tr!(
                    "AppArmor: o kernel não restringe user namespaces; o perfil não é preciso.",
                    "AppArmor: the kernel does not restrict user namespaces; the profile is not needed."
                )
            )
        }
        ProfileStep::NoParser => println!(
            "{}",
            tr!(
                "AppArmor: o kernel tem a restrição de user namespace, mas falta o apparmor_parser; instale o pacote do AppArmor e rode `sandmux setup` de novo.",
                "AppArmor: the kernel has the user namespace restriction, but apparmor_parser is missing; install the AppArmor package and run `sandmux setup` again."
            )
        ),
        ProfileStep::UpToDate => {
            println!(
                "{}",
                trf!(
                    "AppArmor: o perfil do lançador já está em {PROFILE_PATH}.",
                    "AppArmor: the launcher profile is already at {PROFILE_PATH}."
                )
            )
        }
        ProfileStep::Install => {
            let parser = parser.unwrap_or_else(|| PathBuf::from("apparmor_parser"));
            let interactive = std::io::stdin().is_terminal();
            if root {
                install_profile(&want, &parser, false)?;
                println!("{}", installed_msg());
            } else if !no_sudo
                && interactive
                && has_sudo()
                && ask(&trf!(
                    "O lançador precisa de um perfil do AppArmor em {PROFILE_PATH}. Instalar com sudo?",
                    "The launcher needs an AppArmor profile at {PROFILE_PATH}. Install it with sudo?"
                ))?
            {
                install_profile(&want, &parser, true)?;
                println!("{}", installed_msg());
            } else {
                ok = false;
                println!(
                    "{}",
                    trf!(
                        "AppArmor: o lançador precisa de um perfil próprio. Como root, rode:\n\n{}\nSem ele, o sandbox não sobe enquanto kernel.apparmor_restrict_unprivileged_userns for 1.",
                        "AppArmor: the launcher needs its own profile. As root, run:\n\n{}\nWithout it, the sandbox does not start while kernel.apparmor_restrict_unprivileged_userns is 1.",
                        manual_commands(&want)
                    )
                );
            }
        }
    }
    if root || no_daemon {
        println!(
            "{}",
            tr!(
                "Daemon: não iniciado (o daemon é por usuário; rode `sandmux setup` como o seu usuário).",
                "Daemon: not started (the daemon is per user; run `sandmux setup` as your user)."
            )
        );
    } else {
        sandmux::client::connect_or_start().context(tr!("subir o daemon", "start the daemon"))?;
        println!("{}", tr!("Daemon: rodando.", "Daemon: running."));
    }
    println!();
    let code = crate::doctor::run()?;
    Ok(if ok { code } else { code.max(1) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passo_do_perfil() {
        assert_eq!(profile_step(false, true, None, "p"), ProfileStep::NotNeeded);
        assert_eq!(profile_step(true, false, None, "p"), ProfileStep::NoParser);
        assert_eq!(
            profile_step(true, true, Some("p"), "p"),
            ProfileStep::UpToDate
        );
        assert_eq!(
            profile_step(true, true, Some("velho"), "p"),
            ProfileStep::Install
        );
        assert_eq!(profile_step(true, true, None, "p"), ProfileStep::Install);
        let m = manual_commands("PERFIL\n");
        assert!(m.starts_with("sudo tee /etc/apparmor.d/sandmux-run"));
        assert!(m.contains("\nPERFIL\nEOF\nsudo apparmor_parser -r"));
    }
}
