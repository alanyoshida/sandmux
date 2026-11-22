# Sessions and the `sandmuxd` daemon

> Português: [sessions.md](../sessions.md)

`sandmuxd` is the per-user process that holds the sessions. A session is a harness (or the shell) confined by `sandmux-run` inside a pseudoterminal that the daemon keeps open. The session outlives the terminal it came from.

## When `run` uses the daemon

| Call | What happens |
|---|---|
| `sandmux run <harness>` in a terminal (input and output are terminals) | The client does the checks and questions (preset, harness installed, first-run import), asks the daemon for the session and attaches the terminal to it |
| `sandmux run <harness>` without a terminal (script, pipe, CI) | Runs directly, in the foreground, without the daemon: the output is the harness's, with no terminal sequences, and the exit code is its own |

## Commands

| Command | Effect |
|---|---|
| `sandmux ls [--group <group>]` | Lists the sessions with ID, name (`<harness>:<folder>`), [group](#tab-groups), state, attached terminals, memory and processes in use with the caps, and folder |
| `sandmux group list\|move <id> <group>\|rename <group> <new>` | The [tab groups](#tab-groups): list, move a session to another group, rename a group |
| `sandmux stats <id>` | Usage detail: memory and processes in use, peaks and caps, CPU used, out-of-memory kills and refused process creations ([docs/limits.md](limits.md)), and the ports published on the host ([docs/network.md](network.md)) |
| `sandmux net [<id>]` | The latest logged network connections, of one session or all ([docs/network.md](network.md)) |
| `sandmux net deny\|allow\|forget <pattern> <id>` | Changes the network rules of an alive session's tab, taking effect on the next connection |
| `sandmux net mode <mode> <id>` | Switches the tab's network mode |
| `sandmux files deny\|allow\|forget <pattern> <id>` | Denies, allows or removes a file pattern in a session's tab, taking effect on the next open ([docs/files.md](files.md)) |
| `sandmux db add\|list\|remove\|log` | Database connections in the vault and the log of the database MCP's queries ([docs/database.md](database.md)) |
| `sandmux mcp list [<id>]` | The session's MCP servers, or those of the harness in the current folder, with their state ([docs/mcp.md](mcp.md)) |
| `sandmux mcp deny\|allow\|forget <name> <id>` | Disables, puts on the allowlist or removes from the rules an MCP server in the tab; takes effect at the next launch |
| `sandmux folder add <id> <folder> [--rw]` | Mounts a host folder at `/mnt/sandmux/<name>` in the session, right away ([docs/files.md](files.md#new-folders-live)) |
| `sandmux folder remove <id> <name>` | Unmounts a folder from `/mnt/sandmux` |
| `sandmux folder list <id>` | Lists the session's folders in `/mnt/sandmux` |
| `sandmux files [<id>] [--days N]` | The logged accesses to the agent's folders, of one session or all ([docs/files.md](files.md)) |
| `sandmux net rules [<id>]` | The effective network mode and rules, with the origin |
| `sandmux attach [id]` | Attaches the terminal. Without an ID, to the only alive session |
| `sandmux detach <id>` | Detaches all terminals from the session |
| `sandmux kill <id>` | Asks the harness to end (SIGTERM and SIGHUP); after 5 seconds, it ends it by force. The session leaves the list |
| `sandmux restart <id>` | Relaunches an ended or stopped session, with the same options and the same profile, resolving the configuration again. The `pass-env` values come from the terminal that asks. With `--force`, it also restarts a live session: the harness is ended (TERM, and KILL after 5 s) and comes back with the current configuration, without the ended-tab notification |
| `sandmux update <harness> [--yes] [-- args]` | Runs the harness's official updater on the host, outside the sandbox ([details](#updating-the-harness)) |
| `sandmux daemon status` | State of the daemon, without starting it |
| `sandmux daemon stop` | Ends the harnesses and stops the daemon. Alive sessions come back at the next start |
| `sandmux daemon restart` | Stops and starts with the installed binary; use it after updating sandmux |
| `sandmux daemon disable` | Stops the daemon and removes the systemd unit |
| `sandmux daemon boot [on\|off]` | Turns the [start at boot](#start-at-boot) on or off, without a login; without an argument, shows the state |

States of a session: **alive**, **ended** (with the harness's exit code and, when there was an out-of-memory kill, the note "out of memory"), **stopped** (with the reason: missing `--pass-env` after a restore, work folder deleted, failure to relaunch) **waiting for vault** (restored with the `--pass-env` values stored in the [vault](vault.md), or the hermes gateway session or a chat tab with the vault created; starts when the vault is unlocked) and **restarting in N s** (the [hermes gateway](hermes.md) ended and comes back after the wait).

## Tab groups

Each session (tab) belongs to a group, and each TUI shows one group at a time, like tmux sessions:

- **Creating:** `sandmux run <harness> --group studies` creates the session in the `studies` group. Without `--group`, it goes to `principal`. A tab created in the TUI goes to the group the TUI is showing.
- **Existence:** a group exists while it has tabs; `principal` always exists. There is no command to create or delete a group: it is born with its first tab and goes away with the last one.
- **Names:** 1 to 32 characters, among letters, digits, `-`, `_` and `.`.
- **Commands:**
  - `sandmux group list` lists the groups, with the number of tabs;
  - `sandmux group move <id> <group>` moves a session to another group;
  - `sandmux group rename <group> <new>` renames a group; renaming to a name that already exists merges the two.
- **TUI:** `sandmux tui --group <group>` opens the TUI on the group. Without it, it opens on `principal`, or, with `principal` empty, on the first group with tabs. Inside the TUI, Ctrl+b `(` and `)` switch groups, and Ctrl+b `G` opens the group list ([tui.md](tui.md#tab-groups)).
- **Outside groups:** `attach <id>` and the other commands by identifier ignore groups. The group comes back with the session on restore.

## Attached mode

The user's terminal becomes the session's terminal. On attach, the daemon sends the history (for the terminal's own scrollback), then the current screen and the terminal modes the harness turned on.

| Keys | Effect |
|---|---|
| Ctrl+b d | Detaches |
| Ctrl+b Ctrl+b | Sends a Ctrl+b to the harness |
| Ctrl+b followed by another key | The key is discarded |

Ctrl+b is recognized as byte 0x02 and also in the form of the kitty keyboard protocol (`CSI 98 ; 5 u`), which opencode turns on.

Closing the terminal, or sending SIGTERM to the client, only detaches. On detach, the client undoes in the user's terminal the alternate screen, the mouse, focus events, bracketed paste and the kitty protocol.

Several terminals can attach to the same session; all of them get the output and all of them type. The session's screen has the smallest number of columns and rows among the attached terminals.

## Shell inside a tab's sandbox

`sandmux enter [id]` (without an identifier, the only live session), or Ctrl+b `t` in the TUI, opens a new session, `dentro:<folder>`, with a shell inside the target tab's sandbox. The shell sees exactly what the agent sees:

- **files:** the same view, the same starting folder, the same Landlock;
- **file rules:** opens go through the same rules and the same path as the agent's, the kernel in a kernel-confined tab and the supervisor in the others, and into the same log (a `.env` is still denied);
- **network and restrictions:** the same network and the same seccomp;
- **processes:** the same ones, because the shell is in the same pid namespace (`ps` shows the agent).

The shell counts against the target tab's caps, and what it does on the network and with files shows up in that tab's logs.

The inside tab ends when the shell exits, or when the target sandbox ends. After a daemon restart, it comes back stopped, because the target sandbox is a different one; Ctrl+b `r` enters again. An inside tab does not open another one. A tab launched by an older sandmux version must be restarted first.

## Shell configuration

The `shell` preset starts with a minimal configuration file, generated by sandmux, with `[sandmux]` in the prompt. The import brings the configuration you already have, as with the harnesses: on the first run, by answering the question, or later, with `sandmux import shell`.

Only the files of the shell the preset uses come in (yours, when it is in the system folders; otherwise, `bash`):

| Shell | Files |
|---|---|
| `bash` | `.bashrc`, `.bash_aliases`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.inputrc`, `.oh-my-bash`, `.bash_it` |
| `zsh` | `.zshrc`, `.zshenv`, `.zprofile`, `.zlogin`, `.zlogout`, `.p10k.zsh`, `.oh-my-zsh` |
| `fish` | `.config/fish` (without `fish_variables`), `.config/omf`, `.local/share/omf` |

- **Frameworks:** the oh-my-bash, bash-it, oh-my-zsh and oh-my-fish folders come along, without the `.git`, `cache` and `log` folders.
- **Left out:** the files of the other shells and of the programs the shell calls, such as `starship.toml`. A program that does not exist in the sandbox fails with the shell's own message, and the rest of the file runs.
- **Secrets:** in the startup files, `export GITHUB_TOKEN=...` (and `set -gx NAME value` in fish) with a credential name becomes a fake value, which `sandmux vault import` ties to the real one. In the framework files, only values shaped like tokens are replaced. Values like `$(pass show x)` stay as they are.
- **Prompt:** the imported `.bashrc`, `.zshrc` or `config.fish` gets a snippet at the end that puts `[sandmux]` in front of your prompt.
- **First run:** the import replaces the minimal file. Without a terminal (script, pipe), `shell` starts without importing, without asking for `--import` or `--no-import`; `--import` imports.
- **HOME already created:** `sandmux import shell` lists the minimal file as different; `sandmux import shell --overwrite` replaces it.

## Updating the harness

The harness code goes into the sandbox read-only, and auto-update is turned off in there. `sandmux update <harness>` runs the official updater on the host, outside the sandbox and outside the daemon, in the foreground, in your terminal:

| Harness | Updater |
|---|---|
| `opencode` | `~/.opencode/bin/opencode upgrade` |
| `hermes` | `~/.local/bin/hermes update` |
| `claude` | `~/.local/bin/claude update` |
| `omp` | `~/.local/bin/omp update` |
| `agy` | `~/.local/bin/agy update` |

- The command exits with the updater's exit code.
- The arguments after `--` go to the updater: `sandmux update omp -- --check`.
- `shell` has no updater. A harness that is not installed is refused with the installation instructions, as in `run`: sandmux does not install harnesses.
- **Live tabs:** if the harness has live tabs, the command lists them and asks first, because the updater replaces files they may be using. `--yes` skips the question; without a terminal and without `--yes`, the command refuses. Afterwards, it says which tabs to restart (Ctrl+b `r`, or `sandmux restart <id> --force`) to use the new version.

sandmux itself is updated by the package or by `install.sh` ([install.md](install.md)).

## History and clipboard

- Each session keeps the last 10,000 lines that left the main screen, in memory (`sandmuxd --history-lines N` changes the limit). By default, nothing from the sessions' output is written to disk, not even to the daemon's log. The exception is the [history on disk](#history-on-disk), which is optional.
- Writes to the clipboard by the harness (OSC 52) are stripped from the output, unless the session was created with `--allow-clipboard`. Clipboard queries (`OSC 52 ; … ; ?`) are always stripped. The filter recognizes OSC started by `ESC ]` or by the C1 in UTF-8 (`C2 9D`) and terminated by BEL, `ESC \` or `C2 9C`, even when split across reads; an OSC above 1 MiB is discarded.

### History on disk

With `screen-history = true` in `[global]` ([config.md](config.md)), or the History on disk item in the General section of the TUI settings, the tabs' output survives a daemon restart and a crash. It comes off, because the output can show secrets the agent printed.

- **Encrypted:** each tab's output goes, every 2 seconds, to `$XDG_STATE_HOME/sandmux/history/<id>.bin`, encrypted with XChaCha20-Poly1305. The key is random and kept in the [vault](vault.md), and each file only decrypts for its own tab. Without a vault, nothing is saved. The file has mode 600, and the folder, 700.
- **Locked vault:** the output waits in memory, up to 2 MiB per tab, and is written when the vault opens. Anything past that is lost, and the daemon log says so.
- **Coming back:** when the tab is relaunched with the vault open, the history comes before the new output, and the line "sandmux: the tab came back" marks the restart. With the vault locked, it comes back on unlock, and the attached terminals are redrawn with it.
- **Size:** above 8 MiB, the file is compacted to the most recent 4 MiB.
- **Cleanup:** closing the tab deletes its file, and turning the option off deletes them all, within 12 seconds. Changing the vault password does not change the history key.

## How the daemon starts

When a command needs the daemon and it does not answer:

1. With systemd user, the client writes `~/.config/systemd/user/sandmuxd.service` (with `ExecStart` pointing to the installed `sandmuxd` and `XDG_DATA_HOME`, when set), enables it at login and starts it, warning the first time. If the binary moves, the unit is rewritten.
2. Without systemd, or with `SANDMUX_DAEMON_START=direct`, the client starts `sandmuxd` as its own process, detached from the terminal, with the log in `$XDG_STATE_HOME/sandmux/sandmuxd.log` (or `~/.local/state/sandmux/sandmuxd.log`).

### Start at boot

With systemd's linger on for your user, the user's service manager starts at boot, and the daemon with it, without a login:

- **Turning it on:** `sandmux daemon boot on` runs `loginctl enable-linger` (without sudo) and enables the unit. On some distros, polkit asks for authorization. If `loginctl` refuses, run `sudo loginctl enable-linger $USER` yourself.
- **Turning it off:** `sandmux daemon boot off` runs `loginctl disable-linger`. The other systemd services of your user also stop starting at boot.
- **State:** `sandmux daemon boot`, `sandmux daemon status` and `sandmux doctor` say whether the daemon starts at boot.

At boot, with nobody logged in, no tab comes back:

- The tabs that would come back stay **waiting for login** in `sandmux ls`, and `(login)` in the TUI. This state is saved: a daemon restart before the login does not lose them.
- The daemon does not query the [keyring](vault.md#keyring), which is locked until the login.
- At the first login (on screen or over SSH), the tabs come back by themselves. Then the daemon tries the keyring and notifies on the desktop the tabs still waiting for the vault.

With someone logged in when the daemon starts, everything goes as before. The daemon learns about the login from logind (`/run/systemd/users/<uid>`); without logind, it counts as logged in.

## Socket and access

- Socket: `$XDG_RUNTIME_DIR/sandmux/sandmuxd.sock`, or `/tmp/sandmux-<uid>/sandmuxd.sock` without `XDG_RUNTIME_DIR`. The folder has mode 700; a folder that already exists with another owner, another mode or as a symbolic link is refused.
- Each connection has its UID checked with `SO_PEERCRED`; another UID is refused.
- A lock (`sandmuxd.lock`, with the PID) ensures one daemon per user.
- The sandbox cannot reach the socket: `/run` and `/tmp` are new in there, and Landlock denies the host's abstract sockets. The escape suite tries both paths.

## Protocol

Unix stream socket. Each frame has 4 bytes of length (big-endian, content only, up to 16 MiB), 1 byte of type and the content:

| Type | Content |
|---|---|
| 0 | Control message in JSON (`{"t": "list"}`, `{"t": "created", "id": 1, "name": "shell:app"}`, …) |
| 1 | Terminal bytes: client input or session output |

The first message is the greeting (`hello`), with the protocol number, the version, the data directory, the configuration folder and the language of the client (the daemon answers in that language). This version uses protocol 48. Different numbers end the connection, and the client shows both versions and points to `sandmux daemon restart`. A different data directory or configuration folder is also refused, with both folders in the message.

Along with the session request, the client sends the daemon the options given to `run`, the named profile, the current folder, the variables of the safe list, `SHELL` and the `pass-env` values of the effective policy. The daemon resolves the configuration layers ([docs/config.md](config.md)) and composes the launch specification with the same function as the direct `run`.

## Session list and restore

The daemon writes `sessions.json` in the data directory (mode 600, atomically) on every change. For each session: ID, name, harness, folder, options given to `run`, named profile, names of the `pass-env` variables of the last launch, state and group. The file also keeps the tabs' [pane](tui.md#panes) layouts, with the sessions in the leaves; an ended session leaves the layout. Never the variables' values, never the resolved policy, never the output. The file has `"version": 2`; the unversioned list, written by the previous change, is read and converted.

On start, the daemon reads the list:

| Stored session | On start |
|---|---|
| Alive, without `pass-env` in the policy resolved now | Relaunched on its own, without an attached terminal, with `TERM=xterm-256color` |
| Alive, with `pass-env` and the values stored in the [vault](vault.md) | Waiting for vault; starts on its own when the vault is unlocked |
| Alive, with `pass-env` without the values in the vault | Stopped until `sandmux restart <id>` in a terminal with the variables |
| hermes gateway, alive or restarting, with the vault created | Waiting for vault; starts on its own when the vault is unlocked |
| Alive opencode tab, with a local provider (port in `host-ports`) that uses a key from the vault, and the vault created | Waiting for vault; starts with the local server's model list when the vault is unlocked ([network.md](network.md#host-ports)) |
| Alive chat tab, with the vault created | Waiting for vault; starts on its own, with the conversation, when the vault is unlocked ([chat.md](chat.md)) |
| Alive, with the named profile deleted or the configuration invalid | Stopped, with the reason |
| Alive, with the folder deleted | Stopped, with the reason |
| Ended or stopped | Comes back as it was |

When the daemon ends, by stop, restart or failure, no sandbox process keeps running: each session's `sandmux-run` is born with `PR_SET_PDEATHSIG` tied to the daemon, and the sandbox's process 1, tied to `sandmux-run`.

## Limits

- **Raw passthrough.** The output goes raw to the clients. A terminal larger than the smallest attached one shows the session in the top-left corner.
- **Terminal queries.** With two clients attached, a harness query to the terminal (attributes, background color, cursor position) gets two answers. In a session restored without a client, the queries made at startup go unanswered.
- **`tty` inside the sandbox.** `tty` answers "not a tty", even though the input is a terminal: the sandbox's `/dev/pts` is a new instance, without the host's pseudoterminal. This also applies to the direct `run`.
- **Full-screen history.** In a session on the alternate screen (opencode, hermes), the resent history is that of the main screen, which is usually empty.

## Variables for tests and diagnostics

| Variable | Effect |
|---|---|
| `SANDMUX_DAEMON_START=direct` | The client never uses systemd |
| `SANDMUX_START_MODE` | How the daemon says it was started (`systemd`, `direct`); filled in by the client and by the unit |
| `SANDMUX_PROTOCOL_OVERRIDE` | The daemon announces another protocol number; only useful to test the refusal |
| `SANDMUX_SECRET_SCAN_SECS` | Interval, in seconds, of the periodic scan for real keys in the synthetic HOMEs (60 by default) |
| `SANDMUX_LOGIND_USER`, `SANDMUX_LINGER_DIR`, `SANDMUX_LOGINCTL` | The user state file in logind, the linger folder and the `loginctl` command, instead of the system ones; the tests simulate the boot and the login |
| `SANDMUX_NOTIFY_BUS` | Bus address for the desktop notifications, instead of the session bus; the tests point it to a bus of their own |
| `SANDMUX_SECRET_INOTIFY=0` | The real-key watcher does not use inotify, only the periodic scan; only useful to test it |
