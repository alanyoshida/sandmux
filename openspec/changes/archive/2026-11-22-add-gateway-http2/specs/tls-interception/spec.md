## MODIFIED Requirements

### Requirement: Terminação e reabertura do TLS
Numa conexão decifrada, o gateway MUST terminar o TLS do sandbox com um certificado do domínio do SNI assinado pela CA do lançamento e abrir TLS até o destino real, verificando o certificado dele pelas CAs do sistema. O protocolo segue o requisito "Protocolo das conexões decifradas". Um certificado inválido do destino MUST bloquear a conexão, com o motivo no registro.

#### Scenario: Destino com certificado inválido
- **WHEN** um domínio da lista de decifração apresenta um certificado que as CAs do sistema não validam
- **THEN** a conexão do sandbox é fechada, e o registro tem o bloqueio com o motivo

## ADDED Requirements

### Requirement: Protocolo das conexões decifradas
O gateway MUST oferecer ao destino os protocolos que o ClientHello do sandbox oferece (`h2` e `http/1.1`), e MUST oferecer `h2` ao sandbox só quando o destino o negociou. Um cliente que oferece só `http/1.1`, ou nenhum protocolo, MUST falar HTTP/1.1 nas duas pontas, e um destino sem `h2` MUST receber HTTP/1.1.

#### Scenario: Cliente e destino com HTTP/2
- **WHEN** o agente roda `curl --http2 https://<domínio decifrado>/`, e o destino fala h2
- **THEN** a conexão é HTTP/2 nas duas pontas, e o `curl` mostra `HTTP/2 200`

#### Scenario: Cliente só com HTTP/1.1
- **WHEN** um cliente de WebSocket oferece só `http/1.1` a um destino que também fala h2
- **THEN** a conexão é HTTP/1.1 nas duas pontas, e o upgrade funciona

### Requirement: Pedidos de uma conexão HTTP/2
Numa conexão HTTP/2 decifrada, cada pedido MUST passar pela mesma troca de falsos, pelas mesmas respostas locais do gateway e pela mesma reassinatura de um pedido HTTP/1.1, e MUST gerar a mesma linha `https` no registro. Os pedidos de uma mesma conexão MUST andar em paralelo: um pedido longo não espera o anterior.

#### Scenario: Chave trocada num pedido HTTP/2
- **WHEN** o agente manda, por HTTP/2, um pedido com o falso de uma chave do cofre a um domínio dela
- **THEN** o destino recebe o real, e o registro tem uma linha `https` com o nome do segredo

### Requirement: gRPC decifrado
Numa conexão HTTP/2 decifrada, o gateway MUST passar os corpos em fluxo nos dois sentidos, sem esperar o fim de um para mandar o outro, e MUST passar os trailers da resposta (`grpc-status`, `grpc-message`) sem mudar.

#### Scenario: Biblioteca gRPC com uma identidade de GCP
- **WHEN** uma aba com uma identidade de GCP usa uma biblioteca gRPC do Google (Pub/Sub) em `*.googleapis.com`
- **THEN** o pedido chega com o token real, e a biblioteca recebe a resposta e o `grpc-status`

### Requirement: Desligar o HTTP/2 do gateway
A chave `http2` da configuração (global, perfil ou projeto, valendo a mais específica) MUST valer `true` por padrão. Com `http2 = false`, o gateway MUST oferecer só `http/1.1` nas duas pontas das conexões decifradas seguintes.

#### Scenario: HTTP/2 desligado no projeto
- **WHEN** o projeto tem `http2 = false`, e o agente roda `curl --http2` num domínio decifrado
- **THEN** a conexão é HTTP/1.1 nas duas pontas
