# Spec Delta

## ADDED Requirements

### Requirement: Renomear uma sessão
`sandmux rename <id> <nome>` MUST trocar o nome da sessão, que aparece em `sandmux ls` e na barra de abas da TUI. O nome novo MUST ser gravado na lista de sessões e sobreviver ao reinício do daemon. Um nome vazio, com quebra de linha ou com mais de 40 caracteres MUST ser recusado.

#### Scenario: Nome novo
- **WHEN** o usuário roda `sandmux rename 1 api`
- **THEN** `sandmux ls` mostra a sessão 1 com o nome `api`, também depois de `sandmux daemon restart`

#### Scenario: Nome inválido
- **WHEN** o usuário roda `sandmux rename 1 ""`
- **THEN** o comando termina com erro e o nome não muda
