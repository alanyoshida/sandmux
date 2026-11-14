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
