# oh-my-pi

> English: [en/omp.md](en/omp.md)

O preset `omp` roda o oh-my-pi instalado na sua máquina dentro do sandbox, como o opencode, o hermes e o Claude Code.

```sh
sandmux run omp
```

## Instalação

O sandmux usa o binário `~/.local/bin/omp` do host, em somente leitura: para atualizar, rode `sandmux update omp`, que roda o `omp update` fora do sandbox ([sessions.md](sessions.md#atualizar-o-harness)). Sem o binário, o `run` recusa e diz onde ele deveria estar. Na primeira execução, o oh-my-pi extrai a biblioteca nativa dele em `~/.omp/natives` do HOME sintético.

## Chaves

As chaves dos provedores vêm do cofre, como no opencode e no hermes. Os provedores com segredo no cofre entram por variável de ambiente (`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY` e afins) com o valor falso, e o gateway troca pelo real no tráfego. Para cadastrar uma chave:

```sh
sandmux vault add openrouter --domain openrouter.ai
```

O oh-my-pi guarda os logins dele num banco SQLite, `~/.omp/agent/agent.db`. Esse banco não é importado, e um login feito dentro do sandbox fica no banco do HOME sintético, com o valor real. Prefira as chaves pelo cofre.

## Importação

Na primeira execução, o sandmux pergunta se importa a configuração que você já tem. Ele copia de `~/.omp/agent`:

- `config.yml`;
- as pastas `extensions/`, `agents/`, `rules/`, `managed-skills/`, `skills/`, `commands/` e `prompts/`.

Ficam de fora:

- o `agent.db`, com as credenciais;
- os outros bancos (`history.db`, `models.db`, `skill-descriptions.db`);
- as sessões, os caches e os registros.
