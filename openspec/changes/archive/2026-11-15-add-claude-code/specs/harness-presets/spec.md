## ADDED Requirements

### Requirement: Preset do Claude Code
O preset `claude` MUST rodar o Claude Code instalado no host (`~/.local/bin/claude`, com a instalação em `~/.local/share/claude`) em somente leitura, com a atualização automática desligada. Sem a instalação, o `run` MUST terminar com a dica de instalar. A importação MUST copiar `~/.claude.json` e as configurações, agentes, comandos, skills, hooks e plugins de `~/.claude`, e MUST deixar de fora `~/.claude/.credentials.json`, as conversas e os caches.

#### Scenario: Importação sem o token do host
- **WHEN** o usuário importa o Claude Code, e `~/.claude/.credentials.json` existe
- **THEN** o HOME sintético tem `settings.json` e `.claude.json`, e não tem `.credentials.json`
