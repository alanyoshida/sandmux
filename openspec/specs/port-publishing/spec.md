# port-publishing Specification

## Purpose
Define como as portas que o agente abre dentro do sandbox chegam ao host: a detecção das portas em escuta, a publicação no `127.0.0.1` do host, o encaminhamento das conexões, o fechamento e a lista das portas publicadas.

## Requirements

### Requirement: Detecção das portas em escuta
O supervisor de cada sandbox com rede MUST detectar, em até 2 segundos, cada porta TCP que um processo do sandbox passa a escutar em `127.0.0.1`, em `0.0.0.0` ou em `::`. Portas escutadas só em outros endereços MUST NOT ser publicadas.

#### Scenario: Servidor de desenvolvimento
- **WHEN** o agente sobe um servidor HTTP em `0.0.0.0:3000` dentro do sandbox
- **THEN** em até 2 segundos `http://127.0.0.1:3000` responde no host

### Requirement: Publicação só no localhost do host
Com a allowlist de portas vazia, cada porta detectada MUST ganhar uma porta no `127.0.0.1` do host, com o mesmo número quando ele está livre, ou o próximo número livre acima dele. Com a allowlist preenchida, só as portas dela MUST ser publicadas. Uma porta sem endereço na allowlist MUST NOT aceitar conexões por outro endereço da máquina.

#### Scenario: Porta ocupada no host
- **WHEN** a porta 3000 do host já está em uso e o agente escuta na 3000 do sandbox
- **THEN** o sandbox é publicado na próxima porta livre, e a lista mostra `3000 → 3001` (ou o número escolhido)

#### Scenario: Porta fora da allowlist
- **WHEN** a configuração tem `ports = ["8642"]` e o agente escuta nas portas 8642 e 3000
- **THEN** só a 8642 é publicada no host

### Requirement: Encaminhamento para dentro do sandbox
Cada conexão aceita na porta do host MUST ser levada para a porta correspondente do sandbox, no `127.0.0.1` do namespace de rede dele, copiando os dados nos dois sentidos.

#### Scenario: Dados nos dois sentidos
- **WHEN** um cliente no host manda um pedido para a porta publicada
- **THEN** o servidor do sandbox recebe o pedido e o cliente recebe a resposta

### Requirement: Fechamento
Quando o agente deixa de escutar numa porta, ou o sandbox termina, a porta do host MUST ser fechada em até 2 segundos.

#### Scenario: Servidor encerrado
- **WHEN** o servidor do sandbox termina
- **THEN** a porta do host deixa de aceitar conexões

### Requirement: Lista das portas
O supervisor MUST manter a lista das portas publicadas de um lançamento (porta do sandbox, porta do host e endereço) num arquivo que o daemon lê, e `sandmux stats <id>` MUST mostrar as portas da sessão e as publicadas pelos contêineres do Docker privado da pasta.

#### Scenario: Stats
- **WHEN** a sessão 1 escuta na porta 3000 e o usuário roda `sandmux stats 1`
- **THEN** a saída tem `3000 → 127.0.0.1:3000`

### Requirement: Endereço por porta
Uma porta da allowlist com endereço (`8443@0.0.0.0`) MUST ser publicada também nesse endereço do host, no mesmo número, além do `127.0.0.1`. Se o número estiver ocupado no endereço, a publicação nele MUST falhar com registro. Um endereço diferente de `127.0.0.1` MUST gerar um aviso no lançamento.

#### Scenario: Webhook pela rede local
- **WHEN** a configuração tem `ports = ["8443@0.0.0.0"]` e o hermes escuta na 8443
- **THEN** uma conexão ao IP da rede local do host na 8443 chega ao servidor do sandbox

### Requirement: Allowlist de portas ao vivo
Uma mudança na allowlist de portas, pela configuração ou pela aba, MUST valer em até 2 segundos, sem relançar o agente: portas que saíram fecham, e portas que entraram abrem.

#### Scenario: Liberar com o agente rodando
- **WHEN** o agente escuta na 3000, que não está na allowlist, e o usuário acrescenta `3000` em `ports`
- **THEN** a 3000 é publicada em até 2 segundos

### Requirement: Registro das conexões de entrada
Com a allowlist preenchida, cada conexão aceita numa porta publicada MUST virar uma linha `inbound` no registro de rede, com o endereço de quem conectou, a porta do sandbox, os bytes nos dois sentidos e a duração.

#### Scenario: Conexão registrada
- **WHEN** um cliente conecta na porta 8642 publicada e a conexão termina
- **THEN** o registro tem uma linha `inbound` com o IP do cliente e a porta 8642
