# Spec Delta

## MODIFIED Requirements

### Requirement: Cadastro de segredos
Com o cofre desbloqueado, `sandmux vault add <nome> [--domain <padrão>]... [--tool <ferramenta>]...` MUST guardar um segredo com o valor lido do terminal sem eco, ou da entrada padrão, nunca de um argumento, e com um valor falso gerado. `sandmux vault list` MUST mostrar só os nomes, os domínios, as ferramentas e o valor falso. `sandmux vault remove <nome>` MUST apagar o segredo. Cada mudança MUST ser gravada no arquivo na hora.

#### Scenario: Listar sem valores
- **WHEN** o cofre tem o segredo `openrouter` com o domínio `openrouter.ai` e o usuário roda `sandmux vault list`
- **THEN** a saída tem `openrouter` e `openrouter.ai`, e não tem o valor

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o usuário roda `sandmux vault add x`
- **THEN** o comando falha pedindo `sandmux vault unlock`
