## ADDED Requirements

### Requirement: WebSocket decifrado
Numa conexão decifrada, um pedido de upgrade para WebSocket MUST ser repassado ao destino sem o cabeçalho `Sec-WebSocket-Extensions`, para que as mensagens não sejam comprimidas. Com a resposta `101`, o gateway MUST passar as mensagens nos dois sentidos até uma das pontas fechar. O upgrade MUST gerar uma linha `https` no registro com o método, o caminho, o status `101` e os bytes dos dois sentidos ao fim da conexão.

#### Scenario: WebSocket num domínio decifrado
- **WHEN** o agente abre um WebSocket com um domínio da lista de decifração
- **THEN** as mensagens passam nos dois sentidos, e ao fim o registro tem uma linha `https` com o status `101`
