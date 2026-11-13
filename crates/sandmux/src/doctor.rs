//! `sandmux doctor`: verificações de suporte, com causa e correção.

use std::path::PathBuf;

use anyhow::Result;
use sandmux_launcher::REQUIRED_ABI;
use sandmux_launcher::support::{self, SupportReport};
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

pub fn short_reason(r: &SupportReport) -> String {
    if !r.landlock_ok() {
        return format!(
            "Landlock ABI {} encontrado, exige {REQUIRED_ABI}",
            r.landlock_abi
        );
    }
    match &r.userns {
        Err(e) => e.clone(),
        Ok(()) => "motivo desconhecido".into(),
    }
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
    let mut r = support::check_via(&crate::runner_path());
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
    }
    println!("\nRede: nesta versão o sandbox usa a rede do host, sem filtro e sem registro.");
    println!("Arquivos de segredo que já estão na pasta de trabalho ficam legíveis pelo agente.");

    Ok(if r.can_launch() { 0 } else { 1 })
}
