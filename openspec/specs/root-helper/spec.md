# root-helper Specification

## Purpose
O auxiliar root (`sandmux-helper`): o serviço do sistema que prende ao cgroup do Docker privado de uma pasta os programas BPF LSM do confinamento, a pedido do usuário dono do cgroup, só para restringir, e só enquanto a conexão do pedido estiver aberta.

## Requirements
### Requirement: Pedidos ao auxiliar root
O `sandmux-helper` MUST rodar como serviço do sistema, com root, num socket Unix local que qualquer usuário alcança. Ele MUST aceitar só seis pedidos: o estado (versão e BPF LSM ativo), o confinamento e a observação de um cgroup v2 de quem pede, a marcação e a desmarcação de arquivos, e o escopo de um user namespace de quem pede, os cinco últimos com descritores abertos por quem pede. Outro pedido, ou um cgroup ou namespace de outro usuário, MUST ser recusado com erro, sem efeito.

#### Scenario: cgroup de outro usuário
- **WHEN** um usuário comum pede o confinamento passando o diretório de um cgroup que pertence ao root
- **THEN** o auxiliar recusa o pedido, e nenhum programa é preso àquele cgroup

#### Scenario: Pedido desconhecido
- **WHEN** um cliente manda ao socket uma linha que não é um pedido conhecido
- **THEN** o auxiliar responde com erro e continua atendendo os outros clientes

#### Scenario: Desmarcar
- **WHEN** um confinamento tem `dados.csv` marcado como negado e quem o pediu manda o `unmark` de `dados.csv`
- **THEN** os processos daquele cgroup voltam a abrir `dados.csv`, e o auxiliar solta o descritor do arquivo

### Requirement: Identidade de quem pede
O auxiliar MUST identificar quem pede pelas credenciais do socket (`SO_PEERCRED`) de cada conexão, e MUST NOT usar um uid, um nome ou um caminho que venha no pedido.

#### Scenario: Pedido com outro uid
- **WHEN** um usuário comum manda um pedido com um campo `uid` igual a 0
- **THEN** o auxiliar ignora o campo e decide pelo uid da conexão

### Requirement: Regras só restringem
Um confinamento ligado pelo auxiliar MUST apenas negar operações aos processos do cgroup pedido e dos cgroups abaixo dele. Ele MUST NOT conceder acesso, nem mudar o que acontece a processos de fora desse cgroup, inclusive para os arquivos marcados.

#### Scenario: Arquivo do sistema marcado
- **WHEN** um usuário marca `/etc/hostname` no confinamento do seu Docker
- **THEN** só os processos daquele Docker deixam de abrir o arquivo, e os outros processos da máquina o abrem como antes

### Requirement: Duração das regras
O confinamento e as marcas MUST durar enquanto a conexão que os criou estiver aberta, e MUST ser desfeitos quando ela fecha, inclusive quando o cliente morre. O auxiliar MUST manter os arquivos marcados referenciados enquanto a marca existir, para que ela não se perca.

#### Scenario: Daemon morre
- **WHEN** o daemon que ligou o confinamento recebe SIGKILL
- **THEN** o auxiliar desfaz o confinamento e solta os arquivos marcados

### Requirement: Limites por usuário
O auxiliar MUST limitar por usuário o número de confinamentos ativos (16) e o número de marcas de cada um (16384), e MUST recusar o pedido que passa do limite, sem afetar os confinamentos que já existem.

#### Scenario: Confinamentos demais
- **WHEN** um usuário com 16 confinamentos ativos pede mais um
- **THEN** o auxiliar recusa o pedido com um erro que diz o limite

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

### Requirement: Observação das execuções
O pedido `observe` MUST carregar só a observação das execuções e das conexões TCP no cgroup pedido e nos de baixo, sem regra de arquivos nem marca, e o `confine` MUST incluir a das execuções. Cada execução MUST virar um evento com o caminho executado, o pid e os argumentos; cada conexão TCP, um evento com o pid, a porta de origem e o destino, pela conexão que fez o pedido. A observação MUST NOT negar nem atrasar nada, e MUST terminar quando a conexão com o auxiliar fecha.

#### Scenario: Aba observada
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente roda `ls -la`
- **THEN** o daemon recebe um evento com o caminho do `ls` e os argumentos `ls` e `-la`

#### Scenario: Conexão da aba
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente abre uma conexão TCP para `1.1.1.1:443`
- **THEN** o daemon recebe um evento com o pid do processo, a porta de origem e o destino `1.1.1.1:443`

#### Scenario: Conexão fechada
- **WHEN** o daemon que pediu o `observe` termina
- **THEN** o auxiliar solta os programas, e as execuções e as conexões daquele cgroup deixam de gerar eventos

### Requirement: Escopo de um sandbox
O pedido `scope` MUST receber o user namespace de um sandbox, criado por quem pede, e MUST negar aos processos desse namespace e dos namespaces abaixo dele: a conexão a um socket abstrato criado fora do sandbox, um sinal a um processo de fora do sandbox e um `ioctl` num dispositivo fora da lista do pedido, com as mesmas exceções do Landlock. O escopo MUST valer até a conexão do pedido fechar. São até 256 escopos ativos por usuário.

#### Scenario: Socket abstrato do host
- **WHEN** um sandbox registrado tenta se conectar a um socket abstrato criado por um processo do host
- **THEN** a conexão é negada

#### Scenario: Socket abstrato do próprio sandbox
- **WHEN** dois processos de um sandbox registrado se comunicam por um socket abstrato criado lá dentro
- **THEN** a comunicação funciona

#### Scenario: Sinal para fora
- **WHEN** um processo de um sandbox registrado manda um sinal ao supervisor do sandbox, que está fora dele
- **THEN** o sinal é negado

#### Scenario: Namespace de outro usuário
- **WHEN** um usuário comum pede o `scope` passando o user namespace de outro usuário
- **THEN** o auxiliar recusa o pedido, e nada muda para os processos daquele namespace

#### Scenario: Fim do pedido
- **WHEN** a conexão do pedido `scope` fecha
- **THEN** o namespace sai do registro, e os programas são soltos quando não resta nenhum escopo
