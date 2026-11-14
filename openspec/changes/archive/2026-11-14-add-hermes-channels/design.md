## Context

- **Gateway do hermes:** `hermes gateway run` roda em primeiro plano. Os canais (Telegram, Discord, Slack) e o agendador de tarefas (`cron/`) vivem nele. O hermes impede dois gateways no mesmo HOME com um `flock` em `gateway.lock`, que vale entre namespaces de PID, porque o arquivo é o mesmo.
- **Bibliotecas dos canais no hermes:**
  - **python-telegram-bot** (httpx): o token vai no caminho, `https://api.telegram.org/bot<token>/<método>`, e nos downloads, `/file/bot<token>/...`.
  - **discord.py** (aiohttp): o REST usa `Authorization: Bot <token>`, que a troca de cabeçalhos já cobre. O gateway de eventos é um WebSocket em `gateway.discord.gg`; o token vai nas mensagens de texto IDENTIFY (op 2) e RESUME (op 6), em JSON sem compressão no sentido do cliente.
  - **slack-bolt** em socket mode: os dois tokens (`xoxb-` e `xapp-`) vão em `Authorization: Bearer`. O WebSocket (`wss-primary.slack.com`) usa a URL devolvida por `apps.connections.open`, sem token.
- **Proxy decifrado (change 12):** o `mitm::serve` é HTTP/1.1 com `hyper`, sem upgrade. Um WebSocket num domínio decifrado quebraria hoje.
- **Troca (changes 13 e 25):** os valores reais chegam pelo `VaultInject` a cada conexão decifrada. A troca é feita só nos cabeçalhos, inclusive dentro do `Basic`.
- **Ferramentas (change 25):** `ToolSecret` vira variáveis e arquivos só da execução. O hermes lê o `~/.hermes/.env` por cima do ambiente, então os falsos dos canais também vão no `.env` daquela execução, como as chaves de inferência.
- **Daemon:** já existe o estado `AguardandoCofre`, usado pelas sessões com `pass-env`. O `vault_unlocked` relança essas sessões.
- **Decisões do usuário (2026-11-14):**
  - o gateway roda numa sessão própria do daemon;
  - o pareamento dos canais e o cron não são importados;
  - a verificação usa servidores falsos locais e, depois, um bot real do Telegram do usuário, com o token posto no cofre por ele.

## Goals / Non-Goals

**Goals:**
- Gateway do hermes de longa duração, com canais e cron, confinado, sobrevivendo a quedas e a reboots.
- Tokens dos três canais só no cofre, trocados no tráfego ao dono, inclusive na URL e no WebSocket.

**Non-Goals:**
- WhatsApp (fica para depois da v1).
- Importar o pareamento e o cron do host.
- Trocar o segredo do webhook do Telegram (`TELEGRAM_WEBHOOK_SECRET`): ele é conferido pelo hermes nos pedidos que chegam, e um falso quebraria a conferência. Ele fica no `.env` como valor comum, e o modo padrão é o polling.
- Trocar valores no corpo de pedidos HTTP.

## Decisions

### Sessão do gateway
- **`--gateway`:** `LaunchOptions.gateway: bool` (protocolo 19). Com ele, o comando do harness vira `hermes gateway run`, e os `args` do usuário vêm depois. Só o hermes aceita a opção.
- **Unicidade:** o daemon recusa um segundo gateway vivo ou aguardando o cofre. O `flock` do hermes continua como garantia de fundo.
- **Reinício:** no fim do processo sem `kill` do usuário, o daemon relança a sessão depois de uma espera: 5 s, dobrando até 300 s. Um gateway que viveu 10 minutos volta aos 5 s. Durante a espera, a sessão aparece como "reiniciando em N s". A alternativa, deixar encerrada como as outras, contradiz "longa duração".
- **Reboot:** na restauração, a sessão do gateway fica `AguardandoCofre` quando o cofre existe e está trancado. Sem cofre, ela sobe direto, e os canais com token em claro no `.env` funcionam como antes. O `vault_unlocked` relança a sessão sem valores de `pass-env`, quando ela não tem nenhum.
- **Gateway do host:** o daemon lê o `~/.hermes/gateway.pid` real. Se o PID está vivo e o `/proc/<pid>/cmdline` tem `gateway`, o `run --gateway` e o `doctor` avisam. Só avisa; não para o processo do usuário.

