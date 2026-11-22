## MODIFIED Requirements

### Requirement: Saída limpa do auxiliar
Numa saída pedida (`SIGTERM`), o auxiliar MUST avisar cada conexão antes de soltar as regras dela e MUST NOT atender conexões novas. Um lançamento que encontra o auxiliar trocando MUST tentar o registro do escopo de novo por até 30 s. Quando o socket do auxiliar não existe, o lançamento MUST desistir em até 1 s.

#### Scenario: Lançamento durante a troca do auxiliar
- **WHEN** uma aba é lançada enquanto o auxiliar sai e o seguinte sobe
- **THEN** o lançamento registra o escopo no auxiliar seguinte e sobe

#### Scenario: Aviso antes de soltar as regras
- **WHEN** o auxiliar recebe `SIGTERM` com um confinamento ligado
- **THEN** quem pediu vê a conexão terminar enquanto as regras ainda valem

#### Scenario: Lançamento sem o auxiliar instalado
- **WHEN** o kernel tem o Landlock ABI 4, o socket do auxiliar não existe, e o usuário lança uma aba
- **THEN** o lançamento é recusado em menos de 2 s, apontando o `sandmux doctor`, sem o comando rodar
