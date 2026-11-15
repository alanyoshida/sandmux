## Why

Segunda parte do item "Mais harnesses", escolhido pelo usuário (2026-11-15): o oh-my-pi (`omp`).

O oh-my-pi guarda as credenciais num banco SQLite (`~/.omp/agent/agent.db`), não em arquivos de texto. Decisão do usuário (2026-11-15): importar sem esse banco; as chaves vêm do cofre.

## What Changes

- **Preset `omp`:** o binário `~/.local/bin/omp` do host roda confinado, em somente leitura. O oh-my-pi extrai a biblioteca nativa dele em `~/.omp/natives` do HOME sintético.
- **Chaves:** as chaves falsas dos provedores com segredo no cofre entram por variável de ambiente (`OPENROUTER_API_KEY` e afins), como no opencode e no hermes, e o gateway troca pelo real.
- **Importação:** copia de `~/.omp/agent` o `config.yml` e as pastas `extensions/`, `agents/`, `rules/`, `managed-skills/`, `skills/`, `commands/` e `prompts/`. Ficam de fora:
  - o `agent.db`, que tem as credenciais;
  - os outros bancos (`history.db`, `models.db`, `skill-descriptions.db`);
  - as sessões, os caches e os registros.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: o preset `omp` e a importação dele.

## Impact

- **`sandmux-policy`:** `presets.rs`, com o `omp` em `PRESETS`, `importable` e `FAKE_KEY_HARNESSES`.
- **`sandmux-import`:** a tabela do `omp`.
- **Testes e documentação:** o teste do preset; `docs/omp.md` e o par em inglês, README.
