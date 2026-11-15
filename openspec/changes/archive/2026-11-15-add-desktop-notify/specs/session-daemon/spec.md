## ADDED Requirements

### Requirement: Notificações do desktop
Com `[global] notify` ligado, que é o padrão, o daemon MUST mandar uma notificação ao desktop, pelo serviço `org.freedesktop.Notifications` do barramento de sessão, nestes casos:

- um pedido de rede de uma aba é bloqueado;
- uma chave real do HOME sintético é movida ao cofre, ou passa a esperar o cofre ou um domínio;
- o harness de uma aba termina sem o usuário ter pedido;
- depois de a lista de sessões ser restaurada, há abas esperando o cofre e ele continua trancado.


#### Scenario: Bloqueio com a TUI fechada
- **WHEN** nenhuma TUI está aberta e o agente de uma aba tenta um domínio da denylist
- **THEN** o desktop mostra uma notificação com o nome da aba, o domínio e o motivo do bloqueio

#### Scenario: Aba encerrada pelo usuário
- **WHEN** o usuário encerra uma aba com `sandmux kill`
- **THEN** nenhuma notificação de aba encerrada é mandada

### Requirement: Texto e falhas das notificações
O texto de cada notificação MUST dizer a aba e o que aconteceu, sem valores de segredo. Sem o serviço de notificações, o daemon MUST seguir sem erro. Com `[global] notify = false`, o daemon MUST NOT mandar notificações.

#### Scenario: Desligadas
- **WHEN** `[global] notify = false` e um pedido é bloqueado
- **THEN** nenhuma notificação é mandada

### Requirement: Agrupamento das notificações
Eventos do mesmo tipo na mesma aba, a menos de 60 segundos um do outro, MUST atualizar a notificação anterior, com a contagem, em vez de abrir outra.

#### Scenario: Vários bloqueios seguidos
- **WHEN** o agente tenta três domínios bloqueados em poucos segundos
- **THEN** o desktop mostra uma notificação só, que termina dizendo três bloqueios

### Requirement: Supressão pelo foco
O daemon MUST NOT mandar notificação sobre uma aba que uma TUI aberta está mostrando, quando essa TUI informou que está com o foco do terminal. O foco MUST ser guardado por processo da TUI, e o de uma TUI que terminou MUST ser descartado.

#### Scenario: TUI em foco na aba
- **WHEN** a TUI está em foco mostrando a aba 1, e um pedido da aba 1 é bloqueado
- **THEN** nenhuma notificação é mandada, e o aviso fica na TUI

#### Scenario: TUI sem foco
- **WHEN** a TUI perdeu o foco do terminal, e um pedido da aba que ela mostra é bloqueado
- **THEN** a notificação é mandada
