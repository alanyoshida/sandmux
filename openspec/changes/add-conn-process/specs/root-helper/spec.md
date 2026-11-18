## MODIFIED Requirements

### Requirement: Observação das execuções
O pedido `observe` MUST carregar só a observação das execuções e das conexões TCP no cgroup pedido e nos de baixo, sem regra de arquivos nem marca. O confinamento (`confine`) MUST incluir a observação das execuções. Cada execução MUST virar um evento com o caminho que o kernel executou, o pid e os argumentos. Cada conexão TCP que um processo do cgroup abre MUST virar um evento com o pid, a porta de origem e o destino. Os eventos vão pela conexão que fez o pedido. A observação MUST NOT negar nem atrasar uma execução ou uma conexão, e MUST terminar quando a conexão com o auxiliar fecha.

#### Scenario: Aba observada
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente roda `ls -la`
- **THEN** o daemon recebe um evento com o caminho do `ls` e os argumentos `ls` e `-la`

#### Scenario: Conexão da aba
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente abre uma conexão TCP para `1.1.1.1:443`
- **THEN** o daemon recebe um evento com o pid do processo, a porta de origem e o destino `1.1.1.1:443`

#### Scenario: Conexão fechada
- **WHEN** o daemon que pediu o `observe` termina
- **THEN** o auxiliar solta os programas, e as execuções e as conexões daquele cgroup deixam de gerar eventos
