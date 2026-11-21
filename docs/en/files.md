# Files

> Português: [files.md](../files.md)

sandmux sees every file open by the agent, with the agent running, and logs the accesses to the agent's folders. It also denies and allows files on the spot and hides the secret-looking files of the work folder. The decision is made by the sandbox supervisor or, with the root helper, by the [kernel](#by-the-kernel).

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

## By the kernel

With the [root helper](helper.md) and the BPF LSM, a tab's opens leave the supervisor: the kernel decides and logs, without the round trip of each `open`. The daemon attaches the confinement BPF LSM programs to the tab, the same ones as the [private Docker](docker.md#ebpf-confinement), and marks the files by inode:

- the secret-looking files of the work folder give `ENOENT`, as with the supervisor;
- the `file-deny` ones, with what exists inside the denied folders, give `EACCES`;
- `.git` is protected as in Docker.

The opens in the agent's folders arrive through the kernel queue, with the path in the agent's view, and go into the log like the supervisor's. Only the mounts of those folders send events: the system, the toolchains and the caches do not go through the queue.

It applies on its own when all of this exists; without one of them, the tab stays with the supervisor, and the summary says why:

- a daemon session (`run` without a terminal stays with the supervisor);
- the BPF LSM active and the root helper, in a version that confines tabs;
- the tab's own scope in the user systemd (the same as the [caps](limits.md));
- a `file-deny` with paths inside the work folder only. A name (`*.sql`) or a path outside may apply in any folder, and the marks cover the work folder only.

The launcher asks the daemon after it builds the sandbox root and before it creates the command, and waits up to 10 s for the answer. In kernel mode, the command has no open filter; the execution filter stays when executions are logged by the supervisor. The entry shell (`Ctrl+b t`) stays with the supervisor.

Differences from the supervisor:

- **New files from the host:** a secret, or a file in a denied folder, created outside the sandbox during the session is marked by the folder watch (inotify, or the scan every 10 s without watches) within ~1 s. In that window, the agent can open the file. The secrets the agent itself creates stay its own, as with the supervisor.
- **Live policy:** denying and allowing on the spot mark and unmark the files within 1 s.
- **Access:** the line says `r` or `w`; a read-write open shows as `w`.
- **Bulk reads:** the log comes out complete. The kernel does not send again the open of a file, in the same mode, less than 1 s after the last one. Renaming the file or creating a link to it, from inside the tab, makes the next open go into the log with the new path. When the queue fills anyway, the helper freezes the tab for a few milliseconds, until the queue drains ([the events](helper.md#a-tabs-files)). A file renamed on the host during the session and opened by the new name less than 1 s after the last open by the old name stays in the log with the old name only. The same holds for a file seen through two of the agent's folders.
- **Helper drop:** without the marks, nothing runs in the tab. The daemon freezes its scope (`cgroup.freeze`) until the helper is back (systemd starts it on the next connection) and redoes the confinement, with the current policy; the tab goes on where it stopped. Without the confinement back within 30 s, the daemon kills everything in the scope (`cgroup.kill`). On a clean exit of the helper (`systemctl restart`, an upgrade), it tells the daemon before dropping the rules; killed by force, a short window is left between the exit and the freeze.
- **Special cases:** magic links and `/proc/self` follow the kernel and Landlock rules; a marked file stays denied through any path, because the mark belongs to the inode.

The launch protection, in `sandmux stats <id>` and in the panel, shows the "file opens" line: "by the kernel" or "supervisor: <reason>". `sandmux doctor` shows whether the kernel path can apply on this machine.

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

[By the kernel](#by-the-kernel), in an Ubuntu 24.04 VM (median of 3 runs, against the same work outside the sandbox): reading 13 thousand files of the work folder 1.05x on kernel 6.8 and 1.14x on 7.0, against 2.7x and 3.0x for the supervisor in the same VM (measured on 2026-11-21, with repeats removed in the kernel); the Python module import stays the same as outside (the system does not go through the queue).

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

### Executions

Every program run in a tab (by the agent or in a shell) becomes a line with access `exec`, the executed path and the arguments (`args`, up to 1024 bytes). The `via` field says the source:

- **`kernel`:** with the [root helper](helper.md) and the BPF LSM active, a program on the kernel's exec tracepoint logs what the kernel actually executed. It cannot be forged.
- **`supervisor`:** without them, the open supervisor also receives `execve` and `execveat`, reads the path and the arguments in the process memory and lets the kernel proceed. Another thread can swap the path or the arguments between the read and the execution, so this source can be fooled.

The supervisor logs from the start of the tab. When the helper takes over (right after the tab's scope appears), the daemon turns it off; if the connection to the helper drops, it comes back. In the handover, one execution may show up from both sources. With the helper, the folder's private Docker executions are logged too, with the `docker-<id>` tag.

The path is what was passed to `execve`: a program called as `./script` shows as `./script`. The execution's environment is not logged. No execution is denied by this log.

The launch protection, in `sandmux stats <id>` and in the panel, shows the "executions" line: "by the kernel" or "supervisor: <reason>". `sandmux files` shows the arguments after the path.

### Private Docker containers

With the [eBPF confinement](docker.md#ebpf-confinement), the log also has the lines of the folder's private Docker containers, with the `docker-<id>` tag and the host path:

- every denied operation, with the reason (`segredo` (secret), `file-deny: <pattern>`, `.git` or `cobertura` (cover));
- every file a container creates in the work folder, once per Docker start.

Containers' ordinary reads and writes are not logged. A line with the folder's path and access `?` says how many events were dropped with the queue full. `sandmux files <id>` shows these lines together with the session's, with `docker` in the session column.

## Retention

The file log follows the retention of the network log: `net-log-days` (default 30 days) and `net-log-max` (default `"500M"`), in `[global]` in the [configuration](config.md). The daemon deletes old files when it starts and every hour.
