//! `sandmux doctor`: verificações de suporte, com causa e correção.

use std::path::PathBuf;

use anyhow::Result;
use sandmux_launcher::REQUIRED_ABI;
use sandmux_launcher::support;
use sandmux_policy::{Host, Layout};
use sandmux_spec::{tr, trf};

const PROFILE_TEMPLATE: &str = include_str!("../../../packaging/apparmor/sandmux-run");

fn runner_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sandmux-run")))
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin/sandmux-run"))
}

/// Perfil AppArmor com o caminho real do lançador.
pub fn apparmor_profile() -> String {
    PROFILE_TEMPLATE.replace("@SANDMUX_RUN@", &runner_path().to_string_lossy())
}

/// A linha `[ok  ]` ou `[FALHA]` do terminal.
fn line(ok: bool, label: &str, detail: &str) -> String {
    let mark = if ok { "ok  " } else { tr!("FALHA", "FAIL") };
    format!(
        "[{mark}] {label}{}",
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    )
}

pub fn run() -> Result<i32> {
    run_with(false)
}

/// Uma chave real encontrada, para mostrar (sem o valor).
fn describe(harness: &str, r: &sandmux_import::RealSecrets) -> Vec<String> {
    r.found
        .iter()
        .map(|x| {
            let prov = crate::vaultcmd::provider_of(&r.original, x.line)
                .map(|p| trf!(", provedor `{p}`", ", provider `{p}`"))
                .unwrap_or_default();
            trf!(
                "{harness}: {}{prov}, chave `{}` (linha {})",
                "{harness}: {}{prov}, key `{}` (line {})",
                r.file.display(),
                x.key,
                x.line
            )
        })
        .collect()
}

/// As chaves reais nos HOMEs sintéticos dos harnesses.
pub(crate) fn real_secrets(host: &Host) -> Vec<(&'static str, sandmux_import::RealSecrets)> {
    let layout = Layout::new(&host.data_dir);
    let mut out = Vec::new();
    for &h in sandmux_import::SCANNED {
        let home = layout.home(h);
        if home.is_dir() {
            out.extend(
                sandmux_import::find_real_secrets(h, &home)
                    .into_iter()
                    .map(|r| (h, r)),
            );
        }
    }
    out
}

/// O login real do Google gravado pelo `agy` no HOME sintético (um login
/// feito dentro do sandbox antes da captura).
pub(crate) fn agy_login(
    host: &Host,
) -> Option<(std::path::PathBuf, sandmux_import::agy_login::Login)> {
    let path = Layout::new(&host.data_dir)
        .home("agy")
        .join(sandmux_import::agy_login::TOKEN_FILE);
    let l = sandmux_import::agy_login::Login::read(&path)?;
    (!l.reals().is_empty()).then_some((path, l))
}

fn agy_login_line(l: &sandmux_import::agy_login::Login) -> String {
    trf!(
        "agy: {}, login do Google ({} token(s))",
        "agy: {}, Google login ({} token(s))",
        sandmux_import::agy_login::TOKEN_FILE,
        l.reals().len()
    )
}

/// O nome no cofre de uma chave real achada num HOME sintético.
pub(crate) fn key_name(
    h: &str,
    r: &sandmux_import::RealSecrets,
    x: &sandmux_import::secrets::Replacement,
) -> String {
    match crate::vaultcmd::provider_of(&r.original, x.line) {
        Some(p) => format!("{h}:{}:{p}.{}", r.file.display(), x.key),
        None => format!("{h}:{}:{}", r.file.display(), x.key),
    }
}

/// Os domínios sugeridos para uma chave real na TUI: a inferência do
/// `doctor --fix`, ou, sem ela, o host da URL mais próxima no arquivo.
pub(crate) fn suggest_domains(
    host: &Host,
    h: &str,
    r: &sandmux_import::RealSecrets,
    x: &sandmux_import::secrets::Replacement,
) -> Vec<String> {
    let provider = crate::vaultcmd::provider_of(&r.original, x.line);
    let d =
        crate::vaultcmd::infer_domains_for(host, h, &x.key, provider.as_deref(), Some(&r.original));
    if !d.is_empty() {
        return d;
    }
    sandmux::secretdomains::nearby_url_domain(&r.original, x.line)
        .into_iter()
        .collect()
}

