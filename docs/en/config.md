# Configuration and profiles

> Português: [config.md](../config.md)

sandmux works without any configuration: the built-in baseline and the harness preset are already the safe default. The configuration file only stores the exceptions you want to repeat without typing them on every `sandmux run`.

## The file

`$XDG_CONFIG_HOME/sandmux/config.toml`, or `~/.config/sandmux/config.toml` when `XDG_CONFIG_HOME` is not set. `sandmux doctor` shows the path and whether the file is valid.

The file is optional. It has three kinds of policy section, plus two that do not change the sandboxes: `[tui]`, with the prefix key, the shortcuts, the theme and the bar of the interface ([docs/en/tui.md](tui.md)), and `[assistant]`, with the models of the chat and of the command modal and the custom providers ([docs/en/chat.md](chat.md)):

```toml
# Applies to every run.
[global]
ro = ["~/docs"]

# Named profile: only applies with `sandmux run --profile vm`.
[profile.vm]
kvm = true

# Project profile: applies on its own in the folder and its subfolders.
[project."/home/ana/proj"]
gpu = false
clipboard = true
pass-env = ["OPENROUTER_API_KEY"]
```

| Key | Type | Same effect as |
|---|---|---|
| `ro` | list of paths | `--ro` |
| `rw` | list of paths | `--rw` |
| `pass-env` | list of variable names | `--pass-env` |
| `repo` | true or false | `--repo` / `--no-repo` |
| `kvm` | true or false | `--kvm` / `--no-kvm` |
| `gpu` | true or false | `--gpu` / `--no-gpu` |
| `clipboard` | true or false | `--allow-clipboard` / `--no-allow-clipboard` |
| `harness-asks` | true or false | (configuration only; see below) |
| `window` | true, false or `"direct"` | `--window` / `--window-direct` / `--no-window` (see [window.md](window.md)) |
| `memory` | text: `"50%"`, `"16G"` or `"max"` | `--memory` (see [limits.md](limits.md)) |
| `processes` | number or `"max"` | `--processes` |
| `network` | `"aberta"`, `"inferencia"`, `"custom"` or `"sem-rede"` | `--network` (see [network.md](network.md)) |
| `deny` | list of destination patterns | `--deny` |
| `allow` | list of destination patterns (`custom` mode) | `--allow` |
| `allow-off` | list of registered patterns that are turned off | (no option) |
| `catalog` | list of groups from the common domains catalog | (no option) |
| `catalog-off` | list of items turned off from the exfiltration and inference catalogs | (no option) |
| `decrypt` | list of destination patterns whose TLS is decrypted ([network.md](network.md)) | `--decrypt` |
| `file-deny` | list of file patterns that do not open ([files.md](files.md#file-policy)) | `--file-deny` |
| `file-allow` | list of allowed file patterns, which override `file-deny` and the secrets | `--file-allow` |
| `secret-files` | list of patterns added to the list of secret-looking files | (no option) |
| `secret-files-off` | list of patterns that no longer count as secrets | (no option) |
| `host-ports` | list of host port numbers brought to the sandbox's `127.0.0.1`, for local services such as LM Studio ([network.md](network.md#host-ports)) | `--host-port` |
| `ports` | list of published inbound ports, `8642` (only on `127.0.0.1`) or `8443@0.0.0.0` (also on that address); when it is not empty, only they leave the sandbox ([network.md](network.md#port-allowlist)) | `--port` |
| `db` | list of names of vault database connections that the session's database MCP may use ([database.md](database.md)); `"*"` allows all of them | `--db` |
| `mcp-deny` | list of names of disabled MCP servers ([mcp.md](mcp.md)) | `--mcp-deny` |
| `mcp-allow` | list of names of MCP servers; when it is not empty, only they stay enabled | `--mcp-allow` |
| `ssh-keys` | list of names of SSH keys from the [vault](vault.md#ssh-keys) that the session's SSH agent may use; `"*"` allows all of them | `--ssh-key` |
| `net-log-days` | number of days, only in `[global]` | (retention of the network log, default 30) |
| `net-log-max` | size, such as `"500M"`, only in `[global]` | (cap of the network log, default `"500M"`) |
| `notify` | `true` or `false`, only in `[global]` | (desktop notifications: blocks, real keys, ended tabs; default `true`; [tui.md](tui.md#desktop-notifications)) |
| `screen-history` | `true` or `false`, only in `[global]` | (encrypted screen history on disk, with the key in the vault; default `false`; [sessions.md](sessions.md#history-on-disk)) |
| `language` | `"pt"` or `"en"`, only in `[global]` | (language of sandmux: TUI, command line and messages; without the key, the system's; [see below](#language)) |

- **Paths:** absolute or starting with `~/`.
- **`pass-env`:** stores only names, never values. The value comes from the environment of whoever launches, with the same warning as the option. A missing variable refuses the launch.
- **Profile names:** lowercase letters, numbers, `-` and `_`.
- **Project folders:** absolute paths.

An invalid file makes `run` refuse the launch, with the file, the line and the reason: malformed TOML, unknown section or key, wrong type, relative path, a value in `pass-env`, or the same path in `ro` and `rw` of the same section. Nothing in the file is silently ignored.


### Live tabs

The configuration applies to the tabs launched after the change. Live tabs keep the old value until they are restarted: the settings page says how many there are, and `sandmux config set`, `unset`, `add` and `remove` list the live sessions when the daemon is running. Ctrl+b `r` on the tab, or `sandmux restart <id> --force`, restarts with the new configuration.

### Harness prompts

By default, sandmux launches the harnesses in the mode where they do not ask for confirmation to edit, run commands, leave the project folder or read files like `.env`, because the sandbox is what blocks (Landlock, the [file policy](files.md) and the [network](network.md)):

| Harness | How |
|---|---|
| opencode | `OPENCODE_PERMISSION` with everything set to `allow` |
| Claude Code | `--dangerously-skip-permissions` and `--settings '{"skipDangerousModePermissionPrompt":true}'` |
| hermes | `HERMES_YOLO_MODE=1` (hermes's fixed blocks still apply) |
| oh-my-pi | `--auto-approve` |
| Antigravity CLI | `--dangerously-skip-permissions` |

`harness-asks = true` (global, profile or project, or the "Harness prompts" item in the TUI settings page) launches the harness as it is, with the prompts. The adjustment goes through the environment and the arguments of each launch: the harness configuration files in the synthetic HOME do not change. The `shell` and the chat tab are not affected.

## Layers

The policy of each launch adds up layers, from the least to the most specific:

1. built-in baseline;
2. harness preset;
3. `[global]`;
4. named profile (`--profile`);
5. project profile;
6. `run` options.

- **On and off switches, caps and network** (`repo`, `kvm`, `gpu`, `clipboard`, `memory`, `processes`, `network`): the most specific layer that set the key wins. A `gpu = false` in global is undone by `gpu = true` in the project, and the project is undone by `--no-gpu` in `run`.
- **Lists** (`ro`, `rw`, `pass-env`, `deny`, `allow`, `allow-off`, `catalog`, `catalog-off`, `decrypt`, `ssh-keys`, `file-deny`, `file-allow`, `secret-files`, `secret-files-off`, `mcp-deny`, `mcp-allow`, `db`, `ports`): the items of all layers add up; for a port that appears in two layers, the address from the most specific one applies. In the network, deny always wins ([network.md](network.md)). If the same path is in `ro` in one layer and in `rw` in another, the most specific one applies.
- **Fixed baseline:** profiles only add or adjust these keys. The baseline items (covered `/etc` files, toolchains, caches, known credentials) cannot be turned off by a profile.

### Project profile

Applies when the work folder, with links resolved, is the section's folder or a subfolder of it. With profiles in nested folders, only the one of the closest folder applies. `/home/ana/proj` does not apply to `/home/ana/proj2`.

The project profile lives in your configuration, outside the folder. Nothing inside the work folder changes the policy, because the agent can write there: a `.sandmux.toml` or a `.config/sandmux/config.toml` in the folder are ignored. Accepted cost: the profile does not travel with the repository.

## Commands

| Command | Effect |
|---|---|
| `sandmux config show [--workdir FOLDER] [--profile NAME]` | The effective policy for the current folder. Each key appears as "default" or "modified", with the layer it came from |
| `sandmux config list` | The named profiles and the folders with a project profile, with the keys of each one. A folder that no longer exists appears marked |
| `sandmux config set <key> <value>` | Sets an on and off key (`true` or `false`, also `sim` and `não`) or a cap (`memory`, `processes`), or the network mode (`network`) |
| `sandmux config unset <key>` | Deletes an on and off key or a cap |
| `sandmux config add <key> <item>` | Adds an item to `ro`, `rw`, `pass-env`, `deny`, `allow`, `allow-off`, `catalog`, `catalog-off`, `decrypt`, `ssh-keys`, `file-deny`, `file-allow`, `secret-files`, `secret-files-off`, `mcp-deny`, `mcp-allow`, `db` or `ports` |
| `sandmux config remove <key> <item>` | Removes an item from a list |

`set`, `unset`, `add` and `remove` require the target:

- `--global`;
- `--profile <name>`;
- `--project [<folder>]`, which without a folder uses the current one.

The command:

- validates the result before writing;
- writes only what you set, never the default values;
- preserves the comments and the formatting of what you wrote by hand;
- deletes the section that became empty;
- writes atomically.

A new file is created with mode 600 and the folder with 700.

The [TUI settings page](tui.md#settings) (Ctrl+b `e`) writes through the same editor, with the same rules, and also the keys of `[tui]`, `[assistant]` and those that only apply in `[global]` (`net-log-days`, `net-log-max`, `language`, `notify`, `screen-history`).

## Language

sandmux speaks Portuguese or English: the TUI, the command line output and errors, the daemon's warnings and the validation messages of this file. The language comes, in this order:

1. from the `SANDMUX_LANG` variable (`pt` or `en`);
2. from the `language` key of `[global]`;
3. from the system: `LC_ALL`, `LC_MESSAGES` or `LANG`, the first one that is set. A value starting with `pt` is Portuguese; any other, including `C`, is English.

```toml
[global]
language = "en"
```

The names of the keys, sections and values of this file, of the subcommands and of the options are the same in both languages; in English, the TUI and `config show` show the translated name of some values (`aberta` shows as `open (aberta)`), and the file keeps the Portuguese value. The client tells its language in the greeting, and the daemon answers in that language. The TUI settings page has the language in the General section, and the change applies right away.

Examples:

```bash
sandmux config set kvm true --profile vm
```

```bash
sandmux config add ro ~/docs --project
```

```bash
sandmux config show
```

## The configuration out of the agent's reach

The sandbox never receives the configuration folder with write access. These are refused as the work folder, in `--rw` or in the `rw` key:

- the folder itself;
- a folder inside it;
- a folder that contains it, such as `--rw ~/.config`.

It may enter read-only (`--ro ~/.config`), because it stores no secret.

## Paths the host runs

The HOME paths that the host runs outside the sandbox are also refused with write access, in `--rw`, in the `rw` key, as the work folder and in live-mounted folders. With write access to them, the agent could plant something that runs on the next terminal or login:

- shell and session startup files: `.bashrc`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.zshrc`, `.zprofile`, `.zshenv`, `.zlogin`, `.zlogout`, `.xprofile`, `.xinitrc`, `.xsession`, `.xsessionrc` and `.pam_environment`;
- folders: `.bashrc.d`, `.oh-my-zsh`, `.local/bin`, `.local/share/applications`, `.config/autostart`, `.config/systemd`, `.config/environment.d`, `.config/fish` and `.config/plasma-workspace`.

This applies to the path, to anything inside it and to a folder that contains it. Read-only, all of them may enter (`--ro ~/.local/bin`).

## When a change applies

The policy is resolved on each launch, with the file as it is at that moment. The session list stores the options given to `run` and the profile name, not the resolved policy.

- **Live session:** keeps the policy of its launch. The change applies when it is relaunched (`sandmux restart`, `sandmux daemon restart` or reboot).
- **Restore after reboot:** a live session becomes stopped, with the reason in `sandmux ls`, when:
  - its policy now has `pass-env`;
  - its named profile was deleted;
  - the file is invalid.
- **`sandmux restart` of a session whose profile was deleted:** exits with an error, and the session stays as it was.

## Client and daemon

The daemon uses the same configuration file as the client. The handshake checks the configuration folder, and a client with another `XDG_CONFIG_HOME` is refused, with both folders in the message. The systemd unit carries the `XDG_CONFIG_HOME` of the client that installed it.

## Going back to the previous version

The previous version of sandmux ignores the configuration file. Its daemon does not read the session list in the new format: after installing the previous binary and running `sandmux daemon restart`, the session list starts empty. The synthetic HOMEs and the write layers are not affected.
