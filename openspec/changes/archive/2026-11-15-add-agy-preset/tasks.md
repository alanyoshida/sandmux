## 1. Preset e importação

- [x] 1.1 Preset `agy` em `presets.rs`: `PRESETS`, `importable`, `FAKE_KEY_HARNESSES`, `plan` (binário em somente leitura, `AGY_CLI_DISABLE_AUTO_UPDATE=true`, `--settings {"modelProvider":"gemini"}` com `GEMINI_API_KEY`) e `updater` (`agy update`)
- [x] 1.2 `compose.rs`: `--dangerously-skip-permissions` em `no_asks` e as variáveis falsas passadas ao plano do `agy`
- [x] 1.3 Tabela de importação do `agy` e `state_roots` em `sandmux-import`, sem o token, os segredos de plugins, os projetos, as conversas e os caches
- [x] 1.4 Mensagens e ajuda que listam os harnesses (`run`, `update`, harnesses com atualizador)

## 2. Verificação e documentação

- [x] 2.1 Testes de unidade (atualizador, `--settings` com a chave) e de integração com um `agy` falso (importação sem o token, binário em somente leitura, variável de atualização, sem perguntas e `harness-asks`, `sandmux update agy`)
- [x] 2.2 `docs/agy.md` e `docs/en/agy.md`, README e `README.en.md`; roadmap
- [x] 2.3 Suíte completa (`scripts/check.sh`) sob o vigia
