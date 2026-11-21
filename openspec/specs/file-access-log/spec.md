# file-access-log Specification

## Purpose
Define como o sandmux vê cada abertura de arquivo do agente, com o agente em execução: o supervisor abre no lugar do processo e decide pelo arquivo aberto, e os acessos às pastas do agente vão para um registro com consulta e retenção.

## Requirements

### Requirement: Aberturas pelo supervisor
Numa aba confinada pelo kernel (o auxiliar root, o BPF LSM e o escopo próprio), as aberturas MUST ser decididas e registradas pelo kernel, sem o supervisor. Nas outras, toda chamada `open`, `openat`, `openat2` e `creat` MUST ser atendida pelo supervisor, que abre o arquivo dentro da raiz do sandbox e entrega o descritor; o arquivo entregue MUST ser o que ele abriu, mesmo que o caminho mude depois. Sem o supervisor, essas aberturas MUST falhar.

#### Scenario: Abertura comum
- **WHEN** o agente lê e escreve arquivos na pasta de trabalho, no HOME e em `/tmp`
- **THEN** as leituras e escritas funcionam como antes

#### Scenario: Link absoluto dentro da pasta
- **WHEN** a pasta de trabalho tem um link `x` para `/etc/hostname` e o agente abre `x`
- **THEN** o agente lê o `/etc/hostname` do sandbox, e não o do host

#### Scenario: Descritores do próprio processo
- **WHEN** o agente usa `cat <(echo oi)` ou abre `/proc/self/status`
- **THEN** a abertura funciona, com o descritor e o processo dele

#### Scenario: Aba confinada pelo kernel
- **WHEN** o auxiliar root está instalado, o BPF LSM está ativo e o agente lê mil arquivos da pasta de trabalho
- **THEN** as leituras funcionam sem passar pelo supervisor, e o registro tem uma linha de leitura para cada arquivo

#### Scenario: Auxiliar reiniciado com a aba rodando
- **WHEN** a conexão do daemon com o auxiliar cai numa aba confinada pelo kernel, e o auxiliar volta
- **THEN** a aba fica congelada até o confinamento ser refeito, com as marcas da política atual, e segue; sem o confinamento de volta em 30 s, tudo no escopo da aba morre

### Requirement: Regras do Landlock nas aberturas
O supervisor MUST conferir cada abertura contra as regras de leitura e de escrita do Landlock da especificação, pelo caminho do arquivo aberto na visão do agente. Uma abertura fora das regras MUST falhar com `EACCES`. Os links mágicos de outros processos (`/proc/<pid>/fd`, `cwd` e `root`) MUST ser recusados.

#### Scenario: Escrita fora das regras
- **WHEN** o agente abre para escrita um arquivo que o Landlock só deixa ler, como `/etc/hostname`
- **THEN** a abertura falha com `EACCES`

#### Scenario: Teste de fuga
- **WHEN** o teste de fuga do sandbox roda com as aberturas pelo supervisor
- **THEN** todas as fugas continuam bloqueadas

### Requirement: Registro de arquivos
Cada acesso distinto (caminho e tipo de acesso) às pastas do agente (pasta de trabalho, HOME sintético, pastas extras e pasta de entrada) MUST virar uma linha em `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl`, na primeira vez em cada lançamento. A linha MUST ter a hora, a etiqueta, o caminho, o acesso (`r`, `w`, `rw` ou `create`) e o motivo do bloqueio, quando houver. Um bloqueio MUST sempre ser registrado. Aberturas fora das pastas do agente MUST NOT ser registradas.

#### Scenario: Leitura e escrita na pasta de trabalho
- **WHEN** o agente lê `a.txt` duas vezes e cria `b.txt` na pasta de trabalho
- **THEN** o registro tem uma linha `r` de `a.txt` e uma linha `create` de `b.txt`

#### Scenario: Sistema fora do registro
- **WHEN** o agente roda `ls /usr/bin`
- **THEN** nenhuma linha de `/usr` vai para o registro

### Requirement: Consulta e retenção do registro de arquivos
`sandmux files [<id>]` MUST mostrar as linhas recentes do registro de arquivos, de uma sessão ou de todas, com a hora, a sessão, o acesso e o caminho. O daemon MUST aplicar à pasta `files` a retenção do registro de rede (`net-log-days` e `net-log-max`).

#### Scenario: Consultar uma sessão
- **WHEN** o agente da sessão 1 criou `b.txt` e o usuário roda `sandmux files 1`
- **THEN** a saída tem uma linha com `create` e o caminho de `b.txt`

