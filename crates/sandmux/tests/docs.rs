//! Tarefa 3.3: o exemplo de docs/launch-spec.md é aceito pelo lançador.

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
