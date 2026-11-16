## Context

- O change 58 montou a janela por aba: o daemon cria o socket do compositor (security-context e o repassador do Wayland em `wlproxy.rs`), e a composição (`compose.rs`) põe um `tmpfs` em `/run/user/<uid>` com `wayland-0`, `pipewire-0` e `pulse/native`. Sem a janela, não há `XDG_RUNTIME_DIR` no sandbox.
- O daemon já fala com o D-Bus de sessão do usuário pelo zbus 4.4 (`daemon/notify.rs`, change 48), e os testes sobem um `dbus-daemon` próprio com um servidor de notificações falso (`tests/notify.rs`).
- A política de rede compilada decide um nome com `NetPolicy::decide_name` (`sandmux-spec/src/net.rs`).
- O roadmap registrava a decisão do agente de um proxy de D-Bus próprio, e não o `xdg-dbus-proxy`: com o proxy, o portal veria o app do host, e o OpenURI viraria um canal de saída fora do gateway.

### Prova de viabilidade (2026-11-16)
Um servidor com o zbus 4.4 em modo ponto a ponto (recurso `p2p`) atendeu cada cliente numa conexão própria. O servidor implementava:

- `Hello`, `AddMatch`, `GetNameOwner`, `NameHasOwner`, `ListNames`, `StartServiceByName` e `GetId` em `/org/freedesktop/DBus`;
- `org.freedesktop.Notifications` e `org.freedesktop.portal.Settings.ReadOne`.

Clientes reais aceitaram o servidor como barramento de sessão:

- `gdbus call --session` leu o `color-scheme`;
- `busctl` chamou o `GetServerInformation`.

O `notify-send` do host foi barrado pelo perfil do AppArmor dele (`/etc/apparmor.d/notify-send`), que só deixa conectar no caminho padrão `/run/user/<uid>/bus`, e é nesse caminho que o socket entra no sandbox. O `xdg-open` (xdg-utils) usa o OpenURI do portal pelo `gdbus` quando existe `$XDG_RUNTIME_DIR/flatpak-info`, e a GIO usa os portais com `GIO_USE_PORTALS=1`.

## Goals / Non-Goals

**Goals:**
- Aparência, notificações e abrir link para programas confinados, sem nenhum acesso ao D-Bus de sessão do usuário.
- O abrir link respeita a política de rede da aba e fica no registro.

**Non-Goals:**
- Outros portais (seletor de arquivos, captura de tela, câmera, segredos, impressão).
- Um barramento de verdade entre processos do sandbox (um programa não oferece serviço a outro).
- As ações e os ícones das notificações.
- Abrir arquivos e pastas pelo portal.
- O retorno automático em `localhost` dos logins, que é outro item.

## Decisions

### Barramento mínimo por conexão
O daemon cria, por aba com o portal, um socket Unix na pasta privada da aba (como os outros sockets dela) e o monta em `/run/user/<uid>/bus` no sandbox. Cada conexão aceita vira um servidor zbus ponto a ponto, e o daemon serve, em cada conexão, os objetos abaixo.

- `/org/freedesktop/DBus`, com a interface `org.freedesktop.DBus`:
  - `Hello` devolve um nome único `:1.N` por conexão;
  - `AddMatch` e `RemoveMatch` aceitam e não fazem nada;
  - `GetNameOwner` e `NameHasOwner` respondem só pelos nomes servidos: `org.freedesktop.DBus`, `org.freedesktop.portal.Desktop` e `org.freedesktop.Notifications`;
  - `ListNames`, `ListActivatableNames`, `StartServiceByName` (já rodando) e `GetId`;
  - `RequestName` devolve "já existe" e não registra nada.
- `/org/freedesktop/portal/desktop`, com as interfaces `org.freedesktop.portal.Settings`, `org.freedesktop.portal.Notification` e `org.freedesktop.portal.OpenURI`, cada uma com a propriedade `version`.
- `/org/freedesktop/Notifications`, com a interface `org.freedesktop.Notifications`.

Num servidor ponto a ponto o campo de destino não é olhado: o zbus despacha pelo caminho e pela interface, e um caminho ou interface desconhecidos recebem `UnknownObject` ou `UnknownMethod`. Assim, nenhuma chamada sai do conjunto acima, e nada é repassado como mensagem ao D-Bus do usuário. O daemon só faz, do lado de fora, as três ações abaixo, com dados que ele mesmo monta.

Alternativa descartada: repassar ao D-Bus do usuário com filtro. Seria um canal mais largo, o portal do host veria o app como um app do host, e um erro no filtro abriria o barramento inteiro.

