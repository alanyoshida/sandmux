## 1. Diagnóstico em dados

- [x] 1.1 `doctor.rs`: `report()` com grupos e verificações (`state`, `label`, `summary`, `help`, `cli`), e `run_with(false)` imprimindo os `cli`. Verificação: os testes atuais da saída do `sandmux doctor` (`cli.rs`, `daemon.rs`, `config.rs`, `limits.rs`, `docker.rs`, `vault.rs`) passam sem mudança
- [x] 1.2 `secretdomains.rs`: `nearby_url_domain(text, line)`. Verificação: testes de unidade (URL na linha seguinte, a mais próxima ganha, `localhost` vira `host.sandmux.internal`, nada além de 10 linhas)

## 2. Seção na página

- [x] 2.1 `settings.rs` e `render.rs`: a seção Diagnóstico (lista agrupada, detalhe embaixo, contagem de falhas na lateral, `r`), lida ao abrir a página e ao entrar na seção. Verificação: teste de integração na TUI
- [x] 2.2 Ação `doctor` (`D`) em `TUI_ACTIONS`, na ajuda e no menu, abrindo a página na seção. Verificação: o mesmo teste abre pela tecla
- [x] 2.3 Fluxo do `f`: senha do cofre quando trancado, um formulário de domínios por chave (inferido, sugerido ou vazio, editável), Esc pula o arquivo, envio só depois de todas as chaves do arquivo, `apply_fakes`, confirmação do login do `agy`, aviso no fim e relatório relido. Verificação: teste de integração (chave do OpenRouter com `openrouter.ai`; chave com a URL próxima e o domínio editado; arquivo pulado sem nada no cofre; nenhum valor real na tela)

## 3. Documentação e suíte

- [x] 3.1 `docs/tui.md` (a seção, a tecla `D`, o `f`) e `docs/vault.md` (mover pela TUI), com os pares em inglês; roadmap. Verificação: o teste dos docs passa
- [x] 3.2 Suíte completa (`scripts/check.sh`) sob o vigia, com `CARGO_BUILD_JOBS=8`
