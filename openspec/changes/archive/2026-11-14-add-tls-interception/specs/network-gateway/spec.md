# Spec Delta

## MODIFIED Requirements

### Requirement: Registro de rede
O gateway MUST registrar cada conexão TCP, cada fluxo UDP, cada consulta de DNS e cada pedido HTTP decifrado em `$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl`, com a data e a hora, o identificador do sandbox, a pasta de trabalho, o protocolo, o domínio, o IP e a porta de destino, os bytes de cada lado, a duração e o bloqueio; nos pedidos decifrados, também o método, o caminho sem a query e o status. O conteúdo das conexões MUST NOT ser gravado.

#### Scenario: Linha de uma conexão
- **WHEN** o agente abre uma conexão TCP, manda 10 bytes e fecha
- **THEN** o registro tem uma linha com o destino, a porta e 10 bytes enviados, e nenhum byte do conteúdo
