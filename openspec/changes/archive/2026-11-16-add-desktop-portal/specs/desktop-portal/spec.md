## Purpose

O portal do desktop para programas confinados: um barramento D-Bus mínimo do sandmux, com a aparência, as notificações e o abrir link filtrado pela política de rede da aba, sem acesso ao D-Bus de sessão do usuário.

## ADDED Requirements

### Requirement: Opção do portal
O `sandmux run` MUST aceitar `--portal` e `--no-portal`, e a configuração MUST aceitar `portal = true|false` no global, nos perfis e nos projetos, com a precedência das outras chaves de liga e desliga. O padrão MUST ser ligado nas abas do daemon. Sem terminal, o `run` MUST seguir sem portal e recusar o `--portal` explícito. A página de configurações MUST ter o item na seção Sandbox.

#### Scenario: Padrão
- **WHEN** o usuário abre uma aba de shell sem a opção e sem a chave
- **THEN** o sandbox tem `/run/user/<uid>/bus` e `DBUS_SESSION_BUS_ADDRESS` apontando para ele

#### Scenario: Desligado
- **WHEN** a aba é lançada com `--no-portal`
- **THEN** o sandbox não tem `DBUS_SESSION_BUS_ADDRESS` nem o socket

### Requirement: Barramento mínimo do sandmux
Com o portal, o daemon MUST servir no socket da aba um barramento próprio, que responde ao `Hello` e às consultas de nome do `org.freedesktop.DBus` e só aos serviços `org.freedesktop.portal.Desktop` e `org.freedesktop.Notifications`. Qualquer outro objeto ou interface MUST receber erro, e nenhuma mensagem do sandbox MUST chegar ao D-Bus de sessão do usuário.

#### Scenario: Cliente comum
- **WHEN** um processo confinado chama `gdbus call --session` ao `org.freedesktop.portal.Desktop`
- **THEN** a chamada é respondida pelo sandmux

#### Scenario: Serviço que não existe
- **WHEN** um processo confinado chama um serviço do desktop fora da lista, como o `org.freedesktop.secrets`
- **THEN** a chamada recebe erro, e nada chega ao D-Bus do usuário

### Requirement: Aparência pelo portal
O `org.freedesktop.portal.Settings` MUST responder `Read`, `ReadAll` e `ReadOne` só do namespace `org.freedesktop.appearance` (`color-scheme`, `accent-color` e `contrast`), com os valores do portal do host, ou "sem preferência" quando o host não tem portal.

#### Scenario: Tema escuro
- **WHEN** o portal do host diz `color-scheme = 1` e um programa confinado lê a chave
- **THEN** ele recebe 1

### Requirement: Notificações do sandbox
O `Notify` do `org.freedesktop.Notifications` e o `AddNotification` do portal MUST chegar ao desktop pelo daemon, com o nome da aba no título, o título e o corpo cortados e sem marcação, e sem ícone, ações, sons nem imagens do sandbox. O daemon MUST limitar a 5 notificações a cada 10 segundos por aba, descartando o excesso sem erro.

#### Scenario: notify-send
- **WHEN** um processo da aba `shell:app` roda `notify-send "Pronto" "build ok"`
- **THEN** o desktop recebe uma notificação com o título `shell:app: Pronto` e o corpo `build ok`

#### Scenario: Excesso
- **WHEN** um processo manda 20 notificações num segundo
- **THEN** só as 5 primeiras chegam ao desktop

### Requirement: Abrir link pela política de rede
O `OpenURI` do portal MUST aceitar só URLs `http` e `https` cujo host a política de rede da aba permite, e abri-las direto no navegador padrão do host. Um host bloqueado MUST ser recusado. Cada pedido, aberto ou recusado, MUST entrar no registro de rede da aba. `OpenFile` e `OpenDirectory` MUST ser recusados.

#### Scenario: Domínio permitido
- **WHEN** a aba está no modo só inferência e o programa pede para abrir `https://openrouter.ai/keys`
- **THEN** o navegador do host abre a página, e o registro de rede tem o pedido

#### Scenario: Domínio fora da política
- **WHEN** a aba está no modo só inferência e o programa pede para abrir `https://exemplo.com`
- **THEN** nada abre, e o registro tem o pedido recusado com o motivo

#### Scenario: xdg-open
- **WHEN** um processo confinado roda `xdg-open https://openrouter.ai`
- **THEN** o pedido chega ao `OpenURI` do sandmux
