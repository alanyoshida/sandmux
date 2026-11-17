# Files

> Português: [files.md](../files.md)

sandmux sees every file open by the agent, with the agent running, and logs the accesses to the agent's folders. It also denies and allows files on the spot and hides the secret-looking files of the work folder.

## How it works

A seccomp filter with notification (`SECCOMP_RET_USER_NOTIF`) sends every `open`, `openat`, `openat2` and `creat` of the agent to the sandbox supervisor. The supervisor:

1. reads the path from the process's memory;
2. opens the file itself, inside the sandbox root (`RESOLVE_IN_ROOT`: absolute links and `..` stay inside it);
3. checks the Landlock read and write rules against the path of the file it actually opened;
4. hands the descriptor to the process as the result of the call (`SECCOMP_IOCTL_NOTIF_ADDFD`).

The process receives the file the supervisor opened: swapping the path in memory after it was read, from another thread, changes nothing. Landlock still applies to everything else (executing, renaming, deleting, creating folders). If the supervisor dies, opens fail.

Special cases:

- **`/proc/self`, `/dev/fd/<n>`, `/dev/stdin`:** are translated to the process itself, and `cat <(echo hi)` works. `/proc/net`, `/proc/mounts` and `/etc/mtab` too.
- **Magic links of other processes** (`/proc/<pid>/fd`, `cwd`, `root`, `exe`): refused with `EACCES`. Tools like `lsof` do not see the open files of other processes inside the sandbox.
- **`O_PATH`:** goes through the kernel, under Landlock, without logging. An `O_PATH` descriptor gives no access to the content; reading requires another open, which goes through the supervisor.

## File policy

The supervisor decides each open by the file it actually opened, in this order:

1. matches `file-allow`: opens;
2. matches `file-deny`: fails with `EACCES` ("Permission denied");
3. is a secret-looking file in the work folder that the agent did not create: fails with `ENOENT` (as if it did not exist); opening for writing fails with `EACCES`;
4. the rest follows the Landlock rules.

A change applies on the next open, without relaunching the agent: the daemon rewrites the launch policy when the configuration changes or the tab is edited, and the supervisor rereads it.

### Patterns

- **Path:** starts with `/` or `~/`, like `~/proj/privado` or `/srv/dados/**/*.csv`. `*` and `?` apply within a component, and `**` applies to any number of components.
- **Name:** without `/`, like `*.sql` or `node_modules`; matches a file or a folder anywhere.
- **Folder:** a pattern that matches a folder applies to everything inside it.

| Key | `run` option | Effect |
|---|---|---|
| `file-deny` | `--file-deny` | does not open |
| `file-allow` | `--file-allow` | opens, overriding `file-deny` and the secrets |
| `secret-files` | | adds patterns to the secrets list |
| `secret-files-off` | | removes patterns from the secrets list (exceptions) |

The lists add up across the [configuration](config.md) layers. For the tab of a live session:

```bash
sandmux files deny dados.sql 3      # denies in session 3's tab
sandmux files allow ~/proj/ok.sql 3 # allows
sandmux files forget dados.sql 3    # removes the pattern from the tab
```

In the TUI, Ctrl+b `f` shows the tab's accesses, and `b` and `l` deny and allow the chosen path ([tui.md](tui.md)).

### Secret-looking files

In the work folder, these files do not open for the agent: `.env` and `.env.*` (except `.env.example`, `.env.sample` and `.env.template`), `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.kdbx`, `id_rsa*`, `id_ecdsa*` and `id_ed25519*` (except the `*.pub` ones), `.npmrc`, `.pypirc`, `.netrc`, `credentials*.json`, `*.tfstate` and `.git-credentials`.

- **The name still shows in `ls`:** `getdents` and `stat` do not go through the supervisor; only the content is kept out.
- **Created by the agent:** a file like this that the agent itself creates (by opening, renaming or linking) stays open to it, also in the following launches. The list stays in `$XDG_STATE_HOME/sandmux/files/created/`, out of the agent's reach. If you later fill a file created by the agent with a real secret, deny it with `sandmux files deny`.
- **Certificates:** a public certificate in `.pem` in the work folder is also hidden; allow it with `file-allow` or `secret-files-off`.
- **Renaming and linking:** renaming or linking a secret to another name is denied, so it does not show up under another name.
- **In the private Docker:** containers do not go through the supervisor. The secrets and `file-deny` paths that exist when the folder's Docker starts reach them empty, with limits the supervisor does not have: see [docker.md](docker.md#secrets-and-denied-paths).

