# Hermes: gateway, channels and cron

> Português: [hermes.md](../hermes.md)

Hermes replies on message channels (Telegram, Discord and Slack) and runs scheduled tasks. Both live in its gateway, `hermes gateway run`. In sandmux, the gateway runs confined in a dedicated daemon session, and the channel tokens stay in the [vault](vault.md), outside the sandbox.

```bash
sandmux vault add telegram          # asks for the bot token, without echo
sandmux run hermes --gateway        # in a chosen folder, such as ~/bots
```

## The gateway session

- **Command:** `sandmux run hermes --gateway` creates a session in the daemon that runs `hermes gateway run` in the current folder (or in the one from `--workdir`). The arguments after `--` go to `gateway run`. With a terminal, `run` attaches to it as to any session; without a terminal, it only creates it and shows the identifier.
- **Name:** `hermes-gateway:<folder>` in `sandmux ls` and in the TUI.
- **One at a time:** a second `--gateway` is refused while the first is alive, restarting or waiting for vault, even in another folder. Hermes itself also locks the `gateway.lock` of the synthetic HOME.
- **Restart:** when the gateway exits without `sandmux kill`, the session shows "restarting in N s" and comes back on its own. The wait starts at 5 seconds and doubles on each consecutive exit, up to 5 minutes. A gateway that stayed alive for 10 minutes goes back to 5 seconds. `sandmux kill <id>` ends it without relaunching.
- **After a reboot:** with the vault created, the session shows "waiting for vault" and starts on its own on unlock (`sandmux vault unlock`, or the password asked for by the TUI). Without a vault, it starts right away.
- **Cron:** scheduled tasks run inside the gateway, so they follow the session. They live in hermes's synthetic HOME (`~/.hermes/cron`), which persists.
- **Host gateway:** two gateways on the same bot compete for the messages (Telegram refuses the second `getUpdates`). `run --gateway` and `sandmux doctor` warn when the host's hermes gateway is running (the process from the real `~/.hermes/gateway.pid`). Stop the host's one with `hermes gateway stop`.
- **Pairing and tasks:** the import does not bring the channel pairing or the scheduled tasks from the host's hermes. Pair the channels and create the tasks inside the sandbox.

## Channel tokens

Each token goes to the vault as a secret with the channel's tool. The agent sees a fake value, and the sandmux gateway swaps the fake for the real one in the traffic to the owner, as with the inference keys.

| Channel | Registration | Tool | Variable in the sandbox | Where the token travels |
|---|---|---|---|---|
| Telegram | `sandmux vault add telegram` | `telegram` (domain `api.telegram.org`) | `TELEGRAM_BOT_TOKEN` | in the URL path, `/bot<token>/<method>` |
| Discord | `sandmux vault add discord` | `discord` (domains `discord.com` and `gateway.discord.gg`) | `DISCORD_BOT_TOKEN` | in the REST `Authorization` and in the WebSocket IDENTIFY message |
| Slack (bot) | `sandmux vault add slack-bot --domain slack.com` | `slack` | `SLACK_BOT_TOKEN` | in the `Authorization` |
| Slack (socket mode) | `sandmux vault add slack-app --domain slack.com` | `slack-app` (the value starts with `xapp-`) | `SLACK_APP_TOKEN` | in the `Authorization` |

- **Without `--domain`:** a name that contains `telegram`, `discord` or `slack` brings the domains from the table. On Slack, the value's prefix decides between `slack` (`xoxb-`) and `slack-app` (`xapp-`).
- **In hermes:** hermes reads `~/.hermes/.env` on top of the environment. That is why the fakes also go in that run's `.env`, mounted read-only on top of the stored one.
- **Fake format:** the fake imitates the channel's token: `0000000000:sandmux-fake-...` on Telegram, `xoxb-sandmux-fake-...` and `xapp-sandmux-fake-...` on Slack. That way hermes's format checks pass.
- **Log:** the path of a request to Telegram goes to the log with the fake, never with the real one. The Discord WebSocket becomes an `https` line with status `101`, written when the connection ends.
- **Telegram webhook:** hermes's default is polling, which needs no inbound port. With a webhook, use the [port allowlist](network.md#port-allowlist) (`--port 8443@0.0.0.0`). The webhook secret (`TELEGRAM_WEBHOOK_SECRET`) stays out of the swap: hermes checks it on incoming requests, and the fake works as the secret itself.

## Limits

- **WhatsApp:** not supported in this version.
- **Fragmented messages:** the swap in the WebSocket applies per frame. A fake split between two frames of a fragmented message is not swapped. Discord sends IDENTIFY in a single frame.
- **Large frames:** a WebSocket frame above 16 MiB closes the connection.
