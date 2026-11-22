# Root helper (`sandmux-helper`)

> Português: [helper.md](../helper.md)

`sandmux-helper` is the only part of sandmux that runs as root. It exists for one thing: to attach to the cgroup of a folder's [private Docker](docker.md) the BPF LSM programs that decide, in the kernel, what containers can open and change. Without it, the rest of sandmux works the same, and the private Docker starts with the unprivileged protections only. On a kernel with Landlock below ABI 6 (Ubuntu 24.04 with 6.8), it also completes the confinement of each tab ([sandbox scope](#sandbox-scope)), and then the sandbox only starts with it.

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
- A user only asks for the scope of a user namespace that belongs to them, and never the helper's own.
- **Per-user limits:** 16 active confinements, each with up to 16384 marks, and 256 sandbox scopes. A request above that is refused, without affecting the others.

## Live and events

- **Unmark:** the daemon also asks for `unmark`, when the folder's file policy changes and a path is no longer denied. The helper removes the mark and releases the file's descriptor.
- **Events:** the programs send an event through a kernel queue when they deny an operation and when a container creates a file on the folder's file system. The helper reads the queue and sends the events to the daemon over the same confinement connection. The event has the type, the hook, the inode and the created file's name, never the content of a file. With the queue full, extra events are dropped and counted, and containers do not wait (a tab waits: see below).

## A tab's files

