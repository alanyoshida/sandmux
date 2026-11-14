# Spec Delta

## Purpose
Define a rede do sandbox: o namespace de rede próprio, a interface criada sem privilégio, o gateway que termina e refaz cada conexão, o DNS, os modos aberta e sem rede, os bloqueios fixos, o acesso à rede local e aos serviços da máquina, e o registro de cada conexão com a retenção.

## ADDED Requirements

### Requirement: Namespace de rede por sandbox
Todo sandbox MUST ter um namespace de rede próprio, criado sem privilégio, com o `lo` e, no modo aberta, uma interface `sandmux0` com o endereço `10.0.2.100/24` e a rota padrão por `10.0.2.2`. O processo confinado MUST NOT ver as interfaces do host.

#### Scenario: Interfaces
- **WHEN** um processo confinado lista as interfaces
- **THEN** aparecem só `lo` e `sandmux0`

### Requirement: Todo tráfego pelo gateway
No modo aberta, toda conexão TCP e todo datagrama UDP do sandbox para fora MUST passar pelo gateway do sandbox, que roda fora do sandbox e refaz a conexão a partir do host. O gateway MUST terminar junto com o sandbox.

#### Scenario: Conexão a um serviço
- **WHEN** um processo confinado abre uma conexão TCP para um servidor alcançável pelo host
- **THEN** a conexão funciona, e o gateway registra o destino

### Requirement: DNS pelo gateway
O sandbox MUST usar como servidor de nomes o endereço `10.0.2.3`, respondido pelo gateway com o resolvedor do host. Cada consulta MUST ser registrada com o nome consultado, e as respostas MUST ligar o IP ao domínio no registro das conexões seguintes.

#### Scenario: Resolução
- **WHEN** um processo confinado resolve um nome que o host resolve
- **THEN** o processo recebe o mesmo endereço, e o registro tem a consulta com o nome

### Requirement: Modo sem rede
Com o modo `sem-rede` (opção `--network sem-rede` ou chave `network = "sem-rede"`), o namespace do sandbox MUST ter só o `lo`, e toda tentativa de conexão para fora MUST falhar na hora.

#### Scenario: Sem rede
- **WHEN** o sandbox foi lançado com `--network sem-rede` e um processo tenta abrir uma conexão TCP para fora
- **THEN** a conexão falha com rede inalcançável

### Requirement: Modo de rede nas camadas
A chave `network` e a opção `--network` MUST aceitar `aberta` e `sem-rede`, e seguir a precedência das camadas da configuração (vence a camada mais específica). O padrão MUST ser `aberta`.

#### Scenario: Projeto sem rede
- **WHEN** o perfil do projeto tem `network = "sem-rede"` e o usuário roda `sandmux run shell` na pasta do projeto
- **THEN** o sandbox não tem rede para fora

### Requirement: Bloqueios fixos
Em todo modo, o gateway MUST bloquear os metadados de cloud (`169.254.169.254` e `fd00:ec2::254`), o QUIC (UDP para a porta 443) e o ICMP. Cada bloqueio MUST ser registrado.

#### Scenario: Metadados de cloud
- **WHEN** um processo confinado tenta conectar em `169.254.169.254:80`
- **THEN** a conexão é recusada, e o registro tem o bloqueio

#### Scenario: QUIC
- **WHEN** um processo confinado manda um datagrama UDP para a porta 443 de um servidor
- **THEN** o datagrama não sai, e o registro tem o bloqueio

### Requirement: Rede local e serviços da máquina
No modo aberta, o sandbox MUST alcançar a rede local. O nome `host.sandmux.internal`, respondido pelo DNS do gateway, MUST levar ao `127.0.0.1` do host, e a variável `SANDMUX_HOST` MUST ter esse nome.

#### Scenario: Serviço local
- **WHEN** um servidor escuta em `127.0.0.1:8080` no host e o agente acessa `http://host.sandmux.internal:8080`
- **THEN** a conexão chega ao servidor do host

### Requirement: Registro de rede
O gateway MUST registrar cada conexão TCP, cada fluxo UDP e cada consulta de DNS em `$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl`, com a data e a hora, o identificador do sandbox, a pasta de trabalho, o protocolo, o domínio, o IP e a porta de destino, os bytes de cada lado, a duração e o bloqueio. O conteúdo das conexões MUST NOT ser gravado.

#### Scenario: Linha de uma conexão
- **WHEN** o agente abre uma conexão TCP, manda 10 bytes e fecha
- **THEN** o registro tem uma linha com o destino, a porta e 10 bytes enviados, e nenhum byte do conteúdo

### Requirement: Domínio no registro
O domínio de uma conexão MUST vir da consulta de DNS do próprio sandbox, do SNI do TLS ou do cabeçalho `Host` do HTTP. Uma conexão por IP sem domínio conhecido MUST ser marcada como "sem domínio".

#### Scenario: Conexão com domínio
- **WHEN** o agente resolve `exemplo.test` e conecta no IP devolvido
- **THEN** a linha da conexão tem o domínio `exemplo.test`

#### Scenario: Conexão direta por IP
- **WHEN** o agente conecta num IP sem ter consultado nome nenhum
- **THEN** a linha da conexão tem o domínio "sem domínio"

### Requirement: Retenção do registro
Arquivos do registro com mais de 30 dias, ou que passam do teto de 500 MB somados, MUST ser apagados, os mais velhos primeiro, quando o daemon sobe e uma vez por hora. Os dois limites MUST ser ajustáveis por `net-log-days` e `net-log-max` em `[global]`.

#### Scenario: Arquivo velho
- **WHEN** o registro tem um arquivo de 40 dias atrás e o daemon sobe
- **THEN** o arquivo é apagado, e os de dias recentes ficam

### Requirement: Consultar o registro
`sandmux net [<id>]` MUST mostrar as últimas conexões registradas, de uma sessão ou de todas, com a hora, a sessão, o domínio, o destino, o volume e o bloqueio.

#### Scenario: Uma sessão
- **WHEN** a sessão 1 acessou `host.sandmux.internal:8080` e o usuário roda `sandmux net 1`
- **THEN** a saída tem uma linha com `host.sandmux.internal` e a porta 8080
