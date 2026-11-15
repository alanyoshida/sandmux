## 1. TUI

- [x] 1.1 Emulador da aba com `vt100::Parser<Clip>`, a fila de cópias e o limite de 1 MiB
- [x] 1.2 `copy_text`: OSC 52 e `arboard` com o `Clipboard` guardado no `App`; a seleção e a cópia do harness passam por ele

## 2. Verificação e documentação

- [x] 2.1 Testes: a cópia liberada do harness chega ao terminal pela TUI; bloqueada, não chega; unidade do `Clip`
- [x] 2.2 `docs/tui.md`
- [x] 2.4 Clique no ✕ da aba encerrada fecha a aba, com teste
- [ ] 2.3 Verificação à mão pelo usuário, no tmux desta máquina, com o opencode e `clipboard = true`
