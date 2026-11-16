# Hermes: gateway, canais e cron

O hermes responde em canais de mensagem (Telegram, Discord e Slack) e roda tarefas agendadas. As duas coisas vivem no gateway dele, o `hermes gateway run`. No sandmux, o gateway roda confinado numa sessão própria do daemon, e os tokens dos canais ficam no [cofre](vault.md), fora do sandbox.

```bash
sandmux vault add telegram          # pede o token do bot, sem eco
sandmux run hermes --gateway        # numa pasta escolhida, como ~/bots
```

## A sessão do gateway

- **Comando:** `sandmux run hermes --gateway` cria no daemon uma sessão que roda `hermes gateway run` na pasta atual (ou na de `--workdir`). Os argumentos depois de `--` vão para o `gateway run`. Com terminal, o `run` anexa a ela como a qualquer sessão; sem terminal, só cria e mostra o identificador.
- **Nome:** `hermes-gateway:<pasta>` em `sandmux ls` e na TUI.
- **Um por vez:** um segundo `--gateway` é recusado enquanto o primeiro está vivo, reiniciando ou aguardando o cofre, mesmo noutra pasta. O próprio hermes também trava o `gateway.lock` do HOME sintético.
- **Reinício:** quando o gateway termina sem `sandmux kill`, a sessão fica "reiniciando em N s" e volta sozinha. A espera começa em 5 segundos e dobra a cada saída seguida, até 5 minutos. Um gateway que ficou vivo por 10 minutos volta aos 5 segundos. `sandmux kill <id>` encerra sem relançar.
- **Depois de um reboot:** com o cofre criado, a sessão fica "aguardando cofre" e sobe sozinha no desbloqueio (`sandmux vault unlock`, ou a senha pedida pela TUI). Sem cofre, ela sobe direto.
- **Cron:** as tarefas agendadas rodam dentro do gateway, então seguem a sessão. Elas ficam no HOME sintético do hermes (`~/.hermes/cron`), que persiste.
- **Gateway do host:** dois gateways no mesmo bot disputam as mensagens (o Telegram recusa o segundo `getUpdates`). O `run --gateway` e o `sandmux doctor` avisam quando o gateway do hermes do host está rodando (o processo do `~/.hermes/gateway.pid` real). Pare o do host com `hermes gateway stop`.
- **Pareamento e tarefas:** a importação não traz o pareamento dos canais nem as tarefas agendadas do hermes do host. Pareie os canais e crie as tarefas dentro do sandbox.

## Tokens dos canais

Cada token vai para o cofre como um segredo com a ferramenta do canal. O agente vê um valor falso, e o gateway do sandmux troca o falso pelo real no tráfego ao dono, como nas chaves de inferência.

| Canal | Cadastro | Ferramenta | Variável no sandbox | Onde o token viaja |
|---|---|---|---|---|
| Telegram | `sandmux vault add telegram` | `telegram` (domínio `api.telegram.org`) | `TELEGRAM_BOT_TOKEN` | no caminho da URL, `/bot<token>/<método>` |
| Discord | `sandmux vault add discord` | `discord` (domínios `discord.com` e `gateway.discord.gg`) | `DISCORD_BOT_TOKEN` | no `Authorization` do REST e na mensagem IDENTIFY do WebSocket |
| Slack (bot) | `sandmux vault add slack-bot --domain slack.com` | `slack` | `SLACK_BOT_TOKEN` | no `Authorization` |
| Slack (socket mode) | `sandmux vault add slack-app --domain slack.com` | `slack-app` (o valor começa com `xapp-`) | `SLACK_APP_TOKEN` | no `Authorization` |

- **Sem `--domain`:** um nome que contém `telegram`, `discord` ou `slack` traz os domínios da tabela. No Slack, o prefixo do valor decide entre `slack` (`xoxb-`) e `slack-app` (`xapp-`).
- **No hermes:** o hermes lê o `~/.hermes/.env` por cima do ambiente. Por isso os falsos vão também no `.env` daquela execução, montado em somente leitura por cima do guardado.
- **Formato do falso:** o falso imita o token do canal: `0000000000:sandmux-fake-...` no Telegram, `xoxb-sandmux-fake-...` e `xapp-sandmux-fake-...` no Slack. Assim as conferências de formato do hermes passam.
- **Registro:** o caminho de um pedido ao Telegram vai para o registro com o falso, nunca com o real. O WebSocket do Discord vira uma linha `https` com o status `101`, gravada quando a conexão termina.
- **Webhook do Telegram:** o padrão do hermes é o polling, que não precisa de porta de entrada. Com webhook, use a [allowlist de portas](network.md#allowlist-de-portas) (`--port 8443@0.0.0.0`). O segredo do webhook (`TELEGRAM_WEBHOOK_SECRET`) fica fora da troca: o hermes o confere nos pedidos que chegam, e o falso funciona como o próprio segredo.

## Sem perguntas

O hermes roda com `HERMES_YOLO_MODE=1`, sem pedir aprovação para comandos perigosos; os bloqueios fixos dele continuam, e o sandbox é a barreira. `harness-asks = true` na configuração devolve as aprovações ([config.md](config.md#perguntas-do-harness)).

## Limites

- **WhatsApp:** não é suportado nesta versão.
- **Mensagens fragmentadas:** a troca no WebSocket vale por quadro. Um falso partido entre dois quadros de uma mensagem fragmentada não é trocado. O Discord manda o IDENTIFY num quadro só.
- **Quadros grandes:** um quadro de WebSocket acima de 16 MiB fecha a conexão.
