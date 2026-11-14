## MODIFIED Requirements

### Requirement: Ferramentas de um segredo
Um segredo do cofre MUST poder ter uma lista de ferramentas, dadas por `sandmux vault add --tool <ferramenta>` (repetível): `gh`, `git`, `npm`, `pip`, `cargo`, `docker`, `telegram`, `discord`, `slack` e `slack-app`. Sem `--tool`, a lista MUST vir dos domínios conhecidos do cenário "Domínios conhecidos". Uma ferramenta desconhecida MUST ser recusada.

#### Scenario: Domínios conhecidos
- **WHEN** o usuário guarda um segredo com o domínio `github.com`, `registry.npmjs.org`, `pypi.org`, `crates.io`, `ghcr.io`, `registry-1.docker.io`, `docker.io`, `api.telegram.org`, `discord.com` ou `slack.com`, sem `--tool`
- **THEN** o segredo fica com as ferramentas `gh` e `git` (`github.com`), `npm` (`registry.npmjs.org`), `pip` (`pypi.org`), `cargo` (`crates.io`), `docker` (`ghcr.io`, `registry-1.docker.io` e `docker.io`), `telegram` (`api.telegram.org`), `discord` (`discord.com`) ou, no `slack.com`, `slack-app` quando o valor começa com `xapp-` e `slack` nos demais

#### Scenario: Registry privado
- **WHEN** o usuário roda `sandmux vault add npm-empresa --domain npm.empresa.com --tool npm`
- **THEN** o segredo fica com a ferramenta `npm`

#### Scenario: Nome de um canal
- **WHEN** o usuário roda `sandmux vault add discord`, sem `--domain`
- **THEN** o segredo fica com os domínios `discord.com` e `gateway.discord.gg` e a ferramenta `discord`

### Requirement: Valor falso na configuração da ferramenta
Em toda sessão com o cofre desbloqueado, cada segredo com ferramentas MUST chegar a elas com o valor falso, pela forma de cada ferramenta do cenário "Forma de cada ferramenta", sem alterar os arquivos guardados no HOME sintético.

#### Scenario: Forma de cada ferramenta
- **WHEN** o cofre tem um segredo para cada ferramenta e uma sessão sobe
- **THEN** o `gh` recebe `GH_TOKEN`; o `git`, um ajudante de credenciais para `https://<domínio>` pelas variáveis `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` e `GIT_CONFIG_VALUE_n`; o `npm`, um `~/.npmrc` só daquela execução com a linha do token do domínio; o `pip`, `PIP_EXTRA_INDEX_URL` com o usuário e o falso (fora do `pypi.org`); o `cargo`, `CARGO_REGISTRY_TOKEN`; o `docker`, um `~/.docker/config.json` só daquela execução com `auths` do domínio; e `telegram`, `discord`, `slack` e `slack-app`, as variáveis `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN` e `SLACK_APP_TOKEN`, também no `~/.hermes/.env` daquela execução no hermes

#### Scenario: gh no sandbox
- **WHEN** o cofre desbloqueado tem um segredo de `github.com` e o agente roda `gh auth status`
- **THEN** o `gh` vê um token, que é o valor falso, e o valor real não está em nenhum arquivo nem variável do sandbox

## ADDED Requirements

### Requirement: Falso no formato do canal
Um segredo guardado por `sandmux vault add` com a ferramenta `telegram`, `slack` ou `slack-app` MUST receber um valor falso no formato do token do canal: `0000000000:sandmux-fake-` e 32 caracteres no Telegram; `xoxb-sandmux-fake-` e 32 caracteres no `slack`; e `xapp-sandmux-fake-` e 32 caracteres no `slack-app`.

#### Scenario: Token do Telegram
- **WHEN** o usuário guarda o token do bot com `sandmux vault add telegram`
- **THEN** o `sandmux vault list` mostra um falso que começa com `0000000000:sandmux-fake-`
