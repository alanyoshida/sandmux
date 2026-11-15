## MODIFIED Requirements

### Requirement: Atalhos configuráveis
A seção `[tui]` do `config.toml` MUST aceitar `prefix` (`"C-<letra>"`) e, em `[tui.keys]`, a troca da tecla de cada ação (`new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help` e as acrescentadas depois, como `enter`) por um caractere. Duas ações escolhidas em `[tui.keys]` na mesma tecla, ou uma ação desconhecida, MUST tornar o arquivo inválido. Uma tecla escolhida que é a padrão de outra ação MUST vencer, e a outra ação MUST ficar sem tecla, para que uma ação nova não invalide um arquivo que já existia.

#### Scenario: Prefixo trocado
- **WHEN** `[tui]` tem `prefix = "C-a"` e o usuário digita Ctrl+a n
- **THEN** a próxima aba fica ativa, e Ctrl+b vai cru para a sessão

#### Scenario: Tecla de outra ação padrão
- **WHEN** `[tui.keys]` tem `new = "t"`, a tecla padrão de `enter`
- **THEN** o arquivo é válido, `t` cria uma aba nova, e `enter` fica sem tecla

#### Scenario: Atalho repetido
- **WHEN** `[tui.keys]` tem `new = "t"` e `close = "t"`
- **THEN** o arquivo é inválido, e a mensagem cita as duas ações
