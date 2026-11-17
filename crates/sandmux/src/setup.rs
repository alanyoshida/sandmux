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

/// As unidades do auxiliar root, instaladas pelo `sandmux setup`.
pub const UNIT_DIR: &str = "/etc/systemd/system";
/// Onde os pacotes põem as unidades.
pub const PACKAGE_UNIT_DIR: &str = "/usr/lib/systemd/system";
const HELPER_SOCKET_UNIT: &str = include_str!("../../../packaging/systemd/sandmux-helper.socket");
const HELPER_SERVICE_UNIT: &str =
    include_str!("../../../packaging/systemd/sandmux-helper.service.in");

/// As duas unidades (socket e serviço), com o caminho do binário.
pub fn helper_units(bin: &Path) -> (String, String) {
    (
        HELPER_SOCKET_UNIT.to_string(),
        HELPER_SERVICE_UNIT.replace("@SANDMUX_HELPER@", &bin.display().to_string()),
    )
}

/// O binário e cada pasta acima dele são do root e não podem ser alterados
/// por outro usuário? Sem isso, a unidade daria root a quem altera o
/// binário. `meta` devolve o dono e o modo de um caminho.
pub fn helper_path_safe(
    bin: &Path,
    meta: impl Fn(&Path) -> Option<(u32, u32)>,
) -> std::result::Result<(), String> {
    let mut p = Some(bin);
    while let Some(x) = p {
        let Some((uid, mode)) = meta(x) else {
            return Err(trf!(
                "não foi possível ler {}",
                "could not read {}",
                x.display()
            ));
        };
        if uid != 0 {
            return Err(trf!(
                "{} não é do root",
                "{} is not owned by root",
                x.display()
            ));
        }
        if mode & 0o022 != 0 {
            return Err(trf!(
                "{} pode ser alterado por outros usuários",
                "{} can be changed by other users",
                x.display()
            ));
        }
        p = x.parent();
    }
    Ok(())
}

fn real_meta(p: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).ok().map(|m| (m.uid(), m.mode()))
}

/// O que fazer com o auxiliar root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperStep {
    /// Não há `sandmux-helper` ao lado do `sandmux`.
    NoBinary,
    /// O binário fica num caminho que o usuário pode alterar.
    Unsafe(String),
    /// As unidades já estão instaladas (pelo setup ou por um pacote).
    UpToDate,
    /// Gravar as unidades e ligar o socket.
    Install(PathBuf),
}

pub fn helper_step(
    bin: Option<PathBuf>,
    meta: impl Fn(&Path) -> Option<(u32, u32)>,
    installed: Option<&str>,
    package: bool,
) -> HelperStep {
    let Some(bin) = bin else {
        return HelperStep::NoBinary;
    };
    if package {
        return HelperStep::UpToDate;
    }
    if let Err(e) = helper_path_safe(&bin, meta) {
        return HelperStep::Unsafe(e);
    }
    if installed == Some(helper_units(&bin).1.as_str()) {
        return HelperStep::UpToDate;
    }
    HelperStep::Install(bin)
}

/// Os comandos que instalam o auxiliar, para quem roda como root.
pub fn helper_manual_commands(bin: &Path) -> String {
    let (sock, svc) = helper_units(bin);
    format!(
        "sudo tee {UNIT_DIR}/sandmux-helper.socket >/dev/null <<'EOF'\n{sock}EOF\nsudo tee {UNIT_DIR}/sandmux-helper.service >/dev/null <<'EOF'\n{svc}EOF\nsudo systemctl daemon-reload\nsudo systemctl enable --now sandmux-helper.socket\n"
    )
}

fn install_helper(bin: &Path, sudo: bool) -> Result<()> {
    let (sock, svc) = helper_units(bin);
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
    let sock_path = format!("{UNIT_DIR}/sandmux-helper.socket");
    let svc_path = format!("{UNIT_DIR}/sandmux-helper.service");
    run(&["tee", &sock_path], Some(&sock))?;
    run(&["tee", &svc_path], Some(&svc))?;
    run(&["systemctl", "daemon-reload"], None)?;
    run(
        &["systemctl", "enable", "--now", "sandmux-helper.socket"],
        None,
    )?;
    // Um serviço de uma versão anterior que ainda roda pega o binário novo.
    let _ = run(
        &["systemctl", "try-restart", "sandmux-helper.service"],
        None,
    );
    Ok(())
}

/// O `sandmux-helper` ao lado do `sandmux` em execução.
fn helper_binary() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?.canonicalize().ok()?;
    let h = me.parent()?.join("sandmux-helper");
    h.is_file().then_some(h)
}