/// `sandmux doctor --fix`: leva ao cofre as chaves reais dos HOMEs
/// sintéticos e deixa os falsos nos arquivos (só depois de o cofre aceitar
/// todas as do arquivo).
fn fix_secrets(host: &Host) -> Result<i32> {
    use crate::vaultcmd::{Input, ask_domain, call, infer_domains_for, provider_of};
    use sandmux_proto::Msg;
    use std::io::IsTerminal;
    let found = real_secrets(host);
    let agy = agy_login(host);
    if found.is_empty() && agy.is_none() {
        println!(
            "{}",
            tr!(
                "Nenhuma chave real dentro do sandbox: nada a mover.",
                "No real keys inside the sandbox: nothing to move."
            )
        );
        return Ok(0);
    }
    let mut input = Input::new();
    match call(&Msg::VaultStatus)? {
        Msg::VaultInfo { exists: false, .. } => {
            anyhow::bail!(tr!(
                "não há cofre: crie um com `sandmux vault init` e rode `sandmux doctor --fix` de novo",
                "there is no vault: create one with `sandmux vault init` and run `sandmux doctor --fix` again"
            ))
        }
        Msg::VaultInfo {
            unlocked: false, ..
        } => {
            let p = input.secret(tr!("Senha do cofre: ", "Vault password: "))?;
            call(&Msg::VaultUnlock {
                password: p.to_string(),
            })?;
        }
        _ => {}
    }
    let layout = Layout::new(&host.data_dir);
    let tty = std::io::stdin().is_terminal();
    let mut moved = 0;
    let mut harnesses = Vec::new();
    for (h, r) in &found {
        println!();
        for l in describe(h, r) {
            println!("  {l}");
        }
        if tty {
            eprint!(
                "{}",
                trf!(
                    "Mover {} chave(s) de {} para o cofre? [S/n] ",
                    "Move {} key(s) from {} to the vault? [Y/n] ",
                    r.found.len(),
                    r.file.display()
                )
            );
            let mut l = String::new();
            std::io::stdin().read_line(&mut l)?;
            let t = l.trim().to_lowercase();
            if !(t.is_empty() || t == "s" || t == "sim" || t == "y") {
                continue;
            }
        }
        let mut ok = true;
        for x in &r.found {
            let provider = provider_of(&r.original, x.line);
            let name = key_name(h, r, x);
            let mut domains =
                infer_domains_for(host, h, &x.key, provider.as_deref(), Some(&r.original));
            if domains.is_empty() {
                eprintln!("{name}:");
                domains = ask_domain(&mut input)?;
            }
            match call(&Msg::VaultAdd {
                name: name.clone(),
                value: x.real.clone(),
                domains: domains.clone(),
                fake: Some(x.fake.clone()),
                tools: Vec::new(),
            }) {
                Ok(_) if domains.is_empty() => {
                    println!(
                        "{}",
                        trf!(
                            "  {name}: no cofre, sem domínio (não há troca no tráfego)",
                            "  {name}: in the vault, no domain (no swap in traffic)"
                        )
                    )
                }
                Ok(_) => println!(
                    "{}",
                    trf!(
                        "  {name}: no cofre ({})",
                        "  {name}: in the vault ({})",
                        domains.join(", ")
                    )
                ),
                Err(e) => {
                    println!(
                        "{}",
                        trf!(
                            "  {name}: o cofre recusou ({e:#}); {} fica como estava",
                            "  {name}: the vault refused ({e:#}); {} stays as it was",
                            r.file.display()
                        )
                    );
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            sandmux_import::apply_fakes(h, &layout.home(h), &layout.manifest(h), r)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            moved += r.found.len();
            if !harnesses.contains(h) {
                harnesses.push(*h);
            }
        }
    }
    if let Some((path, l)) = &agy {
        println!("\n  {}", agy_login_line(l));
        let yes = !tty || {
            eprint!(
                "{}",
                tr!(
                    "Mover o login do Google do agy para o cofre? [S/n] ",
                    "Move the agy Google login to the vault? [Y/n] "
                )
            );
            let mut a = String::new();
            std::io::stdin().read_line(&mut a)?;
            let t = a.trim().to_lowercase();
            t.is_empty() || t == "s" || t == "sim" || t == "y"
        };
        if yes {
            match crate::vaultcmd::store_agy_login(l, path) {
                Ok(()) => {
                    println!(
                        "  agy:access, agy:refresh: {}",
                        tr!("no cofre", "in the vault")
                    );
                    moved += l.reals().len();
                    harnesses.push("agy");
                }
                Err(e) => println!(
                    "{}",
                    trf!(
                        "  agy: o cofre recusou ({e:#}); o arquivo fica como estava",
                        "  agy: the vault refused ({e:#}); the file stays as it was"
                    )
                ),
            }
        }
    }
    println!(
        "{}",
        trf!(
            "\n{moved} chave(s) levadas ao cofre; os arquivos têm os valores falsos.",
            "\n{moved} key(s) moved to the vault; the files have the fake values."
        )
    );
    if !harnesses.is_empty() {
        println!(
            "{}",
            trf!(
                "Reinicie as abas de {} (o harness pode ter a chave antiga na memória).",
                "Restart the {} tabs (the harness may have the old key in memory).",
                harnesses.join(tr!(" e ", " and "))
            )
        );
    }
    Ok(0)
}

// ---- o relatório ----

/// O resultado de uma verificação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    /// Um aviso que não impede nada.
    Warn,
    Fail,
    /// Só informação.
    Info,
}

/// Uma verificação: a linha da TUI, a causa e a correção, e o texto que o
/// terminal imprime.
#[derive(Debug, Clone)]
pub struct Check {
    pub state: State,
    pub label: String,
    pub summary: String,
    /// Causa e correção, mostradas embaixo na TUI.
    pub help: String,
    /// O texto do `sandmux doctor` no terminal; vazio quando sai junto com
    /// outra verificação.
    pub cli: String,
}

impl Check {
    fn new(state: State, label: impl Into<String>, summary: impl Into<String>) -> Check {
        Check {
            state,
            label: label.into(),
            summary: summary.into(),
            help: String::new(),
            cli: String::new(),
        }
    }

    /// Uma linha de seção do terminal: `  rótulo: resumo`.
    fn item(state: State, label: &str, summary: impl Into<String>) -> Check {
        let mut c = Check::new(state, label, summary);
        c.cli = format!("  {label}: {}", c.summary);
        c
    }

    fn help(mut self, h: impl Into<String>) -> Check {
        self.help = h.into();
        self
    }

    fn cli(mut self, c: impl Into<String>) -> Check {
        self.cli = c.into();
        self
    }

    /// Põe um texto antes do `cli` (o título da seção no terminal).
    fn after(mut self, head: &str) -> Check {
        self.cli = format!("{head}\n{}", self.cli);
        self
    }
}

/// Um grupo de verificações.
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub checks: Vec<Check>,
}

