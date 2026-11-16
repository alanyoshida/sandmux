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

Without the Gemini key, `agy` uses the Google login ([below](#google-login)).

## Google login

The Google login token never stays in the sandbox. It goes to the vault, as `agy:access` (the access token) and `agy:refresh` (the refresh token), and the confined `agy` gets fake values with the same format (`ya29.sandmux…` and `1//sandmux…`). The gateway swaps the fake for the real one:

- the access token, in requests to any `*.googleapis.com`;
- the refresh token, in the body of the request to `oauth2.googleapis.com/token`.

The `agy` tabs decrypt `*.googleapis.com` for this. Other tabs do not decrypt those hosts because of the login, and the fake, in them, stays fake.

**Host login:** if you already signed in to `agy` outside sandmux, the import moves that login to the vault and writes the token file with the fakes to the synthetic HOME. You do not sign in again. Google does not replace the refresh token on each renewal, so the `agy` outside and the one inside use the same login without one knocking the other out.

- **Closed vault:** the vault must be open during the import. When it is closed, or there is no vault, the import brings the rest and warns. After opening the vault (`sandmux vault unlock`, or `sandmux vault init`), run `sandmux import agy --overwrite`.
- **Another account on the host:** if you sign in with another account in the host's `agy`, `sandmux import agy --overwrite` moves the new login to the vault. The fakes in the synthetic HOME stay the same.
- **Signing out on the host** may revoke the token, and then the confined `agy` stops renewing. Sign in again on the host and run `sandmux import agy --overwrite`, or sign in through sandmux (below).

**Login through sandmux:** with no login on the host, sign in from the confined `agy` itself:

1. Choose "Google OAuth". `agy` shows an address.
2. Open the address in the host's browser and authorize.
3. The page shows a code. Paste the code into `agy` (the automatic return does not reach the sandbox).

In the answer to the login and to each renewal, the gateway stores the real tokens in the vault and hands the fakes to `agy`. The vault must be open. With no vault that accepts them, the answer is refused, and no real token reaches the sandbox. Only in `agy` tabs is this endpoint captured: `gcloud` in a confined shell, which uses the same endpoint, gets the answer as it came.

**Old login:** a login made inside the sandbox before this version left the real token in the synthetic HOME. The daemon moves it to the vault by itself and leaves the fakes in the file, as it does with [keys written inside the sandbox](vault.md#keys-written-inside-the-sandbox). With the vault locked, it waits for the vault to open, and `sandmux doctor` shows the file.

**What the token allows:** the login asks for the `cloud-platform` scope, besides `aicode` and the profile ones. Since the real token goes to any `*.googleapis.com`, the agent in the `agy` tab can use the fake to act on your Google Cloud through the gateway. Each use is in the network log with the name `agy:access`. The `id_token` in the login answer, which `agy` reads to learn the account's email, reaches the sandbox as it came.

**Inference-only mode:** the inference catalog has `oauth2.googleapis.com` (the renewal) and the `agy` model hosts (`aicode.googleapis.com`, `cloudcode-pa.googleapis.com` and `daily-cloudcode-pa.googleapis.com`). The first login through sandmux needs those hosts allowed.

## Import

On the first run, sandmux asks whether to import the configuration you already have. It copies:

- from `~/.gemini/antigravity-cli`, `settings.json` and `keybindings.json`;
- from `~/.gemini/config`, the configuration shared with the app: `config.json`, `mcp_config.json`, `hooks.json`, the global rules (`AGENTS.md`, `GEMINI.md`, `skills.json`, `rules.json`, `plugins.json`, `agents.json`) and the `hooks/`, `plugins/`, `skills/`, `agents/`, `rules/` and `workflows/` folders;
- the global rules `~/.gemini/GEMINI.md` and `~/.gemini/AGENTS.md`.

Left out:

- the plugin secrets (`~/.config/antigravity/plugin-secrets.json`);
- the projects (`~/.gemini/config/projects` and `cache/projects.json`), which hold host paths and identifiers;
- the conversations, the history, the logs, the caches and the databases.

The Google login token (`antigravity-oauth-token`) is not copied as is: with the vault open, it goes to the vault, and the synthetic HOME gets the file with the fakes ([Google login](#google-login)).

A secret in what is copied, such as a key in an MCP's `env`, is swapped for a fake value, as with the other harnesses ([vault.md](vault.md)).