The daemon also asks for `confine` on a tab's scope, when the tab's launcher asks for the [files by the kernel](files.md#by-the-kernel). Besides the `dev` of the work folder, the request carries the mounts (`mnt_id`) of the agent's folders in the sandbox view. Three things change from Docker:

- **New marks:** `secret` gives `ENOENT` (the secret disappears, as with the supervisor) and `fdeny` gives `EACCES`, on opens and on operations inside a marked folder.
- **Open event:** the `file_open` program sends, for each file opened in one of the requested mounts, the path (through `bpf_d_path`, in the sandbox view, with the path's bytes only), the inode, the mount and whether the open reads and whether it writes (the file's `FMODE_READ` and `FMODE_WRITE` bits, also in the event of a denied open). The other mounts send nothing. This confinement's queue has 1 MiB.
- **Repeats:** an LRU map of the confinement (16384 entries) keeps the file (device, inode and generation), the mode and the time of the last event. An open of the same file and mode within 1 s sends no event. The rename and link programs take the file out of the map. An event that did not fit in the queue does not enter the map.
- **Full queue:** one helper thread only drains the kernel queue, and another sends the events to the daemon. When the kernel queue goes past 3/4, the helper's goes past 32768 events or an event is dropped, the helper freezes the tab's cgroup (`cgroup.freeze`). It thaws it when the helper's queue falls below 4096 and the kernel's below 1/4. Each freeze goes into the helper's journal, with its length ("tab frozen for N ms"). After the connection ends, or on a clean exit, the helper does not thaw, because the rules go away with the connection; the daemon thaws, when it redoes the confinement.
- **Status:** `status` says `tab`: a helper that confines tabs. With an older helper, without that field, the tab stays with the supervisor.

## Execution and connection observation

The `observe` request turns on only the observation of executions in a user cgroup (a tab's scope), without file rules or marks. A program on the `sched_process_exec` tracepoint sends the executed path, the process, the parent and the arguments over the same connection, and one on `sched_process_exit` sends the end of each process (not of each thread), when the kernel has the tracepoint. The daemon uses the executions and the ends for the tab's [process tree](tui.md#processes). Two `fexit` programs, on the exit of `tcp_v4_connect` and `tcp_v6_connect`, send each TCP connection a process in the cgroup opens: the pid, the source port and the destination. The daemon uses these events for the [process of each connection](network.md#process-of-each-connection) in the network log. The Docker confinement (`confine`) includes the same observation of executions, without the ends. There are up to 64 active observations per user, besides the 16 confinements. The observation only logs: no execution or connection is denied or delayed.

## Sandbox scope

On a kernel with Landlock ABI 4 or 5, Landlock lacks the abstract socket and signal scoping and, on ABI 4, the device `ioctl` restriction. The launcher (`sandmux-run`) then asks the helper for the sandbox `scope`, before the command runs, with:
- the user namespace each launch creates;
- the sandbox's process 1;
- the list of devices in the sandbox's `/dev`.

Global BPF LSM programs, loaded with the first scope and released when none is left, deny a process in that user namespace (and the ones below it):
- a connection to an abstract socket created outside the sandbox;
- a signal to a process outside, or to the sandbox's process 1;
- an `ioctl` on a device outside the list, with the same commands Landlock allows.

The scope lasts while the launcher's connection is open, that is, while the sandbox lives. Without the helper or without the BPF LSM, the launch is refused, and `sandmux doctor` shows the paths ([Ubuntu 24.04](install.md#ubuntu-2404)). On a kernel with ABI 6, the launcher does not ask for the scope.

When the helper restarts (an upgrade, a crash), the scope comes back on the next helper:

- **In a daemon session:** the launcher hands the scope parameters and the sandbox's user namespace to the daemon, and the daemon watches one connection to the helper per tab. On a drop, it freezes the tab, registers the scope again (and the [files by the kernel](files.md#by-the-kernel), when the tab has them) and thaws it. Nothing in the tab runs without the scope.
- **In `run` without the daemon:** the launcher itself registers again as soon as the next helper answers. In that interval, the sandbox runs without the scope.

Without the scope back within 30 s, the sandbox ends. A launch that finds the helper being swapped tries again for up to 30 s. On 6.8, loading and dropping the scope programs takes seconds; so, on a kernel below ABI 6, the helper loads them when it starts, and they stay loaded for 60 s after the last scope.

## How long the rules last

The rules and marks apply while the daemon's connection to the helper is open. They go away when:

- the folder's Docker stops, or the tab ends;
- the daemon ends, even when killed by `SIGKILL`;
- the helper stops.

If the helper stops with the Docker running, the daemon stops the folder's Docker, which does not go on without the rules. A tab with its files by the kernel stays frozen until the confinement is redone with the helper back, and stops if it is not back within 30 s. On `SIGTERM` (`systemctl restart`, an upgrade), the helper shuts the writing side of each connection before it exits: whoever asked learns about it with the rules still in force, and they go away when it closes the connection (or after 2 s). Connections that arrive in the meantime close without an answer, and whoever asked tries the next helper. The next `docker` starts another Docker and turns the confinement on again, if the helper answers. The systemd socket starts the helper again on the next connection.

## Install

The helper needs two things:

- the kernel with the BPF LSM active, that is, `bpf` in the list in `/sys/kernel/security/lsm`;
- the binary in a root-owned folder.

**`sandmux setup`** writes the `sandmux-helper.socket` and `sandmux-helper.service` units to `/etc/systemd/system` and enables the socket. It uses `sudo` after asking, as it does with the AppArmor profile ([install.md](install.md#sandmux-setup)).

**After updating sandmux**, the running helper needs a restart: it only changes when restarted, and writing the new binaries is not enough. `install.sh` with `sudo` and the `.deb` and `.rpm` packages restart it on install, and `sandmux setup` restarts it when it finds a helper older than sandmux. By hand: `sudo systemctl restart sandmux-helper.service`. While the old helper is up, what depends on the new version may be missing or incomplete. `sandmux doctor` warns about it with `[warn] root helper version`, showing the helper's version and sandmux's, and the Processes section of the TUI shows the same warning. It is a warning, not a failure: the `doctor` exit code does not change.

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

The tests that need root (`crates/sandmux/tests/docker_ebpf.rs`) stay outside `check.sh`. They load BPF programs into the kernel, and a bug in a global program takes down the whole machine. That is why the default is to run them in an Ubuntu 24.04 VM:

```bash
scripts/vm-ubuntu-2404.sh up
scripts/vm-ubuntu-2404.sh docker
scripts/vm-ubuntu-2404.sh check-root
```

- `up` boots the VM, and `docker` installs the rootless Docker in it, once.
- `check-root` compiles the tests and the helper with musl and copies them into the VM, at the same path. Then it starts the helper as root in there, restarting it when a test asks, and runs the tests as the VM's user. A test filter can follow `check-root`.

`scripts/check-root.sh` runs the same tests on this machine's kernel:

```bash
sudo scripts/check-root.sh
```

It warns and asks before loading BPF on the host; `--host` skips the question. The script starts the helper as root on a temporary socket and runs the tests as the user who called `sudo`, with binaries compiled by that user. That is why allowing it in `sudoers` without a password gives root to whoever runs as that user. If you do that, do it only while you need it.
