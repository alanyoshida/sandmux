## ADDED Requirements

### Requirement: Login antigo do Antigravity CLI no HOME sintético
A varredura de chaves reais do daemon MUST reconhecer o arquivo do token do `agy` no HOME sintético. Um access ou refresh real ali MUST ir para o cofre como `agy:access` e `agy:refresh` (com os domínios da captura), e o arquivo MUST ser regravado com os falsos. Com o cofre trancado, o arquivo MUST esperar o cofre, como as outras chaves.

#### Scenario: Login feito no sandbox antes do change
- **WHEN** o HOME sintético do `agy` tem um token real de um login feito dentro do sandbox, e o cofre está aberto
- **THEN** em poucos segundos o cofre tem `agy:access` e `agy:refresh`, e o arquivo tem `ya29.sandmux…` e `1//sandmux…`
