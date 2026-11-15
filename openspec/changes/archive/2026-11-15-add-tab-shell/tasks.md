## 1. Lançador

- [x] 1.1 Auxiliar de entrada: namespaces, processo de espera, shell com o confinamento do comando, aberturas pelo supervisor
- [x] 1.2 Socket `<controle>.enter` no supervisor e `sandmux-run --enter`

## 2. Daemon e clientes

- [x] 2.1 Harness `dentro`: escolha da aba alvo, lançamento num PTY, etiqueta própria, restauração parada
- [x] 2.2 Ctrl+b `t` na TUI e `sandmux enter [id]`
- [x] 2.3 A tecla escolhida em `[tui.keys]` vence a padrão de outra ação (o `t` novo não invalida arquivos que já o usavam)

## 3. Verificação e documentação

- [x] 3.1 Testes: mesma pasta, mesmos processos, segredo negado, `/etc` só leitura, aba alvo continua viva; tecla da TUI
- [x] 3.2 `docs/sessions.md`, `docs/tui.md`, README e as versões em inglês; roadmap
