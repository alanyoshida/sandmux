# Antigravity CLI

> Português: [agy.md](../agy.md)

The `agy` preset runs Google's Antigravity CLI installed on your machine inside the sandbox, like opencode, hermes, Claude Code and oh-my-pi.

```sh
sandmux run agy
```

## No prompts

`agy` runs with `--dangerously-skip-permissions`, without asking for confirmation for its tools: the sandbox is the barrier. `agy`'s own blocked-command list and the deny rules you wrote still apply. `harness-asks = true` in the configuration brings the prompts back ([config.md](config.md#harness-prompts)).

## Installation

sandmux uses the host's `~/.local/bin/agy` binary, read-only. Without the binary, `run` refuses and says where it should be.

Automatic updates stay off inside the sandbox, through the `AGY_CLI_DISABLE_AUTO_UPDATE=true` variable. To update, run `sandmux update agy`, which runs `agy update` outside the sandbox ([sessions.md](sessions.md#updating-the-harness)).

## Keys

The recommended path is the Gemini API key in the vault:

```sh
sandmux vault add gemini --domain generativelanguage.googleapis.com
```

With the key in the vault, `agy` gets `GEMINI_API_KEY` with the fake value and is launched with `--settings '{"modelProvider":"gemini"}'`, which applies only to that run and does not change your `settings.json`. `agy` sends the key in the `x-goog-api-key` header to `generativelanguage.googleapis.com`, and the gateway swaps the fake for the real one. The same applies to `--pass-env GEMINI_API_KEY`, which hands the real key to the agent. The keys of the other providers with a secret in the vault also come in through variables, as in opencode and hermes.

Without the Gemini key, `agy` asks for the Google login, as it would outside sandmux. The host's login is not imported. A login made inside the sandbox stays in the synthetic HOME (`~/.gemini/antigravity-cli/antigravity-oauth-token`), with the real token, within the agent's reach. That login also uses domains that the `inferencia` network mode does not allow. Prefer the key through the vault.

## Import

On the first run, sandmux asks whether to import the configuration you already have. It copies:

- from `~/.gemini/antigravity-cli`, `settings.json` and `keybindings.json`;
- from `~/.gemini/config`, the configuration shared with the app: `config.json`, `mcp_config.json`, `hooks.json`, the global rules (`AGENTS.md`, `GEMINI.md`, `skills.json`, `rules.json`, `plugins.json`, `agents.json`) and the `hooks/`, `plugins/`, `skills/`, `agents/`, `rules/` and `workflows/` folders;
- the global rules `~/.gemini/GEMINI.md` and `~/.gemini/AGENTS.md`.

Left out:

- the Google login token (`antigravity-oauth-token`) and the plugin secrets (`~/.config/antigravity/plugin-secrets.json`);
- the projects (`~/.gemini/config/projects` and `cache/projects.json`), which hold host paths and identifiers;
- the conversations, the history, the logs, the caches and the databases.

A secret in what is copied, such as a key in an MCP's `env`, is swapped for a fake value, as with the other harnesses ([vault.md](vault.md)).
