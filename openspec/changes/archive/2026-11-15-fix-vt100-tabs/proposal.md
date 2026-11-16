## Why

Depois do change 64 (DECAWM), o usuário relatou (2026-11-15) que a resposta do `agy` ainda embaralhava na TUI. Uma segunda captura, gravada com a aba no tamanho real (176×47), reproduzida no emulador das abas e no tmux, mostrou as telas se separando logo depois de `CSI 14 Z`. É o CBT (Cursor Backward Tabulation), que o renderizador do `agy` usa para mover o cursor para a esquerda com menos bytes (o `xterm-256color` declara `cbt`). O `vt100` 0.18.1 não implementa o `CSI Z`, nem o `CSI I` (CHT), e o cursor ficava parado: o texto seguinte saía no lugar errado.

## What Changes

- **CBT e CHT no emulador:** a cópia do `vt100` (`vendor/vt100`) trata `CSI Z` (voltar N paradas de tabulação, a cada 8 colunas, a partir da última coluna quando há quebra pendente) e `CSI I` (avançar N paradas).
- **Verificado:** as duas capturas do `agy` dão a mesma tela que o tmux, na tela final e em 16 pontos intermediários.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a emulação da aba move o cursor pelas tabulações para a frente e para trás.

## Impact

- **`vendor/vt100`:** `src/perform.rs`, `src/screen.rs`, `src/grid.rs` e o `SANDMUX.md`.
- **Testes:** os casos de CBT e CHT conferidos no tmux (o CHT, que o tmux não implementa, pelo ECMA-48) e uma aba da TUI.
