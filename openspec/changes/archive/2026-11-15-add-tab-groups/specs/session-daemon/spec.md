## ADDED Requirements

### Requirement: Grupo de cada sessão
Toda sessão MUST pertencer a um grupo. Uma sessão criada sem grupo MUST ir para o grupo `principal`. O grupo MUST ser guardado na lista de sessões, voltar na restauração e aparecer na listagem. Uma sessão gravada antes dos grupos MUST voltar no `principal`. O nome de um grupo MUST ter de 1 a 32 caracteres, entre letras, dígitos, `-`, `_` e `.`, e um nome fora da regra MUST ser recusado com a regra na mensagem.

#### Scenario: Run com grupo
- **WHEN** o usuário roda `sandmux run shell --group estudos`
- **THEN** a sessão fica no grupo `estudos`, e `sandmux ls` mostra `estudos` na coluna GRUPO

#### Scenario: Restauração
- **WHEN** o daemon reinicia com sessões em dois grupos
- **THEN** cada sessão volta no seu grupo

### Requirement: Mover e renomear grupos
`sandmux group move <id> <grupo>` MUST levar a sessão para o grupo dado, criando-o se preciso. `sandmux group rename <grupo> <novo>` MUST levar todas as sessões do grupo para o nome novo. `sandmux group list` MUST listar os grupos com o número de sessões, com o `principal` sempre presente.

#### Scenario: Renomear
- **WHEN** o grupo `estudos` tem duas sessões e o usuário roda `sandmux group rename estudos rust`
- **THEN** as duas sessões ficam no grupo `rust`, e `estudos` some da lista
