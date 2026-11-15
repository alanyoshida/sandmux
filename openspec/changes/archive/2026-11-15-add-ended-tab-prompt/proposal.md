## Why

Na primeira execução do hermes no sandbox, o assistente de configuração dele terminou ("Setup Complete!") e o hermes saiu, como faz fora do sandbox. A aba ficou encerrada, e o caminho de volta (Ctrl+b r) só aparecia numa linha da tela. O usuário pediu (2026-11-14) que a TUI pergunte se quer reiniciar quando a aba termina.

## What Changes

- **Modal de aba encerrada:** quando o harness da aba ativa termina, a TUI abre um modal com o código de saída. Enter ou `r` reinicia a sessão na mesma aba, `x` fecha a aba e Esc mantém a aba encerrada, como hoje.
- **Aba em segundo plano:** quando uma aba que não está ativa termina, o modal aparece ao ativá-la, uma vez.
- **Só no fim acontecido com a TUI aberta:** abas que já estavam encerradas ao abrir a TUI não perguntam. A sessão do gateway do hermes, que reinicia sozinha, nunca fica encerrada e não pergunta.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a aba encerrada pergunta se deve reiniciar.

## Impact

- **`sandmux`:** `tui/mod.rs` (a marca de pergunta por aba e o `Modal::Ended`) e `tui/render.rs` (o modal).
- **Documentação:** `docs/tui.md`.
