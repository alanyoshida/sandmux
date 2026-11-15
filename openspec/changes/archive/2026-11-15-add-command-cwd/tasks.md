## 1. Pasta atual

- [x] 1.1 Protocolo 33 com `Cwd`/`CwdOf`; no daemon, a pasta do grupo em primeiro plano do PTY da sessão (`tcgetpgrp` e `/proc/<pgid>/cwd`)
- [x] 1.2 TUI: a pasta pedida ao abrir o modal de comando, a linha `pasta` no modal e a pasta no texto de sistema, com a pasta de trabalho como recuo

## 2. Testes e documentação

- [x] 2.1 Testes: o provedor recebe a pasta depois de um `cd`, e o modal a mostra
- [x] 2.2 Documentação: `docs/chat.md` e o par em inglês
