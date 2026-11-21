## MODIFIED Requirements

### Requirement: Registro de arquivos
Cada acesso distinto (caminho e tipo de acesso) às pastas do agente (pasta de trabalho, HOME sintético, pastas extras e pasta de entrada) MUST virar uma linha em `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl`, na primeira vez em cada lançamento. A linha MUST ter a hora, a etiqueta, o caminho, o acesso (`r`, `w`, `rw` ou `create`) e o motivo do bloqueio, quando houver. Um bloqueio MUST sempre ser registrado. Aberturas fora das pastas do agente MUST NOT ser registradas.

#### Scenario: Leitura e escrita na pasta de trabalho
- **WHEN** o agente lê `a.txt` duas vezes e cria `b.txt` na pasta de trabalho
- **THEN** o registro tem uma linha `r` de `a.txt` e uma linha `create` de `b.txt`

#### Scenario: Sistema fora do registro
- **WHEN** o agente roda `ls /usr/bin`
- **THEN** nenhuma linha de `/usr` vai para o registro

#### Scenario: Leitura e escrita numa aba confinada pelo kernel
- **WHEN** a aba é confinada pelo kernel e o agente abre um arquivo da pasta de trabalho para leitura e escrita, e outro numa pasta em `file-deny`
- **THEN** o registro tem uma linha `rw` do primeiro e uma linha `rw` bloqueada do segundo, como numa aba pelo supervisor
