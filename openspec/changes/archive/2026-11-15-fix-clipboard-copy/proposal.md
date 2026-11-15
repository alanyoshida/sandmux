## Why

No primeiro uso real da `v0.1.0`, a cópia do opencode para o clipboard não funcionou. O opencode liga o mouse, então o arrasto vai para ele, e ele copia por OSC 52. O daemon retira essa sequência por padrão (decisão 2.7). E, mesmo com `clipboard = true`, a TUI não a repassa: o emulador de terminal da aba (`vt100`) consome a sequência e não a manda ao terminal do usuário. A cópia da própria TUI só usa OSC 52, que depende do terminal e, dentro do tmux, da opção `set-clipboard`.

## What Changes

- **Cópia do harness liberada:** com a escrita no clipboard liberada para a sessão, o OSC 52 de gravação que o harness manda passa a chegar ao usuário pela TUI. Hoje ele se perde no emulador da aba.
- **Clipboard do sistema (decisão do usuário):** a TUI grava o que copia, tanto a própria seleção quanto a cópia liberada do harness, também direto no clipboard do sistema (Wayland com o protocolo de data-control, ou X11), além do OSC 52. Assim a cópia funciona no tmux e em terminais sem OSC 52.
- **Clique no ✕ (relato do usuário, 2026-11-14):** o ✕ de uma aba encerrada na barra parecia um botão de fechar e não fazia nada. Agora o clique nele fecha a aba, sem confirmação, porque a sessão já terminou.
- **O bloqueio padrão continua:** a cópia do harness só passa com `clipboard = true` ou `--allow-clipboard`. Não há aviso novo.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a cópia da TUI vai também ao clipboard do sistema, a cópia liberada do harness chega ao usuário pela TUI, e o clique no ✕ fecha a aba encerrada.

## Impact

- **`sandmux`:**
  - `tui/mod.rs`: o emulador da aba com os callbacks do `vt100`, a cópia por OSC 52 e pelo clipboard do sistema;
  - `tui/paste.rs`: gravação de texto com o `arboard`, que já é dependência.
- **Documentação:** `docs/tui.md`.