### Segredos dos canais
- **Ferramentas novas:** `telegram`, `discord`, `slack` e `slack-app`.
- **Domínios conhecidos** (`KNOWN_TOOLS`): `api.telegram.org` → `telegram`; `discord.com` → `discord`; `slack.com` → `slack` ou `slack-app`, pelo prefixo `xapp-` do valor. Essa escolha é feita no `vault add`, que conhece o valor, e no primeiro desbloqueio, para os segredos antigos.
- **Nomes conhecidos** (`known_domains`, para `vault add` sem `--domain` e para o `vault import`): `telegram` → `api.telegram.org`; `discord` → `discord.com` e `gateway.discord.gg`; `slack` → `slack.com`.
- **Variáveis:** `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN` e `SLACK_APP_TOKEN`, pelo `env_for`. No hermes, as mesmas entram no `.env` da execução, junto dos falsos de inferência.
- **Falso com formato:** no `vault add` com ferramenta de canal, o falso imita o token: `0000000000:sandmux-fake-<32>` (casa com `\d+:[A-Za-z0-9_-]{30,}`, que o hermes usa no painel), `xoxb-sandmux-fake-<32>` e `xapp-sandmux-fake-<32>`. O Discord fica com o falso comum. O falso de um segredo vindo da importação continua o da importação.

### Troca na URL
- `inject::rewrite_path(uri, list)` troca o falso pelo real no caminho, só nos segredos do domínio. A query não é tocada. O caminho do registro é calculado antes da troca e por isso guarda o falso.

### WebSocket decifrado
- **Pedido:** o `mitm::serve` reconhece `Connection: upgrade` e `Upgrade: websocket`, tira o `Sec-WebSocket-Extensions` e repassa o pedido.
- **Resposta:** com `101`, os dois lados são promovidos (`hyper::upgrade::on` no pedido e na resposta; `with_upgrades` no servidor e no cliente).
- **Sentido sandbox → destino:** um leitor de quadros WebSocket (RFC 6455) desmascara as mensagens de texto e as continuações delas, troca o falso pelo real e reemite cada quadro com o tamanho novo, mascarado com a mesma chave. Os quadros de controle e binários passam sem mudança.
- **Sentido destino → sandbox:** cópia direta.
- **Limites:** quadro de até 16 MiB; um quadro maior fecha a conexão. Um quadro com RSV1 (compressão negociada mesmo assim) passa sem troca.
- **Registro:** uma linha `https` com o caminho, o status 101 e os bytes, no fim, com o nome do segredo quando houve troca.
- **Conexão HTTP:** depois de um upgrade, a conexão com o destino deixa de servir outros pedidos. O cliente HTTP do sandbox abre outra conexão, como já faz.

## Risks / Trade-offs

- **[Dois gateways no mesmo bot]** O do host e o do sandbox disputam o `getUpdates` do Telegram, com erro 409. → Aviso no `run --gateway` e no `doctor`.
- **[Webhook do Telegram]** Com webhook, o segredo do webhook fica em claro no `.env` e a porta precisa da allowlist do change 19. → Documentado; o polling é o padrão.
- **[Mensagens grandes no WebSocket]** Um quadro muito grande é recusado. → 16 MiB cobre o Discord, que limita mensagens bem abaixo disso.
- **[Reinício sem fim]** Um gateway mal configurado reinicia para sempre. → Espera de até 5 minutos e motivo visível em `sandmux ls`.
- **[Sem cofre após reboot]** O gateway sobe sem o cofre, e os canais com token só no cofre falham na autenticação com o valor falso. → É o caso em que o cofre não existe; com cofre, a sessão espera.

## Migration Plan

- O protocolo sobe para 19. Clientes e daemons de versões diferentes seguem a regra existente (o daemon é reiniciado).
- Segredos já guardados com domínio de canal ganham as ferramentas no primeiro desbloqueio, como no change 25. O falso deles não muda.

## Open Questions

- Nenhuma.

## Achados da implementação

- **Python 3.13+ e o certificado do proxy:** o `ssl.create_default_context()` do Python 3.13+ liga o `VERIFY_X509_STRICT` e recusa um certificado sem o Authority Key Identifier. O certificado por domínio do proxy decifrado (change 12) não tinha essa extensão, o que quebraria o próprio hermes (httpx) em qualquer domínio decifrado. Ele passou a levar o Authority Key Identifier, o `keyUsage` `digitalSignature` e o `extendedKeyUsage` `serverAuth`.
- **Fim do túnel do WebSocket:** o `shutdown` do lado do sandbox pelo ipstack só manda o FIN sem dados pendentes de ACK e pode não terminar sozinho. Ele é repetido em tentativas curtas por até 2 segundos, como no caminho TCP comum, e o túnel dá 5 segundos ao outro sentido depois que um termina.
- **Aviso do gateway do host:** o `gateway.pid` do hermes é um JSON com `pid`. O aviso exige um argumento exatamente `gateway` na linha de comando do processo, para não confundir com outro processo que só cite a palavra.
