## Why

Mais um item de "Mais harnesses": o Antigravity CLI do Google (comando `agy`), instalado na máquina do usuário (versão 1.5.3, em `~/.local/bin/agy`). Pedido do usuário (2026-11-15): um preset no mesmo modelo do `opencode`, `hermes`, `claude` e `omp`.

O `agy` entra pelo login do Google (OAuth, token em `~/.gemini/antigravity-cli/antigravity-oauth-token`) ou por uma chave da API do Gemini (`GEMINI_API_KEY`, com `modelProvider: "gemini"`). O token do login do host não pode entrar no sandbox. A chave, como nos outros harnesses, fica no cofre e chega ao agente como valor falso.

## What Changes

- **Preset `agy`:** o binário `~/.local/bin/agy` do host roda confinado, em somente leitura, com a atualização automática desligada pela variável `AGY_CLI_DISABLE_AUTO_UPDATE=true`.
- **Chave:** o `agy` recebe as chaves falsas dos provedores com segredo no cofre, como o opencode, o hermes e o oh-my-pi. Com `GEMINI_API_KEY` no ambiente (falsa, do cofre, ou por `--pass-env`), o `agy` é lançado com `--settings {"modelProvider":"gemini"}`, que vale só para aquela execução. O gateway troca o falso pelo real no cabeçalho `x-goog-api-key` dos pedidos a `generativelanguage.googleapis.com`.
- **Sem perguntas:** por padrão, o `agy` é lançado com `--dangerously-skip-permissions`; `harness-asks = true` devolve as perguntas.
- **Importação:** copia as configurações do usuário (`~/.gemini/antigravity-cli/settings.json` e `keybindings.json`), a configuração compartilhada de `~/.gemini/config` (`config.json`, `mcp_config.json`, `hooks.json`, as regras globais, e as pastas de hooks, plugins, skills, agentes, regras e workflows) e as regras globais `~/.gemini/GEMINI.md` e `~/.gemini/AGENTS.md`. Ficam de fora:
  - o token do login (`antigravity-oauth-token`) e os segredos de plugins (`~/.config/antigravity/plugin-secrets.json`);
  - as conversas, o histórico, os registros, os caches e os bancos;
  - os projetos (`~/.gemini/config/projects` e `cache/projects.json`), que apontam para pastas do host.
- **Atualizador:** `sandmux update agy` roda `agy update` no host, fora do sandbox.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: o preset `agy` e a importação dele, o `agy update` na atualização pelo sandmux e o `agy` no modo sem perguntas.

## Impact

- **`sandmux-policy`:** `presets.rs` (o `agy` em `PRESETS`, `importable`, `FAKE_KEY_HARNESSES`, `plan` e `updater`), `compose.rs` (`no_asks` e as variáveis falsas no plano) e a mensagem dos harnesses com atualizador.
- **`sandmux-import`:** a tabela do `agy` e `state_roots`.
- **`sandmux`:** a ajuda dos comandos `run` e `update`.
- **Testes e documentação:** testes do preset, da importação, do sem-perguntas e do atualizador com um harness falso; `docs/agy.md` e o par em inglês, README e o par em inglês.