/// O diagnóstico inteiro.
#[derive(Debug, Clone)]
pub struct Report {
    pub groups: Vec<Group>,
    /// O sandbox pode ser lançado nesta máquina.
    pub can_launch: bool,
    /// Quantas chaves reais há dentro do sandbox (o que `f` pode mover).
    pub real_keys: usize,
    /// O fim do texto do terminal.
    footer: String,
}

impl Report {
    /// Quantas verificações falharam.
    pub fn failures(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| &g.checks)
            .filter(|c| c.state == State::Fail)
            .count()
    }

    /// O texto do `sandmux doctor` no terminal.
    pub fn print(&self) {
        println!("sandmux doctor\n");
        for c in self.groups.iter().flat_map(|g| &g.checks) {
            if !c.cli.is_empty() {
                println!("{}", c.cli);
            }
        }
        println!("{}", self.footer);
    }
}

/// Um texto de várias linhas recuadas numa linha só, para a TUI.
fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Uma verificação de suporte: a linha `[ok  ]` ou `[FALHA]` e, na falha, a
/// dica.
fn support_check(ok: bool, label: &str, detail: &str, hint: Option<String>) -> Check {
    let mut cli = line(ok, label, detail);
    let mut help = String::new();
    if let Some(h) = hint {
        cli.push('\n');
        cli.push_str(&h);
        help = flat(&h);
    }
    Check {
        state: if ok { State::Ok } else { State::Fail },
        label: label.to_string(),
        summary: detail.to_string(),
        help,
        cli,
    }
}

/// O diagnóstico, sem imprimir nada. `quiet` não deixa o lançador escrever
/// no terminal (a TUI).
pub fn report(quiet: bool) -> Report {
    let runner = sandmux::launch::runner_path();
    let mut r = if quiet {
        support::check_quiet(&runner)
    } else {
        support::check_via(&runner)
    };
    // Mesma variável do `run`: só desliga a camada, para conferir o modo de recuo.
    if std::env::var_os("SANDMUX_DISABLE_OVERLAY").is_some() && r.overlay.is_ok() {
        r.overlay = Err(tr!(
            "desligada por SANDMUX_DISABLE_OVERLAY",
            "turned off by SANDMUX_DISABLE_OVERLAY"
        )
        .into());
    }
    // Só muda o texto do diagnóstico: simula a leitura da restrição do AppArmor.
    if std::env::var_os("SANDMUX_ASSUME_APPARMOR_RESTRICT").is_some() {
        r.apparmor_restrict_userns = Some(true);
    }
    let host = Host::detect().ok();
    let mut real_keys = 0;
    let mut groups = vec![sandbox_group(&r, host.as_ref())];
    if let Some(h) = &host {
        groups.push(install_group(h));
    }
    groups.push(config_group());
    groups.push(daemon_group());
    if let Some(h) = &host {
        groups.push(vault_group(h));
        let (g, n) = secrets_group(h);
        real_keys = n;
        groups.push(g);
    }
    let footer = format!(
        "{}\n{}",
        tr!(
            "\nRede: o sandbox sai pelo gateway do sandmux, com o modo de rede, a denylist e o\n  registro de cada conexão (`sandmux net`).",
            "\nNetwork: the sandbox goes out through the sandmux gateway, with the network mode, the denylist and\n  a log of each connection (`sandmux net`)."
        ),
        tr!(
            "Arquivos com cara de segredo na pasta de trabalho ficam escondidos do agente\n  (`sandmux files` e a política de arquivos).",
            "Files that look like secrets in the work folder are hidden from the agent\n  (`sandmux files` and the file policy)."
        )
    );
    Report {
        groups,
        can_launch: r.can_launch(),
        real_keys,
        footer,
    }
}

/// O diagnóstico; com `fix`, leva as chaves reais dos HOMEs sintéticos ao
/// cofre.
pub fn run_with(fix: bool) -> Result<i32> {
    if fix {
        let host = Host::detect()?;
        return fix_secrets(&host);
    }
    let r = report(false);
    r.print();
    Ok(if r.can_launch { 0 } else { 1 })
}

/// A ajuda do Landlock que falta: os dois caminhos. Um kernel 6.12 ou mais
/// novo (no Ubuntu 24.04, o kernel HWE) e, com o Landlock ABI 4 ou 5, o
/// auxiliar root com o `bpf` na lista de LSMs do boot. O `sandmux setup`
/// não muda o boot: os comandos ficam aqui.
fn landlock_help(
    abi: i32,
    os_release: &str,
    lsm: Option<&str>,
    helper: Option<&Result<(), String>>,
) -> String {
    let ubuntu_2404 = os_release.lines().any(|l| l == "ID=ubuntu")
        && os_release.lines().any(|l| l == "VERSION_ID=\"24.04\"");
    let mut out = if ubuntu_2404 {
        tr!(
            "Sem ele o sandbox não sobe.\nCaminho 1, o recomendado: o kernel HWE do Ubuntu 24.04, que tem o Landlock completo. Rode `sudo apt install linux-generic-hwe-24.04` e reinicie.",
            "Without it the sandbox does not start.\nPath 1, the recommended one: the Ubuntu 24.04 HWE kernel, which has the full Landlock. Run `sudo apt install linux-generic-hwe-24.04` and reboot."
        )
        .to_string()
    } else {
        tr!(
            "Sem ele o sandbox não sobe.\nCaminho 1: um kernel 6.12 ou mais novo, com o Landlock ligado (lsm=landlock).",
            "Without it the sandbox does not start.\nPath 1: a kernel 6.12 or newer, with Landlock enabled (lsm=landlock)."
        )
        .to_string()
    };
    if abi < 4 {
        return out;
    }
    let has_bpf = lsm.is_some_and(sandmux_ebpf::lsm_has_bpf);
    out.push('\n');
    if has_bpf {
        out.push_str(tr!(
            "Caminho 2: o BPF LSM já está ligado; falta o auxiliar root. Rode `sandmux setup`, que o instala com sudo.",
            "Path 2: the BPF LSM is already on; the root helper is missing. Run `sandmux setup`, which installs it with sudo."
        ));
        if let Some(Err(e)) = helper {
            out.push_str(&trf!(
                " (O auxiliar respondeu: {e}.)",
                " (The helper said: {e}.)"
            ));
        }
    } else {
        let list = lsm
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .unwrap_or("landlock,lockdown,yama,integrity,apparmor");
        out.push_str(&trf!(
            "Caminho 2: o auxiliar root completa o Landlock por BPF LSM, que precisa estar na lista de LSMs do boot. Rode:\n\necho 'GRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT lsm={list},bpf\"' | sudo tee /etc/default/grub.d/99-sandmux-lsm.cfg\nsudo update-grub\n\nreinicie, e rode `sandmux setup` para instalar o auxiliar.",
            "Path 2: the root helper completes Landlock through the BPF LSM, which must be in the boot LSM list. Run:\n\necho 'GRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT lsm={list},bpf\"' | sudo tee /etc/default/grub.d/99-sandmux-lsm.cfg\nsudo update-grub\n\nreboot, and run `sandmux setup` to install the helper."
        ));
    }
    out
}

