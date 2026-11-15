# Memory and process caps

> Português: [limits.md](../limits.md)

Each sandbox runs with a memory cap and a process cap. A runaway agent, whether a build that never stops, a test stuck in a loop or a fork bomb coming from prompt injection, hits its own session's cap without bringing down the machine or the other sessions.

## Defaults

| Cap | Default | When exceeded |
|---|---|---|
| Memory | 75% of the machine's RAM, computed at each launch. The sandbox does not use swap | The kernel kills processes of that sandbox only, starting with the one using the most memory. The rest of the session keeps running |
| Processes and threads | 8192, combined | Creating a process fails only inside the sandbox (`fork: Resource temporarily unavailable`) |

The caps apply to daemon sessions and to `run` without a terminal.

## Adjusting

The `memory` and `processes` keys of the [configuration file](config.md), and the `--memory` and `--processes` options of `run`, follow the same layers as the other keys. The most specific layer wins, and the `run` option beats them all.

| Key and option | Values |
|---|---|
| `memory`, `--memory` | Percentage of RAM, from 1% to 100% (`"50%"`); a size with `K`, `M`, `G` or `T` (`"16G"`, `"512M"`); or `"max"`, no cap. Minimum: `128M` |
| `processes`, `--processes` | A number (`512`) or `"max"`, no cap. Minimum: 32 |

```toml
[global]
memory = "50%"

[project."/home/ana/monorepo"]
memory = "max"
processes = 16384
```

```bash
sandmux config set memory 16G --profile pesado
```

```bash
sandmux run shell --processes 256
```

## How it works

Each launch runs in a transient systemd user scope, created with `systemd-run --user --scope`, with the launcher inside it. The scope's properties are:

- `MemoryMax`, the memory cap;
- `MemorySwapMax=0`;
- `TasksMax`, the process cap;
- `OOMPolicy=continue`, so that systemd does not stop the whole session at the first out-of-memory kill.

The scope goes away when the sandbox ends.

The agent cannot touch the caps. Inside the sandbox, `/sys/fs/cgroup` is read-only, so writing to `memory.max`, `pids.max` or `cgroup.procs`, or creating a cgroup, is denied. The escape suite tries all of that.

### Without support

The caps need two things:

- the systemd user instance answering `systemd-run --user`;
- the `memory` and `pids` controllers delegated to `user@<uid>.service`.

Without them, the session runs without caps, and each launch warns:

```
sandmux: warning: this session runs without a memory and process cap: <reason>; see `sandmux doctor`
```

`sandmux doctor` shows the cause. The sandbox works anyway: the cap protects the machine, but it is not a security boundary.

## Usage

`sandmux ls` shows, for each alive session, the memory and processes in use, each with its cap:

```
ID  NAME           STATE                       CLIENTS  MEMORY      PROC     FOLDER
1   opencode:proj  alive                       1        412M/34.5G  37/8192  /home/ana/proj
2   shell:proj     ended (137, out of memory)  0                             /home/ana/proj
```

`sandmux stats <id>` shows the detail:

- memory in use, peak and cap;
- processes in use, peak and cap;
- CPU time used;
- processes killed for lack of memory;
- process creations refused by the cap.

A session that ended after an out-of-memory kill shows up as "out of memory".

`sandmux stats` ends with the protection summary of the last launch: level, write layer, folders, GPU and KVM, clipboard, `pass-env`, caps and network. It is the same summary as the TUI's side panel.

The private Docker of each folder has its own caps, with the folder's values, and shows up in a section of its own in `sandmux stats` ([docs/docker.md](docker.md)).

## Going back to the previous version

The previous version refuses the `memory` and `processes` keys in `config.toml`, because they are unknown. Remove them before installing the previous binary.