fn setup_helper(root: bool, no_sudo: bool) -> Result<()> {
    let installed =
        std::fs::read_to_string(Path::new(UNIT_DIR).join("sandmux-helper.service")).ok();
    let package = Path::new(PACKAGE_UNIT_DIR)
        .join("sandmux-helper.service")
        .exists();
    match helper_step(helper_binary(), real_meta, installed.as_deref(), package) {
        HelperStep::NoBinary => println!(
            "{}",
            tr!(
                "Auxiliar root: o sandmux-helper não está ao lado do sandmux; o Docker privado sobe sem o confinamento por eBPF.",
                "Root helper: sandmux-helper is not next to sandmux; the private Docker starts without the eBPF confinement."
            )
        ),
        HelperStep::Unsafe(why) => println!(
            "{}",
            trf!(
                "Auxiliar root: não instalado, porque {why}. O serviço roda como root, então o binário precisa estar numa pasta do root (como /usr/local/bin, pelo install.sh com sudo). O Docker privado sobe sem o confinamento por eBPF.",
                "Root helper: not installed, because {why}. The service runs as root, so the binary must be in a root-owned folder (such as /usr/local/bin, through install.sh with sudo). The private Docker starts without the eBPF confinement."
            )
        ),
        HelperStep::UpToDate => println!(
            "{}",
            tr!(
                "Auxiliar root: as unidades do sandmux-helper já estão instaladas.",
                "Root helper: the sandmux-helper units are already installed."
            )
        ),
        HelperStep::Install(bin) => {
            let interactive = std::io::stdin().is_terminal();
            if root {
                install_helper(&bin, false)?;
            } else if !no_sudo
                && interactive
                && has_sudo()
                && ask(tr!(
                    "O confinamento do Docker privado por eBPF usa um auxiliar root (sandmux-helper), ativado por socket. Instalar as unidades do systemd com sudo?",
                    "The private Docker's eBPF confinement uses a root helper (sandmux-helper), socket-activated. Install the systemd units with sudo?"
                ))?
            {
                install_helper(&bin, true)?;
            } else {
                println!(
                    "{}",
                    trf!(
                        "Auxiliar root: para ligar o confinamento do Docker por eBPF, rode como root:\n\n{}\nSem ele, o Docker privado sobe só com as proteções sem privilégio.",
                        "Root helper: to turn on the Docker eBPF confinement, run as root:\n\n{}\nWithout it, the private Docker starts with the unprivileged protections only.",
                        helper_manual_commands(&bin)
                    )
                );
                return Ok(());
            }
            println!(
                "{}",
                trf!(
                    "Auxiliar root: unidades gravadas em {UNIT_DIR} e socket ligado.",
                    "Root helper: units written to {UNIT_DIR} and socket enabled."
                )
            );
        }
    }
    Ok(())
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
    setup_helper(root, no_sudo)?;
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

    fn meta_de<'a>(tab: &'a [(&'a str, u32, u32)]) -> impl Fn(&Path) -> Option<(u32, u32)> + 'a {
        move |p: &Path| tab.iter().find(|x| Path::new(x.0) == p).map(|x| (x.1, x.2))
    }

    #[test]
    fn auxiliar_so_num_caminho_do_root() {
        let root_tab = [
            ("/", 0, 0o40755),
            ("/usr", 0, 0o40755),
            ("/usr/local", 0, 0o40755),
            ("/usr/local/bin", 0, 0o40755),
            ("/usr/local/bin/sandmux-helper", 0, 0o100755),
        ];
        let bin = PathBuf::from("/usr/local/bin/sandmux-helper");
        assert_eq!(
            helper_step(Some(bin.clone()), meta_de(&root_tab), None, false),
            HelperStep::Install(bin.clone())
        );
        let (_, svc) = helper_units(&bin);
        assert!(svc.contains("ExecStart=/usr/local/bin/sandmux-helper\n"));
        assert_eq!(
            helper_step(Some(bin.clone()), meta_de(&root_tab), Some(&svc), false),
            HelperStep::UpToDate
        );
        // No HOME: do usuário.
        let home_tab = [
            ("/", 0, 0o40755),
            ("/home", 0, 0o40755),
            ("/home/u", 1000, 0o40750),
            ("/home/u/.local", 1000, 0o40755),
            ("/home/u/.local/bin", 1000, 0o40755),
            ("/home/u/.local/bin/sandmux-helper", 1000, 0o100755),
        ];
        let hb = PathBuf::from("/home/u/.local/bin/sandmux-helper");
        match helper_step(Some(hb), meta_de(&home_tab), None, false) {
            HelperStep::Unsafe(why) => assert!(why.contains("sandmux-helper"), "{why}"),
            x => panic!("{x:?}"),
        }
        // Pasta do root, mas gravável por todos.
        let mut tab = root_tab;
        tab[3].2 = 0o40777;
        assert!(matches!(
            helper_step(Some(bin.clone()), meta_de(&tab), None, false),
            HelperStep::Unsafe(_)
        ));
        assert_eq!(
            helper_step(None, meta_de(&root_tab), None, false),
            HelperStep::NoBinary
        );
        // Instalado por pacote: não mexe.
        assert_eq!(
            helper_step(Some(bin.clone()), meta_de(&tab), None, true),
            HelperStep::UpToDate
        );
        let m = helper_manual_commands(&bin);
        assert!(m.contains("sudo systemctl enable --now sandmux-helper.socket"));
        assert!(m.contains("ListenSequentialPacket=/run/sandmux-helper.sock"));
    }
}
