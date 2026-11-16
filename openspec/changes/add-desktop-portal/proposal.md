## Why

Hoje nada do D-Bus entra no sandbox. Por isso um programa confinado não consegue ler o tema claro ou escuro do desktop, mandar uma notificação nem abrir um link no navegador do host. O caso mais visível é o login de um harness: o `agy` e o Claude Code mostram a URL e pedem para colar o código, porque o `xdg-open` de dentro não abre nada. O change 58 (janela) deixou isso registrado como limite, e o portal foi priorizado pelo usuário em 2026-11-15, com três serviços:

- Settings (a aparência);
- Notificações;
- abrir link, só para domínios que a política de rede da aba permite.

O ponto que ficou em aberto, se o portal vem com a janela ou numa opção própria, foi decidido pelo usuário em 2026-11-16.

## What Changes

- **Opção `portal`:** chave nova `portal = true|false` no global, nos perfis e nos projetos, e as opções `--portal` e `--no-portal` no `sandmux run`.
  - Decisão do usuário (2026-11-16): fica ligada por padrão em toda aba do daemon, com ou sem janela.
  - A página de configurações ganha o item "Portal" na seção Sandbox, e o painel de proteção da TUI mostra o estado.
- **D-Bus do sandmux, não o do usuário:** com o portal ligado, o sandbox ganha `/run/user/<uid>/bus`, servido pelo daemon. É um barramento mínimo, que responde ao `org.freedesktop.DBus` (o `Hello` e as consultas de nome) e a poucos serviços. Qualquer outro destino ou interface recebe erro. O D-Bus de sessão do usuário continua fora do sandbox.
- **Aparência (`org.freedesktop.portal.Settings`):** `Read`, `ReadAll` e `ReadOne` só do namespace `org.freedesktop.appearance` (`color-scheme`, `accent-color` e `contrast`). O daemon lê os valores do portal do host; sem portal no host, responde "sem preferência".
- **Notificações:**
  - Decisão do usuário (2026-11-16): o portal (`org.freedesktop.portal.Notification`) e o `org.freedesktop.Notifications` direto, que o `notify-send` usa.
  - Os dois são repassados ao desktop pelo daemon, com o nome da aba no título. Ícones, ações, sons e dicas do sandbox não passam.
  - O tamanho é limitado, e a frequência também, por aba.
- **Abrir link (`org.freedesktop.portal.OpenURI.OpenURI`):**
  - Só `http` e `https`, e só para um host que a política de rede da aba permite.
  - Decisão do usuário (2026-11-16): o link abre direto no navegador padrão do host, sem perguntar.
  - Cada pedido, aberto ou recusado, entra no registro de rede da aba.
  - `OpenFile` e `OpenDirectory` são recusados.
- **Os programas de dentro usam o portal:** o sandbox ganha `DBUS_SESSION_BUS_ADDRESS` e `GIO_USE_PORTALS=1`, e um `$XDG_RUNTIME_DIR/flatpak-info` vazio, que faz o `xdg-open` abrir links pelo portal. A pasta `XDG_RUNTIME_DIR` em memória, que hoje só existe com a janela, passa a existir com o portal; com os dois, é a mesma pasta.
- **Só nas abas do daemon:** o barramento vive no daemon. No `run` sem terminal, o padrão segue sem portal, em silêncio; um `--portal` explícito termina com erro, como a janela.
- **BREAKING (interno):** as opções da sessão levam o portal, e o protocolo entre o cliente e o daemon passa a 37. Depois de atualizar, `sandmux daemon restart`.

## Capabilities

### New Capabilities
- `desktop-portal`: a opção `portal`, o barramento mínimo do sandmux, a aparência, as notificações e o abrir link filtrado pela política de rede.

### Modified Capabilities
- `sandbox-isolation`: o socket do barramento do sandmux entra como exceção aos sockets do host, e `DBUS_SESSION_BUS_ADDRESS`, `GIO_USE_PORTALS` e `XDG_RUNTIME_DIR` entram com o portal.

## Impact

- `crates/sandmux/src/daemon/portal.rs` (novo): o barramento por aba (zbus em modo ponto a ponto, um servidor por conexão), os serviços e o repasse ao host.
- `crates/sandmux/src/daemon/notify.rs`: o repasse das notificações do sandbox.
- `crates/sandmux-policy`: a chave `portal`, a composição do `XDG_RUNTIME_DIR` com o `bus` e o `flatpak-info`, e as variáveis.
- `crates/sandmux-proto`: o portal nas opções da sessão, protocolo 37.
- `crates/sandmux/src/main.rs`, `launch.rs` e a TUI (configurações e painel).
- O zbus ganha o recurso `p2p`.
- `docs/window.md`, `docs/config.md`, `docs/sessions.md` e os pares em inglês.
