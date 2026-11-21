## ADDED Requirements

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