fn sandbox_group(r: &support::SupportReport, host: Option<&Host>) -> Group {
    let mut checks = Vec::new();
    let abi_detail = if r.landlock_abi < 0 {
        tr!("ausente no kernel", "missing from the kernel").to_string()
    } else if r.landlock_full() {
        trf!("ABI {}", "ABI {}", r.landlock_abi)
    } else if r.landlock_ok() {
        trf!(
            "ABI {}, completado pelo auxiliar root (BPF LSM)",
            "ABI {}, completed by the root helper (BPF LSM)",
            r.landlock_abi
        )
    } else {
        trf!(
            "ABI {} (exige {REQUIRED_ABI}, kernel 6.12 ou mais novo, ou a 4 com o auxiliar root)",
            "ABI {} (requires {REQUIRED_ABI}, kernel 6.12 or newer, or 4 with the root helper)",
            r.landlock_abi
        )
    };
    let hint = (!r.landlock_ok()).then(|| {
        let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let lsm = std::fs::read_to_string(sandmux_ebpf::LSM_LIST).ok();
        landlock_help(r.landlock_abi, &os, lsm.as_deref(), r.helper.as_ref())
    });
    let c = support_check(
        r.landlock_ok(),
        tr!(
            "Landlock com escopo de sockets abstratos e de sinais",
            "Landlock with abstract socket and signal scoping"
        ),
        &abi_detail,
        hint.map(|h| format!("  {}", h.replace('\n', "\n  "))),
    );
    checks.push(c);

    let userns = tr!(
        "user namespace sem privilégio, com montagem",
        "unprivileged user namespace, with mounts"
    );
    checks.push(match &r.userns {
        Ok(()) => support_check(true, userns, "", None),
        Err(e) if r.apparmor_restrict_userns == Some(true) => {
            // Sem recuo nas linhas do comando: o heredoc só termina com
            // `EOF` no começo da linha.
            let hint = trf!(
                "\n  A distro restringe user namespaces sem privilégio por AppArmor\n  (kernel.apparmor_restrict_unprivileged_userns=1). O lançador precisa de um\n  perfil próprio. Para gravar o perfil em /etc/apparmor.d/sandmux-run e\n  carregá-lo, cole no terminal:\n\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n{}EOF\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n",
                "\n  The distro restricts unprivileged user namespaces through AppArmor\n  (kernel.apparmor_restrict_unprivileged_userns=1). The launcher needs its own\n  profile. To write the profile to /etc/apparmor.d/sandmux-run and\n  load it, paste into the terminal:\n\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n{}EOF\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n",
                apparmor_profile()
            );
            support_check(false, userns, e, Some(hint)).help(tr!(
                "A distro restringe user namespaces sem privilégio por AppArmor (kernel.apparmor_restrict_unprivileged_userns=1), e o lançador precisa de um perfil próprio. Rode `sandmux doctor` no terminal: ele mostra o comando para colar, que grava o perfil em /etc/apparmor.d/sandmux-run e o carrega (com sudo).",
                "The distro restricts unprivileged user namespaces through AppArmor (kernel.apparmor_restrict_unprivileged_userns=1), and the launcher needs its own profile. Run `sandmux doctor` in the terminal: it shows the command to paste, which writes the profile to /etc/apparmor.d/sandmux-run and loads it (with sudo)."
            ))
        }
        Err(e) => support_check(
            false,
            userns,
            e,
            Some(
                tr!(
                    "\n  Confira se user namespaces sem privilégio estão habilitados no kernel\n  (user.max_user_namespaces maior que zero).\n",
                    "\n  Check that unprivileged user namespaces are enabled in the kernel\n  (user.max_user_namespaces greater than zero).\n"
                )
                .into(),
            ),
        ),
    });

    let overlay = tr!(
        "camada de escrita (overlay em user namespace)",
        "write layer (overlay in a user namespace)"
    );
    checks.push(match &r.overlay {
        Ok(()) => support_check(true, overlay, "", None),
        Err(e) => support_check(
            false,
            overlay,
            e,
            Some(
                tr!(
                    "  Sem ela, os caches começam vazios e as toolchains ficam em somente leitura.\n  O sandbox funciona mesmo assim.",
                    "  Without it, caches start empty and toolchains are read-only.\n  The sandbox works anyway."
                )
                .into(),
            ),
        ),
    });

    let limits = tr!(
        "tetos de memória e de processos (escopo do systemd de usuário)",
        "memory and process caps (user systemd scope)"
    );
    checks.push(match sandmux_policy::limits::support() {
        Ok(()) => {
            let total = sandmux_policy::limits::mem_total().unwrap_or(0);
            support_check(
                true,
                limits,
                &trf!(
                    "padrão: {} de memória (75% da RAM) e 8192 processos",
                    "default: {} of memory (75% of RAM) and 8192 processes",
                    sandmux_policy::limits::human(total / 100 * 75)
                ),
                None,
            )
        }
        Err(e) => support_check(
            false,
            limits,
            &e,
            Some(
                tr!(
                    "  Sem eles, as sessões rodam sem teto de memória e de processos. O sandbox\n  funciona mesmo assim. Os tetos precisam do systemd de usuário (systemd-run\n  --user) com os controladores memory e pids delegados ao user@<uid>.service.",
                    "  Without them, sessions run with no memory or process cap. The sandbox\n  works anyway. The caps need user systemd (systemd-run\n  --user) with the memory and pids controllers delegated to user@<uid>.service."
                )
                .into(),
            ),
        ),
    });

    if let Some(h) = host {
        let docker = tr!(
            "Docker privado por pasta (Docker sem root)",
            "private Docker per folder (rootless Docker)"
        );
        checks.push(match sandmux::docker::support(&h.user, h.uid) {
            Ok(_) => support_check(
                true,
                docker,
                tr!(
                    "rede pelo gateway do sandmux",
                    "network through the sandmux gateway"
                ),
                None,
            ),
            Err(e) => support_check(
                false,
                docker,
                &e,
                Some(
                    tr!(
                        "  Sem ele, as sessões rodam sem Docker privado: o `docker` lá dentro mostra\n  este motivo. O sandbox funciona mesmo assim. O Docker sem root precisa do\n  rootlesskit, do dockerd, do containerd, do newuidmap e do newgidmap, e de uma\n  faixa de 65536 IDs do usuário em /etc/subuid e /etc/subgid.",
                        "  Without it, sessions run with no private Docker: `docker` inside shows\n  this reason. The sandbox works anyway. Rootless Docker needs\n  rootlesskit, dockerd, containerd, newuidmap and newgidmap, and a\n  range of 65536 IDs for the user in /etc/subuid and /etc/subgid."
                    )
                    .into(),
                ),
            ),
        });
        let cache = tr!(
            "cache de imagens do Docker entre pastas",
            "Docker image cache across folders"
        );
        checks.push(match sandmux::docker::registry_binary() {
            Some(b) => support_check(
                true,
                cache,
                &trf!(
                    "espelho do Docker Hub com {}",
                    "Docker Hub mirror with {}",
                    b.display()
                ),
                None,
            ),
            None => support_check(
                false,
                cache,
                tr!(
                    "desligado: falta o binário registry",
                    "off: the registry binary is missing"
                ),
                Some(
                    tr!(
                        "  Sem ele, cada pasta baixa as próprias imagens. Para ligar o cache, instale o\n  pacote docker-registry (Debian e Ubuntu) ou o binário registry do projeto\n  distribution.",
                        "  Without it, each folder pulls its own images. To turn the cache on, install the\n  docker-registry package (Debian and Ubuntu) or the registry binary from the\n  distribution project."
                    )
                    .into(),
                ),
            ),
        });
        let ebpf = tr!("confinamento do Docker por eBPF", "Docker eBPF confinement");
        checks.push(match sandmux::helper::availability() {
            Ok(detail) => support_check(true, ebpf, &detail, None),
            Err(why) => support_check(
                false,
                ebpf,
                &why,
                Some(
                    tr!(
                        "  Sem ele, o Docker privado sobe só com as proteções sem privilégio, e um\n  contêiner com --privileged consegue desfazer as coberturas dos segredos e a\n  proteção do .git. Para ligar, rode `sandmux setup`, que instala o auxiliar\n  root (sandmux-helper) com sudo; o kernel precisa de `bpf` na lista de LSMs\n  (parâmetro lsm= do boot). Detalhes em docs/helper.md.",
                        "  Without it, the private Docker starts with the unprivileged protections only,\n  and a container with --privileged can undo the secret covers and the .git\n  protection. To turn it on, run `sandmux setup`, which installs the root\n  helper (sandmux-helper) with sudo; the kernel needs `bpf` in the LSM list\n  (the lsm= boot parameter). Details in docs/en/helper.md."
                    )
                    .into(),
                ),
            ),
        });
    }
    let tabs = tr!("arquivos das abas pelo kernel", "tab files by the kernel");
    // Opcional: sem o caminho do kernel, o supervisor, como antes.
    checks.push(match sandmux::helper::tab_availability() {
        Ok(detail) => support_check(true, tabs, &detail, None),
        Err(why) => {
            let hint = tr!(
                "  Sem ele, o supervisor do sandmux decide e registra cada abertura de arquivo\n  das abas, de 3 a 3,5 vezes mais lento na leitura em massa. Com o auxiliar\n  root (`sandmux setup`) e o `bpf` na lista de LSMs, o kernel decide. Detalhes\n  em docs/files.md.",
                "  Without it, the sandmux supervisor decides and logs each file open of the\n  tabs, 3 to 3.5 times slower on bulk reads. With the root helper\n  (`sandmux setup`) and `bpf` in the LSM list, the kernel decides. Details in\n  docs/en/files.md."
            );
            Check::new(State::Info, tabs, why.clone())
                .help(flat(hint))
                .cli(format!("[info] {tabs}: {why}\n{hint}"))
        }
    });
    // O auxiliar só troca ao ser reiniciado: depois de atualizar os binários,
    // o antigo continua de pé.
    if let Some(old) = sandmux::helper::outdated() {
        let label = tr!("versão do auxiliar root", "root helper version");
        let summary = trf!(
            "auxiliar root {old}, mais antigo que o sandmux {}",
            "root helper {old}, older than sandmux {}",
            env!("CARGO_PKG_VERSION")
        );
        let hint = tr!(
            "  O auxiliar root em execução ainda é o da versão anterior: ele só troca ao\n  ser reiniciado. O que depende dele pode faltar ou sair incompleto (a árvore\n  de processos, por exemplo, sai sem os pais). Rode `sandmux setup`, que o\n  reinicia com sudo.",
            "  The running root helper is still the previous version's: it only changes\n  when restarted. What depends on it may be missing or incomplete (the\n  process tree, for example, comes without parents). Run `sandmux setup`,\n  which restarts it with sudo."
        );
        let mark = tr!("aviso", "warn");
        checks.push(
            Check::new(State::Warn, label, summary.clone())
                .help(flat(hint))
                .cli(format!("[{mark}] {label}: {summary}\n{hint}")),
        );
    }

    let tun = std::fs::metadata("/dev/net/tun")
        .map(|m| std::os::unix::fs::FileTypeExt::is_char_device(&m.file_type()))
        .unwrap_or(false);
    let log = sandmux::paths::state_dir().join("net");
    let net = tr!(
        "rede pelo gateway (namespace de rede próprio e TUN)",
        "network through the gateway (own network namespace and TUN)"
    );
    checks.push(if tun {
        support_check(
            true,
            net,
            &trf!("registro em {}", "log in {}", log.display()),
            None,
        )
    } else {
        support_check(
            false,
            net,
            tr!("/dev/net/tun não existe", "/dev/net/tun does not exist"),
            Some(
                tr!(
                    "  Sem o TUN, só o modo sem rede funciona (`--network sem-rede` ou\n  `network = \"sem-rede\"`). O módulo do kernel é o `tun`.",
                    "  Without TUN, only the no-network mode works (`--network sem-rede` or\n  `network = \"sem-rede\"`). The kernel module is `tun`."
                )
                .into(),
            ),
        )
    });

    if r.apparmor_restrict_userns == Some(true) && r.userns.is_ok() {
        let note = tr!(
            "\n  Nota: a distro restringe user namespaces por AppArmor, e o lançador passou na\n  verificação (o perfil do sandmux-run está carregado, ou o processo que chamou\n  o sandmux tem um perfil que libera user namespaces).",
            "\n  Note: the distro restricts user namespaces through AppArmor, and the launcher passed\n  the check (the sandmux-run profile is loaded, or the process that called\n  sandmux has a profile that allows user namespaces)."
        );
        checks.push(
            Check::new(
                State::Info,
                "AppArmor",
                tr!(
                    "restringe user namespaces; o lançador passou",
                    "restricts user namespaces; the launcher passed"
                ),
            )
            .help(flat(note))
            .cli(note),
        );
    }
    Group {
        name: tr!("Sandbox", "Sandbox").into(),
        checks,
    }
}

