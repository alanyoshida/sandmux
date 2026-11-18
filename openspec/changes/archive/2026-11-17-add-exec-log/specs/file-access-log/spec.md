## ADDED Requirements

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
