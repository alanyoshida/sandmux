## MODIFIED Requirements

### Requirement: Pedidos ao auxiliar root
O `sandmux-helper` MUST rodar como serviço do sistema, com root, num socket Unix local que qualquer usuário alcança. Ele MUST aceitar só cinco pedidos: a consulta de estado (versão e BPF LSM ativo), o confinamento e a observação de um cgroup v2 que pertence a quem pede, e a marcação e a desmarcação de arquivos de um confinamento, os quatro últimos com descritores abertos por quem pede. Qualquer outro pedido, ou um cgroup de outro usuário, MUST ser recusado com erro, sem efeito.

#### Scenario: cgroup de outro usuário
- **WHEN** um usuário comum pede o confinamento passando o diretório de um cgroup que pertence ao root
- **THEN** o auxiliar recusa o pedido, e nenhum programa é preso àquele cgroup

#### Scenario: Pedido desconhecido
- **WHEN** um cliente manda ao socket uma linha que não é um pedido conhecido
- **THEN** o auxiliar responde com erro e continua atendendo os outros clientes

#### Scenario: Desmarcar
- **WHEN** um confinamento tem `dados.csv` marcado como negado e quem o pediu manda o `unmark` de `dados.csv`
- **THEN** os processos daquele cgroup voltam a abrir `dados.csv`, e o auxiliar solta o descritor do arquivo

## ADDED Requirements

### Requirement: Observação das execuções
O pedido `observe` MUST carregar só a observação das execuções no cgroup pedido e nos de baixo, sem regra de arquivos nem marca. O confinamento (`confine`) MUST incluir a mesma observação. Cada execução MUST virar um evento com o caminho que o kernel executou, o pid e os argumentos, mandado pela conexão que fez o pedido. A observação MUST NOT negar nem atrasar uma execução, e MUST terminar quando a conexão fecha.

#### Scenario: Aba observada
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente roda `ls -la`
- **THEN** o daemon recebe um evento com o caminho do `ls` e os argumentos `ls` e `-la`

#### Scenario: Conexão fechada
- **WHEN** o daemon que pediu o `observe` termina
- **THEN** o auxiliar solta o programa, e as execuções daquele cgroup deixam de gerar eventos
