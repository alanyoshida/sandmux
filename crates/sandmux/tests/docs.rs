//! Documentação acompanha o código: o exemplo de docs/launch-spec.md é aceito
//! pelo lançador, docs/sessions.md cita os comandos e a versão do protocolo, e
//! docs/config.md cita as chaves e os comandos do `config`.

#[test]
fn exemplo_da_documentacao_e_valido() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/launch-spec.md"
    ))
    .unwrap();
    let start = doc.find("```json\n").expect("bloco json") + "```json\n".len();
    let end = start + doc[start..].find("```").unwrap();
    let spec = sandmux_spec::LaunchSpec::from_json(&doc.as_bytes()[start..end]).unwrap();
    assert_eq!(spec.command, vec!["bash"]);
    assert_eq!(spec.mounts.len(), 15);
}

#[test]
fn documentacao_das_sessoes_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/sessions.md"
    ))
    .unwrap();
    assert!(
        doc.contains(&format!("usa o protocolo {}.", sandmux_proto::PROTOCOL)),
        "a versão do protocolo na documentação está desatualizada"
    );
    for c in [
        "sandmux ls",
        "sandmux attach",
        "sandmux detach",
        "sandmux kill",
        "sandmux restart",
        "sandmux daemon status",
        "sandmux daemon stop",
        "sandmux daemon restart",
        "sandmux daemon disable",
        "--allow-clipboard",
        "--history-lines",
        "SANDMUX_DAEMON_START",
        "SANDMUX_PROTOCOL_OVERRIDE",
        "sandmux stats",
        "sandmux net",
    ] {
        assert!(doc.contains(c), "docs/sessions.md não cita {c}");
    }
    assert!(
        doc.contains(
            &sandmux::paths::sessions_file(std::path::Path::new(""))
                .display()
                .to_string()
        )
    );
}

#[test]
fn documentacao_da_configuracao_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/config.md"))
        .unwrap();
    for k in sandmux_policy::config::LIST_KEYS
        .iter()
        .chain(sandmux_policy::config::BOOL_KEYS)
        .chain(sandmux_policy::config::LIMIT_KEYS)
        .chain(sandmux_policy::config::NET_KEYS)
        .chain(sandmux_policy::config::NET_LIST_KEYS)
        .chain(sandmux_policy::config::GLOBAL_ONLY_KEYS)
        .chain([
            &sandmux_policy::config::SSH_KEYS,
            &sandmux_policy::config::DB_KEY,
            &sandmux_policy::config::PORTS_KEY,
            &sandmux_policy::config::HOST_PORTS_KEY,
        ])
        .chain(sandmux_policy::config::FILE_LIST_KEYS)
        .chain(sandmux_policy::config::MCP_LIST_KEYS)
    {
        assert!(
            doc.contains(&format!("| `{k}` |")),
            "docs/config.md não cita a chave {k}"
        );
    }
    for c in [
        "sandmux config show",
        "sandmux config list",
        "sandmux config set",
        "sandmux config unset",
        "sandmux config add",
        "sandmux config remove",
        "--global",
        "--profile",
        "--project",
        "--no-kvm",
        "--gpu",
        "--no-allow-clipboard",
        "--no-repo",
        sandmux_policy::config::FILE,
    ] {
        assert!(doc.contains(c), "docs/config.md não cita {c}");
    }
    // O exemplo do arquivo é válido.
    let start = doc.find("```toml\n").expect("bloco toml") + "```toml\n".len();
    let end = start + doc[start..].find("```").unwrap();
    let cfg = sandmux_policy::config::Config::parse(
        std::path::Path::new("docs/config.md"),
        &doc[start..end],
    )
    .unwrap();
    assert_eq!(cfg.profiles.len(), 1);
    assert_eq!(cfg.projects.len(), 1);
}

