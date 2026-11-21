## MODIFIED Requirements

### Requirement: Eventos do confinamento
O auxiliar MUST mandar a quem pediu o confinamento, pela mesma conexão, um evento a cada operação negada pelos programas, a cada arquivo criado pelos processos do cgroup e, quando o pedido lista montagens a registrar, a cada abertura nelas, com o caminho e se grava. O evento MUST ter o tipo, o gancho e o inode ou o nome, e MUST NOT ter o conteúdo de arquivo nenhum. No Docker privado, com a fila cheia, os eventos a mais MUST ser descartados e contados, sem bloquear os processos do cgroup.

#### Scenario: Leitura negada
- **WHEN** um contêiner do confinamento tenta ler um arquivo marcado como negado
- **THEN** o daemon que pediu o confinamento recebe um evento de bloqueio com o inode do arquivo

#### Scenario: Fila cheia
- **WHEN** um contêiner cria arquivos mais depressa do que o daemon lê os eventos
- **THEN** as criações seguem sem atraso, e o próximo evento entregue diz quantos foram descartados

#### Scenario: Abertura registrada
- **WHEN** o confinamento de uma aba lista a montagem da pasta de trabalho e o agente lê `src/main.rs`
- **THEN** o daemon recebe um evento de abertura com o caminho de `src/main.rs`, de leitura

## ADDED Requirements

### Requirement: Aberturas repetidas de uma aba
Uma abertura do mesmo arquivo (dispositivo, inode e geração), no mesmo modo, menos de 1 s depois da última que virou evento MUST NOT mandar evento. Renomear o arquivo ou criar um link para ele, pelos processos do cgroup, MUST fazer a abertura seguinte mandar evento. Uma abertura cujo evento não coube na fila MUST NOT contar como a última.

#### Scenario: Mesmo arquivo lido muitas vezes
- **WHEN** o agente lê `a.txt` mil vezes seguidas
- **THEN** o daemon recebe um evento de abertura de `a.txt`, de leitura, e a fila não enche

#### Scenario: Arquivo renomeado pela aba
- **WHEN** o agente lê `a.txt`, renomeia para `b.txt` e lê `b.txt` logo depois
- **THEN** o daemon recebe um evento de abertura de `b.txt`

### Requirement: Espera da aba com a fila cheia
Num confinamento com montagens a registrar (uma aba), o auxiliar MUST ler a fila do kernel numa thread que não espera o envio ao daemon. Quando a fila do kernel passa de 3/4, a fila do auxiliar passa do teto ou um evento é descartado, o auxiliar MUST congelar o cgroup da aba e MUST descongelar quando as duas esvaziarem. Cada congelamento MUST ir para o registro do auxiliar, com a duração.

#### Scenario: Leitura em massa de arquivos diferentes
- **WHEN** o agente lê trinta mil arquivos diferentes da pasta de trabalho, mais depressa do que o daemon trata os eventos
- **THEN** a aba fica parada por momentos, e o daemon recebe um evento de abertura de cada arquivo, sem descarte

### Requirement: Descongelamento só com a conexão aberta
O auxiliar MUST descongelar a aba que congelou pela fila só com a conexão do confinamento aberta e sem a saída limpa começada. Depois do fim da conexão, ou com a saída limpa começada, o auxiliar MUST NOT descongelar, porque as regras da aba saem com a conexão.

#### Scenario: Conexão termina com a aba congelada
- **WHEN** a conexão do daemon com o auxiliar termina enquanto a aba está congelada pela fila
- **THEN** a aba continua congelada, e só o daemon a descongela, depois de refazer o confinamento
