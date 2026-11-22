# Secrets vault

> Português: [vault.md](../vault.md)

The vault keeps secrets outside the sandbox: API keys, tokens and the sessions' `--pass-env` values. It lives in a file encrypted with your password, and the daemon only opens it in memory after you type the password. The agent only sees fake values; the gateway swaps the fake for the real value in the traffic, and only in requests to the secret's owner.

## Creating and unlocking

```bash
sandmux vault init      # creates the vault, asking for the password twice
sandmux vault status    # exists? unlocked? how many secrets
sandmux vault unlock    # unlocks it in the daemon
sandmux vault lock      # locks it and wipes the secrets from the daemon's memory
sandmux vault passwd    # changes the password
```

The daemon starts at login with the vault locked. To unlock it:

- **In the TUI:** when it opens, it shows a modal asking for the password, without showing what is typed. Enter unlocks; Esc leaves it locked.
- **Outside it:** `sandmux vault unlock` asks for the password in the terminal, without echo.
- **Through the system keyring:** optional; see [Keyring](#keyring).

Without a terminal (in a script), the commands read the password and the values from standard input, one line per question.

### Keyring

The system keyring can hold the vault password, so the daemon opens the vault by itself. It works with any Secret Service keyring (`org.freedesktop.secrets`): gnome-keyring, KWallet, KeePassXC. It comes off.

```bash
sandmux vault keyring on    # asks for the password, checks it against the vault and stores it in the keyring
sandmux vault keyring       # on or off, and, when on, whether the keyring answers
sandmux vault keyring off   # deletes the password from the keyring
```

- **Only the password** goes to the keyring. The vault stays in the encrypted file, and the secrets never go through the keyring.
- **When the daemon uses it:** at startup, before serving the first client, and when a client asks for the state of the locked vault (the TUI does this when it opens), at most every 5 seconds. Opening through the keyring has the effect of `vault unlock`: the tabs waiting for the vault come up. If the keyring does not answer in 3 seconds, does not have the password or has a password that no longer opens the vault, the vault stays locked and the password is asked for as before. The daemon log says why, without the password.
- **When off,** sandmux does not query the keyring. The "on" marker is the `vault.keyring` file in the data folder, with nothing secret in it. In the keyring, the item has the service `sandmux` and the user equal to the data folder.
- **Password change:** `sandmux vault passwd` rewrites the password in the keyring when it is on.
- **The cost:** with the keyring on, any program of yours outside the sandbox can read the password while the session keyring is open. Without it, the password only exists in your head and in the daemon's memory. The sandbox does not reach D-Bus, so the agent cannot get to the keyring.
- **Before login,** the user's keyring is closed, and the vault does not open by itself.

## Secrets

```bash
sandmux vault add openrouter --domain openrouter.ai   # asks for the value, without echo
sandmux vault list                                    # names, domains and fake values
sandmux vault set openrouter                          # replaces only the value (expired key)
sandmux vault remove openrouter
```

Each secret has a name, the value and, optionally, the owner domains (patterns like those of the [network](network.md)), which say where the value may go once the swap in the traffic exists. With the vault unlocked, the gateway [decrypts the TLS](network.md#tls-decryption) of those domains and logs the method and the path of each request. The value never goes in a command-line argument, because arguments are visible to other processes.

### Replacing a key that expired

`sandmux vault set <name>` asks only for the new value, without echo. The fake, the domains and the tools stay the same, so no file of the harness needs to change, including the ones the import or `doctor --fix` wrote with the fake. The gateway asks the daemon for the secrets on each connection, and the new value applies on the next connection, without restarting the tab (an already open connection keeps the old value until it closes; Ctrl+b `r` forces it).

Storing again with `sandmux vault add` and the same name also replaces the value, but creates a new fake: a harness file with the old fake stops working. An SSH key is replaced by storing the new key (`sandmux vault add-ssh`), and a database connection, with `sandmux db add`.

## From the TUI

The Vault section of the settings page (Ctrl+b `e`) shows the vault state and the list of secrets, with the name, the kind (token or SSH) and the domains. Values are never shown. Tab or → enters the list, and the keys do the same as the subcommands:

| Key | Action |
|---|---|
| `n` | Creates the vault (password and confirmation) |
| `u` / `l` | Unlocks / locks |
| `p` | Changes the password |
| `k` | Turns the [keyring](#keyring) on, asking for the password, or off |
| `a` | Adds a secret: name, value and comma-separated domains (`localhost` becomes `host.sandmux.internal`) |
| `s` | Adds an SSH key: name, file, passphrase (empty if none) and servers |
| `v` | Replaces only the value of the chosen secret, keeping the fake and the domains (a key that expired) |
| `d` | Changes the domains of the chosen secret, without asking for the value again |
| `x` | Removes the chosen secret, after `y` to confirm |
| `r` | Reloads the vault |
| `g` | Opens the [vault graph](#vault-graph) |

Passwords and values show as `•` and are wiped from the TUI memory after being sent to the daemon. Database connections, which the graph only shows, and key import stay on the command line.

### Vault graph

In the Vault section, `g` opens the vault graph in full screen. It shows, starting from a domain, the secrets that can go to it, and starting from a secret, all its owner domains. Values are never shown.

- **On the left, the list:** one item for each owner domain, with the number of secrets. Each domain text is an item: `github.com` and `*.github.com` stay separate, because they match different requests. At the end come "No domain", with the secrets that have no owner (their value is not swapped in traffic), and "Databases", with the [database connections](#database-connections). Each of these groups only shows up when it has an item.
- **In the middle, the graph:** the chosen domain in the center and its secrets around it, each one linked by an edge. The mark tells the kind: `●` token, `◆` SSH key, `■` database connection, `▲` cloud identity. A secret with other owners gets `+N` after the name (N is the number of other owners) and also shows up in the graph of each of them. With more than 8 secrets, they split into rings: 8 in the first, 12 in the second, 16 in the third.
- **On the right, the details** of the focused secret: the kind, the fake value (or the fingerprint of an SSH key), the tools and all the owner domains. For a database connection: the kind, the server, the port, the database and the user, without the password.

**Zoom:** the terminal text does not change size. The zoom, from 0.5x to 2.0x, changes the distance between the nodes and how much each one shows: below 0.7x, only the mark; up to 1.3x, the mark and the name; from there on, a card with the name, the kind and the tools. The footer shows the current zoom.

| Key | Action |
|---|---|
| ↑ ↓ | Pick the domain; with the focus on a secret, the previous and the next one |
| Tab or → | Move the focus to the domain's secrets; Tab again goes to the next one |
| ← | Gives the focus back to the list |
| Enter | Opens and closes the details, when they do not fit on the side |
| `o` | Goes to the next owner of the focused secret, keeping it focused |
| `+` `-` | Zoom |
| `h` `j` `k` `l` | Move the graph left, down, up and right |
| `0` | Back to the center and to 1.0x zoom |
| `/` | Filters the list by the name of the domain or of a secret (Enter keeps it, Esc clears it) |
| `a` `s` | Add a secret or an SSH key, with the chosen domain already filled in |
| `d` `v` `x` | Change the domains, replace the value or remove the focused secret |
| `u` | Unlocks the vault, when it is locked |
| `r` | Reloads the vault |
| Esc | Clears the filter, gives the focus back to the list or goes back to the Vault section |

- **Mouse:** the wheel over the graph changes the zoom, dragging moves the graph, a click on the list picks the domain and a click on a node puts the focus on it.
- **Editing:** the forms are the ones of the Vault section, and the graph is redrawn after each change. The section's `l`, `k`, `p` and `n` keys do not work on this screen. A database connection is read-only here: it is managed with `sandmux db`.
- **Width:** with 100 columns or more, the list, the graph and the details sit side by side. From 70 to 99, the details open with Enter, in a box over the graph. With fewer than 70, the screen shows only the list.
- **Font without Braille:** the edges use Braille dots. If they show up as squares in your terminal, switch to half blocks, in "Graph edges" in the Appearance section or in the file:

```toml
[tui]
graph-edges = "blocos"
```

- **Limit:** the positions are fixed, in rings. With long names and many secrets in a domain, two names can overlap; zoom and movement separate them.

## Key swap in the traffic

Each secret has a fake value (`sandmux-fake-` and 32 characters): the one the import put in its place, or one generated by `sandmux vault add`. `sandmux vault list` shows the fake, which is what the agent sees anyway.

- **In the sandbox:** in opencode, hermes, oh-my-pi and Antigravity CLI sessions, with the vault unlocked, each inference provider with a secret in the vault enters the environment on its own with the fake value: `OPENROUTER_API_KEY` (`openrouter.ai`), `ANTHROPIC_API_KEY` (`api.anthropic.com`), `OPENAI_API_KEY` (`api.openai.com`), `GEMINI_API_KEY` (`generativelanguage.googleapis.com`), `DEEPSEEK_API_KEY` (`api.deepseek.com`), `MISTRAL_API_KEY` (`api.mistral.ai`), `GROQ_API_KEY` (`api.groq.com`) and `XAI_API_KEY` (`api.x.ai`). In opencode, the injected configuration points the provider's key to the variable; in hermes, the fake goes in that run's `~/.hermes/.env`; in the Antigravity CLI, `GEMINI_API_KEY` makes `agy` use the key instead of the Google login ([agy.md](agy.md#keys)). A `--pass-env` of the same variable is ignored, with a warning: the fake wins, and the real value does not enter.
- **In the gateway:** the domains that own the secrets are [decrypted](network.md#tls-decryption). On each decrypted connection, the gateway asks the daemon for the secrets over the private socket and swaps, in the headers, the fake for the real value, only when the request's domain owns the secret. The fake sent to another destination stays fake. The body is never changed.
- **Locked vault:** nothing is swapped; requests go out with the fake and the provider refuses them. Unlock and try again.
- **Captured logins:** the tokens of the logins the gateway captures (`claude:*`, from Claude Code, and `agy:*`, from the Antigravity CLI) apply only in the tabs of their harness: the other tabs do not decrypt those domains because of them, and the fake stays fake ([claude.md](claude.md), [agy.md](agy.md#google-login)).
- **Log:** the `https` line of a request with a swap has the name of the secret used (`key: openrouter` in `sandmux net` and in the panel), never the value.
- **Inference-only mode:** with the vault unlocked and some secret for a provider in the catalog, the `inferencia` mode allows only the providers with a secret.

To use OpenRouter without the key in the sandbox:

```bash
sandmux vault add openrouter --domain openrouter.ai
sandmux run opencode
```

## Tools

A secret can serve development tools, `gh`, `git`, `npm`, `pip`, `cargo` and `docker`, and hermes channels, `telegram`, `discord`, `slack` and `slack-app` ([hermes.md](hermes.md#channel-tokens)). With the vault unlocked, each session gets the fake value already in the tool's configuration, and the gateway swaps the fake for the real value in the request to the owner domain, as with the inference keys.

```bash
sandmux vault add github --domain github.com                          # gh and git, by the domain
sandmux vault add npm-empresa --domain npm.empresa.com --tool npm
sandmux vault add registry --domain registry.empresa.com --tool docker
```

Without `--tool`, the known domains decide: `github.com` leads to `gh` and `git`; `registry.npmjs.org` to `npm`; `pypi.org` to `pip`; `crates.io` to `cargo`; `ghcr.io`, `registry-1.docker.io` and `docker.io` to `docker`; `api.telegram.org` to `telegram`; `discord.com` to `discord`; and `slack.com` to `slack-app` when the value starts with `xapp-`, or to `slack` otherwise. Without `--domain`, a name containing `telegram`, `discord` or `slack` brings the channel's domains. `sandmux vault list` shows each secret's tools. A secret registered earlier gets the tools of the known domains at the first unlock.

What enters each session, for each exact domain of the secret (a wildcard such as `*.empresa.com` has no host to write in the configuration; the swap in the traffic applies to it, but use the exact domain for the tool):

| Tool | In the sandbox |
|---|---|
| `gh` | `GH_TOKEN` with the fake |
| `git` | a credential helper for `https://<domain>` through the variables `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` and `GIT_CONFIG_VALUE_n`, which answers user `sandmux` and the fake. The variables come before `.gitconfig` |
| `npm` | a `~/.npmrc` for that run only: the one stored in the synthetic HOME plus `//<domain>/:_authToken=<fake>` |
| `pip` | `PIP_EXTRA_INDEX_URL=https://sandmux:<fake>@<domain>/simple`, outside `pypi.org` |
| `cargo` | `CARGO_REGISTRY_TOKEN` with the fake, for `crates.io` |
| `docker` | a `~/.docker/config.json` for that run only: the stored one plus `auths` for the domain. The [private Docker](docker.md#private-registries) swaps the fake on `docker pull` |
| `telegram`, `discord`, `slack`, `slack-app` | `TELEGRAM_BOT_TOKEN`, `DISCORD_BOT_TOKEN`, `SLACK_BOT_TOKEN` and `SLACK_APP_TOKEN` with the fake, also in that run's `~/.hermes/.env`. The fake mimics the token format: `0000000000:sandmux-fake-...`, `xoxb-sandmux-fake-...` and `xapp-sandmux-fake-...` |

The files stored in the synthetic HOME do not change: the session's `~/.npmrc` and `~/.docker/config.json` are mounted on top and go away at the end. A `docker login` inside the sandbox writes the fake to that file, and the real value never enters. A `--pass-env GH_TOKEN` with a `gh` secret in the vault is ignored, with a warning.

git over HTTPS, pip and container registries send the user and password in `Authorization: Basic` (base64 of `user:password`): the gateway opens the base64, swaps the fake and closes it again. The fake is also swapped in the URL path (Telegram carries the token in `/bot<token>/`) and in the text messages of a WebSocket from the sandbox to the owner (Discord sends the token in IDENTIFY). The body of an HTTP request is never changed.

## SSH keys

git over SSH uses the sandmux SSH agent: the private key stays in the vault, and the daemon signs with it. The key never enters the sandbox.

```bash
sandmux vault add-ssh github --file ~/.ssh/id_ed25519 --server github.com   # asks for the passphrase, if any
sandmux run opencode --ssh-key github
```

- **Registration:** `sandmux vault add-ssh` reads the OpenSSH private key (Ed25519, ECDSA P-256 or P-384, or RSA), asks for the passphrase without echo when it is encrypted, and stores the key without a passphrase, encrypted in the vault, with its servers (`--server`, exact domains, at least one). `sandmux vault list` shows `github  ssh  (github.com)  SHA256:...`, never the key. An SSH key has no fake value and does not take part in the token swap.
- **Session keys:** the `ssh-keys` key of the [configuration](config.md) (in a profile or in the project profile) and the `--ssh-key <name>` of `run` say which keys the session may use; `"*"` allows all of them. Without any, the session has no agent. A name that is not in the open vault produces a warning at launch.
- **In the sandbox:** the agent lives at `/run/sandmux/ssh-agent.sock`, with `SSH_AUTH_SOCK` pointing to it. It lists only the chosen keys and signs only with them; requests to add, remove or lock keys are refused, and RSA only signs with SHA-2 (`rsa-sha2-256` or `rsa-sha2-512`). With the vault locked, the list is empty; an unlock takes effect right away, without relaunching.
- **Log:** each signature becomes an `ssh-sign` line in the [network log](network.md#log), with the key's name and fingerprint (`ssh signature: github` in `sandmux net` and in the panel).
- **Servers:** outbound SSH only reaches the servers of the chosen keys, on any port ([network.md](network.md#how-the-gateway-decides)). In `custom` mode, the server must also be allowed.
- **`known_hosts`:** the session gets a `~/.ssh/known_hosts` for that run only, with the one stored in the synthetic HOME plus the lines of the host's `~/.ssh/known_hosts` for the servers of the chosen keys (including those with hashed names). Without those lines on the host, `ssh` asks to confirm the server's key on the first connection; connect once from the host, or confirm inside the session.

In a session with an agent, `/etc/ssh/ssh_config.d` becomes a copy of the system's `.conf` files, owned by the sandbox user: without that, `ssh` refuses the included files, which inside the sandbox appear owned by `nobody`.

Hardware keys (YubiKey) and third-party agents (such as 1Password's) are not supported: the sandmux agent only uses the keys in the vault.

## Cloud identities

The vault also stores cloud identities (`sandmux vault add-cloud gcp <name>` and `sandmux vault add-cloud kubernetes`): the agent uses `gcloud`, the Google libraries and `kubectl` with a fake token, and the real one stays outside the sandbox, with the daemon. In the TUI and in the graph, an identity shows up in its domain (`*.googleapis.com` for GCP, `<name>.kube.sandmux.internal` for Kubernetes), read-only: `x` removes it. Details in [cloud.md](cloud.md).

## Database connections

The vault also keeps PostgreSQL and MySQL connections (`sandmux db add`), with the password encrypted, for the built-in database MCP. Details in [database.md](database.md).

## Import keys

The [import](../../README.en.md) replaces each secret in a harness's files with a fake value (`sandmux-fake-...`) and records the swap in a manifest, without the real value. `sandmux vault import` walks through the manifests and, for each fake value still without a secret in the vault, shows the harness, the file, the key and the line of the imported file (with the fake as `<fake>`, so you can recognize the provider), and asks for the real value. An empty answer skips the item.

When you type or paste a secret or the vault password, each character shows up as `•`: the value never appears on the screen. Backspace deletes one character and Ctrl+U deletes everything.

### Keys written inside the sandbox

A key pasted inside the harness, as in opencode's "Connect provider" or in hermes's setup wizard, stays in plain text in the synthetic HOME, within the agent's reach.

- **Scan:** `sandmux doctor` looks for real credentials in the configuration files (JSON, YAML, `.env`) that the import copies, in the opencode and hermes synthetic HOMEs, with the same rules as the import. It lists each one (harness, file, provider, key and line), without the value.
- **`sandmux doctor --fix`:** moves those keys to the vault, with the domain discovered as above. The file is only rewritten with fake values after the vault accepts all of its keys; the real value does not stay in any file of the synthetic HOME.
- **From the TUI:** the Diagnosis section of the settings (Ctrl+b `D`) does the same with the `f` key. Each key's domain comes in a field you can edit. When nothing points to the domain, the field suggests the host of the URL closest to the key in the file ([tui.md](tui.md#diagnosis)).
- **Afterwards:** restart the harness's tabs, which may have the old key in memory.
- **Antigravity CLI login:** `doctor` also finds the real token of a Google login made inside the sandbox (`~/.gemini/antigravity-cli/antigravity-oauth-token` in the `agy` HOME), and `--fix` moves it to the vault as `agy:access` and `agy:refresh` ([agy.md](agy.md#google-login)).

#### Automatic move

The daemon does the same as `doctor --fix` by itself, without asking:

- **When:** right after the harness writes one of those files (the daemon watches their folders), at startup, every 60 seconds and right after the vault is opened.
- **How:** with the vault open and each key's domain discovered as below, each key goes to the vault with a new fake, and only then is the file rewritten with the fakes. If the vault refuses a key, or if the harness rewrote the file during the scan, the file stays as it was. The daemon log has the harness, the file and the key names, never the values.
- **Notices:** the harness's tab shows how many keys were moved since the launch, and asks you to restart the tab, because the harness still has the real one in memory. It also shows how many are waiting for the vault (locked or missing) and how many are waiting for a domain. Those waiting for the vault go when it is opened. Those without a domain are left for `sandmux doctor --fix` or the TUI's Diagnosis section, which ask. The notices show up in `sandmux ls`, in the tab's `✱` mark, in the bar alerts and in the "real keys" line of the TUI panel ([tui.md](tui.md)).

Without a vault, nothing swaps the fake for the real value, and the harness gets "invalid key" from the provider. That is why the import report and `sandmux doctor` remind you of `sandmux vault init` and `sandmux vault import`. The secret is named `<harness>:<file>:<provider>.<key>` (or `<harness>:<file>:<key>`, when there is no provider around it), linked to the fake value and, when the key's name indicates the provider (such as `OPENROUTER_API_KEY`), to its domain. After that, the harness uses the imported file as is, and the gateway swaps its fake for the real value. In hermes, which reads `.env` on top of the environment, this is how the imported key starts to apply.

Each secret is linked to the domains where it may go: the gateway only swaps the fake for the real value in a request to one of them. That way, a tricked agent that sends the fake to another site sends only the fake. The command discovers the domain on its own, in this order:

1. **Key name:** `OPENROUTER_API_KEY` becomes `openrouter.ai`.
2. **Provider around the line:** the `"openrouter": {` in opencode's `auth.json` becomes `openrouter.ai`.
3. **The provider's `baseURL` in the opencode configuration:** `http://127.0.0.1:8888/v1` becomes `host.sandmux.internal` (the name of your machine inside the sandbox), and `https://llm.empresa.com/v1` becomes `llm.empresa.com`.

Only when none of that points to the owner does the command ask, after the value. The answer can be `localhost`, `127.0.0.1` or `::1` for a server on your machine, which become `host.sandmux.internal`. The same goes for `sandmux vault add --domain localhost`. When storing, it states the domain, or warns that the secret was left without a domain. An empty answer stores the secret without a domain, and then there is no swap. For a local model server reached through the [host ports](network.md#host-ports) (LM Studio, Unsloth Studio, Ollama with a key), the domain is `host.sandmux.internal`.

## `--pass-env` and restore

With the vault unlocked, a session's `--pass-env` values go encrypted to the vault. After a reboot (or `sandmux daemon restart`), the session shows up as **waiting for vault** in `sandmux ls` and on the TUI bar (`(vault)` next to the name), and starts on its own, with the same values, when the vault is unlocked. `sandmux kill` removes the session's values from the vault.

With the vault missing or locked at `run`, nothing is stored: `run` warns, and on a reboot the session stays stopped until a `sandmux restart` in a terminal with the variables, as before the vault.

## How it is stored

- **File:** `~/.local/share/sandmux/vault.json` (in the data directory), mode 600, written atomically.
- **Cipher:** the key comes from the password through Argon2id (64 MiB, 3 passes), with a random salt; the content is encrypted with XChaCha20-Poly1305, with a new nonce on each write and the header as associated data. Nothing of the content stays in plain text: not the values, not the secrets' names, not the sessions.
- **Memory:** while unlocked, the daemon keeps the derived key and the content, and wipes them on lock. The daemon process is marked as non-dumpable, so that neither `/proc/<pid>/mem` nor a core dump hand over the secrets.
- **Forgotten password:** there is no way to recover it. Delete the file and create another vault.

## Limits

- The swap happens only in the headers of decrypted HTTP/1.1 requests, including inside `Authorization: Basic`. Providers that ask for the key in the body, or programs that pin the server's certificate, are not supported.
- The `--pass-env` of a variable without a secret in the vault still puts the real value inside the sandbox.
- A process of the same user, outside the sandbox, with the privilege to read other processes' memory (such as a debugger with `CAP_SYS_PTRACE`), can reach the open vault. The threat sandmux targets is the agent inside the sandbox.
- The system keyring is left for after v1.
- OAuth, token refresh and registries that require a client certificate are not supported.
- While the session lives and the vault is open, the agent can sign as many times as it wants with the chosen keys; each signature stays in the log.
- SSH keys do not reach the private Docker's containers.
