//! Documentação acompanha o código: o exemplo de docs/launch-spec.md é aceito
//! pelo lançador, e docs/sessions.md cita os comandos e a versão do protocolo.

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
