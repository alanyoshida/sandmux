## MODIFIED Requirements

### Requirement: Leitura do manifesto da importação
`sandmux vault import` MUST percorrer os manifestos gerados pela importação de cada harness e, para cada valor falso ainda sem segredo no cofre, mostrar o harness, o arquivo, a chave e a linha do arquivo importado (com o falso como `<falso>`), e pedir o valor real, sem mostrá-lo: cada caractere digitado aparece como `•`, que MUST ficar no cofre ligado ao valor falso. Uma resposta vazia MUST pular o item.

#### Scenario: Chave do opencode
- **WHEN** a importação do opencode trocou a chave do OpenRouter em `auth.json` por um valor falso e o usuário roda `sandmux vault import`
- **THEN** o comando pede o valor real dessa chave, e o cofre passa a ter um segredo ligado ao valor falso

#### Scenario: Digitação visível sem o valor
- **WHEN** o usuário cola uma chave de 43 caracteres na pergunta do `sandmux vault import`
- **THEN** o terminal mostra 43 bolinhas, e o valor não aparece