#[test]
fn documentacao_dos_tetos_acompanha_o_codigo() {
    use sandmux_policy::limits::{self, MemoryLimit, ProcessLimit};
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/limits.md"))
        .unwrap();
    assert_eq!(limits::DEFAULT_MEMORY, MemoryLimit::Percent(75));
    assert_eq!(limits::DEFAULT_PROCESSES, ProcessLimit::Count(8192));
    for c in [
        "75% da RAM",
        "8192",
        "`128M`",
        "Mínimo: 32",
        "--memory",
        "--processes",
        "MemorySwapMax=0",
        "OOMPolicy=continue",
        "sandmux stats",
        "memória esgotada",
    ] {
        assert!(doc.contains(c), "docs/limits.md não cita {c}");
    }
    let s = limits::scope_args(
        &limits::Limits {
            memory: Some(1),
            processes: Some(1),
        },
        "x",
    )
    .join(" ");
    for p in ["MemorySwapMax=0", "OOMPolicy=continue"] {
        assert!(s.contains(p), "o escopo não usa {p}");
    }
    // O exemplo de arquivo é válido.
    let start = doc.find("```toml\n").expect("bloco toml") + "```toml\n".len();
    let end = start + doc[start..].find("```").unwrap();
    sandmux_policy::config::Config::parse(std::path::Path::new("docs/limits.md"), &doc[start..end])
        .unwrap();
}

#[test]
fn documentacao_do_docker_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/docker.md"))
        .unwrap();
    for c in [
        sandmux_policy::compose::DOCKER_SOCKET,
        "DOCKER_HOST",
        "127.0.0.1",
        "--net=none",
        "docker-registry",
        "registry-1.docker.io",
        "docker-<id>",
        "/etc/subuid",
        "sandmux stats",
        "sandmux doctor",
        "não está disponível nesta máquina",
        "Registries privados",
        "--tool docker",
        "seccomp=unconfined",
        "ENOSYS",
        "/dev/kvm",
        "/dev/fuse",
        "cgroup namespace",
        sandmux::docker::EVACUATE_CGROUP,
        "secret-files-off",
        &sandmux_policy::MAX_DOCKER_COVERS.to_string(),
        "--privileged",
    ] {
        assert!(doc.contains(c), "docs/docker.md não cita {c}");
    }
    let d = sandmux::docker::Dirs::new(std::path::Path::new("/d"), std::path::Path::new("/r"), "x");
    assert!(
        sandmux::docker::dockerd_command(&d, None)
            .join(" ")
            .contains("--ip 127.0.0.1")
    );
}

#[test]
fn documentacao_da_tui_acompanha_o_codigo() {
    let doc =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/tui.md")).unwrap();
    for (action, key) in sandmux_policy::config::TUI_ACTIONS {
        assert!(
            doc.contains(&format!("`{action}`")),
            "docs/tui.md não cita a ação {action}"
        );
        assert!(
            doc.contains(&format!("| `{key}`"))
                || doc.contains(&format!("`{key}` /"))
                || doc.contains(&format!("/ `{key}`")),
            "docs/tui.md não cita a tecla {key} de {action}"
        );
    }
    assert_eq!(sandmux_policy::config::TuiConfig::default().prefix, 'b');
    assert!(doc.contains("Ctrl+b") && doc.contains("sandmux tui"));
    // O exemplo de [tui] é válido.
    let i = doc.find("```toml\n[tui]").expect("exemplo de [tui]") + "```toml\n".len();
    let end = i + doc[i..].find("```").unwrap();
    let c =
        sandmux_policy::config::Config::parse(std::path::Path::new("docs/tui.md"), &doc[i..end])
            .unwrap();
    assert_eq!(c.tui.prefix, 'a');
    // O estilo da barra e o exemplo sem a fonte.
    for k in [
        "`style`",
        "`bar`",
        "\"blocos\"",
        "\"powerline\"",
        "\"simples\"",
        "Ctrl+b `e`",
        "Ctrl+b `:`",
        "\"bottom\"",
        "\"top\"",
    ] {
        assert!(doc.contains(k), "docs/tui.md não cita {k}");
    }
    let i = doc
        .find("```toml\n# Sem uma fonte")
        .expect("exemplo do estilo simples")
        + "```toml\n".len();
    let end = i + doc[i..].find("```").unwrap();
    let c =
        sandmux_policy::config::Config::parse(std::path::Path::new("docs/tui.md"), &doc[i..end])
            .unwrap();
    assert_eq!(c.tui.style, sandmux_policy::config::BarStyle::Simples);
    assert!(c.tui.bar_top);
}

