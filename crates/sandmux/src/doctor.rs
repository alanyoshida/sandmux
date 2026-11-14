//! `sandmux doctor`: verificações de suporte, com causa e correção.

use std::path::PathBuf;

use anyhow::Result;
use sandmux_launcher::REQUIRED_ABI;
use sandmux_launcher::support;
use sandmux_policy::{Host, Layout};

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
    let mark = if ok { "ok  " } else { "FALHA" };
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
    let mut r = support::check_via(&sandmux::launch::runner_path());
    // Mesma variável do `run`: só desliga a camada, para conferir o modo de recuo.
    if std::env::var_os("SANDMUX_DISABLE_OVERLAY").is_some() && r.overlay.is_ok() {
        r.overlay = Err("desligada por SANDMUX_DISABLE_OVERLAY".into());
    }
    // Só muda o texto do diagnóstico: simula a leitura da restrição do AppArmor.
    if std::env::var_os("SANDMUX_ASSUME_APPARMOR_RESTRICT").is_some() {
        r.apparmor_restrict_userns = Some(true);
    }
    println!("sandmux doctor\n");

    let abi_detail = if r.landlock_abi < 0 {
        "ausente no kernel".to_string()
    } else {
        format!(
            "ABI {} (exige {REQUIRED_ABI}, kernel 6.12 ou mais novo)",
            r.landlock_abi
        )
    };
    line(
        r.landlock_ok(),
        "Landlock com escopo de sockets abstratos e de sinais",
        &abi_detail,
    );

    match &r.userns {
        Ok(()) => line(true, "user namespace sem privilégio, com montagem", ""),
        Err(e) => {
            line(false, "user namespace sem privilégio, com montagem", e);
            if r.apparmor_restrict_userns == Some(true) {
                // Sem recuo nas linhas do comando: o heredoc só termina com
                // `EOF` no começo da linha.
                println!(
                    "\n  A distro restringe user namespaces sem privilégio por AppArmor\n  (kernel.apparmor_restrict_unprivileged_userns=1). O lançador precisa de um\n  perfil próprio. Para gravar o perfil em /etc/apparmor.d/sandmux-run e\n  carregá-lo, cole no terminal:\n\nsudo tee /etc/apparmor.d/sandmux-run >/dev/null <<'EOF'\n{}EOF\nsudo apparmor_parser -r /etc/apparmor.d/sandmux-run\n",
                    apparmor_profile()
                );
            } else {
                println!(
                    "\n  Confira se user namespaces sem privilégio estão habilitados no kernel\n  (user.max_user_namespaces maior que zero).\n"
                );
            }
        }
    }

    match &r.overlay {
        Ok(()) => line(true, "camada de escrita (overlay em user namespace)", ""),
        Err(e) => {
            line(false, "camada de escrita (overlay em user namespace)", e);
            println!(
                "  Sem ela, os caches começam vazios e as toolchains ficam em somente leitura.\n  O sandbox funciona mesmo assim."
            );
        }
    }

    match sandmux_policy::limits::support() {
        Ok(()) => {
            let total = sandmux_policy::limits::mem_total().unwrap_or(0);
            line(
                true,
                "tetos de memória e de processos (escopo do systemd de usuário)",
                &format!(
                    "padrão: {} de memória (75% da RAM) e 8192 processos",
                    sandmux_policy::limits::human(total / 100 * 75)
                ),
            )
        }
        Err(e) => {
            line(
                false,
                "tetos de memória e de processos (escopo do systemd de usuário)",
                &e,
            );
            println!(
                "  Sem eles, as sessões rodam sem teto de memória e de processos. O sandbox\n  funciona mesmo assim. Os tetos precisam do systemd de usuário (systemd-run\n  --user) com os controladores memory e pids delegados ao user@<uid>.service."
            );
        }
    }

    if let Ok(h) = Host::detect() {
        match sandmux::docker::support(&h.user, h.uid) {
            Ok(_) => {
                line(
                    true,
                    "Docker privado por pasta (Docker sem root)",
                    "rede pelo gateway do sandmux",
                );
            }
            Err(e) => {
                line(false, "Docker privado por pasta (Docker sem root)", &e);
                println!(
                    "  Sem ele, as sessões rodam sem Docker privado: o `docker` lá dentro mostra\n  este motivo. O sandbox funciona mesmo assim. O Docker sem root precisa do\n  rootlesskit, do dockerd, do containerd, do newuidmap e do newgidmap, e de uma\n  faixa de 65536 IDs do usuário em /etc/subuid e /etc/subgid."
                );
            }
        }
        match sandmux::docker::registry_binary() {
            Some(b) => line(
                true,
                "cache de imagens do Docker entre pastas",
                &format!("espelho do Docker Hub com {}", b.display()),
            ),
            None => {
                line(
                    false,
                    "cache de imagens do Docker entre pastas",
                    "desligado: falta o binário registry",
                );
                println!(
                    "  Sem ele, cada pasta baixa as próprias imagens. Para ligar o cache, instale o\n  pacote docker-registry (Debian e Ubuntu) ou o binário registry do projeto\n  distribution."
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
            "rede pelo gateway (namespace de rede próprio e TUN)",
            &format!("registro em {}", log.display()),
        );
    } else {
        line(
            false,
            "rede pelo gateway (namespace de rede próprio e TUN)",
            "/dev/net/tun não existe",
        );
        println!(
            "  Sem o TUN, só o modo sem rede funciona (`--network sem-rede` ou\n  `network = \"sem-rede\"`). O módulo do kernel é o `tun`."
        );
    }

    if r.apparmor_restrict_userns == Some(true) && r.userns.is_ok() {
        println!(
            "\n  Nota: a distro restringe user namespaces por AppArmor, e o lançador passou na\n  verificação (o perfil do sandmux-run está carregado, ou o processo que chamou\n  o sandmux tem um perfil que libera user namespaces)."
        );
    }

    if let Ok(host) = Host::detect() {
        let layout = Layout::new(&host.data_dir);
        println!("\nDiretório de dados: {}", host.data_dir.display());
        println!(
            "Pasta de entrada (visível em ~/inbox, somente leitura): {}",
            layout.inbox().display()
        );
        // Dois gateways do hermes no mesmo bot disputam as mensagens.
        if let Some(w) = sandmux_policy::hermes_config::host_gateway_alive(&host.home) {
            println!("Hermes: {w}");
        }
    }
    config_section();
    daemon_section();
    println!(
        "\nRede: o sandbox sai pelo gateway do sandmux, com o modo de rede, a denylist e o\n  registro de cada conexão (`sandmux net`)."
    );
    println!(
        "Arquivos com cara de segredo na pasta de trabalho ficam escondidos do agente\n  (`sandmux files` e a política de arquivos)."
    );

    Ok(if r.can_launch() { 0 } else { 1 })
}

/// Arquivo de configuração: caminho e validade, sem alterar nada.
fn config_section() {
    use sandmux_policy::config::{self, Config};
    println!("\nConfiguração:");
    let path = match sandmux_policy::Host::detect() {
        Ok(h) => config::path(&h),
        Err(e) => {
            println!("  não foi possível ler o ambiente: {e}");
            return;
        }
    };
    println!("  arquivo: {}", path.display());
    if !path.exists() {
        println!("  estado: não existe (só a baseline e os presets valem)");
        return;
    }
    match Config::load(&path) {
        Ok(c) => {
            let n = |k: usize, um: &str, varios: &str| {
                format!("{k} {}", if k == 1 { um } else { varios })
            };
            println!(
                "  estado: válido ({}, {})",
                n(c.profiles.len(), "perfil", "perfis"),
                n(c.projects.len(), "projeto", "projetos")
            )
        }
        Err(e) => {
            let at = e.line.map(|l| format!("linha {l}: ")).unwrap_or_default();
            println!("  estado: inválido, {at}{}", e.message);
        }
    }
}

/// Estado do daemon e da unidade systemd, sem iniciar nada.
fn daemon_section() {
    use sandmux::client::{Conn, systemd};
    use sandmux_proto::Msg;
    println!("\nDaemon de sessões:");
    println!("  socket: {}", sandmux::paths::socket_path().display());
    match Conn::connect() {
        Ok(Some(mut c)) => match c.call(&Msg::Status) {
            Ok(Msg::StatusInfo {
                pid,
                version,
                start_mode,
                sessions,
                ..
            }) => println!(
                "  estado: rodando (PID {pid}, versão {version}, iniciado por {start_mode}, {sessions} sessões)"
            ),
            Ok(_) | Err(_) => println!("  estado: rodando, mas não respondeu ao pedido de estado"),
        },
        Ok(None) => println!("  estado: parado"),
        Err(e) => println!("  estado: {e:#}"),
    }
    let unit = systemd::unit_path();
    let direct = std::env::var("SANDMUX_DAEMON_START").as_deref() == Ok("direct");
    if !unit.exists() {
        println!(
            "  unidade systemd de usuário: não instalada ({})",
            unit.display()
        );
    } else if direct || !systemd::available() {
        println!(
            "  unidade systemd de usuário: instalada ({})",
            unit.display()
        );
    } else {
        let state = match systemd::enabled() {
            Some(true) => "instalada e habilitada no login",
            Some(false) => "instalada, mas desabilitada",
            None => "instalada",
        };
        println!("  unidade systemd de usuário: {state} ({})", unit.display());
    }
}
