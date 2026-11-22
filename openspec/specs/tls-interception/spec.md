# tls-interception Specification

## Purpose
Define a decifração do TLS no gateway: a CA de cada lançamento, a confiança injetada no sandbox, a lista de domínios decifrados, a terminação e a reabertura do TLS com a verificação do destino, e o registro de método e caminho do tráfego decifrado.

## Requirements

### Requirement: CA por lançamento
Cada lançamento com rede MUST gerar uma CA própria, cuja chave MUST ficar só na memória do supervisor e da especificação (num memfd), sem tocar o disco. Um sandbox MUST NOT confiar na CA de outro lançamento.

#### Scenario: Lançamentos diferentes
- **WHEN** dois sandboxes são lançados
- **THEN** cada um confia numa CA diferente, e nenhum arquivo em disco tem a chave de uma delas

### Requirement: Confiança no sandbox
O sandbox MUST confiar na CA do lançamento além das CAs do sistema: o pacote de CAs do sistema MUST ser coberto por um arquivo com as duas, e as variáveis `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`, `GIT_SSL_CAINFO`, `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` e `AWS_CA_BUNDLE` MUST apontar para ele.

#### Scenario: curl confia no gateway
- **WHEN** um domínio está na lista de decifração e o agente roda `curl https://<domínio>/`
- **THEN** o `curl` aceita o certificado apresentado pelo gateway, sem opção extra

#### Scenario: gcloud confia no gateway
- **WHEN** `*.googleapis.com` está na lista de decifração e o agente roda um comando do `gcloud`
- **THEN** o `gcloud` aceita o certificado apresentado pelo gateway, sem opção extra

#### Scenario: aws confia no gateway
- **WHEN** `*.amazonaws.com` está na lista de decifração e o agente roda um comando do `aws`
- **THEN** o `aws` aceita o certificado apresentado pelo gateway, sem opção extra

### Requirement: Lista de decifração
O gateway MUST decifrar uma conexão TLS só quando o SNI casa com a lista de decifração: os domínios donos dos segredos do cofre desbloqueado e os padrões da chave `decrypt` da configuração e da opção `--decrypt` do `run`, somados entre as camadas. As demais conexões MUST passar cifradas de ponta a ponta. Uma mudança na lista MUST valer na conexão seguinte de uma sessão viva.

#### Scenario: Fora da lista
- **WHEN** o agente acessa por HTTPS um domínio que não está na lista
- **THEN** o certificado que ele recebe é o do servidor real

#### Scenario: Perfil amplia
- **WHEN** o perfil do projeto tem `decrypt = ["host.sandmux.internal"]`
- **THEN** as conexões TLS para `host.sandmux.internal` são decifradas, e cada pedido aparece no registro com o método e o caminho

### Requirement: Terminação e reabertura do TLS
Numa conexão decifrada, o gateway MUST terminar o TLS do sandbox com um certificado do domínio do SNI assinado pela CA do lançamento e abrir TLS até o destino real, verificando o certificado dele pelas CAs do sistema. O protocolo segue o requisito "Protocolo das conexões decifradas". Um certificado inválido do destino MUST bloquear a conexão, com o motivo no registro.

#### Scenario: Destino com certificado inválido
- **WHEN** um domínio da lista de decifração apresenta um certificado que as CAs do sistema não validam
- **THEN** a conexão do sandbox é fechada, e o registro tem o bloqueio com o motivo

### Requirement: Registro do tráfego decifrado
Cada pedido HTTP numa conexão decifrada MUST gerar uma linha no registro de rede com `proto` `https`, o domínio, o método, o caminho sem a query, o status da resposta e os bytes de cada lado. Os cabeçalhos e o corpo MUST NOT ser gravados.

#### Scenario: Pedido com query
- **WHEN** o agente faz `GET /v1/models?key=abc` num domínio decifrado
- **THEN** o registro tem uma linha com `GET`, o caminho `/v1/models` e o status, e não tem `key=abc`

### Requirement: WebSocket decifrado
Numa conexão decifrada, um pedido de upgrade para WebSocket MUST ser repassado ao destino sem o cabeçalho `Sec-WebSocket-Extensions`, para que as mensagens não sejam comprimidas. Com a resposta `101`, o gateway MUST passar as mensagens nos dois sentidos até uma das pontas fechar. O upgrade MUST gerar uma linha `https` no registro com o método, o caminho, o status `101` e os bytes dos dois sentidos ao fim da conexão.

#### Scenario: WebSocket num domínio decifrado
- **WHEN** o agente abre um WebSocket com um domínio da lista de decifração
- **THEN** as mensagens passam nos dois sentidos, e ao fim o registro tem uma linha `https` com o status `101`

### Requirement: Outros upgrades decifrados
Numa conexão decifrada, um pedido de upgrade que não é WebSocket (o SPDY do `kubectl exec` e do `port-forward`, por exemplo) MUST ser repassado ao destino com os cabeçalhos dele. Com a resposta `101`, o gateway MUST passar os bytes nos dois sentidos, como um túnel, até uma das pontas fechar, e MUST gravar uma linha `https` com o método, o caminho, o status `101` e os bytes ao fim.

#### Scenario: kubectl exec por SPDY
- **WHEN** o agente roda `kubectl exec` e o `kubectl` pede o upgrade `SPDY/3.1`
- **THEN** o terminal do contêiner funciona pela conexão, e o registro tem uma linha `https` com o status `101`

### Requirement: Destino de uma identidade de Kubernetes
O destino de uma identidade de Kubernetes MUST ser verificado pela CA do cluster guardada com ela (ou pelas CAs do sistema, quando ela não tem uma), pelo nome do servidor dela, e MUST receber o certificado de cliente dela, quando há. A verificação só MUST ser pulada quando o kubeconfig do host pede `insecure-skip-tls-verify`.

#### Scenario: API server com a CA do cluster
- **WHEN** o API server de `kube:kind-dev` apresenta um certificado assinado pela CA do cluster, que as CAs do sistema não conhecem
- **THEN** o gateway aceita o certificado, e o pedido segue

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