#[test]
fn documentacao_dos_temas_acompanha_o_codigo() {
    let doc =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/tui.md")).unwrap();
    for el in sandmux_policy::config::THEME_ELEMENTS {
        assert!(
            doc.contains(&format!("`{el}`")),
            "docs/tui.md não cita o elemento {el}"
        );
    }
    for th in sandmux_policy::config::THEMES {
        assert!(
            doc.contains(&format!("`{th}`")),
            "docs/tui.md não cita o tema {th}"
        );
    }
}

#[test]
fn documentacao_da_rede_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/network.md"
    ))
    .unwrap();
    for c in [
        "sandmux net",
        "--network sem-rede",
        "host.sandmux.internal",
        "SANDMUX_HOST",
        "10.0.2.100",
        "10.0.2.2",
        "10.0.2.3",
        "10.0.2.254",
        "169.254.169.254",
        "QUIC",
        "net-log-days",
        "net-log-max",
        "$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl",
        sandmux_net::log::NO_DOMAIN,
        sandmux_net::log::SSH_SIGN,
        sandmux_spec::net::SSH_BLOCKED,
        "`ssh`",
        "`inbound`",
        "8443@0.0.0.0",
        "--port",
    ] {
        assert!(doc.contains(c), "docs/network.md não cita {c}");
    }
    assert_eq!(sandmux_net::HOST_NAME, sandmux_policy::compose::HOST_NAME);
    // Os campos da tabela são os da linha do registro.
    let e = serde_json::to_value(sandmux_net::log::Entry {
        ts: String::new(),
        tag: String::new(),
        workdir: Default::default(),
        proto: String::new(),
        domain: String::new(),
        ip: String::new(),
        port: 0,
        up: 0,
        down: 0,
        ms: 0,
        blocked: None,
        new: false,
        method: Some(String::new()),
        path: Some(String::new()),
        status: Some(0),
        secret: Some(String::new()),
        pid: Some(0),
        exe: Some(String::new()),
        proc_via: Some(String::new()),
    })
    .unwrap();
    for k in e.as_object().unwrap().keys() {
        assert!(
            doc.contains(&format!("`{k}`")),
            "docs/network.md não cita o campo {k}"
        );
    }
    // Os catálogos documentados são os embutidos.
    use sandmux_policy::netcatalog;
    for (cat, items) in netcatalog::EXFIL.iter().chain(netcatalog::COMMON) {
        for i in *items {
            assert!(
                doc.contains(&format!("`{i}`")),
                "docs/network.md não cita {i} ({cat})"
            );
        }
    }
    for (g, _) in netcatalog::COMMON {
        assert!(
            doc.contains(&format!("| `{g}` |")),
            "docs/network.md não cita o grupo {g}"
        );
    }
    for i in netcatalog::INFERENCE {
        assert!(
            doc.contains(&format!("`{i}`")),
            "docs/network.md não cita {i}"
        );
    }
    for c in [
        "sandmux net deny",
        "sandmux net allow",
        "sandmux net forget",
        "sandmux net mode",
        "sandmux net rules",
        "--deny",
        "--allow",
        "allow-off",
        "catalog-off",
        "fora da allowlist",
        "fora do modo só inferência",
        "sem domínio",
        "NXDOMAIN",
        "--decrypt",
        "SANDMUX_UPSTREAM_CA",
        "NODE_EXTRA_CA_CERTS",
        "certificado inválido",
    ] {
        assert!(doc.contains(c), "docs/network.md não cita {c}");
    }
    for m in sandmux_policy::config::NetworkMode::ALL {
        assert!(
            doc.contains(&format!("| `{m}`")),
            "docs/network.md não cita o modo {m}"
        );
    }
    // O exemplo de retenção é válido.
    let start = doc.find("```toml\n[global]\nnet-log").expect("bloco toml") + "```toml\n".len();
    let end = start + doc[start..].find("```").unwrap();
    let cfg = sandmux_policy::config::Config::parse(
        std::path::Path::new("docs/network.md"),
        &doc[start..end],
    )
    .unwrap();
    assert_eq!(
        (cfg.net_log_days, cfg.net_log_max),
        (Some(7), Some(1 << 30))
    );
}