### Requirement: Namespaces aninhados nas aberturas
O supervisor MUST abrir o `uid_map`, `gid_map`, `projid_map` e `setgroups` do próprio processo no user namespace pai do dele, para que a gravação do mapeamento siga as regras do kernel como fora do sandbox. O `/proc/self` MUST ser o do processo também quando ele está num pid namespace criado dentro do sandbox.

#### Scenario: Mapeamento de ids
- **WHEN** um processo do sandbox cria um user namespace e grava o próprio uid no `/proc/self/uid_map`
- **THEN** a gravação funciona, como fora do sandbox

#### Scenario: Sem namespace próprio
- **WHEN** um processo que não criou user namespace tenta gravar no `/proc/self/uid_map`
- **THEN** a gravação é recusada

#### Scenario: Pid namespace aninhado
- **WHEN** um processo num pid namespace criado dentro do sandbox abre `/proc/self/status`
- **THEN** ele lê o próprio status, e não o de outro processo

### Requirement: Registro dos contêineres
Com o confinamento por eBPF, o registro de arquivos MUST ter uma linha para cada operação negada a um processo do Docker privado da pasta e para a primeira criação de cada arquivo da pasta de trabalho por um contêiner, em cada subida do Docker. A linha MUST ter a etiqueta `docker-<id>` e o caminho do host. Leituras e escritas comuns dos contêineres MUST NOT entrar. `sandmux files` MUST mostrar essas linhas com as do agente.

#### Scenario: Segredo tentado por um contêiner
- **WHEN** o confinamento vale e um contêiner tenta ler o `.env` da pasta
- **THEN** o registro tem uma linha com a etiqueta `docker-<id>`, o caminho do `.env` no host, o acesso `r` e o bloqueio `segredo`

#### Scenario: Arquivo criado por um contêiner
- **WHEN** um contêiner com a pasta montada em `/w` cria `/w/saida.txt`
- **THEN** o registro tem uma linha `create` com o caminho de `saida.txt` no host

#### Scenario: Criação fora da pasta
- **WHEN** um contêiner cria `/tmp/x` no próprio sistema de arquivos
- **THEN** nenhuma linha vai para o registro

### Requirement: Registro de execuções
O registro de arquivos MUST ter uma linha para cada programa executado por um processo de uma aba (o agente e os shells) e, com o auxiliar root, do Docker privado da pasta. A linha MUST ter o acesso `exec`, o caminho executado, os argumentos (`args`, cortados em 1024 bytes), a fonte (`via`) e a etiqueta do lançamento, ou `docker-<id>`. Nenhuma execução MUST ser negada por este registro.

#### Scenario: curl lançado pelo agente
- **WHEN** o agente de uma aba roda `curl -s https://exemplo.com`
- **THEN** o registro tem uma linha `exec` com a etiqueta da aba, o caminho do `curl` e os argumentos `curl`, `-s` e `https://exemplo.com`

#### Scenario: Contêiner executa um programa
- **WHEN** um contêiner do Docker privado da pasta roda `sh -c 'ls /'`
- **THEN** o registro tem uma linha `exec` com a etiqueta `docker-<id>` para o `sh`, com os argumentos `sh`, `-c` e `ls /`

#### Scenario: Fora das abas
- **WHEN** o usuário roda `ls` num terminal do host
- **THEN** nenhuma linha vai para o registro

### Requirement: Fonte das execuções
Com o auxiliar root e o BPF LSM ativos, as execuções MUST vir do kernel (`via` = `kernel`), pelo caminho que ele de fato executou. Sem eles, as execuções das abas MUST vir do supervisor de aberturas (`via` = `supervisor`), que lê o caminho e os argumentos na memória do processo antes da execução. Até o auxiliar assumir a aba, o supervisor MUST registrar.

#### Scenario: Máquina sem o auxiliar
- **WHEN** o `sandmux-helper` não está instalado e o agente roda `git status`
- **THEN** o registro tem uma linha `exec` do `git` com `via` = `supervisor`

#### Scenario: Com o auxiliar
- **WHEN** o confinamento por eBPF vale e o agente roda `git status` um segundo depois de a aba abrir
- **THEN** o registro tem uma linha `exec` do `git` com `via` = `kernel`

### Requirement: Estado do registro de execuções
A proteção do lançamento mostrada no `sandmux stats` e no painel da TUI MUST dizer de onde vêm as execuções da aba (o kernel ou o supervisor) e, quando é o supervisor, por quê (sem o auxiliar, sem o BPF LSM ou sem o escopo dos tetos). `sandmux files <id>` MUST mostrar os argumentos das linhas `exec`.

#### Scenario: Sem o auxiliar
- **WHEN** o `sandmux-helper` não está instalado e o usuário roda `sandmux stats <id>`
- **THEN** a proteção do lançamento mostra "execuções: pelo supervisor" com o motivo
