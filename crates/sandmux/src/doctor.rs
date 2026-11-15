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

fn line(ok: bool, label: &str, detail: &str) {
    let mark = if ok { "ok  " } else { tr!("FALHA", "FAIL") };
    println!(
        "[{mark}] {label}{}",
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    );
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
fn real_secrets(host: &Host) -> Vec<(&'static str, sandmux_import::RealSecrets)> {
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

fn secrets_section(host: &Host) -> usize {
    let found = real_secrets(host);
    let n: usize = found.iter().map(|(_, r)| r.found.len()).sum();
    if n == 0 {
        println!(
            "{}",
            tr!(
                "\nChaves reais dentro do sandbox: nenhuma.",
                "\nReal keys inside the sandbox: none."
            )
        );
        return 0;
    }
    println!(
        "{}",
        trf!(
            "\nChaves reais dentro do sandbox ({n}): o agente pode lê-las e enviá-las para fora.",
            "\nReal keys inside the sandbox ({n}): the agent can read them and send them out."
        )
    );
    for (h, r) in &found {
        for l in describe(h, r) {
            println!("  {l}");
        }
    }
    println!(
        "{}",
        tr!(
            "  Rode `sandmux doctor --fix` para levá-las ao cofre: no lugar fica um valor falso,\n  que o gateway troca pelo real só no tráfego ao dono da chave. Com o cofre aberto,\n  o daemon faz isso sozinho quando o domínio da chave é conhecido.",
            "  Run `sandmux doctor --fix` to move them to the vault: a fake value takes their place,\n  which the gateway swaps for the real one only in traffic to the key's owner. With the\n  vault open, the daemon does this by itself when the key's domain is known."
        )
    );
    n
}

/// `sandmux doctor --fix`: leva ao cofre as chaves reais dos HOMEs
/// sintéticos e deixa os falsos nos arquivos (só depois de o cofre aceitar
/// todas as do arquivo).
fn fix_secrets(host: &Host) -> Result<i32> {
    use crate::vaultcmd::{Input, ask_domain, call, infer_domains_for, provider_of};
    use sandmux_proto::Msg;
    use std::io::IsTerminal;
    let found = real_secrets(host);
    if found.is_empty() {
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
            let name = match &provider {
                Some(p) => format!("{h}:{}:{p}.{}", r.file.display(), x.key),
                None => format!("{h}:{}:{}", r.file.display(), x.key),
            };
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

/// O diagnóstico; com `fix`, leva as chaves reais dos HOMEs sintéticos ao
/// cofre.
pub fn run_with(fix: bool) -> Result<i32> {
    if fix {
        let host = Host::detect()?;
        return fix_secrets(&host);
    }
    let mut r = support::check_via(&sandmux::launch::runner_path());
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
    println!("sandmux doctor\n");

    let abi_detail = if r.landlock_abi < 0 {
        tr!("ausente no kernel", "missing from the kernel").to_string()
    } else {
        trf!(
            "ABI {} (exige {REQUIRED_ABI}, kernel 6.12 ou mais novo)",
            "ABI {} (requires {REQUIRED_ABI}, kernel 6.12 or newer)",
            r.landlock_abi
        )
    };
    line(
        r.landlock_ok(),
        tr!(
            "Landlock com escopo de sockets abstratos e de sinais",
            "Landlock with abstract socket and signal scoping"
        ),
        &abi_detail,
    );

    match &r.userns {
        Ok(()) => line(
            true,
            tr!(
                "user namespace sem privilégio, com montagem",
                "unprivileged user namespace, with mounts"
            ),
            "",
        ),
        Err(e) => {
            line(
                false,
                tr!(
                    "user namespace sem privilégio, com montagem",
                    "unprivileged user namespace, with mounts"
                ),
                e,
            );
            if r.apparmor_restrict_userns == Some(true) {
                // Sem recuo nas linhas do comando: o heredoc só termina com
                // `EOF` no começo da linha.
                println!(
                    "{}",
                    trf!(
                        "\n  A distro restringe user namespaces sem privilégio por AppArmor\n  (kernel.apparmor_restrict_unprivileged_userns=1). O lançador precisa de um\n  perfil próprio. Para gravar o perfil em /etc/apparmor.d/sandmux-run e\n  carregá-lo, cole no terminal:\n\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n{}EOF\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n",
                        "\n  The distro restricts unprivileged user namespaces through AppArmor\n  (kernel.apparmor_restrict_unprivileged_userns=1). The launcher needs its own\n  profile. To write the profile to /etc/apparmor.d/sandmux-run and\n  load it, paste into the terminal:\n\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n{}EOF\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n",
                        apparmor_profile()
                    )
                );
            } else {
                println!(
                    "{}",
                    tr!(
                        "\n  Confira se user namespaces sem privilégio estão habilitados no kernel\n  (user.max_user_namespaces maior que zero).\n",
                        "\n  Check that unprivileged user namespaces are enabled in the kernel\n  (user.max_user_namespaces greater than zero).\n"
                    )
                );
            }
        }
    }

    match &r.overlay {
        Ok(()) => line(
            true,
            tr!(
                "camada de escrita (overlay em user namespace)",
                "write layer (overlay in a user namespace)"
            ),
            "",
        ),
        Err(e) => {
            line(
                false,
                tr!(
                    "camada de escrita (overlay em user namespace)",
                    "write layer (overlay in a user namespace)"
                ),
                e,
            );
            println!(
                "{}",
                tr!(
                    "  Sem ela, os caches começam vazios e as toolchains ficam em somente leitura.\n  O sandbox funciona mesmo assim.",
                    "  Without it, caches start empty and toolchains are read-only.\n  The sandbox works anyway."
                )
            );
        }
    }

    match sandmux_policy::limits::support() {
        Ok(()) => {
            let total = sandmux_policy::limits::mem_total().unwrap_or(0);
            line(
                true,
                tr!(
                    "tetos de memória e de processos (escopo do systemd de usuário)",
                    "memory and process caps (user systemd scope)"
                ),
                &trf!(
                    "padrão: {} de memória (75% da RAM) e 8192 processos",
                    "default: {} of memory (75% of RAM) and 8192 processes",
                    sandmux_policy::limits::human(total / 100 * 75)
                ),
            )
        }
        Err(e) => {
            line(
                false,
                tr!(
                    "tetos de memória e de processos (escopo do systemd de usuário)",
                    "memory and process caps (user systemd scope)"
                ),
                &e,
            );
            println!(
                "{}",
                tr!(
                    "  Sem eles, as sessões rodam sem teto de memória e de processos. O sandbox\n  funciona mesmo assim. Os tetos precisam do systemd de usuário (systemd-run\n  --user) com os controladores memory e pids delegados ao user@<uid>.service.",
                    "  Without them, sessions run with no memory or process cap. The sandbox\n  works anyway. The caps need user systemd (systemd-run\n  --user) with the memory and pids controllers delegated to user@<uid>.service."
                )
            );
        }
    }

    if let Ok(h) = Host::detect() {
        match sandmux::docker::support(&h.user, h.uid) {
            Ok(_) => {
                line(
                    true,
                    tr!(
                        "Docker privado por pasta (Docker sem root)",
                        "private Docker per folder (rootless Docker)"
                    ),
                    tr!(
                        "rede pelo gateway do sandmux",
                        "network through the sandmux gateway"
                    ),
                );
            }
            Err(e) => {
                line(
                    false,
                    tr!(
                        "Docker privado por pasta (Docker sem root)",
                        "private Docker per folder (rootless Docker)"
                    ),
                    &e,
                );
                println!(
                    "{}",
                    tr!(
                        "  Sem ele, as sessões rodam sem Docker privado: o `docker` lá dentro mostra\n  este motivo. O sandbox funciona mesmo assim. O Docker sem root precisa do\n  rootlesskit, do dockerd, do containerd, do newuidmap e do newgidmap, e de uma\n  faixa de 65536 IDs do usuário em /etc/subuid e /etc/subgid.",
                        "  Without it, sessions run with no private Docker: `docker` inside shows\n  this reason. The sandbox works anyway. Rootless Docker needs\n  rootlesskit, dockerd, containerd, newuidmap and newgidmap, and a\n  range of 65536 IDs for the user in /etc/subuid and /etc/subgid."
                    )
                );
            }
        }
        match sandmux::docker::registry_binary() {
            Some(b) => line(
                true,
                tr!(
                    "cache de imagens do Docker entre pastas",
                    "Docker image cache across folders"
                ),
                &trf!(
                    "espelho do Docker Hub com {}",
                    "Docker Hub mirror with {}",
                    b.display()
                ),
            ),
            None => {
                line(
                    false,
                    tr!(
                        "cache de imagens do Docker entre pastas",
                        "Docker image cache across folders"
                    ),
                    tr!(
                        "desligado: falta o binário registry",
                        "off: the registry binary is missing"
                    ),
                );
                println!(
                    "{}",
                    tr!(
                        "  Sem ele, cada pasta baixa as próprias imagens. Para ligar o cache, instale o\n  pacote docker-registry (Debian e Ubuntu) ou o binário registry do projeto\n  distribution.",
                        "  Without it, each folder pulls its own images. To turn the cache on, install the\n  docker-registry package (Debian and Ubuntu) or the registry binary from the\n  distribution project."
                    )
                );
            }
        }
    }

    let tun = std::fs::metadata("/dev/net/tun")
        .map(|m| std::os::unix::fs::FileTypeExt::is_char_device(&m.file_type()))
        .unwrap_or(false);
    let log = sandmux::paths::state_dir().join("net");
    if tun {
        line(
            true,
            tr!(
                "rede pelo gateway (namespace de rede próprio e TUN)",
                "network through the gateway (own network namespace and TUN)"
            ),
            &trf!("registro em {}", "log in {}", log.display()),
        );
    } else {
        line(
            false,
            tr!(
                "rede pelo gateway (namespace de rede próprio e TUN)",
                "network through the gateway (own network namespace and TUN)"
            ),
            tr!("/dev/net/tun não existe", "/dev/net/tun does not exist"),
        );
        println!(
            "{}",
            tr!(
                "  Sem o TUN, só o modo sem rede funciona (`--network sem-rede` ou\n  `network = \"sem-rede\"`). O módulo do kernel é o `tun`.",
                "  Without TUN, only the no-network mode works (`--network sem-rede` or\n  `network = \"sem-rede\"`). The kernel module is `tun`."
            )
        );
    }

    if r.apparmor_restrict_userns == Some(true) && r.userns.is_ok() {
        println!(
            "{}",
            tr!(
                "\n  Nota: a distro restringe user namespaces por AppArmor, e o lançador passou na\n  verificação (o perfil do sandmux-run está carregado, ou o processo que chamou\n  o sandmux tem um perfil que libera user namespaces).",
                "\n  Note: the distro restricts user namespaces through AppArmor, and the launcher passed\n  the check (the sandmux-run profile is loaded, or the process that called\n  sandmux has a profile that allows user namespaces)."
            )
        );
    }

    if let Ok(host) = Host::detect() {
        let layout = Layout::new(&host.data_dir);
        println!(
            "{}",
            trf!(
                "\nDiretório de dados: {}",
                "\nData directory: {}",
                host.data_dir.display()
            )
        );
        println!(
            "{}",
            trf!(
                "Pasta de entrada (visível em ~/inbox, somente leitura): {}",
                "Inbox folder (visible at ~/inbox, read-only): {}",
                layout.inbox().display()
            )
        );
        // Dois gateways do hermes no mesmo bot disputam as mensagens.
        if let Some(w) = sandmux_policy::hermes_config::host_gateway_alive(&host.home) {
            println!("Hermes: {w}");
        }
    }
    config_section();
    daemon_section();
    vault_section();
    if let Ok(host) = Host::detect() {
        secrets_section(&host);
    }
    println!(
        "{}",
        tr!(
            "\nRede: o sandbox sai pelo gateway do sandmux, com o modo de rede, a denylist e o\n  registro de cada conexão (`sandmux net`).",
            "\nNetwork: the sandbox goes out through the sandmux gateway, with the network mode, the denylist and\n  a log of each connection (`sandmux net`)."
        )
    );
    println!(
        "{}",
        tr!(
            "Arquivos com cara de segredo na pasta de trabalho ficam escondidos do agente\n  (`sandmux files` e a política de arquivos).",
            "Files that look like secrets in the work folder are hidden from the agent\n  (`sandmux files` and the file policy)."
        )
    );

    Ok(if r.can_launch() { 0 } else { 1 })
}

/// Arquivo de configuração: caminho e validade, sem alterar nada.
fn config_section() {
    use sandmux_policy::config::{self, Config};
    println!("{}", tr!("\nConfiguração:", "\nConfiguration:"));
    let path = match sandmux_policy::Host::detect() {
        Ok(h) => config::path(&h),
        Err(e) => {
            println!(
                "{}",
                trf!(
                    "  não foi possível ler o ambiente: {e}",
                    "  could not read the environment: {e}"
                )
            );
            return;
        }
    };
    println!("{}", trf!("  arquivo: {}", "  file: {}", path.display()));
    if !path.exists() {
        println!(
            "{}",
            tr!(
                "  estado: não existe (só a baseline e os presets valem)",
                "  status: does not exist (only the baseline and presets apply)"
            )
        );
        return;
    }
    match Config::load(&path) {
        Ok(c) => {
            let n = |k: usize, um: &str, varios: &str| {
                format!("{k} {}", if k == 1 { um } else { varios })
            };
            println!(
                "{}",
                trf!(
                    "  estado: válido ({}, {})",
                    "  status: valid ({}, {})",
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
                )
            )
        }
        Err(e) => {
            let at = e
                .line
                .map(|l| trf!("linha {l}: ", "line {l}: "))
                .unwrap_or_default();
            println!(
                "{}",
                trf!(
                    "  estado: inválido, {at}{}",
                    "  status: invalid, {at}{}",
                    e.message
                )
            );
        }
    }
}

/// Estado do daemon e da unidade systemd, sem iniciar nada.
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

fn vault_section() {
    let Ok(host) = Host::detect() else { return };
    let exists = sandmux::daemon::vault::path(&host.data_dir).exists();
    let fakes = imported_fakes(&host.data_dir);
    println!(
        "{}",
        trf!(
            "\nCofre: {}",
            "\nVault: {}",
            if exists {
                tr!("existe", "exists")
            } else {
                tr!("não existe", "does not exist")
            }
        )
    );
    if exists {
        println!(
            "  {}",
            if sandmux::keyring::enabled(&host.data_dir) {
                tr!(
                    "Keyring: ligado (o daemon abre o cofre com a senha guardada nele)",
                    "Keyring: on (the daemon opens the vault with the password stored in it)"
                )
            } else {
                tr!("Keyring: desligado", "Keyring: off")
            }
        );
    }
    if !exists && fakes > 0 {
        println!(
            "{}",
            trf!(
                "  A importação trocou {fakes} chave(s) por valores falsos, e sem cofre nada troca o\n  falso pelo real: os harnesses recebem \"chave inválida\". Crie o cofre e guarde\n  as chaves com `sandmux vault init` e `sandmux vault import`.",
                "  The import replaced {fakes} key(s) with fake values, and without a vault nothing swaps the\n  fake for the real one: the harnesses get \"invalid key\". Create the vault and store\n  the keys with `sandmux vault init` and `sandmux vault import`."
            )
        );
    }
}

fn daemon_section() {
    use sandmux::client::{Conn, systemd};
    use sandmux_proto::Msg;
    println!("{}", tr!("\nDaemon de sessões:", "\nSession daemon:"));
    println!("  socket: {}", sandmux::paths::socket_path().display());
    println!(
        "{}",
        trf!(
            "  início no boot: {}",
            "  start at boot: {}",
            crate::sessions::boot_text()
        )
    );
    match Conn::connect() {
        Ok(Some(mut c)) => match c.call(&Msg::Status) {
            Ok(Msg::StatusInfo {
                pid,
                version,
                start_mode,
                sessions,
                ..
            }) => println!(
                "{}",
                trf!(
                    "  estado: rodando (PID {pid}, versão {version}, iniciado por {start_mode}, {sessions} sessões)",
                    "  status: running (PID {pid}, version {version}, started by {start_mode}, {sessions} sessions)"
                )
            ),
            Ok(_) | Err(_) => println!(
                "{}",
                tr!(
                    "  estado: rodando, mas não respondeu ao pedido de estado",
                    "  status: running, but did not answer the status request"
                )
            ),
        },
        Ok(None) => println!("{}", tr!("  estado: parado", "  status: stopped")),
        Err(e) => println!("{}", trf!("  estado: {e:#}", "  status: {e:#}")),
    }
    let unit = systemd::unit_path();
    let direct = std::env::var("SANDMUX_DAEMON_START").as_deref() == Ok("direct");
    if !unit.exists() {
        println!(
            "{}",
            trf!(
                "  unidade systemd de usuário: não instalada ({})",
                "  user systemd unit: not installed ({})",
                unit.display()
            )
        );
    } else if direct || !systemd::available() {
        println!(
            "{}",
            trf!(
                "  unidade systemd de usuário: instalada ({})",
                "  user systemd unit: installed ({})",
                unit.display()
            )
        );
    } else {
        let state = match systemd::enabled() {
            Some(true) => tr!(
                "instalada e habilitada no login",
                "installed and enabled at login"
            ),
            Some(false) => tr!("instalada, mas desabilitada", "installed, but disabled"),
            None => tr!("instalada", "installed"),
        };
        println!(
            "{}",
            trf!(
                "  unidade systemd de usuário: {state} ({})",
                "  user systemd unit: {state} ({})",
                unit.display()
            )
        );
    }
}