### Aparência
`Read`, `ReadAll` e `ReadOne` atendem só `org.freedesktop.appearance` (`color-scheme`, `accent-color`, `contrast`). O daemon lê esses valores do portal do host (`org.freedesktop.portal.Settings.ReadOne` no D-Bus de sessão), com um cache de 5 segundos. Sem portal no host, responde `color-scheme = 0` (sem preferência) e o erro padrão de chave ausente para as outras. Qualquer outro namespace devolve vazio no `ReadAll` e "não encontrado" no `Read`. O sinal `SettingChanged` não é emitido nesta versão.

### Notificações
`Notify` e `AddNotification` viram uma chamada `Notify` do daemon ao desktop, montada assim:

- `app_name` é `sandmux`, sem o ícone do sandbox;
- o título é `<nome da aba>: <título>`, cortado em 120 caracteres;
- o corpo é texto puro cortado em 1000 caracteres, e a marcação HTML é escapada.

Não passam ações, dicas (a não ser `urgency`, limitada a normal), som nem imagem. O limite por aba é de 5 notificações a cada 10 segundos; o excesso recebe uma resposta de sucesso e é descartado, para o programa não entrar em laço de erro.

- `CloseNotification` e `RemoveNotification` fecham só as notificações daquela aba.
- `GetCapabilities` devolve `body`, e `GetServerInformation` devolve o sandmux.

As notificações do sandbox não passam pela supressão da aba em foco das notificações do próprio sandmux: o programa pediu.

### Abrir link
`OpenURI(parent_window, uri, options)`:

1. Aceita só `http` e `https`, com host. O host é decidido pela política de rede da aba (`decide_name`, a mesma que o gateway aplica ao DNS). `Pass` abre; `Block` recusa.
2. Decisão do usuário (2026-11-16): abre direto, sem perguntar. O daemon chama o `OpenURI` do portal do host (o navegador padrão). Sem portal no host, roda `xdg-open` com a URL como argumento, sem shell.
3. Cada pedido vira uma linha no registro de rede da aba (tipo `open-uri`, o host, e `blocked` com o motivo quando recusado). Um pedido recusado também aparece na aba como os bloqueios de rede.
4. Devolve o caminho de um objeto `Request` e emite `Response` (0 aberto, 2 recusado) nele.

`OpenFile` e `OpenDirectory` devolvem `AccessDenied`.

No modo aberta, quase todo domínio passa, então o abrir link vale para quase qualquer site. Isso é coerente com a política da aba e fica no registro.

### Os programas usam o portal
Com o portal, a composição põe:

- o `tmpfs` em `/run/user/<uid>`, com o `bus` e um `flatpak-info` vazio, em somente leitura;
- `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/<uid>/bus` e `GIO_USE_PORTALS=1`.

Com a janela, é a mesma pasta, e os sockets dela continuam lá. O `flatpak-info` só faz o `xdg-open` escolher o caminho do portal; a GLib procura o `/.flatpak-info` na raiz, e não esse arquivo.

### Opção e protocolo
`portal` entra em `BOOL_KEYS`, com padrão `true`, e nas opções da sessão (`SessionOptions.portal`), o que leva o protocolo a 37. Precedência: a opção do `run`, depois o projeto, o perfil e o global. O `run` sem terminal ignora o padrão e recusa o `--portal` explícito.

## Risks / Trade-offs

- [O sinal `Response` sai de uma conexão ponto a ponto, sem o nome de quem mandou] → Os clientes de linha de comando (`gdbus call`, o `xdg-open`) não esperam o sinal. Um teste com a GIO confere se o `Response` é aceito; se não for, o servidor põe no cabeçalho o nome único que devolveu como dono do `org.freedesktop.portal.Desktop`.
- [O agente abre links no navegador do usuário] → Só domínios permitidos pela aba, cada pedido no registro, e a decisão de abrir direto é do usuário. Uma página de phishing num domínio permitido continua possível.
- [O `flatpak-info` muda o comportamento de outros programas] → Ele fica só no `XDG_RUNTIME_DIR` do sandbox, e não na raiz. O efeito conhecido é o `xdg-open` usar o portal; o `xdg-open` de arquivo local passa a ser recusado (`OpenFile`).
- [Spam de notificações] → O limite por aba e os cortes de tamanho.
- [Um app espera um serviço que não existe no barramento (o keyring, por exemplo)] → Recebe erro, como hoje, quando não há D-Bus nenhum; o sandmux já entrega os segredos por outro caminho.