fn install_group(host: &Host) -> Group {
    let layout = Layout::new(&host.data_dir);
    let data = host.data_dir.display().to_string();
    let inbox = layout.inbox().display().to_string();
    let mut checks = vec![
        Check::new(
            State::Info,
            tr!("Diretório de dados", "Data directory"),
            data.clone(),
        )
        .cli(trf!(
            "\nDiretório de dados: {data}",
            "\nData directory: {data}"
        )),
        Check::new(
            State::Info,
            tr!("Pasta de entrada", "Inbox folder"),
            inbox.clone(),
        )
        .help(tr!(
            "Visível no sandbox em ~/inbox, somente leitura.",
            "Visible in the sandbox at ~/inbox, read-only."
        ))
        .cli(trf!(
            "Pasta de entrada (visível em ~/inbox, somente leitura): {inbox}",
            "Inbox folder (visible at ~/inbox, read-only): {inbox}"
        )),
    ];
    // Dois gateways do hermes no mesmo bot disputam as mensagens.
    if let Some(w) = sandmux_policy::hermes_config::host_gateway_alive(&host.home) {
        checks.push(
            Check::new(State::Warn, "Hermes", w.clone())
                .help(w.clone())
                .cli(format!("Hermes: {w}")),
        );
    }
    Group {
        name: tr!("Instalação", "Installation").into(),
        checks,
    }
}

