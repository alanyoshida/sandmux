# Antigravity CLI

> English: [en/agy.md](en/agy.md)

O preset `agy` roda o Antigravity CLI do Google instalado na sua máquina dentro do sandbox, como o opencode, o hermes, o Claude Code e o oh-my-pi.

```sh
sandmux run agy
```

## Sem perguntas

O `agy` roda com `--dangerously-skip-permissions`, sem pedir confirmação para as ferramentas: o sandbox é a barreira. A lista de comandos bloqueados do próprio `agy` e as regras de negação que você escreveu continuam valendo. `harness-asks = true` na configuração devolve as perguntas ([config.md](config.md#perguntas-do-harness)).

## Instalação

O sandmux usa o binário `~/.local/bin/agy` do host, em somente leitura. Sem o binário, o `run` recusa e diz onde ele deveria estar.

A atualização automática fica desligada dentro do sandbox, pela variável `AGY_CLI_DISABLE_AUTO_UPDATE=true`. Para atualizar, rode `sandmux update agy`, que roda o `agy update` fora do sandbox ([sessions.md](sessions.md#atualizar-o-harness)).

## Chaves

O caminho recomendado é a chave da API do Gemini no cofre:

```sh
sandmux vault add gemini --domain generativelanguage.googleapis.com
```

Com a chave no cofre, o `agy` recebe `GEMINI_API_KEY` com o valor falso e é lançado com `--settings '{"modelProvider":"gemini"}'`, que vale só para aquela execução e não muda o seu `settings.json`. O `agy` manda a chave no cabeçalho `x-goog-api-key` para `generativelanguage.googleapis.com`, e o gateway troca o falso pelo real. O mesmo vale para `--pass-env GEMINI_API_KEY`, que entrega a chave real ao agente. As chaves dos outros provedores com segredo no cofre também entram por variável, como no opencode e no hermes.

Sem a chave do Gemini, o `agy` pede o login do Google, como faria fora do sandmux. O login do host não é importado. Um login feito dentro do sandbox fica no HOME sintético (`~/.gemini/antigravity-cli/antigravity-oauth-token`), com o token real, ao alcance do agente. Esse login também usa domínios que o modo de rede `inferencia` não libera. Prefira a chave pelo cofre.

## Importação

Na primeira execução, o sandmux pergunta se importa a configuração que você já tem. Ele copia:

- de `~/.gemini/antigravity-cli`, o `settings.json` e o `keybindings.json`;
- de `~/.gemini/config`, a configuração compartilhada com o app: `config.json`, `mcp_config.json`, `hooks.json`, as regras globais (`AGENTS.md`, `GEMINI.md`, `skills.json`, `rules.json`, `plugins.json`, `agents.json`) e as pastas `hooks/`, `plugins/`, `skills/`, `agents/`, `rules/` e `workflows/`;
- as regras globais `~/.gemini/GEMINI.md` e `~/.gemini/AGENTS.md`.

Ficam de fora:

- o token do login do Google (`antigravity-oauth-token`) e os segredos de plugins (`~/.config/antigravity/plugin-secrets.json`);
- os projetos (`~/.gemini/config/projects` e o `cache/projects.json`), que guardam caminhos e identificadores do host;
- as conversas, o histórico, os registros, os caches e os bancos.

Um segredo no que é copiado, como uma chave no `env` de um MCP, é trocado por um valor falso, como nos outros harnesses ([vault.md](vault.md)).
