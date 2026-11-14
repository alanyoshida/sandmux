# Spec Delta

## Purpose
Define como as portas que o agente abre dentro do sandbox chegam ao host: a detecção das portas em escuta, a publicação no `127.0.0.1` do host, o encaminhamento das conexões, o fechamento e a lista das portas publicadas.

## ADDED Requirements

### Requirement: Detecção das portas em escuta
O supervisor de cada sandbox com rede MUST detectar, em até 2 segundos, cada porta TCP que um processo do sandbox passa a escutar em `127.0.0.1`, em `0.0.0.0` ou em `::`. Portas escutadas só em outros endereços MUST NOT ser publicadas.

#### Scenario: Servidor de desenvolvimento
- **WHEN** o agente sobe um servidor HTTP em `0.0.0.0:3000` dentro do sandbox
- **THEN** em até 2 segundos `http://127.0.0.1:3000` responde no host

### Requirement: Publicação só no localhost do host
Cada porta detectada MUST ganhar uma porta no `127.0.0.1` do host, com o mesmo número quando ele está livre, ou o próximo número livre acima dele. A porta do host MUST NOT aceitar conexões por outro endereço da máquina.

#### Scenario: Porta ocupada no host
- **WHEN** a porta 3000 do host já está em uso e o agente escuta na 3000 do sandbox
- **THEN** o sandbox é publicado na próxima porta livre, e a lista mostra `3000 → 3001` (ou o número escolhido)

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
O supervisor MUST manter a lista das portas publicadas de um lançamento (porta do sandbox e porta do host) num arquivo que o daemon lê, e `sandmux stats <id>` MUST mostrar as portas da sessão e as publicadas pelos contêineres do Docker privado da pasta.

#### Scenario: Stats
- **WHEN** a sessão 1 escuta na porta 3000 e o usuário roda `sandmux stats 1`
- **THEN** a saída tem `3000 → 127.0.0.1:3000`
