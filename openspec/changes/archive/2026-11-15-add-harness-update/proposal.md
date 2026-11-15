## Why

Item "Atualização do harness pelo sandmux" da lista depois da v1, escolhido pelo usuário (2026-11-15).

O código dos harnesses entra no sandbox em somente leitura, e a atualização automática fica desligada lá dentro. Para atualizar, o usuário precisa saber o comando de cada harness e lembrar de rodá-lo fora do sandbox. Depois, as abas abertas continuam com a versão antiga, sem aviso.

## What Changes

- **Comando `sandmux update <harness>`:** roda no host, fora do sandbox, o atualizador oficial do harness instalado:

  | Harness | Atualizador |
  |---|---|
  | `opencode` | `~/.opencode/bin/opencode upgrade` |
  | `hermes` | `~/.local/bin/hermes update` |
  | `claude` | `~/.local/bin/claude update` |
  | `omp` | `~/.local/bin/omp update` |

  - Roda em primeiro plano, no terminal do usuário, e devolve o código de saída do atualizador.
  - Os argumentos depois de `--` vão para o atualizador, como em `sandmux update omp -- --check`.
- **Recusas:**
  - um harness sem atualizador (`shell`) ou desconhecido;
  - um harness não instalado, com a mesma mensagem do `run`.
- **Abas abertas:** antes de atualizar, o comando lista as abas vivas do harness e pede confirmação, porque o atualizador troca arquivos que elas podem estar usando. `--yes` pula a pergunta. Sem terminal e sem `--yes`, recusa. Depois de atualizar, diz quais abas reiniciar para usar a versão nova.
- O README deixa de dizer que o sandmux não atualiza harness. Ele continua sem instalar.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: o comando que atualiza o harness fora do sandbox.

## Impact

- **`sandmux-policy`:** a tabela de atualizadores, ao lado dos presets, com a mesma checagem de instalação.
- **`sandmux` (CLI):** o subcomando `update`, a consulta das abas vivas ao daemon (sem subir o daemon), a confirmação e o aviso de reinício.
- **Testes:** atualizadores falsos num HOME temporário, que registram os argumentos.
- **Documentação:** `docs/sessions.md` e o par em inglês; `docs/claude.md`, `docs/omp.md` e os pares; os dois READMEs.