#[test]
fn documentacao_do_cofre_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/vault.md"))
        .unwrap();
    for c in [
        "sandmux vault init",
        "sandmux vault status",
        "sandmux vault unlock",
        "sandmux vault lock",
        "sandmux vault passwd",
        "sandmux vault add",
        "sandmux vault list",
        "sandmux vault remove",
        "sandmux vault import",
        "aguardando cofre",
        "Argon2id",
        "XChaCha20-Poly1305",
        "vault.json",
        "--pass-env",
        "sandmux-fake-",
        "chave: openrouter",
        "Modo só inferência",
        "--tool",
        "GH_TOKEN",
        "GIT_CONFIG_COUNT",
        "PIP_EXTRA_INDEX_URL",
        "CARGO_REGISTRY_TOKEN",
        "~/.npmrc",
        "~/.docker/config.json",
        "Authorization: Basic",
        "sandmux vault add-ssh",
        "--ssh-key",
        "ssh-keys",
        sandmux_policy::compose::SSH_AGENT_SOCKET,
        "SSH_AUTH_SOCK",
        "known_hosts",
        "assinatura ssh:",
        "rsa-sha2-256",
    ] {
        assert!(doc.contains(c), "docs/vault.md não cita {c}");
    }
    for t in sandmux_policy::presets::TOOLS {
        assert!(
            doc.contains(&format!("`{t}`")),
            "docs/vault.md não cita {t}"
        );
    }
    for (domain, _) in sandmux_policy::presets::KNOWN_TOOLS {
        assert!(
            doc.contains(&format!("`{domain}`")),
            "docs/vault.md não cita o domínio conhecido {domain}"
        );
    }
    for (var, domain) in sandmux_policy::presets::PROVIDERS {
        assert!(
            doc.contains(&format!("`{var}` (`{domain}`)")),
            "docs/vault.md não cita o provedor {var}"
        );
    }
    assert_eq!(
        sandmux_proto::SessionState::AguardandoCofre.to_string(),
        "aguardando cofre"
    );
    // O grafo do cofre: as teclas, os grupos e a chave das arestas.
    for c in [
        "### Grafo do cofre",
        "Arestas do grafo",
        "0.5x",
        "2.0x",
        "`h` `j` `k` `l`",
        "\"Sem domínio\"",
        "\"Bancos\"",
        "`●` token, `◆` chave SSH, `■` conexão de banco",
    ] {
        assert!(doc.contains(c), "docs/vault.md não cita {c}");
    }
    let i = doc
        .find("```toml\n[tui]\ngraph-edges")
        .expect("exemplo de graph-edges")
        + "```toml\n".len();
    let end = i + doc[i..].find("```").unwrap();
    let c =
        sandmux_policy::config::Config::parse(std::path::Path::new("docs/vault.md"), &doc[i..end])
            .unwrap();
    assert!(c.tui.graph_blocks);
    assert!(!sandmux_policy::config::TuiConfig::default().graph_blocks);
}

#[test]
fn documentacao_dos_arquivos_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/files.md"))
        .unwrap();
    for c in [
        "sandmux files",
        "--days",
        "SECCOMP_RET_USER_NOTIF",
        "SECCOMP_IOCTL_NOTIF_ADDFD",
        "RESOLVE_IN_ROOT",
        "$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl",
        "net-log-days",
        "net-log-max",
        "O_PATH",
        "/proc/self",
        "sandmux files deny",
        "sandmux files allow",
        "sandmux files forget",
        "--file-deny",
        "--file-allow",
        ".git/commondir",
        "renameat2",
        "symlinkat",
        "Ctrl+b `f`",
        "sandmux folder add",
        "sandmux folder remove",
        "sandmux folder list",
        "SANDMUX_FOLDERS",
        "PASTAS.txt",
        sandmux_spec::FOLDERS_DIR,
        "Ctrl+b `o`",
    ] {
        assert!(doc.contains(c), "docs/files.md não cita {c}");
    }
    for k in sandmux_policy::config::FILE_LIST_KEYS {
        assert!(
            doc.contains(&format!("| `{k}` |")),
            "docs/files.md não cita a chave {k}"
        );
    }
    for s in sandmux_spec::files::DEFAULT_SECRETS
        .iter()
        .chain(sandmux_spec::files::SECRET_EXCEPTIONS)
    {
        assert!(
            doc.contains(&format!("`{s}`")),
            "docs/files.md não cita o segredo {s}"
        );
    }
    // Os campos da tabela são os da linha do registro.
    let e = serde_json::to_value(sandmux_spec::FileEntry {
        ts: String::new(),
        tag: String::new(),
        workdir: Default::default(),
        path: Default::default(),
        access: String::new(),
        blocked: None,
        new: false,
        args: Vec::new(),
        via: None,
    })
    .unwrap();
    for k in e.as_object().unwrap().keys() {
        assert!(
            doc.contains(&format!("| `{k}` |")),
            "docs/files.md não cita o campo {k}"
        );
    }
    assert_eq!(sandmux_launcher::files::THREADS, 4);
    assert!(doc.contains("Quatro threads"));
}

