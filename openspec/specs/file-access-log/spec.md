# file-access-log Specification

## Purpose
Define como o sandmux vê cada abertura de arquivo do agente, com o agente em execução: o supervisor abre no lugar do processo e decide pelo arquivo aberto, e os acessos às pastas do agente vão para um registro com consulta e retenção.

## Requirements

### Requirement: Aberturas pelo supervisor
Toda chamada `open`, `openat`, `openat2` e `creat` de um processo do sandbox MUST ser atendida pelo supervisor: ele MUST abrir o arquivo dentro da raiz do sandbox e entregar o descritor ao processo como resultado da chamada. O arquivo entregue MUST ser o que o supervisor abriu, mesmo que o caminho mude na memória do processo depois da leitura. Sem o supervisor, as aberturas MUST falhar. O `uid_map`, `gid_map`, `projid_map` e `setgroups` do próprio processo MUST ser abertos no user namespace pai do dele, e o `/proc/self` MUST ser o do processo também quando ele está num pid namespace aninhado.

#### Scenario: Abertura comum
- **WHEN** o agente lê e escreve arquivos na pasta de trabalho, no HOME e em `/tmp`
- **THEN** as leituras e escritas funcionam como antes

#### Scenario: Link absoluto dentro da pasta
- **WHEN** a pasta de trabalho tem um link `x` para `/etc/hostname` e o agente abre `x`
- **THEN** o agente lê o `/etc/hostname` do sandbox, e não o do host

#### Scenario: Descritores do próprio processo
- **WHEN** o agente usa `cat <(echo oi)` ou abre `/proc/self/status`
- **THEN** a abertura funciona, com o descritor e o processo dele

#### Scenario: Mapeamento de ids
- **WHEN** um processo do sandbox cria um user namespace e grava o próprio uid no `/proc/self/uid_map`
- **THEN** a gravação funciona, como fora do sandbox

#### Scenario: Pid namespace aninhado
- **WHEN** um processo num pid namespace criado dentro do sandbox abre `/proc/self/status`
- **THEN** ele lê o próprio status, e não o de outro processo

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
