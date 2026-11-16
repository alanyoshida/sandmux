## ADDED Requirements

### Requirement: Harness sem perguntas de permissão
Por padrão, o sandmux MUST lançar o opencode, o Claude Code, o hermes e o oh-my-pi no modo em que eles não pedem confirmação para editar, rodar comandos, sair da pasta ou ler arquivos, porque o sandbox é a barreira. Com `harness-asks = true` na configuração (global, perfil ou projeto), o sandmux MUST lançar o harness com o comportamento dele.

#### Scenario: opencode
- **WHEN** o usuário roda `sandmux run opencode` sem a chave
- **THEN** o opencode recebe `OPENCODE_PERMISSION` com todas as permissões em `allow`

#### Scenario: Claude Code
- **WHEN** o usuário roda `sandmux run claude` sem a chave
- **THEN** o Claude Code é lançado com `--dangerously-skip-permissions` e sem a confirmação do modo

#### Scenario: hermes e oh-my-pi
- **WHEN** o usuário roda `sandmux run hermes` ou `sandmux run omp` sem a chave
- **THEN** o hermes recebe `HERMES_YOLO_MODE=1` e o oh-my-pi é lançado com `--auto-approve`

#### Scenario: Perguntas de volta
- **WHEN** a configuração tem `harness-asks = true`
- **THEN** nenhum desses ajustes é feito, e o harness pergunta como faria fora do sandmux

#### Scenario: Shell
- **WHEN** o usuário roda `sandmux run shell`
- **THEN** nada muda no shell