#[test]
fn documentacao_dos_mcps_acompanha_o_codigo() {
    let doc =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/mcp.md")).unwrap();
    for c in [
        "sandmux mcp list",
        "sandmux mcp deny",
        "sandmux mcp allow",
        "sandmux mcp forget",
        "--mcp-deny",
        "--mcp-allow",
        "OPENCODE_CONFIG_CONTENT",
        "mcp_servers",
        "Ctrl+b `m`",
        "opencode.jsonc",
    ] {
        assert!(doc.contains(c), "docs/mcp.md não cita {c}");
    }
    for k in sandmux_policy::config::MCP_LIST_KEYS {
        assert!(
            doc.contains(&format!("| `{k}` |")),
            "docs/mcp.md não cita a chave {k}"
        );
    }
}

#[test]
fn documentacao_do_hermes_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/hermes.md"))
        .unwrap();
    for c in [
        "sandmux run hermes --gateway",
        "hermes gateway run",
        "hermes-gateway:<pasta>",
        "aguardando cofre",
        "reiniciando em N s",
        "5 segundos",
        "5 minutos",
        "10 minutos",
        "gateway.pid",
        "TELEGRAM_WEBHOOK_SECRET",
        "0000000000:sandmux-fake-",
        "xoxb-sandmux-fake-",
        "xapp-sandmux-fake-",
        "`101`",
        "16 MiB",
    ] {
        assert!(doc.contains(c), "docs/hermes.md não cita {c}");
    }
    // As ferramentas e variáveis dos canais são as do código.
    for (tool, var) in sandmux_policy::toolsecrets::CHANNEL_ENV {
        assert!(
            doc.contains(&format!("`{tool}`")),
            "docs/hermes.md não cita {tool}"
        );
        assert!(doc.contains(var), "docs/hermes.md não cita {var}");
    }
    for (d, _) in &sandmux_policy::presets::KNOWN_TOOLS[7..] {
        assert!(doc.contains(d), "docs/hermes.md não cita {d}");
    }
    assert_eq!(
        sandmux_proto::SessionState::Reiniciando { segundos: 5 }.to_string(),
        "reiniciando em 5 s"
    );
    let readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
    assert!(readme.contains("docs/hermes.md"));
}

#[test]
fn documentacao_do_banco_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/database.md"
    ))
    .unwrap();
    for c in [
        "sandmux db add",
        "sandmux db list",
        "sandmux db remove",
        "sandmux db log",
        "--db",
        sandmux_policy::compose::DB_SOCKET,
        sandmux_policy::compose::DB_MCP,
        sandmux_policy::compose::DB_MCP_NAME,
        "SANDMUX_DB_SOCKET",
        "$XDG_STATE_HOME/sandmux/db/AAAA-MM-DD.jsonl",
        "net-log-days",
    ] {
        assert!(doc.contains(c), "docs/database.md não cita {c}");
    }
    for t in ["conexoes", "tabelas", "descrever", "query"] {
        assert!(
            doc.contains(&format!("| `{t}` |")),
            "docs/database.md não cita a ferramenta {t}"
        );
    }
}

