## MODIFIED Requirements

### Requirement: Eventos do confinamento
O auxiliar MUST mandar a quem pediu o confinamento, pela mesma conexão, um evento a cada operação negada pelos programas, a cada arquivo criado pelos processos do cgroup e, quando o pedido lista montagens a registrar, a cada abertura nelas, com o caminho e se grava. O evento MUST ter o tipo, o gancho e o inode ou o nome, e MUST NOT ter o conteúdo de arquivo nenhum. Com a fila cheia, os eventos a mais MUST ser descartados e contados, sem bloquear os processos do cgroup.

#### Scenario: Leitura negada
- **WHEN** um contêiner do confinamento tenta ler um arquivo marcado como negado
- **THEN** o daemon que pediu o confinamento recebe um evento de bloqueio com o inode do arquivo

#### Scenario: Fila cheia
- **WHEN** um contêiner cria arquivos mais depressa do que o daemon lê os eventos
- **THEN** as criações seguem sem atraso, e o próximo evento entregue diz quantos foram descartados

#### Scenario: Abertura registrada
- **WHEN** o confinamento de uma aba lista a montagem da pasta de trabalho e o agente lê `src/main.rs`
- **THEN** o daemon recebe um evento de abertura com o caminho de `src/main.rs`, de leitura