/// Arquivo de configuração: caminho e validade, sem alterar nada.
fn config_group() -> Group {
    use sandmux_policy::config::{self, Config};
    let head = tr!("\nConfiguração:", "\nConfiguration:");
    let name = tr!("Configuração", "Configuration").to_string();
    let status = tr!("estado", "status");
    let path = match Host::detect() {
        Ok(h) => config::path(&h),
        Err(e) => {
            let msg = trf!(
                "não foi possível ler o ambiente: {e}",
                "could not read the environment: {e}"
            );
            let c = Check::new(State::Fail, tr!("ambiente", "environment"), msg.clone())
                .cli(format!("{head}\n  {msg}"));
            return Group {
                name,
                checks: vec![c],
            };
        }
    };
    let mut checks = vec![
        Check::item(
            State::Info,
            tr!("arquivo", "file"),
            path.display().to_string(),
        )
        .after(head),
    ];
    if !path.exists() {
        checks.push(Check::item(
            State::Info,
            status,
            tr!(
                "não existe (só a baseline e os presets valem)",
                "does not exist (only the baseline and presets apply)"
            ),
        ));
        return Group { name, checks };
    }
    checks.push(match Config::load(&path) {
        Ok(c) => {
            let n = |k: usize, um: &str, varios: &str| {
                format!("{k} {}", if k == 1 { um } else { varios })
            };
            Check::item(
                State::Ok,
                status,
                trf!(
                    "válido ({}, {})",
                    "valid ({}, {})",
                    n(
                        c.profiles.len(),
                        tr!("perfil", "profile"),
                        tr!("perfis", "profiles")
                    ),
                    n(
                        c.projects.len(),
                        tr!("projeto", "project"),
                        tr!("projetos", "projects")
                    )
                ),
            )
        }
        Err(e) => {
            let at = e
                .line
                .map(|l| trf!("linha {l}: ", "line {l}: "))
                .unwrap_or_default();
            Check::item(
                State::Fail,
                status,
                trf!("inválido, {at}{}", "invalid, {at}{}", e.message),
            )
            .help(trf!(
                "Corrija o arquivo {} no ponto indicado.",
                "Fix the file {} at the point shown.",
                path.display()
            ))
        }
    });
    Group { name, checks }
}

