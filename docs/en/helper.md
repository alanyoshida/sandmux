# Root helper (`sandmux-helper`)

> Português: [helper.md](../helper.md)

`sandmux-helper` is the only part of sandmux that runs as root. It exists for one thing: to attach to the cgroup of a folder's [private Docker](docker.md) the BPF LSM programs that decide, in the kernel, what containers can open and change. Without it, the rest of sandmux works the same, and the private Docker starts with the unprivileged protections only.

## What it does

The daemon asks for the confinement when a folder's private Docker starts, and the helper does two things:

1. Loads the programs and attaches them to that folder's `sandmux-docker-<id>.slice`.
   - They apply to `dockerd`, `containerd`, `runc` and every container of the folder.
   - They do not apply to any other process on the machine, nor to the agent.
2. Marks the files the daemon sends:
   - **denied:** the folder's secrets and `file-deny`, with everything inside the denied folders, and the `/etc` covers;
   - **write-protected:** `.git/hooks`, `.git/config` and `.git/config.worktree`;
   - **git directories:** `commondir`, `hooks`, `config` and `config.worktree` cannot be created in them.

The decision is by file (the inode), not by path. That is why unmounting a cover, removing the read-only flag from a bind, renaming or creating a link do not open a way to the file. The details of what is denied are in [docker.md](docker.md#ebpf-confinement).

## Who can ask for what

- The socket is `/run/sandmux-helper.sock`, and any user can connect. The helper knows who is asking from the socket credentials (`SO_PEERCRED`), not from anything the request says.
- A user can only confine a cgroup whose directory belongs to them. Root can confine any.
- A rule only denies. It never grants access, and it applies only to the processes of the requested cgroup. Marking a system file, such as `/etc/hostname`, only stops that user's Docker from opening it.
- The helper never opens a path. Files arrive as descriptors opened by the user, who thereby proves they can reach the file.
- **Per-user limits:** 16 active confinements, each with up to 16384 marks. A request above that is refused, without affecting the others.

## Live and events

- **Unmark:** the daemon also asks for `unmark`, when the folder's file policy changes and a path is no longer denied. The helper removes the mark and releases the file's descriptor.
- **Events:** the programs send an event through a kernel queue when they deny an operation and when a container creates a file on the folder's file system. The helper reads the queue and sends the events to the daemon over the same confinement connection. The event has the type, the hook, the inode and the created file's name, never the content of a file. With the queue full, extra events are dropped and counted, and containers do not wait.

## Execution and connection observation

The `observe` request turns on only the observation of executions in a user cgroup (a tab's scope), without file rules or marks. A program on the `sched_process_exec` tracepoint sends the executed path, the process and the arguments over the same connection. Two `fexit` programs, on the exit of `tcp_v4_connect` and `tcp_v6_connect`, send each TCP connection a process in the cgroup opens: the pid, the source port and the destination. The daemon uses these events for the [process of each connection](network.md#process-of-each-connection) in the network log. The Docker confinement (`confine`) includes the same observation. There are up to 64 active observations per user, besides the 16 confinements. The observation only logs: no execution or connection is denied or delayed.

## How long the rules last

The rules and marks apply while the daemon's connection to the helper is open. They go away when:

- the folder's Docker stops;
- the daemon ends, even when killed by `SIGKILL`;
- the helper stops.

If the helper stops with the Docker running, the daemon stops the folder's Docker, which does not go on without the rules. The next `docker` starts another Docker and turns the confinement on again, if the helper answers. The systemd socket starts the helper again on the next connection.

## Install

The helper needs two things:

- the kernel with the BPF LSM active, that is, `bpf` in the list in `/sys/kernel/security/lsm`;
- the binary in a root-owned folder.

**`sandmux setup`** writes the `sandmux-helper.socket` and `sandmux-helper.service` units to `/etc/systemd/system` and enables the socket. It uses `sudo` after asking, as it does with the AppArmor profile ([install.md](install.md#sandmux-setup)).

It refuses to install when the binary, or a folder above it, can be changed by a user other than root. That is the case of `~/.local/bin`, where `install.sh` puts the binaries without `sudo`. The reason: the service runs as root, and whoever changes the binary would gain root.

With `--no-sudo`, setup shows the commands to run as root:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now sandmux-helper.socket
```

**Packages:** the `.deb`, the `.rpm` and the AUR package ship the units in `/usr/lib/systemd/system` and enable the socket on install. The Homebrew formula does not ship the helper, because Homebrew does not run as root.

**Kernel without `bpf` in the list:** add `bpf` to the `lsm=` boot parameter, keeping the ones already in `/sys/kernel/security/lsm`. After rebooting, `sandmux doctor` shows the "Docker eBPF confinement" check.

## Turn it off

```bash
sudo systemctl disable --now sandmux-helper.socket
```

Each folder's Docker goes back to the unprivileged protections the next time it starts. `install.sh --uninstall` stops and removes the units.

## How to tell whether it applies

- `sandmux doctor` shows the "Docker eBPF confinement" check, with the reason and the fix when something is missing.
- `sandmux stats <id>` shows, in the private Docker section, "eBPF confinement: on" or "off:" with the reason. The TUI panel shows the "eBPF" line.

## What the service can do

The service unit has these systemd restrictions:

- **capabilities:** only `CAP_BPF` and `CAP_PERFMON`, which loading and attaching BPF LSM programs to a cgroup requires;
- **file system:** `ProtectSystem=strict` and `ProtectHome=yes`;
- **network:** none (`PrivateNetwork=yes`), and Unix sockets only.

The programs are assembled by sandmux itself, from the BTF of the running kernel (`/sys/kernel/btf/vmlinux`), without `clang` and without code compiled elsewhere.

## Tests

The tests that need root run through `scripts/check-root.sh`, outside `check.sh`:

```bash
sudo scripts/check-root.sh
```

The script starts the helper as root on a temporary socket and runs `crates/sandmux/tests/docker_ebpf.rs` as the user who called `sudo`. It runs binaries compiled by that user. That is why allowing it in `sudoers` without a password gives root to whoever runs as that user. If you do that, do it only while you need it.
