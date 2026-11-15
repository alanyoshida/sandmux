# Claude Code

> Português: [claude.md](../claude.md)

The `claude` preset runs the [Claude Code](https://code.claude.com) installed on your machine inside the sandbox, like opencode and hermes.

```sh
sandmux run claude
```

## Installation

sandmux uses the host's native installation: the `~/.local/bin/claude` link, which points to a version in `~/.local/share/claude/versions`. Without it, `run` refuses and shows the install command:

```sh
curl -fsSL https://claude.ai/install.sh | bash
```

The installation goes in read-only, and auto-update is turned off (`DISABLE_AUTOUPDATER=1`). To update, run `sandmux update claude`, which runs `claude update` outside the sandbox ([sessions.md](sessions.md#updating-the-harness)).

## Import

On the first run, sandmux asks whether to import the configuration you already have. It copies into the synthetic HOME:

- `~/.claude.json`;
- from `~/.claude`: `settings.json`, `CLAUDE.md`, `agents/`, `commands/`, `skills/`, `hooks/`, `plugins/` and `output-styles/`.

Left out:

- `~/.claude/.credentials.json`, with your subscription token (see below);
- the conversations (`projects/`), the history and the caches.

## Subscription login

The subscription token (OAuth) never enters the sandbox. It is not imported, for two reasons:

- **Rotation:** Anthropic replaces the refresh token on each renewal. If the sandbox and the Claude Code outside the sandbox used the same login, one would log the other out.
- **Exposure:** the agent would see your account's token.

So you log in once inside sandmux, with `/login` in Claude Code:

1. Pick the subscription account. Claude Code shows an address.
2. Open the address in the host's browser and authorize.
3. The page shows a code. Paste the code into Claude Code (the automatic return to `localhost` does not reach the sandbox).

The gateway decrypts `platform.claude.com` and `api.anthropic.com` in Claude Code tabs. In the response to the login and to each renewal (`/v1/oauth/token`), the gateway:

1. stores the access token and the refresh token in the vault, as `claude:access` and `claude:refresh`;
2. gives Claude Code fakes with the same prefix (`sk-ant-oat01-sandmux…`);
3. swaps, in the following requests, the fake for the real one: in the headers to `api.anthropic.com` and in the renewal body.

The fakes are stable: after a renewal, Claude Code gets the same fakes, and the vault now holds the new real ones.

The "Console" login (pay-per-use API billing) creates an API key (`/api/oauth/claude_cli/create_api_key`). It is captured the same way, as `claude:api-key`.

**Vault:** the vault must exist and be unlocked at login and on renewals. Without a vault that accepts, the response is refused to Claude Code, and no real token reaches the sandbox. Unlock it (`sandmux vault unlock`, or the Vault section of the TUI settings) and repeat `/login`.

**Other providers' keys:** Claude Code does not get the fake keys of the vault's providers (`ANTHROPIC_API_KEY` and the like). With an `ANTHROPIC_API_KEY`, it would switch from the subscription to API billing.

## Network

`platform.claude.com` is in the inference catalog, so inference-only mode lets the renewal through. The first login needs the open network (the default) or `platform.claude.com` allowed.
