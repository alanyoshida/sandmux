## ADDED Requirements

### Requirement: Cofre na página de configurações
A página de configurações da TUI MUST ter a seção "Cofre", com o estado do cofre e a lista de segredos (nome, tipo e domínios, nunca o valor). Pela seção, o usuário MUST poder:
- criar, desbloquear e trancar o cofre;
- trocar a senha;
- acrescentar um segredo (nome, valor e domínios) ou uma chave SSH (arquivo e servidores);
- editar os domínios de um segredo;
- apagar um segredo, depois de confirmar.

Senhas e valores MUST aparecer só como `•` e MUST ser apagados da memória da TUI depois do envio.

#### Scenario: Acrescentar pela TUI
- **WHEN** com o cofre aberto, o usuário acrescenta `lmstudio` com o valor `abc` e o domínio `host.sandmux.internal`
- **THEN** o segredo aparece na lista com o domínio, o valor nunca aparece na tela, e `sandmux vault list` mostra o segredo

#### Scenario: Apagar com confirmação
- **WHEN** o usuário escolhe um segredo, aperta `x` e confirma
- **THEN** o segredo some da lista e do cofre

### Requirement: Troca dos domínios de um segredo
O daemon MUST aceitar a troca dos domínios de um segredo existente sem receber o valor de novo, com o cofre desbloqueado, e MUST validar cada domínio como na criação.

#### Scenario: Editar domínios
- **WHEN** o usuário troca os domínios de `lmstudio` para `host.sandmux.internal` e `localhost`
- **THEN** o segredo fica com os dois domínios, e o valor continua o mesmo
