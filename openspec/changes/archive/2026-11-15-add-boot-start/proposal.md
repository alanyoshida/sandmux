## Why

Item "Daemon no boot" da lista depois da v1, escolhido pelo usuário (2026-11-15): o daemon sobe sem login (linger do systemd), junto com o keyring.

Com o linger ligado, o gerenciador do systemd do usuário sobe no boot, e o `sandmuxd` com ele, porque a unidade está em `default.target`. Isso já acontece nesta máquina (`Linger=yes`). Hoje:

- não há comando para ligar ou ver isso;
- no boot, sem login, o daemon tenta o keyring, que está trancado até o login, e restaura as abas;
- depois do login, nada tenta o keyring de novo até alguém abrir a TUI.

Decisão do usuário (2026-11-15): no boot, sem login, nenhuma aba volta. As abas voltam no primeiro login.

## What Changes

- **`sandmux daemon boot [on|off]`:**
  - `on` liga o linger do usuário (`loginctl enable-linger`, sem sudo) e garante a unidade habilitada;
  - `off` desliga o linger, com o aviso de que outros serviços de usuário também deixam de subir no boot;
  - sem argumento, mostra o estado.
- **Espera do login:** o daemon que sobe sem ninguém logado (estado `lingering` no logind) carrega a lista de sessões, mas não relança nenhuma. As abas que voltariam ficam no estado novo **aguardando o login**, guardado na lista: se o daemon reiniciar de novo antes do login, elas continuam esperando.
- **No primeiro login,** o daemon:
  - relança essas abas;
  - tenta abrir o cofre pelo keyring;
  - avisa no desktop as abas que continuam esperando o cofre.

  Com alguém logado ao subir, tudo segue como hoje.
- **Keyring:** sem login, o daemon não consulta o keyring, que está trancado e poderia pedir a senha num diálogo sem tela.
- **`sandmux daemon status` e `sandmux doctor`** dizem se o daemon sobe no boot.
- **Protocolo 31:** o estado novo da sessão.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `session-daemon`: o início no boot, a espera do login antes de restaurar e o comando `daemon boot`.

## Impact

- **Daemon:**
  - a leitura do estado do usuário no logind (`/run/systemd/users/<uid>`);
  - a restauração adiada e a thread que espera o login;
  - o keyring e a notificação do cofre só com login;
  - o estado `AguardandoLogin`.
- **`sandmux-proto`:** a variante `AguardandoLogin` de `SessionState`; protocolo 31.
- **CLI:** `daemon boot`, a linha nova no `daemon status` e no `doctor`.
- **TUI:** a marca da aba aguardando o login.
- **Testes:**
  - o `loginctl`, o arquivo do logind e a pasta do linger podem ser trocados por variáveis (`SANDMUX_LOGINCTL`, `SANDMUX_LOGIND_USER`, `SANDMUX_LINGER_DIR`), sem nunca mudar o sistema de quem roda os testes;
  - o login é simulado trocando o estado no arquivo.
- **Documentação:** `docs/sessions.md`, `docs/install.md` e os pares em inglês.
