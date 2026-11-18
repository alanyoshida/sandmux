## MODIFIED Requirements

### Requirement: Pedidos ao auxiliar root
O `sandmux-helper` MUST rodar como serviço do sistema, com root, num socket Unix local que qualquer usuário alcança. Ele MUST aceitar só quatro pedidos: a consulta de estado (versão e BPF LSM ativo), o confinamento de um cgroup v2 que pertence a quem pede, e a marcação e a desmarcação de arquivos desse confinamento, os três últimos com descritores abertos por quem pede. Qualquer outro pedido, ou um cgroup de outro usuário, MUST ser recusado com erro, sem efeito.

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

### Requirement: Eventos do confinamento
O auxiliar MUST mandar a quem pediu o confinamento, pela mesma conexão, um evento a cada operação negada pelos programas e a cada arquivo criado pelos processos do cgroup. O evento MUST ter o tipo, o gancho, o número do inode marcado ou do diretório e o nome do arquivo criado, e MUST NOT ter o conteúdo de arquivo nenhum. Com a fila cheia, os eventos a mais MUST ser descartados e contados, sem bloquear os processos do cgroup.

#### Scenario: Leitura negada
- **WHEN** um contêiner do confinamento tenta ler um arquivo marcado como negado
- **THEN** o daemon que pediu o confinamento recebe um evento de bloqueio com o inode do arquivo

#### Scenario: Fila cheia
- **WHEN** um contêiner cria arquivos mais depressa do que o daemon lê os eventos
- **THEN** as criações seguem sem atraso, e o próximo evento entregue diz quantos foram descartados
