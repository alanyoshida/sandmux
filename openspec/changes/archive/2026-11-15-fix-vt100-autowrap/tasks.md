## 1. Emulador

- [x] 1.1 Copiar o `vt100` 0.18.1 para `vendor/vt100` com a licença e o `SANDMUX.md`; `[patch.crates-io]` e `exclude` no workspace
- [x] 1.2 Modo DECAWM (`?7h`/`?7l`) em `text()`, no estado formatado e na diferença de modos

## 2. Testes

- [x] 2.1 Testes do modo na cópia (sobrescrever a última coluna, caractere largo, religar) e de uma aba da TUI com um harness que escreve com a quebra desligada
- [x] 2.2 Reprodução da captura do `agy` (fora do repositório, porque tem a conversa do usuário): a tela do emulador igual à do tmux
