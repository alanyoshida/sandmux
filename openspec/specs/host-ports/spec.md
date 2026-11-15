# host-ports Specification

## Purpose
As portas da máquina do usuário levadas ao 127.0.0.1 do sandbox (host-ports), para serviços locais como servidores de modelo e MCPs, com a política de rede e o registro.

## Requirements

### Requirement: Portas do host no sandbox
Para cada porta de `host-ports`, o supervisor de um sandbox com rede MUST escutar `127.0.0.1:porta` dentro do namespace de rede do sandbox e repassar cada conexão ao `127.0.0.1:porta` do host. Sem `host-ports`, o `127.0.0.1` do sandbox MUST continuar sendo só dele.

#### Scenario: LM Studio no host
- **WHEN** o LM Studio escuta em `127.0.0.1:8888` no host, a configuração tem `host-ports = [8888]`, e o opencode no sandbox chama `http://127.0.0.1:8888/v1`
- **THEN** o pedido chega ao LM Studio do host e a resposta volta ao opencode

#### Scenario: Porta não configurada
- **WHEN** a configuração não tem `host-ports` e o agente conecta em `127.0.0.1:8888` dentro do sandbox
- **THEN** a conexão é recusada, como antes

### Requirement: Registro e bloqueio das portas do host
Cada conexão por uma porta do host MUST gerar uma linha `tcp` no registro de rede, com o domínio `host.sandmux.internal`, o destino `127.0.0.1:porta` e os bytes. Uma regra de bloqueio que casa com `host.sandmux.internal` MUST recusar a conexão e registrar o bloqueio.

#### Scenario: Linha no registro
- **WHEN** o agente usa a porta 8888 do host
- **THEN** `sandmux net` mostra uma linha `tcp` para `host.sandmux.internal` em `127.0.0.1:8888`

### Requirement: Mudança ao vivo
Com a sessão viva, uma porta acrescentada a `host-ports` MUST passar a valer em até 2 segundos, e uma porta tirada MUST deixar de ser escutada dentro do sandbox. Uma porta do host MUST NOT ser publicada de volta como porta de entrada do sandbox.

#### Scenario: Porta acrescentada
- **WHEN** o usuário roda `sandmux config add host-ports 9000 --global` com a sessão viva
- **THEN** em até 2 segundos o agente alcança o `127.0.0.1:9000` do host

### Requirement: Aviso da importação
Quando a importação de um harness copia uma configuração que cita `127.0.0.1`, `localhost` ou `[::1]` com uma porta, ela MUST avisar, listar as portas e mostrar o comando `sandmux config add host-ports <porta> --global` de cada uma.

#### Scenario: opencode com LM Studio
- **WHEN** o `opencode.json` importado tem `"baseURL": "http://127.0.0.1:8888/v1"`
- **THEN** a importação avisa que a porta 8888 do host não é vista no sandbox e mostra `sandmux config add host-ports 8888 --global`
