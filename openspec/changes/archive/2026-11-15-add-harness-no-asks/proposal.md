## Why

Pedido do usuário (2026-11-15): deixar o opencode no modo que não faz perguntas por padrão, porque o bloqueio é do sandbox. Decisões do usuário (2026-11-15): vale também para o Claude Code, o hermes e o oh-my-pi; uma chave na configuração devolve as perguntas.

Hoje cada harness pede confirmação para ações que o sandbox já confina: o opencode pergunta ao sair da pasta do projeto, num loop e ao ler `.env`; o Claude Code pergunta antes de editar e de rodar comandos; o hermes, antes de comandos perigosos; o oh-my-pi, antes das ferramentas. As perguntas interrompem o agente sem acrescentar proteção ao que o Landlock, a política de arquivos e a rede do sandmux já garantem.

## What Changes

- **Sem perguntas por padrão,** pelo mecanismo de cada harness, conferido nas versões instaladas nesta máquina:
  - opencode (1.19.5): `OPENCODE_PERMISSION` com tudo em `allow`, inclusive `external_directory`, `doom_loop` e `read`;
  - Claude Code (2.1.325): `--dangerously-skip-permissions` e `--settings` com `skipDangerousModePermissionPrompt`, para não pedir a confirmação do modo;
  - hermes: `HERMES_YOLO_MODE=1` (os bloqueios fixos do hermes continuam);
  - oh-my-pi (18.12.6): `--auto-approve`.
- **Chave `harness-asks`:** `true` no global, num perfil ou num projeto devolve o comportamento do harness. Também na tela de configurações da TUI. O padrão é `false`.
- O `shell` e a aba de chat não mudam.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: os harnesses rodam sem perguntas de permissão por padrão, e a chave `harness-asks` as devolve.

## Impact

- **`sandmux-policy`:** a chave `harness-asks` nas camadas e no efetivo; na composição, a variável ou as opções de cada harness.
- **TUI:** item na seção Sandbox da tela de configurações.
- **Testes:** composição por harness, com e sem a chave; precedência; o item na TUI.
- **Documentação:** `docs/config.md`, README, `docs/claude.md`, `docs/omp.md`, `docs/hermes.md` e os pares em inglês.
