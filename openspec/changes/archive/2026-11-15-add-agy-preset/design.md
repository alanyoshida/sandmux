## Context

O Antigravity CLI (`agy`) é um binário Go único de 235 MB em `~/.local/bin/agy`. A investigação foi feita em 2026-11-15 na versão 1.5.3, só lendo a instalação real e rodando uma cópia do binário com HOME falso e sem rede (`unshare -rn`), para não tocar no estado real.

O que a investigação mostrou:

- **Estado:** `~/.gemini/antigravity-cli` (configurações do CLI, conversas, registros, caches, bancos, o token do login e um script `bin/agentapi` que aponta para o binário) e `~/.gemini/config` (configuração compartilhada com o app: `config.json`, `mcp_config.json`, `hooks.json`, plugins, regras, skills, agentes, projetos). Os segredos de plugins ficam em `~/.config/antigravity/plugin-secrets.json`. O `~/.gemini/antigravity` e o `~/.gemini/antigravity-ide` são do app de desktop, não do CLI.
- **Login:** OAuth do Google por padrão. Com `GEMINI_API_KEY` no ambiente e `modelProvider: "gemini"` nas configurações, o registro mostra `authenticated via gemini_api_key`, e o pedido de modelo vai para `https://generativelanguage.googleapis.com/v1beta/models/...:streamGenerateContent`. O SDK do Gemini manda a chave no cabeçalho `x-goog-api-key`.
- **Atualização:** a cada início, o `agy` dispara um processo de atualização em segundo plano. `AGY_CLI_DISABLE_AUTO_UPDATE=true` desliga ("Auto-update disabled via environment variable"); com o valor `1`, a atualização roda mesmo assim.
- **Sem perguntas:** `--dangerously-skip-permissions` aprova as ferramentas sem perguntar. Desde a 1.5.2, os comandos da lista de bloqueio do próprio `agy` e as regras de negação explícitas continuam valendo. A opção é aceita antes de um subcomando (`agy --dangerously-skip-permissions mcp list` funciona).
- **Atualizador oficial:** o subcomando `agy update`.
- **Configuração por execução:** `--settings` recebe um objeto JSON de chaves do `settings.json` que vale só para a execução e nunca é gravado.

## Goals / Non-Goals

**Goals:**
- `sandmux run agy` sobe o Antigravity confinado, com importação, chave do Gemini pelo cofre, atualização desligada, sem perguntas e `sandmux update agy`.

**Non-Goals:**
- Capturar o token do login do Google no gateway, como o change 43 faz com o Claude Code. Fica em "Depois da v1".
- O gateway de modelos de empresa do `agy` (`AGY_LLM_GATEWAY_URL`), que poderia levar o `agy` ao OpenRouter. Fica em aberto.

## Decisions

1. **Binário em somente leitura, como o `omp`.** Monta só `~/.local/bin/agy`. O `agy` recria no HOME sintético o que precisa (`bin/agentapi`, `builtin/`, os bancos). Alternativa descartada: montar `~/.gemini/antigravity-cli/bin`, que é estado e é gerado de novo.

2. **`AGY_CLI_DISABLE_AUTO_UPDATE=true`.** A variável foi achada no binário e conferida no registro da cópia isolada. O valor precisa ser `true`. Sem ela, o processo de atualização tentaria trocar o binário, que está em somente leitura, e falharia a cada início, com tráfego para o servidor de atualização.

3. **Chave do Gemini por variável e `--settings` só da execução.** O `agy` entra na lista `FAKE_KEY_HARNESSES` e recebe as chaves falsas do cofre. Quando `GEMINI_API_KEY` está entre as variáveis do lançamento (falsa, do cofre, ou por `--pass-env`), o preset põe `--settings {"modelProvider":"gemini"}` logo depois do executável. O `settings.json` importado fica como está, e, sem chave do Gemini, o `agy` pede o login do Google como faria fora do sandmux. O gateway já decifra `generativelanguage.googleapis.com` quando o cofre tem segredo nesse domínio e troca o falso nos cabeçalhos. Por isso, a troca não precisa de mudança no proxy. Alternativa descartada: gravar `modelProvider` no `settings.json` do HOME sintético, o que mudaria a configuração do usuário e valeria também quando a chave sai do cofre.

4. **O login do Google não é importado.** O token do host fica de fora, como o `.credentials.json` do Claude Code. Sem chave do Gemini, o usuário pode fazer o login dentro do sandbox. Nesse caso, o token fica no HOME sintético, ao alcance do agente. A documentação diz isso e recomenda a chave do cofre.

5. **Tabela de importação.** Entram as configurações do usuário e o que é configuração compartilhada (regras, hooks, plugins, skills, agentes, workflows, MCPs). Ficam de fora os projetos, porque guardam caminhos e identificadores do host, que o `agy` recria, e tudo o que é sessão, cache, registro ou banco. Os segredos que aparecerem no que entra, como chaves no `env` de um MCP, passam pela troca por falsos, como nos outros harnesses.

6. **Sem perguntas pela opção.** `no_asks` põe `--dangerously-skip-permissions` depois do executável, como no Claude Code. O `--mode auto` do `agy` ainda pede revisão de comandos que saem do sandbox dele, então não serve.

## Risks / Trade-offs

- **O `--settings` pode substituir em vez de mesclar** → não conferido na cópia isolada, porque sem rede não dá para ver o modelo escolhido. Se substituir, o `model` e as permissões do `settings.json` importado não valem nas execuções com chave do Gemini. A conferência fica nas verificações à mão pendentes.
- **Login do Google dentro do sandbox** → token real no HOME sintético. Mitigação: documentação e a chave do cofre como caminho recomendado. A captura do token fica para depois.
- **Modo só inferência** → libera `generativelanguage.googleapis.com`, o suficiente para a chave do Gemini. O login do Google usa outros domínios (`cloudcode-pa.googleapis.com`, `oauth2.googleapis.com`) e não funciona nesse modo.
- **O `agy` muda rápido** → a variável de atualização e a opção sem perguntas foram conferidas só na 1.5.3.
