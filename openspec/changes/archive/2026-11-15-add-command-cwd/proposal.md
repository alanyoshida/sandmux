## Why

Item "Pasta atual do shell no modal de comando" da lista depois da v1, escolhido pelo usuário (2026-11-15).

O modal de comando (Ctrl+b `g`) manda ao modelo a pasta de trabalho da aba, que é a pasta em que a aba foi aberta. Depois de um `cd src/api` no shell da aba, o modelo continua achando que está na raiz do projeto, e sugere comandos com caminhos relativos errados.

## What Changes

- **A pasta atual do processo da aba:** o daemon lê a pasta em que está o processo em primeiro plano do terminal da aba, em geral o shell. Ele usa o grupo de primeiro plano do PTY e o `/proc/<pid>/cwd`. Como a pasta de trabalho entra no sandbox no mesmo caminho, o caminho lido é o mesmo que o shell vê.
- **No modal:** ao abrir, o modal pede essa pasta ao daemon, mostra a linha `pasta` e a manda ao modelo no lugar da pasta de trabalho. Se a pasta não puder ser lida (a aba parada, o processo saiu), vale a pasta de trabalho, como hoje.
- **Protocolo 33:** a mensagem `Cwd { id }` e a resposta `CwdOf { path }`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `shell-command-assistant`: o modelo recebe a pasta atual do processo da aba, não a pasta em que ela foi aberta.

## Impact

- **`sandmux-proto`:** `Cwd` e `CwdOf`; protocolo 33.
- **Daemon:** a leitura da pasta pelo PTY da sessão.
- **TUI:** a pasta no estado do modal de comando, a linha no desenho e o texto de sistema.
- **Testes:** o provedor recebe a pasta depois de um `cd`; com a aba parada, a pasta de trabalho.
- **Documentação:** `docs/chat.md` e o par em inglês.
