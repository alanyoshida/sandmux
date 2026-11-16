## ADDED Requirements

### Requirement: Preset do Antigravity CLI
O preset `agy` MUST rodar o binário `~/.local/bin/agy` do host em somente leitura, com a atualização automática desligada pela variável `AGY_CLI_DISABLE_AUTO_UPDATE=true`, e MUST receber as chaves falsas dos provedores com segredo no cofre por variável de ambiente. Quando `GEMINI_API_KEY` está no ambiente do lançamento, o `agy` MUST ser lançado com `--settings {"modelProvider":"gemini"}`, sem mudar o `settings.json` do HOME sintético. Sem o binário, o `run` MUST terminar com a dica de instalar.

#### Scenario: Chave do Gemini no cofre
- **WHEN** o cofre tem a chave de `generativelanguage.googleapis.com`, e o usuário roda `sandmux run agy`
- **THEN** o `agy` recebe `GEMINI_API_KEY` com o valor falso e a opção `--settings {"modelProvider":"gemini"}`, e o gateway troca o falso pelo real

#### Scenario: Sem chave do Gemini
- **WHEN** nem o cofre nem o `--pass-env` trazem `GEMINI_API_KEY`
- **THEN** o `agy` é lançado sem `--settings` e usa o login dele

#### Scenario: Atualização desligada
- **WHEN** o `agy` confinado inicia
- **THEN** ele tem `AGY_CLI_DISABLE_AUTO_UPDATE=true` e não tenta se atualizar

### Requirement: Importação do Antigravity CLI
A importação do `agy` MUST copiar as configurações de `~/.gemini/antigravity-cli`, a configuração compartilhada de `~/.gemini/config` (MCPs, hooks, plugins, regras, skills, agentes e workflows) e as regras globais de `~/.gemini`. Ela MUST deixar de fora o token do login, os segredos de plugins, os projetos e tudo o que é sessão, cache, registro ou banco.

#### Scenario: Importação sem o token do login
- **WHEN** o usuário importa o Antigravity CLI, e `~/.gemini/antigravity-cli/antigravity-oauth-token` existe
- **THEN** o HOME sintético tem `settings.json` e `~/.gemini/config/mcp_config.json`, e não tem o token nem as conversas

#### Scenario: Projetos e segredos de plugins de fora
- **WHEN** existem `~/.gemini/config/projects/` e `~/.config/antigravity/plugin-secrets.json`
- **THEN** nenhum dos dois é copiado para o HOME sintético

## MODIFIED Requirements

### Requirement: Atualização do harness pelo sandmux
`sandmux update <harness>` MUST rodar no host, fora do sandbox, o atualizador oficial do harness instalado:

- `opencode upgrade` no opencode;
- `hermes update` no hermes;
- `claude update` no Claude Code;
- `omp update` no oh-my-pi;
- `agy update` no Antigravity CLI.

O atualizador MUST rodar em primeiro plano, com o terminal do usuário, e o comando MUST terminar com o código de saída dele. Os argumentos depois de `--` MUST ir para o atualizador.


#### Scenario: Atualizar o opencode
- **WHEN** o usuário roda `sandmux update opencode` e não há abas vivas do opencode
- **THEN** o sandmux roda `~/.opencode/bin/opencode upgrade` fora do sandbox e termina com o código de saída dele

#### Scenario: Argumentos para o atualizador
- **WHEN** o usuário roda `sandmux update omp -- --check`
- **THEN** o atualizador roda como `omp update --check`

#### Scenario: Atualizar o Antigravity CLI
- **WHEN** o usuário roda `sandmux update agy`
- **THEN** o sandmux roda `~/.local/bin/agy update` fora do sandbox

### Requirement: Harness sem perguntas de permissão
Por padrão, o sandmux MUST lançar o opencode, o Claude Code, o hermes, o oh-my-pi e o Antigravity CLI no modo em que eles não pedem confirmação para editar, rodar comandos, sair da pasta ou ler arquivos, porque o sandbox é a barreira. Com `harness-asks = true` na configuração (global, perfil ou projeto), o sandmux MUST lançar o harness com o comportamento dele.

#### Scenario: opencode
- **WHEN** o usuário roda `sandmux run opencode` sem a chave
- **THEN** o opencode recebe `OPENCODE_PERMISSION` com todas as permissões em `allow`

#### Scenario: Claude Code
- **WHEN** o usuário roda `sandmux run claude` sem a chave
- **THEN** o Claude Code é lançado com `--dangerously-skip-permissions` e sem a confirmação do modo

#### Scenario: hermes e oh-my-pi
- **WHEN** o usuário roda `sandmux run hermes` ou `sandmux run omp` sem a chave
- **THEN** o hermes recebe `HERMES_YOLO_MODE=1` e o oh-my-pi é lançado com `--auto-approve`

#### Scenario: Antigravity CLI
- **WHEN** o usuário roda `sandmux run agy` sem a chave
- **THEN** o `agy` é lançado com `--dangerously-skip-permissions`

#### Scenario: Perguntas de volta
- **WHEN** a configuração tem `harness-asks = true`
- **THEN** nenhum desses ajustes é feito, e o harness pergunta como faria fora do sandmux

#### Scenario: Shell
- **WHEN** o usuário roda `sandmux run shell`
- **THEN** nada muda no shell
