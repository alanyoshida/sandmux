## Why

O hermes completo (etapa 8) responde em canais de mensagem e roda tarefas agendadas, e as duas coisas vivem no gateway dele (`hermes gateway run`). Hoje esse gateway só vive enquanto a aba do hermes está aberta. Além disso, os tokens dos canais não podem ficar em claro no sandbox, e dois deles não passam pelo caminho da troca atual: o Telegram leva o token na URL, e o Discord o manda numa mensagem do WebSocket.

## What Changes

- **Sessão do gateway:** `sandmux run hermes --gateway` cria no daemon uma sessão que roda `hermes gateway run` na pasta escolhida, com os canais e o cron.
  - Ela reinicia sozinha se o gateway cair, com espera crescente.
  - Depois de um reboot, ela fica "aguardando cofre" enquanto o cofre existe e está trancado, e sobe no desbloqueio.
  - Há uma por vez: uma segunda é recusada enquanto a primeira vive.
  - Ela avisa quando um gateway do hermes do host está rodando, porque dois gateways no mesmo bot brigam pelas mensagens.
- **Tokens dos canais no cofre:**
  - ferramentas novas `telegram`, `discord`, `slack` e `slack-app`;
  - domínios conhecidos: `api.telegram.org`, `discord.com` e `gateway.discord.gg`, e `slack.com`;
  - o falso vai nas variáveis `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN` e `SLACK_APP_TOKEN`, também no `~/.hermes/.env` daquela execução;
  - o falso imita o formato de cada canal.
- **Troca no caminho da URL:** num pedido decifrado ao dono do segredo, o falso no caminho vira o real (Telegram: `/bot<token>/getUpdates`). O registro guarda o caminho com o falso.
- **WebSocket decifrado:** o proxy decifrado passa a aceitar o upgrade para WebSocket. Nas mensagens de texto do sandbox ao dono, o falso vira o real (Discord: IDENTIFY e RESUME). O proxy tira o `Sec-WebSocket-Extensions` do pedido, para que as mensagens não venham comprimidas.
- **Importação:** o pareamento dos canais e as tarefas agendadas do hermes continuam de fora, agora por decisão do usuário. Os canais são pareados e as tarefas criadas dentro do sandbox.
- **Protocolo 19:** `LaunchOptions.gateway`.

## Capabilities

### New Capabilities
- `hermes-gateway`: a sessão de longa duração do gateway do hermes, com os canais e o cron, o reinício, a espera pelo cofre, a unicidade e o aviso do gateway do host.

### Modified Capabilities
- `key-injection`: a troca passa a valer também no caminho da URL e nas mensagens de texto de um WebSocket ao dono do segredo.
- `tls-interception`: o proxy decifrado repassa o upgrade para WebSocket, sem extensões de compressão.
- `tool-secrets`: as ferramentas dos canais do hermes, os domínios conhecidos delas e o falso no formato de cada canal.
- `harness-state-import`: o pareamento de canais e as tarefas agendadas ficam de fora de vez.
- `session-daemon`: a sessão do gateway aguarda o cofre depois de um reboot, mesmo sem `pass-env`.

## Impact

- **`sandmux-net`:** `mitm.rs` (upgrade e WebSocket) e `inject.rs` (troca no caminho e nas mensagens).
- **`sandmux-policy`:** `presets.rs` (ferramentas e domínios), `toolsecrets.rs` (variáveis dos canais) e `compose.rs` (o `.env` do hermes com os falsos dos canais).
- **`sandmux`:**
  - `vaultcmd.rs`: domínios conhecidos e falso com formato;
  - `daemon/vault.rs`: a ferramenta do Slack pelo prefixo do valor;
  - `daemon/session.rs`: reinício, espera pelo cofre e unicidade;
  - `main.rs`: `--gateway`;
  - `doctor.rs`: o gateway do host.
- **`sandmux-proto`:** `LaunchOptions.gateway` e o protocolo 19.
- **Documentação:** `docs/vault.md`, `docs/network.md`, `docs/sessions.md`, uma página nova `docs/hermes.md` e o README.