#[test]
fn documentacao_do_chat_acompanha_o_codigo() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/chat.md"))
        .unwrap();
    for c in [
        "sandmux chat",
        "Ctrl+b `i`",
        "[assistant]",
        "chat-model",
        "models",
        "[assistant.provider.<nome>]",
        "/modelo",
        "conversa.json",
        "aguardando cofre",
        "$XDG_STATE_HOME/sandmux/chat/<id>",
        "/run/sandmux/bin/sandmux-chat",
        "SANDMUX_CHAT_PROVIDERS",
        "SANDMUX_CHAT_KEY_<NOME>",
        "`ask`",
        "protocolo 20",
        "Ctrl+b `g`",
        "command-model",
        "`⏎ Inserir`",
        "ESC[200~",
        "protocolo 21",
    ] {
        assert!(doc.contains(c), "docs/chat.md não cita {c}");
    }
    assert_eq!(
        sandmux_policy::compose::CHAT_BIN,
        "/run/sandmux/bin/sandmux-chat"
    );
    assert_eq!(
        sandmux_policy::assistant::PROVIDERS_ENV,
        "SANDMUX_CHAT_PROVIDERS"
    );
    // A tabela dos provedores é a do código.
    for (name, url, _) in sandmux_policy::assistant::KNOWN {
        assert!(
            doc.contains(&format!("| `{name}` | `{url}` |")),
            "docs/chat.md não cita o provedor {name}"
        );
    }
    // O exemplo de [assistant] é válido.
    let i = doc
        .find("```toml\n[assistant]")
        .expect("exemplo de [assistant]")
        + "```toml\n".len();
    let end = i + doc[i..].find("```").unwrap();
    let cfg =
        sandmux_policy::config::Config::parse(std::path::Path::new("docs/chat.md"), &doc[i..end])
            .unwrap();
    assert_eq!(cfg.assistant.providers.len(), 1);
}

#[test]
fn documentacao_da_instalacao_acompanha_o_codigo() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
    let doc = std::fs::read_to_string(format!("{root}docs/install.md")).unwrap();
    let script = std::fs::read_to_string(format!("{root}packaging/install.sh")).unwrap();
    // As opções do script estão na documentação, e as da documentação no script.
    for o in [
        "--no-sudo",
        "--prefix",
        "--from",
        "--version",
        "--no-setup",
        "--uninstall",
    ] {
        assert!(
            doc.contains(&format!("`{o}")),
            "docs/install.md não cita {o}"
        );
        assert!(
            script.contains(&format!("{o})")),
            "install.sh não trata {o}"
        );
    }
    for c in [
        "sandmux setup",
        "--no-daemon",
        "/etc/apparmor.d/sandmux-run",
        "/usr/local/bin",
        "~/.local/bin",
        "scripts/dist.sh",
        "scripts/distro-test.sh",
        "cargo install --git https://github.com/alanyoshida/sandmux sandmux",
        "x86_64-unknown-linux-musl",
    ] {
        assert!(doc.contains(c), "docs/install.md não cita {c}");
    }
    // Os binários da distribuição são os mesmos nos pacotes e no script.
    let bins = "sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp sandmux-helper";
    for f in [
        "packaging/install.sh",
        "scripts/dist.sh",
        "packaging/rpm/sandmux.spec",
        "packaging/aur/PKGBUILD.in",
        "packaging/homebrew/sandmux.rb.in",
    ] {
        let t = std::fs::read_to_string(format!("{root}{f}")).unwrap();
        for b in bins.split(' ') {
            assert!(t.contains(b), "{f} não cita {b}");
        }
    }
    // As unidades do auxiliar root vão nos pacotes do sistema, e não no
    // Homebrew, que não roda como root.
    for f in [
        "scripts/dist.sh",
        "packaging/rpm/sandmux.spec",
        "packaging/aur/PKGBUILD.in",
    ] {
        let t = std::fs::read_to_string(format!("{root}{f}")).unwrap();
        assert!(t.contains("sandmux-helper.socket"), "{f} sem as unidades");
    }
    let brew = std::fs::read_to_string(format!("{root}packaging/homebrew/sandmux.rb.in")).unwrap();
    assert!(!brew.contains("sandmux-helper.socket"));
    assert!(!brew.contains("bin.install \"bin/sandmux-helper"));
}

