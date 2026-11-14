## ADDED Requirements

### Requirement: Sessão do gateway do hermes
`sandmux run hermes --gateway` MUST criar no daemon uma sessão que roda `hermes gateway run` na pasta de trabalho, confinada como uma sessão do hermes, com os canais de mensagem e as tarefas agendadas do hermes. A sessão MUST aparecer em `sandmux ls` e na TUI como as outras, marcada como gateway. `--gateway` com outro harness MUST ser recusado.

#### Scenario: Gateway sobe
- **WHEN** o usuário roda `sandmux run hermes --gateway` em `~/bots`
- **THEN** o daemon cria uma sessão que roda `hermes gateway run` em `~/bots`, e `sandmux ls` a mostra como gateway

#### Scenario: Outro harness
- **WHEN** o usuário roda `sandmux run opencode --gateway`
- **THEN** o comando falha dizendo que só o hermes tem gateway

### Requirement: Reinício do gateway
Quando o gateway termina sem que o usuário tenha encerrado a sessão, o daemon MUST relançá-lo, esperando 5 segundos na primeira vez e dobrando a espera a cada saída seguida, até 5 minutos. Um gateway que fica vivo por 10 minutos MUST voltar à espera inicial. `sandmux kill` MUST encerrar a sessão sem relançar.

#### Scenario: Gateway cai
- **WHEN** o processo do gateway termina com erro
- **THEN** a sessão é relançada depois de 5 segundos, e uma nova saída logo em seguida espera 10 segundos

#### Scenario: Encerrado pelo usuário
- **WHEN** o usuário roda `sandmux kill <id>` na sessão do gateway
- **THEN** a sessão fica encerrada e não volta

### Requirement: Um gateway por vez
O daemon MUST recusar uma segunda sessão de gateway enquanto outra estiver viva ou aguardando o cofre, dizendo qual é a existente.

#### Scenario: Segundo gateway
- **WHEN** já existe a sessão 3 de gateway viva e o usuário roda `sandmux run hermes --gateway` noutra pasta
- **THEN** o comando falha citando a sessão 3

### Requirement: Gateway do host
Ao criar a sessão do gateway, e no `sandmux doctor`, o sandmux MUST avisar quando um gateway do hermes do host está rodando (o processo do `~/.hermes/gateway.pid` real está vivo). Dois gateways no mesmo bot disputam as mensagens.

#### Scenario: Gateway do host vivo
- **WHEN** o gateway do hermes do host está rodando e o usuário cria a sessão do gateway
- **THEN** o comando avisa que o gateway do host está rodando e que os dois disputam os mesmos bots
