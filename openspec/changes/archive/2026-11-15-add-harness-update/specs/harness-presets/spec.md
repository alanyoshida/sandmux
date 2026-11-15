## ADDED Requirements

### Requirement: Atualização do harness pelo sandmux
`sandmux update <harness>` MUST rodar no host, fora do sandbox, o atualizador oficial do harness instalado:

- `opencode upgrade` no opencode;
- `hermes update` no hermes;
- `claude update` no Claude Code;
- `omp update` no oh-my-pi.

O atualizador MUST rodar em primeiro plano, com o terminal do usuário, e o comando MUST terminar com o código de saída dele. Os argumentos depois de `--` MUST ir para o atualizador.


#### Scenario: Atualizar o opencode
- **WHEN** o usuário roda `sandmux update opencode` e não há abas vivas do opencode
- **THEN** o sandmux roda `~/.opencode/bin/opencode upgrade` fora do sandbox e termina com o código de saída dele

#### Scenario: Argumentos para o atualizador
- **WHEN** o usuário roda `sandmux update omp -- --check`
- **THEN** o atualizador roda como `omp update --check`

### Requirement: Recusa da atualização
`sandmux update` MUST recusar um harness sem atualizador, como o `shell`, e um harness não instalado, com o caminho procurado e a instrução de instalação. O sandmux MUST NOT instalar um harness por esse comando.

#### Scenario: Harness sem atualizador
- **WHEN** o usuário roda `sandmux update shell`
- **THEN** o comando termina com erro e não roda nada

#### Scenario: Harness não instalado
- **WHEN** o usuário roda `sandmux update claude` e `~/.local/bin/claude` não existe
- **THEN** o comando termina com erro, informa o caminho procurado e a instrução de instalação, e não roda nada

### Requirement: Abas vivas na atualização
Quando há abas vivas do harness, `sandmux update` MUST listá-las e pedir confirmação antes de rodar o atualizador, a não ser com `--yes`. Sem terminal e sem `--yes`, o comando MUST recusar sem rodar o atualizador. Depois de uma atualização bem-sucedida, o comando MUST dizer quais abas reiniciar para usar a versão nova.

#### Scenario: Aba viva sem confirmação
- **WHEN** há uma aba viva do hermes e o usuário roda `sandmux update hermes` sem terminal e sem `--yes`
- **THEN** o comando termina com erro, lista a aba e não roda o atualizador

#### Scenario: Aba viva com confirmação
- **WHEN** há uma aba viva do hermes e o usuário roda `sandmux update hermes --yes`
- **THEN** o atualizador roda, e, se ele termina bem, o comando diz para reiniciar a aba
