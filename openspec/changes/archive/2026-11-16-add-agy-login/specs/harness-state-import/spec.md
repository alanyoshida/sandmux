## ADDED Requirements

### Requirement: Login do host na reimportação
Na reimportação do `agy` sem `--overwrite`, o arquivo do token MUST seguir a regra dos outros arquivos: só entra se ainda não existe no HOME sintético. Com `--overwrite` e o cofre aberto, o login do host MUST substituir `agy:access` e `agy:refresh` no cofre, mantendo os falsos de antes. O relatório MUST dizer se o login foi levado ao cofre, sem mostrar nenhum token.

#### Scenario: Novo login no host
- **WHEN** o usuário entrou com outra conta do Google no `agy` do host e roda `sandmux import agy --overwrite` com o cofre aberto
- **THEN** o cofre passa a ter os tokens da conta nova, o arquivo do HOME sintético tem os mesmos falsos de antes, e o relatório diz que o login foi levado ao cofre

#### Scenario: Login já importado
- **WHEN** o HOME sintético já tem o arquivo do token, e o usuário roda `sandmux import agy` sem opções
- **THEN** o arquivo e o cofre não mudam
