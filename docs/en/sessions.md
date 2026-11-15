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
| `sandmux ls` | Lists the sessions with ID, name (`<harness>:<folder>`), state, attached terminals, memory and processes in use with the caps, and folder |
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
| `sandmux restart <id>` | Relaunches an ended or stopped session, with the same options and the same profile, resolving the configuration again. The `pass-env` values come from the terminal that asks |
| `sandmux daemon status` | State of the daemon, without starting it |
| `sandmux daemon stop` | Ends the harnesses and stops the daemon. Alive sessions come back at the next start |
| `sandmux daemon restart` | Stops and starts with the installed binary; use it after updating sandmux |
| `sandmux daemon disable` | Stops the daemon and removes the systemd unit |

States of a session: **alive**, **ended** (with the harness's exit code and, when there was an out-of-memory kill, the note "out of memory"), **stopped** (with the reason: missing `--pass-env` after a restore, work folder deleted, failure to relaunch) **waiting for vault** (restored with the `--pass-env` values stored in the [vault](vault.md), or the hermes gateway session or a chat tab with the vault created; starts when the vault is unlocked) and **restarting in N s** (the [hermes gateway](hermes.md) ended and comes back after the wait).

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
- **file rules:** opens go through the same rules and into the same log (a `.env` is still denied);
- **network and restrictions:** the same network and the same seccomp;
- **processes:** the same ones, because the shell is in the same pid namespace (`ps` shows the agent).

The shell counts against the target tab's caps, and what it does on the network and with files shows up in that tab's logs.

The inside tab ends when the shell exits, or when the target sandbox ends. After a daemon restart, it comes back stopped, because the target sandbox is a different one; Ctrl+b `r` enters again. An inside tab does not open another one. A tab launched by an older sandmux version must be restarted first.

## History and clipboard

- Each session keeps the last 10,000 lines that left the main screen, in memory only (`sandmuxd --history-lines N` changes the limit). Nothing from the sessions' output is written to disk, not even to the daemon's log.
- Writes to the clipboard by the harness (OSC 52) are stripped from the output, unless the session was created with `--allow-clipboard`. Clipboard queries (`OSC 52 ; … ; ?`) are always stripped. The filter recognizes OSC started by `ESC ]` or by the C1 in UTF-8 (`C2 9D`) and terminated by BEL, `ESC \` or `C2 9C`, even when split across reads; an OSC above 1 MiB is discarded.

## How the daemon starts

When a command needs the daemon and it does not answer:

1. With systemd user, the client writes `~/.config/systemd/user/sandmuxd.service` (with `ExecStart` pointing to the installed `sandmuxd` and `XDG_DATA_HOME`, when set), enables it at login and starts it, warning the first time. If the binary moves, the unit is rewritten.
2. Without systemd, or with `SANDMUX_DAEMON_START=direct`, the client starts `sandmuxd` as its own process, detached from the terminal, with the log in `$XDG_STATE_HOME/sandmux/sandmuxd.log` (or `~/.local/state/sandmux/sandmuxd.log`).

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

The first message is the greeting (`hello`), with the protocol number, the version, the data directory, the configuration folder and the language of the client (the daemon answers in that language). This version uses protocol 25. Different numbers end the connection, and the client shows both versions and points to `sandmux daemon restart`. A different data directory or configuration folder is also refused, with both folders in the message.

Along with the session request, the client sends the daemon the options given to `run`, the named profile, the current folder, the variables of the safe list, `SHELL` and the `pass-env` values of the effective policy. The daemon resolves the configuration layers ([docs/config.md](config.md)) and composes the launch specification with the same function as the direct `run`.

## Session list and restore

The daemon writes `sessions.json` in the data directory (mode 600, atomically) on every change. For each session: ID, name, harness, folder, options given to `run`, named profile, names of the `pass-env` variables of the last launch, and state. Never the variables' values, never the resolved policy, never the output. The file has `"version": 2`; the unversioned list, written by the previous change, is read and converted.

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
