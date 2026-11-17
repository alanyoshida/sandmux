## 1. Troca do valor

- [ ] 1.1 `sandmux-proto`: `Msg::VaultSetValue` e protocolo 38 (`docs/sessions.md` e o par em inglês). Verificação: o teste dos docs
- [ ] 1.2 Daemon: `VaultState::set_value` (token existente, falso, domínios e ferramentas mantidos; recusas de nome ausente, SSH e banco) e o tratamento em `conn.rs`. Verificação: testes de unidade em `vault.rs`
- [ ] 1.3 `sandmux vault set <nome>`: valor sem eco, recusa do vazio. Verificação: teste de integração (falso mantido, valor novo trocado no tráfego por um servidor local, nome ausente recusado)

## 2. TUI e documentação

- [ ] 2.1 Seção Cofre: tecla `v`, formulário mascarado, rodapé com a tecla. Verificação: o teste do cofre pela página
- [ ] 2.2 `docs/vault.md` (trocar uma chave vencida, a tecla `v`) e o par em inglês; roadmap. Verificação: o teste dos docs
- [ ] 2.3 Suíte completa (`scripts/check.sh`) sob o vigia, com `CARGO_BUILD_JOBS=16`
