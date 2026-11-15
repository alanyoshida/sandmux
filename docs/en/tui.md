# Tabbed interface

> Português: [tui.md](../tui.md)

`sandmux` without a subcommand, or `sandmux tui`, in a terminal, opens the sandmux interface. Each daemon session is a tab: the same tabs show up in every terminal that opens the TUI, and the sessions keep running when it closes.

```
[sandmux] ana@host:~/projetos/app$ █

 ≡   1  shell:app   2  opencode:api ✕   3  hermes:notas   image pasted…   shell   ~/projetos/app   vault open   ! 1   1h 20m
```

- **Last line, the bar:** in the block style (the default), the `≡` at the start opens the [menu](#menu). Each tab is a block with its position (the number used by the shortcuts), followed by the name and a mark: ✕ for ended (a click on it closes the tab), ‖ for stopped, (vault) for waiting for vault and ↻ for the gateway restarting, plus ✱ when there are [real keys](vault.md#automatic-move) moved or waiting. The active tab has its block highlighted. Notices show up in the middle. On the right, in colored blocks:
  - the active tab's harness;
  - the active tab's folder, in a block of another color;
  - the vault state (open, locked or no vault);
  - the alerts, how many tabs have a block (`!`), a new domain (`•`) and real keys (`✱`), only when there are any;
  - how long the active tab has been alive.

  Without room, the blocks on the right drop out from left to right, and the time goes last.
- **Rest of the screen:** the active tab's terminal, with colors (including true color), attributes, cursor and alternate screen. Switching tabs shows the other one's screen right away.

### Font and bar style

The default style, `"blocos"`, uses only colored blocks and characters that every font has. The `"powerline"` style adds slanted separators and icons, which only exist in fonts with the Powerline and Nerd Font symbols (for example, those of the Nerd Fonts project); without one of them, they show up as squares. `[tui]` has two keys:

| Key | Values | Default |
|---|---|---|
| `style` | `"blocos"`; `"powerline"`, with Nerd Font separators and icons; or `"simples"`, a text-only bar with the active tab in reverse video | `"blocos"` |
| `bar` | `"bottom"` (last line) or `"top"` (first line) | `"bottom"` |

```toml
# Without a font with the Powerline symbols:
[tui]
style = "simples"
bar = "top"
```

```toml
# With a Nerd Font in the terminal, the separators and icons:
[tui]
style = "powerline"
```

With no session, the TUI opens the new tab modal. Without an interactive terminal (pipe, script), `sandmux tui` exits with an error.

## Shortcuts

All of them start with the prefix key, Ctrl+b by default:

| Key | Action |
|---|---|
| `c` | New tab |
| `n` / `p` | Next / previous |
| `1` to `9` | Go to the tab in that position (`0` is the tenth) |
| `w` | Tab list (arrows, Enter, Esc) |
| `,` | Rename the tab |
| `r` | Restart the ended or stopped tab |
| `x` | Close the tab (ends the session, with confirmation) |
| `v` | Paste image from the clipboard |
| `d` | Leave the TUI (the sessions keep running) |
| `?` | Help |
| `s` | Show or hide the side panel |
| `a` | The tab's network screen: accesses and rules |
| `f` | The tab's files screen: accesses, with `b` to block and `l` to allow |
| `o` | The tab's folders screen: mounts and unmounts host folders at `/mnt/sandmux` |
| `m` | The tab's MCP screen: enables and disables servers |
| `i` | Chat modal: a question to a model, which opens a chat tab ([chat.md](chat.md)) |
| `g` | Command modal: the model generates a command, written into the tab without running it ([chat.md](chat.md#command-modal)) |
| `e` | [Settings](#settings) |
| `t` | Shell inside the tab's sandbox ([sessions.md](sessions.md#shell-inside-a-tabs-sandbox)) |
| `:` | [Menu](#menu) with all actions |
| the prefix again | Sends the prefix to the harness |

Any other key after the prefix is discarded. The prefix works as a control byte and also in the form of the kitty keyboard protocol.

### Changing the shortcuts

In the `[tui]` section of the [configuration file](config.md):

```toml
[tui]
prefix = "C-a"

[tui.keys]
new = "t"
close = "q"
```

- **Actions:** `new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help`, `panel`, `network`, `files`, `folders`, `mcp`, `chat`, `command`, `menu`, `settings` and `enter`.
- **Keys:** a visible character that is not a digit, because digits lead to the tabs.
- **Two actions on the same key** chosen in `[tui.keys]` make the file invalid. A chosen key that is another action's default wins, and the other action is left without a key (it shows as `—` in the help and stays in the menu). This way a new action in a new version never makes the file invalid.
- **`[tui]` is not a policy layer:** it changes nothing in the sandboxes.

## Side panel

With a terminal at least 100 columns wide, a 34-column panel sits on the right. The tabs' terminal area shrinks by the same amount, and the sessions get the new size. The panel has five sections about the active tab. Each one starts with a colored strip with the name and, on the right, a summary: the protection level, the session state, how many ports are published, the network mode, how many MCPs are enabled. Values are laid out in columns: the label on the left, in a dimmer tone, and the value aligned to the right.

**Protection** shows what was applied at the session's last launch. What is on or off shows up as status dots, in two columns: a colored `●` for on, a dim `○` for off (clipboard and `pass-env` on in yellow, because they open something to the agent):

- the protection level, with the Landlock ABI;
- the write layer;
- the work folder and the extra folders, read-only or with write access;
- GPU and KVM;
- writing to the clipboard;
- the `pass-env` variables, with the warning that they are within the agent's reach;
- the memory and process caps;
- the network.

**Resources** shows, updated every second:

- memory in use, the peak and the cap, with a usage gauge;
- processes, also with a gauge;
- CPU;
- out-of-memory kills;
- the real keys in the synthetic HOME: moved to the vault since the launch (restart the tab), waiting for the vault or waiting for a domain;
- the folder's private Docker.

The gauge stays green up to 75% of the cap, yellow up to 90% and red above that. An out-of-memory kill shows up in red.

**Ports** shows the ports published on the host's `127.0.0.1`: the ones the agent listens on in the sandbox and those of the folder's private Docker containers, as `3000 → 127.0.0.1:3000 (sandbox)`.

**Network** shows the session's network mode and the tab's last 6 logged connections, in a borderless table: domain (or "no domain"), bytes sent (`↑`) and received (`↓`) and status (the HTTP code, `✗` for a block, "new"). A long domain stays on a line of its own, with the numbers below, and the reason for a block shows up below the line. A domain seen for the first time in the work folder shows up as "new". An inactive tab gets `•` on the bar when the session accesses a new domain and `!` when it has a block, until it becomes active. An HTTP error status and a block show up in red, and "new" in yellow. The full log is in [network.md](network.md).

**MCP** shows the tab's MCP servers, with ✓ in green for enabled and ✗ in red for disabled.

**Order and collapsed sections:** a click on a section's title collapses it (`▸`) or expands it (`▾`), and the `↑` and `↓` arrows at the end of the strip move it one position. Both are saved in `[tui]` and apply the next time the TUI opens:

```toml
[tui]
panel-order = ["rede", "recursos", "protecao", "portas", "mcp"]
panel-collapsed = ["protecao"]
```

The sections are `protecao`, `recursos`, `portas`, `rede` and `mcp`. Those missing from `panel-order` go to the end, in the default order. Going back to the default order, or expanding all of them, deletes the key.

Ctrl+b `s` hides and shows the panel. The `panel` key of `[tui]` (`true` or `false`) sets how the TUI opens.

## Locked vault

When the [vault](vault.md) exists and is locked, the TUI opens with a modal asking for the password. What is typed shows up as `•`. Enter unlocks, and the sessions waiting for the vault start; a wrong password shows the error and asks again; Esc closes the modal and leaves the vault locked. A tab waiting for the vault shows `(vault)` next to its name.

## Network screen

Ctrl+b `f` opens the active tab's files screen: the logged accesses to the agent's folders since yesterday, one per path, from the most recent to the oldest, with `•` on paths never seen in this work folder and the reason for blocks. `b` denies the selected path in the tab (the exact path, like `sandmux files deny`), `l` allows it, ↑ ↓ (or `k` and `j`) move, and Esc closes. The rule takes effect on the next open ([files.md](files.md)).

Ctrl+b `m` opens the active tab's MCP screen: the harness's servers, with the type, the origin, the target and the state. Space enables or disables the selected one in the tab, taking effect at the next launch (Ctrl+b `r`); Esc closes. The panel has the **MCP** section, with `✓` and `✗` ([mcp.md](mcp.md)).

Ctrl+b `o` opens the active tab's folders screen: the host folders mounted at `/mnt/sandmux`, with the mode (`ro` or `rw`). `a` opens a field to type the folder (Enter confirms) and asks for the mode (`r` read-only, `w` read and write); the folder is mounted right away, without relaunching the agent. `x` unmounts the selected one, and Esc closes ([files.md](files.md#new-folders-live)).

Ctrl+b `a` opens the active tab's network screen, with two lists switched with Tab:

- **Accesses:** the tab's last 50 connections and queries, from newest to oldest, with the protocol, the domain, the destination, the volume and "new" or the block.
- **Rules:** the effective mode and rules, with the state (on or off), the kind (deny or allow), the pattern and the origin.

| Key | List | Effect |
|---|---|---|
| `n` | Accesses | Denies the destination of the selected access |
| `l` | Accesses | Allows the destination (applies in `custom` mode) |
| space | Rules | Turns a catalog or allowlist item on or off |
| `x` | Rules | Removes the rule from the layer it came from |
| `m` | both | Switches the tab's mode: open, inference only, custom |
| ↑ ↓ | both | Selects the line |
| Esc | both | Closes the screen |

Denying and allowing ask where to save: `a` in the tab (this session only, stored in the session list), `p` in the project profile or `g` in the global one. Turning on and off saves in the project or in the global one. The change takes effect on the session's next connection. The mode switch applies to the tab; to enter or leave `sem-rede`, use `sandmux net mode` and relaunch the session.

## Menu

Ctrl+b `:`, or a click on the `≡` at the start of the bar (blocos and powerline styles), opens a menu with all the actions and their keys. The arrows select, Enter runs and Esc closes. An action's key, without the prefix, also runs it from inside the menu.

## Settings

Ctrl+b `e`, or "settings" in the menu, opens a full-screen page with the options of the [configuration file](config.md), in sections:

| Section | Options |
|---|---|
| General | language (`auto`, which follows the system, `pt` or `en`; [config.md](config.md#language)) |
| Appearance | theme (with color samples), bar style and position, side panel |
| Shortcuts | prefix key |
| Sandbox | repo, KVM, GPU, clipboard, memory, processes, extra folders, `pass-env`, SSH keys, databases |
| Network | mode, denylist, allowlist, catalogs, decryption, inbound ports, host ports |
| Files | denied and allowed files, secret files |
| MCP | denied and allowed servers |
| Assistant | chat and command models, suggested models (Enter, or `a` in the list, opens the [providers' list](chat.md#model-list); `e` edits the text) |
| Logging | days and cap of the network log |
| Vault | the vault state and secrets, without the values; create, unlock, lock, change the password, add a secret and an SSH key, edit domains and remove ([vault.md](vault.md#from-the-tui)) |

Each item shows the current value and where it comes from ("default", "from global"):

- toggles: `● on` or `○ off`;
- choices: `‹ value ›`;
- texts;
- lists.

**Keys:**

| Key | Action |
|---|---|
| ↑ ↓ | Move through the sections or the items |
| Tab | Switches between the sections and the items |
| ← → | Change a choice |
| Space or Enter | Toggles a toggle |
| Enter | Edits a text (Enter saves, empty goes back to the default) or opens a list |
| `a` and `x` (in the list) | Add and remove |
| `x` (on an item) | Resets the item to the default |
| `g` | Switches the target between the global one and the project of the active tab's folder |
| Esc or `q` | Closes |

A click selects a section or an item, and toggles a toggle.

**Saving:** each change is saved right away to `config.toml` by the same editor as `sandmux config set/add/remove`. It preserves the comments and the rest of the file, and validates the whole file before saving. A refused change shows the reason at the bottom, and the file does not change.

**When it applies:**

- language, theme, bar, panel and prefix: right away;
- assistant models: on the next request;
- session policy: at the tab's next launch (Ctrl+b `r`).

The options of `[tui]`, `[assistant]` and of the log apply only in the global one; with the target on a project, they show up as "global only". The keys of each action (`[tui.keys]`), the colors of each element (`[tui.colors]`), the profiles and the assistant's custom providers stay in the file.

## Color theme

The `theme` key of `[tui]` picks a built-in theme:

- `padrao`: on the bar, a dark background with the active tab's number in lilac and the blocks on the right in pink, green, yellow and cyan; in the simple style, the terminal's colors with reverse video. It is the default.
- `escuro`: dark background with a blue accent.
- `claro`: light background with a dark blue accent.
- `catppuccin`: Catppuccin Mocha, mauve accent.
- `rose-pine`: Rosé Pine, iris accent.
- `tokyo-night`: Tokyo Night (Night), blue accent.
- `kanagawa`: Kanagawa (Wave), crystal blue accent.
- `gruvbox`: dark Gruvbox, orange accent.
- `dracula`: Dracula, purple accent.

The themes paint the bar, the modals, the panel and the settings page. On the page, going through the themes with ← → shows each one right away and already saves the choice.

In `[tui.colors]`, each element can change color on top of the theme:

| Element | What it paints |
|---|---|
| `tab-active-fg`, `tab-active-bg` | Active tab in the simple style |
| `tab-fg`, `tab-bg` | Other tabs (in blocks, the name's color and background; in Powerline, `tab-fg` is the name's color) |
| `tab-index-fg`, `tab-index-bg` | Block with the tabs' number (blocks and Powerline) |
| `tab-active-index-fg`, `tab-active-index-bg` | Block with the active tab's number and its name's color (blocks and Powerline) |
| `chip-fg` | Text of the blocks on the right and of the `≡` (blocks and Powerline) |
| `chip-harness-bg`, `chip-folder-bg`, `chip-vault-bg`, `chip-alert-bg`, `chip-time-bg` | Blocks on the right: harness, folder, vault, alerts and time (blocks and Powerline) |
| `accent` | Accent: the menu's `≡`, the border and the titles of the settings page |
| `muted-fg` | Secondary text of the settings page |
| `ok-fg` | Toggle on and save notice |
| `page-bg`, `page-fg` | Background and text of the settings page |
| `surface-bg` | Sections and selected item on the settings page |
| `bar-bg` | Bar background |
| `status-fg` | Notice on the right of the bar |
| `note-fg`, `note-bg` | Strip of an ended, stopped or in-history tab |
| `modal-border` | Modal border |
| `modal-title` | Modal title |
| `selected-fg`, `selected-bg` | Selected item and active field in modals |
| `error-fg` | Error in modals |
| `selection-fg`, `selection-bg` | Text selected with the mouse |

A color is `"#rrggbb"`, an index from 0 to 255, one of the 16 ANSI names (`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` and the `bright-` versions) or `"default"`, the terminal's color.

```toml
[tui]
theme = "escuro"

[tui.colors]
tab-active-index-bg = "#aa0000"
```

An unknown theme, element or color makes the file invalid. The theme applies when the TUI opens, or right away when it changes through the settings page.

## New tab

The modal asks for three things:

- **harness:** with ← and →, among `opencode`, `hermes` and `shell`;
- **work folder:** by default, the folder the TUI was opened from;
- **named profile:** optional.

Tab switches fields, and Enter creates. The tab goes through the same checks and the same layer resolution as `sandmux run`: an error, such as a dangerous folder, a harness not installed or an invalid configuration, shows up in the modal without creating a session.

On the first run of a harness with configuration to import, the modal itself asks whether to import (y/n). The `pass-env` values come from the environment of the terminal that opened the TUI.

## Ended or stopped tab

- **Ended:** the tab stays on the bar, with the last screen and the exit code. `r` relaunches the session in the same tab, and `x` closes it.
- **Question on exit:** when the harness ends with the TUI open (hermes exits after the setup wizard, for example), a modal shows the code and asks: Enter or `r` restarts, `x` closes the tab, Esc leaves it as it is. If the tab was not active, the question shows up when you activate it, once. Tabs that had already ended when the TUI opened do not ask. A click on the tab's ✕ on the bar also closes it, without confirmation.
- **Stopped:** the tab shows the reason, for example a `pass-env` variable that was missing on restore.

## Mouse

- **Click on the bar:** switches tabs.
- **Harness that turned on the mouse** (opencode, for example): clicks, the wheel and drags go to it, with the lines counted from the terminal area.
- **Harness without mouse** (a shell, for example):
  - the wheel scrolls the tab's history, with a notice on the last line;
  - dragging selects text, which on release goes to your terminal's clipboard through OSC 52 and to the system clipboard (Wayland or X11), which also works inside tmux. Copying through the TUI always works, even with clipboard writing blocked for the harness.
- **Copy by the harness** (opencode copies through OSC 52): blocked by default. With `clipboard = true` in the configuration, or `sandmux run --allow-clipboard`, the TUI passes the copy on to your terminal and to the system clipboard.
- **Shift with drag:** keeps your terminal's native selection, which also works when the harness uses the mouse.

## Pasting an image

The prefix followed by `v` reads the image from the system clipboard (X11, or Wayland with the data-control protocol) and saves it as `colado-<date>.png` in the inbox folder. Then it writes the image's path in the sandbox, inside `~/inbox`, into the active tab, without Enter. That way a screenshot reaches the agent without leaving the work folder.

Without an image in the clipboard, or without access to it, the bar warns and nothing is written.

## Harness modes

Input goes raw to the active tab: the harness gets the keys as in a direct terminal. The modes it turned on that change what your terminal sends stay on in your terminal while the tab is active, and are undone when switching tabs and on exit:

- kitty keyboard protocol;
- focus events;
- bracketed paste;
- application cursor and keypad.

With a modal open, the TUI turns those modes off to understand the keys.

## Limits

- **Inactive tab queries:** a harness query to the terminal (attributes, background color) made in an inactive tab is only answered when the tab becomes active again.
- **Selection with the harness's mouse:** with the mouse turned on by the harness, the TUI's selection does not work; use Shift with drag.
- **No split panes.** Only the tabs and the side panel.
