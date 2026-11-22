## Why

O usuário rodou `sleep` de dentro do opencode e, na seção Processos, o `sleep`, o `bash` e o próprio `opencode` apareceram todos na raiz, sem os pais (2026-11-21). A causa: o auxiliar root em execução era o 0.1.30, de antes da atualização, e o sandmux instalado era o 0.1.32. O pai de cada execução só vem no evento a partir da 0.1.32, e o daemon trata uma execução sem pai como raiz.

O auxiliar root só troca quando é reiniciado, e atualizar os binários não o reinicia. Hoje nada avisa disso: o `sandmux doctor` mostra "auxiliar root 0.1.30" como `[ok]`, e a seção Processos mostra a árvore plana sem explicação.

## What Changes

- **`sandmux doctor`:** um aviso quando o auxiliar root em execução é de uma versão anterior à do sandmux, com as duas versões e a correção (`sandmux setup`, que reinicia o auxiliar). É um aviso, não uma falha: o código de saída não muda. Ele aparece também no Diagnóstico da TUI, que mostra as mesmas verificações.
- **Seção Processos:** com o auxiliar antigo, uma linha de aviso no topo da seção, com as versões e o `sandmux setup`, dizendo que a árvore pode sair sem os pais.
- Sem auxiliar, ou com o auxiliar na mesma versão ou mais nova, nada muda.
- Sem protocolo novo: o `doctor` e a TUI perguntam a versão ao auxiliar pelo pedido `status`, que já existe.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `root-helper`: o aviso de auxiliar antigo no `sandmux doctor`.
- `process-tree`: o aviso de auxiliar antigo na seção Processos.

## Impact

- **`sandmux`:**
  - `helper.rs`: a comparação de versões e a consulta da versão do auxiliar em execução;
  - `doctor.rs`: a verificação nova;
  - `tui/procs.rs`, `tui/settings.rs` e `tui/render.rs`: o aviso na seção Processos.
- **Testes e documentação:** testes de unidade da comparação, testes com um auxiliar falso de versão antiga para o `doctor` e para a TUI; `docs/helper.md`, `docs/tui.md` e as versões em inglês.
