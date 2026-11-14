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
O sandbox MUST confiar na CA do lançamento além das CAs do sistema: o pacote de CAs do sistema MUST ser coberto por um arquivo com as duas, e as variáveis `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE` e `GIT_SSL_CAINFO` MUST apontar para ele.

#### Scenario: curl confia no gateway
- **WHEN** um domínio está na lista de decifração e o agente roda `curl https://<domínio>/`
- **THEN** o `curl` aceita o certificado apresentado pelo gateway, sem opção extra

### Requirement: Lista de decifração
O gateway MUST decifrar uma conexão TLS só quando o SNI casa com a lista de decifração: os domínios donos dos segredos do cofre desbloqueado e os padrões da chave `decrypt` da configuração e da opção `--decrypt` do `run`, somados entre as camadas. As demais conexões MUST passar cifradas de ponta a ponta. Uma mudança na lista MUST valer na conexão seguinte de uma sessão viva.

#### Scenario: Fora da lista
- **WHEN** o agente acessa por HTTPS um domínio que não está na lista
- **THEN** o certificado que ele recebe é o do servidor real

#### Scenario: Perfil amplia
- **WHEN** o perfil do projeto tem `decrypt = ["host.sandmux.internal"]`
- **THEN** as conexões TLS para `host.sandmux.internal` são decifradas, e cada pedido aparece no registro com o método e o caminho

### Requirement: Terminação e reabertura do TLS
Numa conexão decifrada, o gateway MUST terminar o TLS do sandbox com um certificado do domínio do SNI assinado pela CA do lançamento, oferecendo só HTTP/1.1, e abrir TLS até o destino real, verificando o certificado dele pelas CAs do sistema. Um certificado inválido do destino MUST bloquear a conexão, com o motivo no registro.

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
