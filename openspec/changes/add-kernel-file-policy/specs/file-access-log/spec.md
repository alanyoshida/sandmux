## MODIFIED Requirements

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
