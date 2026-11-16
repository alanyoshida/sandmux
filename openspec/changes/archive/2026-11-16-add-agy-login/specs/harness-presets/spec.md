## MODIFIED Requirements

### Requirement: Importação do Antigravity CLI
A importação do `agy` MUST copiar as configurações de `~/.gemini/antigravity-cli`, a configuração compartilhada de `~/.gemini/config` (MCPs, hooks, plugins, regras, skills, agentes e workflows) e as regras globais de `~/.gemini`. Ela MUST deixar de fora os segredos de plugins, os projetos e tudo o que é sessão, cache, registro ou banco. O token do login MUST NOT ser copiado como está: ele segue a regra do login do host.

#### Scenario: Importação sem o token do login
- **WHEN** o usuário importa o Antigravity CLI, e `~/.gemini/antigravity-cli/antigravity-oauth-token` existe
- **THEN** o HOME sintético tem `settings.json` e `~/.gemini/config/mcp_config.json`, não tem as conversas, e nenhum token real do login

#### Scenario: Projetos e segredos de plugins de fora
- **WHEN** existem `~/.gemini/config/projects/` e `~/.config/antigravity/plugin-secrets.json`
- **THEN** nenhum dos dois é copiado para o HOME sintético

## ADDED Requirements

### Requirement: Login do host do Antigravity CLI
Com o login do Google no host e o cofre aberto, a importação do `agy` MUST guardar o access e o refresh no cofre (`agy:access` e `agy:refresh`, os mesmos da captura) e escrever no HOME sintético o arquivo do token com a mesma estrutura, os falsos no lugar dos tokens e os demais campos do host. Com o cofre trancado ou ausente, MUST seguir sem o login e dizer como importá-lo depois.

#### Scenario: Sem login de novo
- **WHEN** o host tem o login do `agy`, o cofre está aberto, e o usuário roda `sandmux run agy` pela primeira vez aceitando a importação
- **THEN** o `agy` confinado abre já logado, o arquivo do token no HOME sintético tem `ya29.sandmux…` e `1//sandmux…`, e o cofre tem os reais

#### Scenario: Cofre trancado
- **WHEN** o host tem o login do `agy` e o cofre está trancado na importação
- **THEN** a importação copia o resto, o HOME sintético fica sem o arquivo do token, e o relatório diz para abrir o cofre e rodar `sandmux import agy --overwrite`
