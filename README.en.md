# sandmux

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/alanyoshida/sandmux)](https://github.com/alanyoshida/sandmux/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

> Português: [README.md](README.md)

Terminal multiplexer for running coding agents inside a security sandbox.

sandmux starts agent harnesses (opencode, hermes) in tabs, tmux style. Each tab runs confined: the agent sees the work folder and nothing else from your HOME, network traffic is logged and can be blocked, and no real credential enters the sandbox.

> **Status: in development, before v1.** These exist and have tests: the confinement, the session daemon, the TUI with tabs and panel, the configuration with profiles, the resource caps, the private Docker per folder, the network through the gateway with logging and domain filtering, the published ports, the file access log, the secret vault with the swapping of keys and tokens in the traffic, and the git SSH agent. Stages 7 (MCP and database) and 8 (full hermes: inbound ports, gateway, channels and cron) are complete; stage 9 (assistant: chat tab and command modal) as well. Stage 10 brings the installer, the packages and the first release. The section "What already works" describes what exists; the rest of the document describes what was decided for v1.

## Why

An agent harness today runs with the same access as you: it reads `~/.ssh`, cloud credentials, tokens from other programs and any folder on the disk. An agent manipulated by prompt injection can read these secrets and send them out.

sandmux starts from one principle: **secure by default, with minimal friction**. A secure baseline comes built in and works without configuring anything. You only manage the exceptions.

## How it works

You enter the project folder and start the agent:

```bash
cd ~/projetos/app
sandmux run opencode
```

The folder the command was run from is mounted inside the sandbox at the same path, with read and write access. It is the real folder, not a copy: the code the agent creates shows up on the host immediately, and you can open it in the editor while it works.

```
host                                  sandbox
~/projetos/app/   <-- same folder --> ~/projetos/app/    read and write
~/ (rest)                             ~/ (synthetic HOME, separate)
/usr, /etc, ...   ------------------> read-only
```

What the agent sees:

| Resource | Inside the sandbox |
|---|---|
| Work folder | The real folder, with read and write access. `.git/hooks` and `.git/config` are read-only |
| HOME | A synthetic HOME per harness. The real HOME does not exist in there |
| System (`/usr`, `/etc`) | Read-only, with the sensitive files covered |
| Toolchains (cargo, nvm, uv, go and others) | Read from the host, written to a private layer. `cargo install` and `npm install -g` work without changing the host |
| Host sockets (ssh-agent, D-Bus, Docker, Wayland, X11) | Out of reach |
| Host processes | Invisible |
| Environment variables | Only a safe list; the rest is not inherited |

Running the command directly in the HOME, in the root or in a folder that contains the HOME is refused. It has to be a subfolder.

## What already works

### Installation

```bash
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

The script asks for `sudo` to install into `/usr/local/bin` and the launcher's AppArmor profile. Without `sudo` (refused or with `--no-sudo`), it installs into `~/.local/bin`. The release also has `.deb`, `.rpm`, the tarball, the AUR `PKGBUILD` and the Homebrew formula. From source:

```bash
cargo install --git https://github.com/alanyoshida/sandmux sandmux
```

After installing from a package or with `cargo install`, run:

```bash
sandmux setup
```

`setup` installs the launcher's AppArmor profile when the distro restricts user namespaces (Ubuntu, by default), asking for `sudo`. It also enables the daemon and finishes with `sandmux doctor`, which checks the kernel (Landlock ABI 6 or newer), the user namespaces, the write layer, the resource caps and the network. Details in [docs/en/install.md](docs/en/install.md).

### Commands

| Command | What it does |
|---|---|
| `sandmux` (or `sandmux tui`) | Opens the interface with tabs, one per session, with the bar in colored blocks at the bottom: switch with Ctrl+b n/p/1–9, new tab with Ctrl+b c, list with Ctrl+b w, menu with Ctrl+b `:`, settings with Ctrl+b `e` (toggles and themes such as Catppuccin, Rosé Pine, Tokyo Night, Kanagawa, Gruvbox and Dracula, written to `config.toml`), the `doctor` diagnosis with Ctrl+b `D`, quit with Ctrl+b d ([docs/en/tui.md](docs/en/tui.md)) |
| `sandmux run <harness>` | Starts the harness confined in the current folder. Harnesses: `opencode`, `hermes`, `claude` (Claude Code, with the subscription login kept outside the sandbox; [docs/en/claude.md](docs/en/claude.md)) `omp` (oh-my-pi; [docs/en/omp.md](docs/en/omp.md)), `agy` (Google's Antigravity CLI, with the Google login kept outside the sandbox; [docs/en/agy.md](docs/en/agy.md)) and `shell` (a confined shell, without an agent). On the first run, asks whether to import the configuration you already have. In a terminal, creates a session in the daemon and attaches to it; without a terminal (script, pipe, CI), runs directly, in the foreground, without the daemon . The harnesses run without permission prompts, because the sandbox is the barrier; `harness-asks = true` brings the prompts back ([docs/en/config.md](docs/en/config.md#harness-prompts)) |
| `sandmux ls` | Lists the sessions: identifier, name, group, state (alive, ended with the exit code, stopped with the reason), attached terminals and folder |
| `sandmux attach [id]` | Attaches this terminal to a session. Without an identifier, to the only live session |
| `sandmux group list\|move\|rename` | Tab groups, like tmux sessions: each TUI shows one group, and `sandmux run --group <group>` creates the session in it ([docs/en/sessions.md](docs/en/sessions.md#tab-groups)) |
| `sandmux enter [id]` | Opens a shell inside the sandbox of a live session, with what the agent sees (Ctrl+b `t` in the TUI; [docs/en/sessions.md](docs/en/sessions.md#shell-inside-a-tabs-sandbox)) |
| `sandmux detach <id>` | Detaches all terminals from a session, without ending it |
| `sandmux kill <id>` | Ends the session and removes it from the list |
| `sandmux rename <id> <name>` | Changes the name of a session (the tab name) |
| `sandmux stats <id>` | Shows the session's memory and processes: in use, peaks, caps, CPU, and what the cap blocked |
| `sandmux restart <id>` | Relaunches an ended or stopped session, in the same folder and with the same options; `--force` also restarts a live one (Ctrl+b `r` in the TUI, with confirmation) |
| `sandmux daemon status`, `stop`, `restart`, `disable`, `boot` | Daemon state; stop (live sessions come back on the next start); restart with the installed binary; stop and remove the systemd unit; start at boot, without a login, with the tabs coming back at the first login ([docs/en/sessions.md](docs/en/sessions.md#start-at-boot)) |
| `sandmux config show`, `list`, `set`, `unset`, `add`, `remove` | Shows the effective policy with the origin of each value, lists the profiles and changes the configuration file (`~/.config/sandmux/config.toml`) |
| `sandmux import <harness>` | Copies the harness's existing configuration into the synthetic HOME, replacing the secrets with fake values. Run again, it copies only what is new; `--overwrite` replaces. For `shell`, it brings the `.bashrc`, `.zshrc` and the like of the shell in use, with its framework ([docs/en/sessions.md](docs/en/sessions.md#shell-configuration)) |
| `sandmux update <harness>` | Runs the harness's official updater on the host, outside the sandbox, and says which tabs to restart ([docs/en/sessions.md](docs/en/sessions.md#updating-the-harness)) |
| `sandmux doctor --fix` | Moves into the vault the real keys written inside the sandbox (opencode's "Connect provider", for example), leaving fake values in their place |
| `sandmux doctor` | Shows whether the machine has the required support and what is missing; in the TUI, the Diagnosis section of the settings (Ctrl+b `D`) shows the same and moves the real keys to the vault |

`run` options:

| Option | Effect |
|---|---|
| `--workdir <folder>` | Uses another work folder instead of the current folder |
| `--ro <path>`, `--rw <path>` | Adds a host path read-only or writable. Mounting a known credential (`~/.kube`, `~/.aws`) gives a warning; write access to paths the host runs (`~/.bashrc`, `~/.local/bin`) is refused ([docs/en/config.md](docs/en/config.md#paths-the-host-runs)) |
| `--pass-env <NAME>` | Passes a variable from your environment inside, with a warning. For API keys and tokens, prefer the vault, which hands over only a fake value |
| `--ssh-key <NAME>` | Lets the session's SSH agent use the vault SSH key with that name (`*` for all of them) |
| `--import`, `--no-import` | Answers the first-run question without an interactive terminal |
| `--profile <name>` | Applies a named profile from the configuration file |
| `--repo`, `--no-repo` | In a repository subfolder, mounts (or not) the whole repository read-only so git works |
| `--kvm`, `--no-kvm` | Allows (or not) `/dev/kvm` |
| `--gpu`, `--no-gpu` | Exposes (or not) the GPU, exposed by default when there is one |
| `--window`, `--window-direct`, `--no-window` | Lets (or not) the agent open windows on the desktop and use audio: through the compositor's restricted socket, with the privileged protocols hidden, or through the plain socket; never X11 ([docs/en/window.md](docs/en/window.md)) |
| `--no-portal` | Removes the desktop portal, on by default: theme, notifications and opening links in your browser only for domains the tab's network allows, through a D-Bus of sandmux's own ([docs/en/window.md](docs/en/window.md#desktop-portal)) |
| `--memory <cap>`, `--processes <cap>` | Changes the memory cap (`50%`, `16G`, `max`) or the process cap (`512`, `max`) |
| `--network <mode>` | `aberta` (default), `inferencia`, `custom` or `sem-rede` |
| `--deny <pattern>`, `--allow <pattern>` | Denies or allows (in `custom` mode) a destination in this tab: `exemplo.com`, `*.exemplo.com`, IPv4 or CIDR |
| `--allow-clipboard`, `--no-allow-clipboard` | Lets (or not) the harness write to your terminal's clipboard (OSC 52). By default writing is blocked, and reading the clipboard is always blocked |
| `-- <arguments>` | Arguments passed on to the harness |

Examples:

```bash
sandmux run shell -- -c "id -u; ls ~"
```

```bash
sandmux run opencode --pass-env OPENROUTER_API_KEY
```

The negative options exist to undo what a profile turned on.

### Configuration and profiles

The `~/.config/sandmux/config.toml` file stores the exceptions you want to repeat: global settings (`[global]`), named profiles (`[profile.<name>]`, with `--profile`) and project profiles (`[project."<folder>"]`), which apply on their own in the folder and its subfolders. The keys are the same as the `run` options: `ro`, `rw`, `pass-env` (names only), `repo`, `kvm`, `gpu`, `clipboard`, `memory`, `processes` and `network`. The most specific layer wins, and the `run` options win over all of them. The agent can never write to this file, and nothing inside the work folder changes the policy. Details in [docs/en/config.md](docs/en/config.md).

```bash
sandmux config set kvm true --profile vm
```

### Private Docker

Each work folder has its own rootless Docker, which the agent uses from inside the session with `docker` and `docker compose`. The containers only see what the sandbox sees: the work folder and the system, never the HOME. Published ports stay only on the host's `127.0.0.1`, and from inside the sandbox they are reached through `host.sandmux.internal`. The containers' traffic goes through the same gateway as the sandbox, with the log and the most restrictive policy among the folder's tabs. With the `docker-registry` package installed, Docker Hub images go into a cache shared across folders. The Docker starts on the first `docker` and stops with the folder's last session. Images and volumes persist per folder. The host's Docker socket never enters. With the root helper installed (`sandmux setup`) and the BPF LSM active in the kernel, the kernel itself denies containers the folder's secrets and writing to `.git`, even with `--privileged`. Details in [docs/en/docker.md](docs/en/docker.md) and [docs/en/helper.md](docs/en/helper.md).

### Network

Each sandbox has its own network, and all traffic goes through a gateway that remakes each connection from the host and logs the destination: domain, port, bytes and duration, never the content. Cloud metadata, QUIC and ICMP are blocked. Your machine's services are at `host.sandmux.internal`. For a harness configured with a local service (LM Studio at `127.0.0.1:8888`, a local MCP), `host-ports` brings the host port to the sandbox's `127.0.0.1`, without editing the configuration; that server's key stays in the vault, and in opencode the local server's models show up on their own.

| Mode (`--network`) | What gets through |
|---|---|
| `aberta` (default) | everything, except the denylist, which already comes with a catalog of exfiltration destinations (paste sites, file upload, tunnels) |
| `inferencia` | only the inference APIs (OpenRouter, Anthropic, OpenAI and others) |
| `custom` | only the allowlist, built with `allow` and groups of common domains (github, npm, pypi…) |
| `sem-rede` | nothing |

A new rule applies on the session's next connection, without relaunching the agent. On the domains that own the vault's secrets (and on those of the `decrypt` key), the gateway decrypts TLS with a CA of its own for each launch and logs the method and path of each request. A server the agent starts (such as `npm run dev` on port 3000) is published on its own on the host's `127.0.0.1`, and the panel shows the port. The `ports` key replaces this with an allowlist, and a port such as `8443@0.0.0.0` receives webhooks from outside, with every connection in the log. `sandmux net` shows the log and changes the tab's rules; in the TUI, the panel shows the latest connections and Ctrl+b `a` opens the network screen, which denies and allows straight from the accesses. Details in [docs/en/network.md](docs/en/network.md).

```bash
sandmux net 1
```

### Files

Every file open by the agent goes through the sandbox supervisor (seccomp with notification): it opens the file inside the sandbox root, checks the Landlock rules and hands over the descriptor, without the race of swapping the path after the check. Accesses to the agent's folders (work folder, synthetic HOME and extra folders) go to a log, and `sandmux files` shows those of a session. Secret-looking files in the work folder (`.env`, private keys, `.npmrc` and the like) do not open for the agent, except the ones it creates itself; `file-deny`, `sandmux files deny` and the Ctrl+b `f` screen deny files on the spot; and a `.git/commondir` created by the agent is denied. A new folder enters the live session at `/mnt/sandmux/<name>` (`sandmux folder add`, or Ctrl+b `o`), without relaunching the agent. The measured cost was 1.0x to 1.3x in normal use. Details in [docs/en/files.md](docs/en/files.md).

### MCPs

sandmux lists the MCP servers of each harness (imported ones are enabled) and enables or disables each one as a whole: `mcp-deny` and `mcp-allow` in the configuration, `sandmux mcp deny|allow|forget` in the tab, or Ctrl+b `m` in the TUI. In opencode, the disabled ones go in `OPENCODE_CONFIG_CONTENT`; in hermes, in a `config.yaml` for that run only. Details in [docs/en/mcp.md](docs/en/mcp.md).

### Database

The built-in database MCP queries and changes PostgreSQL and MySQL databases registered in the vault (`sandmux db add`), without the password entering the sandbox: the session chooses the connections with `--db`, the harness receives the `sandmux-db` MCP, and each query goes to a log (`sandmux db log`). Details in [docs/en/database.md](docs/en/database.md).

### Hermes: channels and cron

`sandmux run hermes --gateway` runs the hermes gateway, with the message channels (Telegram, Discord and Slack) and the scheduled tasks, in a dedicated daemon session. The session restarts if the gateway goes down and, after a reboot, waits for the vault to be unlocked. The channel tokens stay in the vault (`sandmux vault add telegram`), and the agent only sees a fake: the sandmux gateway swaps the fake for the real one in the Telegram URL, in the Discord WebSocket and in the Slack headers. Details in [docs/en/hermes.md](docs/en/hermes.md).

### Chat with the models

Ctrl+b `i` in the TUI, or `sandmux chat "question"`, opens a chat tab with a model whose key is in the vault, written `<provider>/<model>` (the default comes from `chat-model` in `[assistant]`). The tab runs confined, with inference-only network, and the real key never enters: the gateway swaps the fake in the traffic. The conversation stays in the tab's private folder, comes back after a restart and is deleted when the tab closes. Without a terminal, `sandmux chat` prints only the answer. Ctrl+b `g` opens the command modal: the model receives only the question, the tab's folder and the shell, and the suggested command is written on the tab's line without running. Details in [docs/en/chat.md](docs/en/chat.md).

### Secret vault

`sandmux vault init` creates a password-encrypted vault (Argon2id and XChaCha20-Poly1305) for API keys and tokens, outside the sandbox. The daemon starts with it locked; the TUI asks for the password when it opens, or `sandmux vault unlock` in the terminal. Optionally, `sandmux vault keyring on` stores the password in the system keyring, and the daemon opens the vault by itself. The `--pass-env` values go into the vault, and a session that uses them comes back on its own after a reboot, as soon as the vault is unlocked. `sandmux vault import` asks for the real values of the keys that the import replaced with fake ones. Details in [docs/en/vault.md](docs/en/vault.md).

For GCP, `sandmux vault add-cloud gcp <name>` stores a cloud identity: the agent uses `gcloud` with a fake token, and the gateway puts in the real one, which never enters the sandbox. For Kubernetes, `sandmux vault add-cloud kubernetes` stores every context of the host kubeconfig: the tab gets a kubeconfig with fakes only, and the gateway connects to the API server with the real credential, including the one from `exec` plugins (EKS, GKE), which run on the host. Details in [docs/en/cloud.md](docs/en/cloud.md).

With the provider's key in the vault (`sandmux vault add openrouter --domain openrouter.ai`), an opencode or hermes session receives only a fake value; the gateway swaps the fake for the real one in the request to `openrouter.ai`, and the key never enters the sandbox. `--pass-env` is no longer needed for inference keys.

The same goes for development tools: with a GitHub token in the vault (`sandmux vault add github --domain github.com`), `gh` and `git push` over HTTPS use a fake value, and the gateway swaps it for the real one, including inside `Authorization: Basic`. Private npm, pip, cargo and Docker registries work the same way (`--tool npm|pip|cargo|docker`).

For git over SSH, the private key goes into the vault (`sandmux vault add-ssh github --file ~/.ssh/id_ed25519 --server github.com`) and the session chooses the keys it may use (`--ssh-key github`, or `ssh-keys` in a profile). The sandmux SSH agent signs inside the sandbox without the key entering it, each signature goes to the log, and outbound SSH only reaches the servers of the chosen keys.

### Memory and process caps

Each sandbox runs with a cap of 75% of RAM, without swap, and of 8192 processes and threads. Whatever goes over the cap is killed or blocked only inside the session. The caps come from a systemd user scope; without it, the session runs without a cap, with a warning. `sandmux ls` shows each session's usage. Details in [docs/en/limits.md](docs/en/limits.md).

### Sessions

In a terminal, `sandmux run` creates a session in the session daemon (`sandmuxd`) and attaches the terminal to it. The agent keeps running when the terminal closes, and `sandmux attach` gets back to it from any terminal, with the history and the current screen. While attached:

| Keys | Effect |
|---|---|
| Ctrl+b d | Detaches; the session continues |
| Ctrl+b Ctrl+b | Sends a Ctrl+b to the harness |

Several terminals can attach to the same session, and all of them type. The screen follows the smallest of them. The history (10,000 lines per session) stays only in memory and disappears when the daemon stops.

The first time it needs the daemon, the client installs and enables the `sandmuxd.service` systemd user unit, which starts the daemon on login, and tells you. Without systemd, the client starts the daemon right then. After a reboot, the sessions that were alive come back on their own. Those that used `--pass-env` come back when the vault is unlocked, if the values were stored in it; without a vault, they stay stopped until a `sandmux restart <id>` in a terminal with the variables. Details in [docs/en/sessions.md](docs/en/sessions.md).

To hand the agent a file from outside the work folder (a PDF, a screenshot), copy it to the inbox folder that `doctor` shows. Inside, it appears at `~/inbox`, read-only.

sandmux's data (synthetic HOMEs, write layers, inbox folder, import manifests) lives in `~/.local/share/sandmux`. To delete a write layer, grant the permission first, because the overlay creates folders without permissions: `chmod -R u+rwX ~/.local/share/sandmux/layers && rm -rf ~/.local/share/sandmux/layers/<harness>`.

### Limits of this stage

- In aberta mode, the agent reaches any destination outside the denylist. To restrict, use the `custom` or `inferencia` modes.
- A variable passed with `--pass-env` is within the agent's reach.
- In hermes, `--pass-env` puts the real value in the `~/.hermes/.env` of that run only, and the file is read-only: hermes cannot save settings to `.env` while running like this. The stored `.env` keeps the fake values. Values with line breaks or `${` are refused.
- The agent does not install system packages (the way to do it is a container in the private Docker) and does not open programs with windows.
- The `.git` protection has known limits, described in [docs/en/git.md](docs/en/git.md).

## What is planned for v1

- **Confinement**: user, mount, pid and network namespaces, Landlock and seccomp, without needing root. Without the required kernel support, the agent does not start; there is no tab without a sandbox. The `bwrap` inside a tab is a sandmux shim that does not mount: Claude Code's bash sandbox works ([docs/en/claude.md](docs/en/claude.md#bash-sandbox)).
- **Persistent sessions**: a daemon keeps the agents alive when the terminal closes.
- **Interface**: tabs with a confined terminal, side panel with the access log, prefix key (Ctrl+b by default), mouse and themes.
- **Network**: all traffic goes through a gateway that logs each domain. Four modes per tab: open (default), inference only, no network and custom (blocked by default, with an allowlist).
- **Secrets outside the sandbox**: an encrypted vault stores the real keys. The agent receives a fake key, swapped for the real one in the outbound traffic, only when the destination is the key's owner. It applies to the inference API, to `gh` and to private registries.
- **Files**: logging of every file access, allowlist and denylist with immediate effect, and secret-looking files hidden from the agent.
- **Docker**: a rootless Docker, private to the sandbox. The host's Docker socket never enters.
- **MCPs**: control over which MCP servers are allowed, and a built-in MCP for PostgreSQL and MySQL with the credential injected outside the sandbox.
- **Assistant**: a chat modal and a modal that generates a shell command and writes it into the terminal, without running it.

The development order, the decisions and the accepted risks are in the [roadmap](openspec/roadmap.md).

## Known limits

- **Target threat**: a manipulated agent trying to read secrets and exfiltrate data. Kernel exploits are out of scope.
- **The work folder belongs to the agent.** It reads everything in it. What it writes there (`Makefile`, `package.json` scripts, `.envrc`) needs to be read before it is executed on the host.
- **In the default network mode (aberta), whatever is not in the denylist gets through**, logged. To restrict, use the `custom` or `inferencia` modes.
- **No programs with windows in v1.** A headless browser works.

## Requirements

- Linux. There is no support for macOS or Windows.
- Kernel 6.12 or newer (Landlock with abstract socket scoping), with unprivileged user namespaces and `/dev/net/tun`. On Ubuntu 24.04, the HWE kernel, or 6.8 with the root helper and the BPF LSM on ([docs/en/install.md](docs/en/install.md#ubuntu-2404)). overlayfs in a user namespace is optional: without it, the caches start empty and the toolchains stay read-only.
- Tested and supported distros: Ubuntu 26.04, Ubuntu 24.04 (in a VM, with the 6.8 and HWE kernels), Debian 13, current Fedora and Arch (the last three in containers, with the test machine's kernel).
- Binaries published only for `x86_64`; on another architecture, build with `cargo install`.
- The harnesses (opencode, hermes, Claude Code, oh-my-pi, Antigravity CLI) are installed by you, on the host. sandmux does not install harnesses; `sandmux update <harness>` runs the official updater outside the sandbox.

## Architecture

Four binaries, each with a trust boundary:

```
+----------------+  socket  +----------------------+  socket  +----------------+
| sandmux        |<-------->| sandmuxd             |<-------->| sandmux-helper |
| client         |          | session daemon       |          | root helper    |
| (CLI and TUI)  |          | PTYs, vault,         |          | eBPF           |
+----------------+          | log, policy          |          | (post-v1)      |
                            +----------+-----------+          +----------------+
                                       |
                                       | launch specification
                                       v
                            +----------------------+
                            | sandmux-run          |
                            | sandbox launcher     |
                            | network gateway      |
                            +----------+-----------+
                                       |
                                       v
                                    harness
```

- `sandmux`: command-line client and terminal interface. It touches neither secrets nor the sandbox.
- `sandmuxd`: session daemon. Hosts the terminals, the vault, the log and the policy.
- `sandmux-run`: sandbox launcher. It only applies a ready-made specification; it knows neither harnesses nor policy. Its supervisor, outside the sandbox, runs the network gateway.
- `sandmux-helper`: root helper for eBPF, planned for after v1.

Written in Rust, with ratatui for the interface.

## Development

The project uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) to plan each change before implementing it.

| File | Content |
|---|---|
| [docs/en/install.md](docs/en/install.md) | Installation: `install.sh`, `sandmux setup`, packages, `cargo install`, the release and the test on the distros |
| [docs/en/launch-spec.md](docs/en/launch-spec.md) | Format of the launch specification between the client and the launcher |
| [vendor/vt100/SANDMUX.md](vendor/vt100/SANDMUX.md) | The copy of the `vt100` emulator with line autowrap (DECAWM), which the published version lacks (in Portuguese) |
| [docs/en/git.md](docs/en/git.md) | What the `.git` protection covers and how git behaves inside the sandbox |
| [docs/en/sessions.md](docs/en/sessions.md) | The session daemon: socket, protocol, session list, restore and limits |
| [docs/en/config.md](docs/en/config.md) | The configuration file: sections, keys, layers, profiles and commands |
| [docs/en/tui.md](docs/en/tui.md) | Interface with tabs: shortcuts and `[tui]`, modals, ended tab, mouse, selection and image paste |
| [docs/en/docker.md](docs/en/docker.md) | Private Docker per folder: file view, socket, lifecycle, persistence, ports and caps |
| [docs/en/helper.md](docs/en/helper.md) | Root helper (`sandmux-helper`): the Docker eBPF confinement, who can ask, installation and root tests |
| [docs/en/limits.md](docs/en/limits.md) | Memory and process caps: defaults, tuning, systemd scope and measurement |
| [docs/en/window.md](docs/en/window.md) | Desktop windows and audio: restricted mode through the security-context and the Wayland relay, direct mode, audio and limits |
| [docs/en/vault.md](docs/en/vault.md) | Secret vault: creating, unlocking, secrets, swapping in the traffic, tools (`gh`, `git`, `npm`, `pip`, `cargo`, `docker`), SSH keys and the SSH agent, import keys, `--pass-env` on restore and how it is stored |
| [docs/en/cloud.md](docs/en/cloud.md) | Cloud credentials: GCP and Kubernetes identities in the vault (reference to the host `gcloud` or `kubectl`, or copy), the fake in the tab, the swap in the gateway, IAM, RBAC and the limits |
| [docs/en/database.md](docs/en/database.md) | Database MCP: connections in the vault, `--db`, the tools, the query log and the limits |
| [docs/en/chat.md](docs/en/chat.md) | Chat with the models: the Ctrl+b `i` modal, `sandmux chat`, the chat tab, the Ctrl+b `g` command modal, models and providers, `[assistant]`, the confinement and the stored conversation |
| [docs/en/hermes.md](docs/en/hermes.md) | Hermes: the gateway session (`--gateway`), the restart, the wait for the vault, the channel tokens and the Telegram webhook |
| [docs/en/mcp.md](docs/en/mcp.md) | MCPs: inventory per harness, enabling and disabling per server, `sandmux mcp` and the Ctrl+b `m` screen |
| [docs/en/files.md](docs/en/files.md) | Files: opens through the supervisor (seccomp), log of accesses to the agent's folders, `sandmux files` and retention |
| [docs/en/network.md](docs/en/network.md) | Network through the gateway: addresses, `host.sandmux.internal`, modes, denylist, allowlist, catalogs, immediate effect, log and `sandmux net` |
| [openspec/roadmap.md](openspec/roadmap.md) | All the project's decisions and the 10-stage plan |
| [openspec/specs/](openspec/specs/) | Current specs: isolation, file system, harness presets, state import, daemon and sessions, configuration, resource caps, private Docker and the tabbed interface |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/proposal.md) | Proposal of the first change: command-line confinement |
| [openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md](openspec/changes/archive/2026-11-13-add-sandbox-cli/design.md) | Technical design of the first change |
| [AGENTS.md](AGENTS.md) | Rules for agents working in this repository |

The first change (`add-sandbox-cli`) is implemented and archived. To run the full check (formatting, lints and tests, including the sandbox escape tests): `./scripts/check.sh`. With [cargo-nextest](https://nexte.st) installed (`cargo install cargo-nextest --locked`), the tests run in parallel across all files, through the configuration in `.config/nextest.toml`; without it, the script uses `cargo test`, one file at a time.

## License

MIT, see [LICENSE](LICENSE).
