## MODIFIED Requirements

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