/// Quantas chaves a importação trocou por valores falsos (todos os
/// manifestos).
pub fn imported_fakes(data_dir: &std::path::Path) -> usize {
    std::fs::read_dir(data_dir.join("manifests"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<sandmux_import::Manifest>(&b).ok())
        .map(|m| m.entries.len())
        .sum()
}

fn vault_group(host: &Host) -> Group {
    let exists = sandmux::daemon::vault::path(&host.data_dir).exists();
    let fakes = imported_fakes(&host.data_dir);
    let state = if exists {
        tr!("existe", "exists")
    } else {
        tr!("não existe", "does not exist")
    };
    let mut checks = vec![
        Check::new(
            if exists { State::Ok } else { State::Info },
            tr!("cofre", "vault"),
            state,
        )
        .cli(trf!("\nCofre: {state}", "\nVault: {state}")),
    ];
    if exists {
        checks.push(Check::item(
            State::Info,
            "Keyring",
            if sandmux::keyring::enabled(&host.data_dir) {
                tr!(
                    "ligado (o daemon abre o cofre com a senha guardada nele)",
                    "on (the daemon opens the vault with the password stored in it)"
                )
            } else {
                tr!("desligado", "off")
            },
        ));
    }
    if !exists && fakes > 0 {
        let text = trf!(
            "  A importação trocou {fakes} chave(s) por valores falsos, e sem cofre nada troca o\n  falso pelo real: os harnesses recebem \"chave inválida\". Crie o cofre e guarde\n  as chaves com `sandmux vault init` e `sandmux vault import`.",
            "  The import replaced {fakes} key(s) with fake values, and without a vault nothing swaps the\n  fake for the real one: the harnesses get \"invalid key\". Create the vault and store\n  the keys with `sandmux vault init` and `sandmux vault import`."
        );
        checks.push(
            Check::new(
                State::Fail,
                tr!("chaves importadas", "imported keys"),
                trf!(
                    "{fakes} com valor falso e sem cofre",
                    "{fakes} with a fake value and no vault"
                ),
            )
            .help(flat(&text))
            .cli(text),
        );
    }
    Group {
        name: tr!("Cofre", "Vault").into(),
        checks,
    }
}

/// O grupo das chaves reais e quantas há.
fn secrets_group(host: &Host) -> (Group, usize) {
    let name = tr!("Chaves reais", "Real keys").to_string();
    let found = real_secrets(host);
    let agy = agy_login(host);
    let n: usize = found.iter().map(|(_, r)| r.found.len()).sum::<usize>()
        + agy.as_ref().map_or(0, |(_, l)| l.reals().len());
    if n == 0 {
        let c = Check::new(
            State::Ok,
            tr!("dentro do sandbox", "inside the sandbox"),
            tr!("nenhuma", "none"),
        )
        .cli(tr!(
            "\nChaves reais dentro do sandbox: nenhuma.",
            "\nReal keys inside the sandbox: none."
        ));
        return (
            Group {
                name,
                checks: vec![c],
            },
            0,
        );
    }
    let help = tr!(
        "O agente pode ler esta chave e enviá-la para fora. A tecla f leva ao cofre: no lugar fica um valor falso, que o gateway troca pelo real só no tráfego ao dono da chave.",
        "The agent can read this key and send it out. The f key moves it to the vault: a fake value takes its place, which the gateway swaps for the real one only in traffic to the key's owner."
    );
    let mut lines = Vec::new();
    let mut checks = Vec::new();
    for (h, r) in &found {
        for (x, l) in r.found.iter().zip(describe(h, r)) {
            let prov = crate::vaultcmd::provider_of(&r.original, x.line)
                .map(|p| trf!("provedor `{p}`, ", "provider `{p}`, "))
                .unwrap_or_default();
            checks.push(
                Check::new(
                    State::Fail,
                    format!("{h}: {}", r.file.display()),
                    trf!(
                        "{prov}chave `{}` (linha {})",
                        "{prov}key `{}` (line {})",
                        x.key,
                        x.line
                    ),
                )
                .help(help),
            );
            lines.push(format!("  {l}"));
        }
    }
    if let Some((_, l)) = &agy {
        checks.push(
            Check::new(
                State::Fail,
                format!("agy: {}", sandmux_import::agy_login::TOKEN_FILE),
                trf!(
                    "login do Google ({} token(s))",
                    "Google login ({} token(s))",
                    l.reals().len()
                ),
            )
            .help(help),
        );
        lines.push(format!("  {}", agy_login_line(l)));
    }
    let cli = format!(
        "{}\n{}\n{}",
        trf!(
            "\nChaves reais dentro do sandbox ({n}): o agente pode lê-las e enviá-las para fora.",
            "\nReal keys inside the sandbox ({n}): the agent can read them and send them out."
        ),
        lines.join("\n"),
        tr!(
            "  Rode `sandmux doctor --fix` para levá-las ao cofre: no lugar fica um valor falso,\n  que o gateway troca pelo real só no tráfego ao dono da chave. Com o cofre aberto,\n  o daemon faz isso sozinho quando o domínio da chave é conhecido.",
            "  Run `sandmux doctor --fix` to move them to the vault: a fake value takes their place,\n  which the gateway swaps for the real one only in traffic to the key's owner. With the\n  vault open, the daemon does this by itself when the key's domain is known."
        )
    );
    checks[0].cli = cli;
    (Group { name, checks }, n)
}

fn daemon_group() -> Group {
    use sandmux::client::{Conn, systemd};
    use sandmux_proto::Msg;
    let status = tr!("estado", "status");
    let mut checks = vec![
        Check::item(
            State::Info,
            "socket",
            sandmux::paths::socket_path().display().to_string(),
        )
        .after(tr!("\nDaemon de sessões:", "\nSession daemon:")),
        Check::item(
            State::Info,
            tr!("início no boot", "start at boot"),
            crate::sessions::boot_text(),
        ),
    ];
    checks.push(match Conn::connect() {
        Ok(Some(mut c)) => match c.call(&Msg::Status) {
            Ok(Msg::StatusInfo {
                pid,
                version,
                start_mode,
                sessions,
                ..
            }) => Check::item(
                State::Ok,
                status,
                trf!(
                    "rodando (PID {pid}, versão {version}, iniciado por {start_mode}, {sessions} sessões)",
                    "running (PID {pid}, version {version}, started by {start_mode}, {sessions} sessions)"
                ),
            ),
            Ok(_) | Err(_) => Check::item(
                State::Fail,
                status,
                tr!(
                    "rodando, mas não respondeu ao pedido de estado",
                    "running, but did not answer the status request"
                ),
            )
            .help(tr!(
                "Reinicie o daemon com `sandmux daemon restart`.",
                "Restart the daemon with `sandmux daemon restart`."
            )),
        },
        Ok(None) => Check::item(State::Info, status, tr!("parado", "stopped")).help(tr!(
            "O daemon sobe sozinho quando uma sessão precisa dele.",
            "The daemon starts by itself when a session needs it."
        )),
        Err(e) => Check::item(State::Fail, status, format!("{e:#}")),
    });
    let unit = systemd::unit_path();
    let label = tr!("unidade systemd de usuário", "user systemd unit");
    let direct = std::env::var("SANDMUX_DAEMON_START").as_deref() == Ok("direct");
    checks.push(if !unit.exists() {
        Check::item(
            State::Info,
            label,
            trf!("não instalada ({})", "not installed ({})", unit.display()),
        )
    } else if direct || !systemd::available() {
        Check::item(
            State::Info,
            label,
            trf!("instalada ({})", "installed ({})", unit.display()),
        )
    } else {
        let state = match systemd::enabled() {
            Some(true) => tr!(
                "instalada e habilitada no login",
                "installed and enabled at login"
            ),
            Some(false) => tr!("instalada, mas desabilitada", "installed, but disabled"),
            None => tr!("instalada", "installed"),
        };
        Check::item(State::Info, label, format!("{state} ({})", unit.display()))
    });
    Group {
        name: tr!("Daemon", "Daemon").into(),
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOBLE: &str = "PRETTY_NAME=\"Ubuntu 24.04.5 LTS\"\nID=ubuntu\nVERSION_ID=\"24.04\"\n";
    const LSM: &str = "lockdown,capability,landlock,yama,apparmor\n";

    #[test]
    fn ajuda_do_ubuntu_2404_no_6_8() {
        sandmux_spec::lang::set(sandmux_spec::lang::Lang::Pt);
        let h = landlock_help(4, NOBLE, Some(LSM), Some(&Err("sem BPF LSM".into())));
        assert!(
            h.contains("sudo apt install linux-generic-hwe-24.04"),
            "{h}"
        );
        assert!(
            h.contains("lsm=lockdown,capability,landlock,yama,apparmor,bpf"),
            "{h}"
        );
        assert!(
            h.contains("sudo update-grub") && h.contains("sandmux setup"),
            "{h}"
        );
    }

    #[test]
    fn ajuda_com_o_bpf_ligado_e_sem_auxiliar() {
        sandmux_spec::lang::set(sandmux_spec::lang::Lang::Pt);
        let h = landlock_help(
            4,
            NOBLE,
            Some("lockdown,landlock,apparmor,bpf"),
            Some(&Err("não responde".into())),
        );
        assert!(
            h.contains("já está ligado") && h.contains("sandmux setup"),
            "{h}"
        );
        assert!(h.contains("não responde"), "{h}");
        assert!(!h.contains("update-grub"), "{h}");
    }

    #[test]
    fn ajuda_de_kernel_sem_landlock_4() {
        sandmux_spec::lang::set(sandmux_spec::lang::Lang::Pt);
        let h = landlock_help(3, "ID=debian\nVERSION_ID=\"12\"\n", Some(LSM), None);
        assert!(h.contains("6.12 ou mais novo"), "{h}");
        assert!(!h.contains("hwe") && !h.contains("Caminho 2"), "{h}");
    }
}
