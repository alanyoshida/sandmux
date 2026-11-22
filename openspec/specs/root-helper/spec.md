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
O auxiliar MUST mandar a quem pediu o confinamento, pela conexão dele, um evento a cada operação negada pelos programas, a cada arquivo criado no cgroup e, quando o pedido lista montagens a registrar, a cada abertura nelas, com o caminho. Numa abertura, negada ou não, MUST dizer se lê e se grava. O evento MUST ter o tipo, o gancho e o inode ou o nome, e MUST NOT ter conteúdo de arquivo. No Docker privado, com a fila cheia, os eventos a mais MUST ser descartados e contados, sem bloquear o cgroup.

#### Scenario: Leitura negada
- **WHEN** um contêiner do confinamento tenta ler um arquivo marcado como negado
- **THEN** o daemon que pediu o confinamento recebe um evento de bloqueio com o inode do arquivo

#### Scenario: Fila cheia
- **WHEN** um contêiner cria arquivos mais depressa do que o daemon lê os eventos
- **THEN** as criações seguem sem atraso, e o próximo evento entregue diz quantos foram descartados

#### Scenario: Abertura registrada
- **WHEN** o confinamento de uma aba lista a montagem da pasta de trabalho e o agente lê `src/main.rs`
- **THEN** o daemon recebe um evento de abertura com o caminho de `src/main.rs`, de leitura

#### Scenario: Abertura de leitura e escrita
- **WHEN** o confinamento de uma aba lista a montagem da pasta de trabalho e o agente abre `notas.md` para leitura e escrita
- **THEN** o daemon recebe um evento de abertura com o caminho de `notas.md`, que lê e grava

### Requirement: Observação das execuções
O pedido `observe` MUST carregar só a observação das execuções e das conexões TCP no cgroup pedido e nos de baixo, sem regra de arquivos nem marca, e o `confine` MUST incluir a das execuções. Cada execução MUST virar um evento com o caminho executado, o pid, o ppid e os argumentos; cada conexão TCP, um evento com o pid, a porta de origem e o destino, pela conexão que fez o pedido. A observação MUST NOT negar nem atrasar nada, e MUST terminar quando a conexão com o auxiliar fecha.

#### Scenario: Aba observada
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente roda `ls -la`
- **THEN** o daemon recebe um evento com o caminho do `ls` e os argumentos `ls` e `-la`

#### Scenario: Conexão da aba
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente abre uma conexão TCP para `1.1.1.1:443`
- **THEN** o daemon recebe um evento com o pid do processo, a porta de origem e o destino `1.1.1.1:443`

#### Scenario: Conexão fechada
- **WHEN** o daemon que pediu o `observe` termina
- **THEN** o auxiliar solta os programas, e as execuções e as conexões daquele cgroup deixam de gerar eventos

#### Scenario: Evento de execução traz o ppid
- **WHEN** um processo dentro do cgroup observado executa um programa
- **THEN** o evento mandado ao daemon MUST incluir o ppid de quem o executou, além do caminho, do pid e dos argumentos

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

### Requirement: Escopo de volta depois de um reinício do auxiliar
Quando o auxiliar para com sandboxes registrados, o escopo de cada um MUST ser registrado de novo no auxiliar seguinte. Numa sessão do daemon, a aba MUST ficar congelada até o escopo voltar, e nada dela roda sem ele; no `run` sem o daemon, o lançador MUST registrar de novo assim que o auxiliar seguinte responder. Sem o escopo de volta em 30 s, o sandbox MUST terminar.

#### Scenario: Auxiliar reiniciado com uma aba rodando
- **WHEN** o auxiliar recebe `SIGTERM` com uma aba do daemon registrada, e o auxiliar seguinte sobe
- **THEN** a aba fica congelada até o escopo ser registrado no auxiliar seguinte, e depois segue com os sinais para fora e os sockets abstratos de fora ainda negados

#### Scenario: Auxiliar que não volta
- **WHEN** o auxiliar para e nenhum outro responde em 30 s
- **THEN** o sandbox termina, e nenhum processo dele roda sem o escopo depois desse prazo

### Requirement: Saída limpa do auxiliar
Numa saída pedida (`SIGTERM`), o auxiliar MUST avisar cada conexão antes de soltar as regras dela e MUST NOT atender conexões novas. Um lançamento que encontra o auxiliar trocando MUST tentar o registro do escopo de novo por até 30 s.

#### Scenario: Lançamento durante a troca do auxiliar
- **WHEN** uma aba é lançada enquanto o auxiliar sai e o seguinte sobe
- **THEN** o lançamento registra o escopo no auxiliar seguinte e sobe

#### Scenario: Aviso antes de soltar as regras
- **WHEN** o auxiliar recebe `SIGTERM` com um confinamento ligado
- **THEN** quem pediu vê a conexão terminar enquanto as regras ainda valem

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

### Requirement: Saída de processo observado
O pedido `observe` MUST mandar um evento com o pid quando um processo do cgroup observado termina, só pela saída do processo (não pela de cada thread dele), quando o kernel tem o tracepoint. Esse evento MUST seguir as regras da observação: não nega nem atrasa a saída, e termina quando a conexão com o auxiliar fecha.

#### Scenario: Processo observado termina
- **WHEN** um processo do cgroup observado termina
- **THEN** o auxiliar MUST mandar ao daemon um evento de saída com o pid dele, pela mesma conexão

#### Scenario: Thread termina
- **WHEN** uma thread que não é a principal de um processo observado termina, e o processo continua
- **THEN** o auxiliar MUST NOT mandar evento de saída

### Requirement: Aviso de auxiliar root antigo
Quando o auxiliar root em execução responde com uma versão anterior à do sandmux, o `sandmux doctor` MUST mostrar um aviso com as duas versões e a correção (`sandmux setup`). O aviso MUST NOT contar como falha nem mudar o código de saída. Sem auxiliar em execução, ou com o auxiliar na mesma versão ou numa mais nova, o `sandmux doctor` MUST NOT mostrar o aviso.

#### Scenario: Auxiliar de antes da atualização
- **WHEN** o sandmux instalado é o 0.1.32, o auxiliar em execução responde 0.1.30 e o usuário roda `sandmux doctor`
- **THEN** a saída tem um aviso com `0.1.30`, `0.1.32` e `sandmux setup`, e o código de saída é o mesmo de sem o aviso

#### Scenario: Auxiliar na mesma versão
- **WHEN** o auxiliar em execução responde a mesma versão do sandmux
- **THEN** o `sandmux doctor` não mostra o aviso

#### Scenario: Sem auxiliar
- **WHEN** nenhum auxiliar responde no socket
- **THEN** o `sandmux doctor` não mostra o aviso de versão

#### Scenario: Comparação por número
- **WHEN** o auxiliar responde 0.1.9 e o sandmux é o 0.1.10
- **THEN** o auxiliar conta como antigo