#[test]
fn documentacao_em_ingles_acompanha_a_portuguesa() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let readme = std::fs::read_to_string(format!("{root}/README.md")).unwrap();
    let readme_en = std::fs::read_to_string(format!("{root}/README.en.md")).unwrap();
    assert!(readme.contains("(README.en.md)"));
    assert!(readme_en.contains("(README.md)"));
    let mut docs: Vec<(String, String)> = vec![("README.en.md".into(), readme_en)];
    for e in std::fs::read_dir(format!("{root}/docs")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "md") {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let en = std::fs::read_to_string(format!("{root}/docs/en/{name}"))
                .unwrap_or_else(|_| panic!("falta docs/en/{name}"));
            assert!(
                en.contains(&format!("(../{name})")),
                "docs/en/{name} sem link para o português"
            );
            docs.push((format!("docs/en/{name}"), en));
        }
    }
    for (name, text) in &docs {
        // Os exemplos de configuração em inglês são válidos.
        let mut rest = text.as_str();
        while let Some(i) = rest.find("```toml\n") {
            let body = &rest[i + 8..];
            let end = body.find("```").unwrap();
            let block = &body[..end];
            if ["[global]", "[tui", "[profile", "[project", "[assistant"]
                .iter()
                .any(|s| block.contains(s))
            {
                sandmux_policy::config::Config::parse(std::path::Path::new(name), block)
                    .unwrap_or_else(|e| panic!("{name}: exemplo inválido: {e}\n{block}"));
            }
            rest = &body[end + 3..];
        }
        // Os links relativos apontam para arquivos que existem.
        let dir = std::path::Path::new(root).join(name);
        let dir = dir.parent().unwrap();
        let mut s = text.as_str();
        while let Some(i) = s.find("](") {
            let after = &s[i + 2..];
            let end = after.find(')').unwrap_or(after.len());
            let link = &after[..end];
            s = &after[end..];
            if link.starts_with("http") || link.starts_with('#') || link.contains(' ') {
                continue;
            }
            let file = link.split('#').next().unwrap();
            if !file.is_empty() {
                assert!(dir.join(file).exists(), "{name}: link quebrado {link}");
            }
        }
    }
}

#[test]
fn auxiliar_root_documentado() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
    let doc = std::fs::read_to_string(format!("{root}docs/helper.md")).unwrap();
    for c in [
        "/run/sandmux-helper.sock",
        "SO_PEERCRED",
        "sandmux-helper.socket",
        "~/.local/bin",
        "16384",
        "scripts/check-root.sh",
        "lsm=",
        "sudo systemctl disable --now sandmux-helper.socket",
        "[aviso] versão do auxiliar root",
        "sudo systemctl restart sandmux-helper.service",
    ] {
        assert!(doc.contains(c), "docs/helper.md não cita {c}");
    }
    assert!(doc.contains(&sandmux::helper::MAX_MARKS.to_string()));
    assert!(doc.contains(&sandmux::helper::MAX_CONFINE_PER_UID.to_string()));
    assert_eq!(sandmux::helper::SOCKET, "/run/sandmux-helper.sock");
    let docker = std::fs::read_to_string(format!("{root}docs/docker.md")).unwrap();
    for c in [
        "## Confinamento por eBPF",
        "EPERM",
        "commondir",
        "(helper.md)",
    ] {
        assert!(docker.contains(c), "docs/docker.md não cita {c}");
    }
    for c in [
        "### Ao vivo",
        "fs.inotify.max_user_watches",
        "docker-<id>",
        "200 ms",
    ] {
        assert!(docker.contains(c), "docs/docker.md não cita {c}");
    }
    let files = std::fs::read_to_string(format!("{root}docs/files.md")).unwrap();
    assert!(files.contains("docker-<id>") && files.contains("### Contêineres do Docker privado"));
    assert!(doc.contains("`unmark`"));
    assert!(doc.contains("`observe`") && doc.contains("64"));
    let files = std::fs::read_to_string(format!("{root}docs/files.md")).unwrap();
    for c in ["### Execuções", "`via`", "`exec`", "execveat", "1024"] {
        assert!(files.contains(c), "docs/files.md não cita {c}");
    }
    let unit =
        std::fs::read_to_string(format!("{root}packaging/systemd/sandmux-helper.socket")).unwrap();
    assert!(unit.contains(&format!(
        "ListenSequentialPacket={}",
        sandmux::helper::SOCKET
    )));
}
