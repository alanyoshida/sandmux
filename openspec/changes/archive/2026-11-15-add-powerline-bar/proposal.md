## Why

O usuário quer a interface do sandmux com o visual da barra do tmux dele (2026-11-14): barra embaixo, separadores inclinados do Powerline, o número da aba num bloco colorido, a aba ativa em destaque e, à direita, informações da sessão. Ele decidiu que o visual novo é o padrão para todos.

## What Changes

- **Barra embaixo, estilo Powerline (padrão):**
  - cada aba é um bloco com o número, seguido do nome e das marcas (✕, ‖, cofre, ↻, ! e •), com separadores inclinados;
  - a aba ativa tem o bloco do número numa cor de destaque e o nome em negrito;
  - os avisos (as mensagens da TUI) ficam no meio.
- **À direita** (decisão do usuário), em blocos coloridos:
  - o harness da aba ativa com a pasta;
  - o estado do cofre (desbloqueado, trancado ou sem cofre);
  - os alertas de rede (quantas abas têm bloqueio `!` e domínio novo `•`, só quando há);
  - o tempo de vida da aba ativa (`1d 47m`).
- **Fonte:** os separadores e os ícones são do Powerline e do Nerd Font. Quem não tem a fonte usa `style = "simples"` em `[tui]`, a barra de antes, sem símbolos especiais. `bar = "top"` volta a barra para cima.
- **Cores:** elementos novos do tema para os blocos (`tab-index-*`, `tab-active-index-*`, `chip-*`), com padrões no estilo do tmux do usuário, e os temas embutidos com valores para eles.
- **Mouse:** os cliques na barra e o deslocamento das linhas da aba seguem a posição da barra.
- **Protocolo 23:** `SessionInfo.started_at`, a hora em que o lançamento atual da sessão começou.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a barra de abas (posição, estilo, blocos da direita) e as chaves `style` e `bar` de `[tui]`.

## Impact

- **`sandmux-policy`:** `config.rs` (`style`, `bar`, elementos do tema).
- **`sandmux-proto`:** `SessionInfo.started_at`; protocolo 23.
- **`sandmux`:**
  - `daemon/`: a hora do lançamento;
  - `tui/render.rs`: a barra nova;
  - `tui/mod.rs`: o estado do cofre, a posição da barra no mouse.
- **Testes:** um ajudante para o texto das abas no estilo novo.
- **Documentação:** `docs/tui.md`.
