## MODIFIED Requirements

### Requirement: Leitura do manifesto da importação
`sandmux vault import` MUST percorrer os manifestos gerados pela importação de cada harness e, para cada valor falso ainda sem segredo no cofre, mostrar o harness, o arquivo e a chave, e pedir o valor real (sem eco), que MUST ficar no cofre ligado ao valor falso. Uma resposta vazia MUST pular o item. Quando o nome da chave não indica um domínio conhecido, o comando MUST perguntar o domínio dono depois do valor; uma resposta vazia guarda o segredo sem domínio.

#### Scenario: Chave do opencode
- **WHEN** a importação do opencode trocou a chave do OpenRouter em `auth.json` por um valor falso e o usuário roda `sandmux vault import`
- **THEN** o comando pede o valor real dessa chave, e o cofre passa a ter um segredo ligado ao valor falso

#### Scenario: Chave de um servidor local
- **WHEN** a importação trocou o `Authorization` de um provedor local do opencode e o usuário roda `sandmux vault import`, informa a chave e responde `host.sandmux.internal` ao domínio
- **THEN** o segredo fica no cofre com o domínio `host.sandmux.internal`, ligado ao valor falso
