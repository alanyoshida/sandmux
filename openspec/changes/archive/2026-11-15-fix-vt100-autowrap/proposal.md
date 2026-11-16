## Why

Relato do usuário (2026-11-15): numa aba do Antigravity CLI (`agy`), a resposta aparecia embaralhada na TUI, com parágrafos repetidos em linhas vizinhas e pedaços soltos na margem. O mesmo fluxo de bytes, visto pelo `sandmux attach` num terminal comum (Konsole), aparecia certo.

Causa, achada com uma captura do fluxo real reproduzida no `vt100` 0.18.1 (o emulador das abas) e no tmux: o `vt100` não implementa o modo DECAWM (`CSI ? 7 h`/`CSI ? 7 l`), a quebra automática de linha. A captura tinha 55 pares de `?7l`/`?7h`, todos ignorados. O renderizador do `agy` (`charmbracelet/ultraviolet`) desliga a quebra para escrever até a última coluna e conta com o cursor ali. No `vt100`, o texto descia uma linha, e cada redesenho por diferença partia de uma tela errada.

## What Changes

- **DECAWM no emulador:** uma cópia local do `vt100` 0.18.1 em `vendor/vt100` (MIT), ligada pelo `[patch.crates-io]`, com o modo 7. Com a quebra desligada, um caractere na última coluna fica nela, e os seguintes a sobrescrevem, como no xterm e no tmux. O modo entra no estado da tela que o daemon manda ao reanexar.
- **Sem mudança no resto do emulador:** a cópia é a versão publicada, com só esse acréscimo, anotado no `vendor/vt100/SANDMUX.md`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a emulação da aba respeita a quebra automática de linha desligada.

## Impact

- **Dependências:** `vendor/vt100` e `[patch.crates-io]` no workspace, excluído dos membros.
- **Testes:** o modo no emulador (unitário na cópia e na TUI) e o redesenho de uma linha cheia com a quebra desligada numa aba.