### `.git/commondir` and the name calls

A `.git/commondir` created by the agent would make the host's git use another git directory, with other hooks. Creating this file in the work folder is denied, whether by open, `mv`, `ln` or `ln -s`.

For that, `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` and `symlinkat` also go through the supervisor. It reads the paths once, resolves the folders inside the sandbox root and decides the policy. Then a helper makes the call inside the sandbox. The helper stays in the sandbox's user and mount namespaces, under the same Landlock, so the kernel decides the mount points and the write rules as it does for the agent. `file-deny` also applies to the target of a rename or link.

## New folders live

A host folder enters the live session, without relaunching the agent, at `/mnt/sandmux/<name>`. `/mnt/sandmux` exists in every sandbox, writable from the start, and the `SANDMUX_FOLDERS` variable points to it: reading, writing, creating, deleting and renaming apply right away, according to the folder's mode.

```bash
sandmux folder add 3 ~/dados --rw   # read and write at /mnt/sandmux/dados
sandmux folder add 3 ~/docs         # read-only at /mnt/sandmux/docs
sandmux folder list 3
sandmux folder remove 3 docs
```

- **Name:** the folder's last component, with `-2`, `-3` when it repeats.
- **Notice to the agent:** `/mnt/sandmux/PASTAS.txt`, read-only, lists each name with the mode and the host folder. Tell the agent where to look.
- **Refusals:** the same as `--rw` (the HOME, the root, a folder that contains the HOME, the sandmux data directory and, with writing, the configuration folder); a folder with credentials gives a warning.
- **Remembered:** the folders stay in the tab and come back in the session's following launches, in the same place. In a stopped session, `folder add` applies on the next launch.
- **Log and policy:** `/mnt/sandmux` joins the agent's folders in the file log, and the file policy applies to it.
- **TUI:** Ctrl+b `o` opens the folders screen ([tui.md](tui.md)).

How it works: a supervisor helper enters the sandbox's user namespace and creates its own mount namespace, a copy of the host's tree. There it copies the folder (`open_tree`), applies the mode and attaches the copy at `/mnt/sandmux/<name>` in the sandbox's mount namespace (`move_mount`). The daemon talks to the supervisor through a per-launch control socket, in the user's runtime directory.

## Cost

Measured on the reference machine (median of 3 runs): `opencode --version` 1.0x; hermes importing the CLI 1.1x to 1.2x; incremental `cargo check` 1.2x to 1.3x; reading 13 thousand files in a row 3.2x. Four supervisor threads serve the opens.

## Log

Each distinct access (path and access type) to the agent's folders becomes a line in `$XDG_STATE_HOME/sandmux/files/YYYY-MM-DD.jsonl` (usually `~/.local/state/sandmux/files/`), with folder 700 and file 600, the first time in each launch. A block is always logged.

- **Agent's folders:** the work folder, the synthetic HOME (with the inbox folder) and the extra folders (`--ro`, `--rw`).
- **Out of the log:** the system, the toolchains and the caches mounted in the HOME (the layers), and `/tmp`.

| Field | Content |
|---|---|
| `ts` | local date and time |
| `tag` | the launch: `s<session>-<n>` in a session, `run-<pid>` in a `run` without a terminal |
| `workdir` | work folder |
| `path` | path in the agent's view |
| `access` | `r` (read), `w` (write), `rw` or `create` (file created) |
| `blocked` | reason for the block (`file-deny: <pattern>`, `segredo` or `landlock`), or `null` |
| `new` | the path appeared for the first time in this work folder |

## `sandmux files`

```bash
sandmux files            # today's accesses, from all sessions
sandmux files 3          # only from session 3
sandmux files --days 7   # from the last 7 days
```

Shows the last 200 lines, with the time, the session, the access, the path and the block.

## Retention

The file log follows the retention of the network log: `net-log-days` (default 30 days) and `net-log-max` (default `"500M"`), in `[global]` in the [configuration](config.md). The daemon deletes old files when it starts and every hour.
