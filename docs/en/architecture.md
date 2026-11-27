# Architecture

> Português: [architecture.md](../architecture.md)

This document explains how sandmux confines an agent, for readers who do not know the project yet. Each section ends with a link to the document that has the details. What each layer protects, and what stays out, is in [security.md](security.md).

## Overview

sandmux is a tmux-style terminal multiplexer. Each tab runs an agent harness (opencode, Claude Code, hermes, oh-my-pi, Antigravity CLI) or a shell, and every tab has a sandbox: there is no tab without confinement.

Inside the tab, the agent sees:

```text
host                                  sandbox
~/projects/app/   <-- same folder --> ~/projects/app/    read and write
~/ (the rest)                         ~/ (synthetic HOME, separate)
/usr, /etc, ...   ------------------> read-only, /etc with the secrets covered
network           ---- gateway ----->  everything goes through the gateway, logged
real keys         ----   vault   ---->  fake values
```

The working folder is the host's real folder, at the same path: what the agent writes shows up in your editor right away. The rest of HOME does not exist in there. The network goes out through a gateway that logs and filters. No real key gets in: the agent receives a fake value, and the gateway swaps it for the real one in the traffic.

All of this uses Linux kernel primitives (namespaces, Landlock, seccomp), with no VM, no container and no root for normal use.

## The binaries and the trust boundaries

```text
+--------------+  user     +----------------------+  socket  +----------------+
| sandmux      |<--------->| sandmuxd             |<-------->| sandmux-helper |
| client       |  socket   | session daemon:      |          | root helper,   |
| (CLI and TUI)|           | PTYs, vault,         |          | optional:      |
+--------------+           | policy, logs         |          | BPF LSM        |
                           +----------+-----------+          +----------------+
                                      | launch specification
                                      v
                           +----------------------+
                           | sandmux-run          |  stays on the host:
                           | launcher and         |  network gateway and
                           | supervisor           |  file supervisor
                           +----------+-----------+
                                      | namespaces, Landlock, seccomp
                                      v
                           +----------------------+
                           | harness (agent)      |  sandmux-chat and
                           | inside the sandbox   |  sandmux-db-mcp also
                           +----------------------+  run here
```

| Binary | Role | Where it runs |
|---|---|---|
| `sandmux` | Command line and TUI. Never touches a secret or a sandbox | On the host, as the user |
| `sandmuxd` | Session daemon, one per user: keeps the tabs' terminals, the vault open in memory, the effective policy and the logs | On the host, as the user |
| `sandmux-run` | Launcher: applies a ready launch specification. Then it stays as the sandbox supervisor, with the network gateway and the file-open supervisor | On the host, as the user |
| `sandmux-helper` | Optional root helper: attaches BPF LSM programs, only to restrict, to the cgroups and user namespaces of whoever asks | On the host, as root |
| `sandmux-chat` | The chat tab with the models | Inside the sandbox |
| `sandmux-db-mcp` | Relay for the database MCP: copies stdio to a daemon socket | Inside the sandbox |

The boundaries:

- **The client** talks to the daemon over a socket in a folder only the user can reach (mode 700). The daemon checks the UID of each connection (`SO_PEERCRED`) and refuses any other user.
- **The daemon** has no interface code. It resolves the policy and sends the launcher a complete specification.
- **The launcher** knows nothing about harnesses or policy. It sets up exactly what the specification describes and refuses a specification that is malformed, of an unknown version, or has a field it does not understand.
- **The root helper** accepts six requests, identifies the caller by the socket credentials, only denies operations, and undoes everything when the connection closes. It has no network, TLS or database.
- **The sandbox** cannot reach the daemon's socket or the helper's: `/run` and `/tmp` are new in there, and Landlock denies the host's abstract sockets.

Details: [sessions.md](sessions.md#socket-and-access), [launch-spec.md](launch-spec.md), [helper.md](helper.md).

## From the command to the tab

```text
sandmux run opencode
  |
  | 1. client: checks, first-run import, request to the daemon
  v
sandmuxd
  | 2. resolves the policy layers (baseline, preset, global, profile,
  |    project, run options) and builds the launch specification
  v
sandmux-run --spec-fd N
  | 3. checks kernel support; without it, refuses (never starts unconfined)
  | 4. creates user, mount, pid and network namespaces
  | 5. builds the view: working folder, synthetic HOME, read-only system,
  |    generated and covered /etc, toolchains with a write layer
  | 6. applies Landlock and the seccomp filter, no capabilities, no setuid
  | 7. stays outside the sandbox as the supervisor: gateway and file opens
  v
harness, with the terminal of the daemon session
```

In a terminal, the tab is a daemon session: it stays alive when you close the terminal, as in tmux, and you come back with `sandmux attach`. Without a terminal (script, pipe, CI), `run` runs directly, in the foreground, without the daemon, with the same sandbox.

Because the sandbox is the barrier, harnesses run without their own permission prompts; `harness-asks = true` brings them back ([config.md](config.md#harness-prompts)).

Details: [sessions.md](sessions.md), [launch-spec.md](launch-spec.md).

## The sandbox layers

```text
  agent (opencode, Claude Code, hermes, ...)
     |
     |  system calls                        network traffic
     v                                         v
+-------------------------------+   +------------------------------+
| seccomp: denies mount, bpf,   |   | network namespace: only lo   |
| perf, modules, io_uring, ...; |   | and a TUN; no direct route   |
| sends file opens to the       |   | out                          |
| supervisor                    |   +--------------+---------------+
+-------------------------------+                  |
| Landlock: what can be read,   |                  v
| written and executed; scope   |   +------------------------------+
| for abstract sockets, signals |   | gateway (in the supervisor): |
| and ioctl                     |   | DNS, domain policy,          |
+-------------------------------+   | decryption only where there  |
| mount namespace: what exists  |   | is a secret, key swap, log   |
| (synthetic HOME, covered      |   +--------------+---------------+
| /etc, minimal /dev)           |                  |
+-------------------------------+                  v
| user and pid namespaces: same |          internet and local network
| UID, no capabilities, only    |
| the sandbox's processes       |
+-------------------------------+
| cgroup: memory and process caps                  |
+--------------------------------------------------+
```

- **user and pid namespaces:** the agent runs with your UID, without capabilities, and sees only the processes of its own sandbox. setuid programs do not raise privilege. It can create nested user namespaces (headless browsers that sandbox themselves work), but it still cannot mount anything.
- **mount namespace:** defines what exists for the agent. Only the working folder, the synthetic HOME, the read-only system, the toolchains and whatever you added with `--ro` or `--rw`.
- **Landlock:** defines what can be done with what exists. It also blocks connecting to the host's abstract sockets, signals to outside processes, and `ioctl` on devices outside the list.
- **seccomp:** denies mount and root switching, `bpf`, `perf_event_open`, module loading, `kexec`, the kernel keyring, `userfaultfd` and `io_uring`, with a permission error. It also sends every file open to the supervisor.
- **cgroup:** a memory cap (75% of RAM by default) and a cap of 8192 processes and threads per launch. Past the cap, the kernel kills processes of that sandbox only.

Details: [files.md](files.md), [network.md](network.md), [limits.md](limits.md).

## Files

- **Working folder:** the real folder, at the same path, read and write. Being HOME, the root, or a folder that contains HOME is refused. `.git/hooks` and `.git/config` are read-only, so nothing the agent plants runs when you use git on the host ([git.md](git.md#what-is-protected)).
- **Synthetic HOME:** one per harness, kept in sandmux's data directory, with the harness configuration imported and the secrets replaced by fake values.
- **Toolchains and caches:** a known list (nvm, bun, cargo, rustup, uv, pyenv, go, linuxbrew) is read from the host and written to a private layer of the harness. `cargo install` and `npm install -g` work without changing the host. Their credential files show up empty.
- **`/etc`:** generated `passwd` and `group`; `shadow`, `sudoers`, host private keys, VPN and Wi-Fi configurations and other sensitive files show up empty. `/proc`, `/tmp` and `/run` are new. `/dev` has only the minimum, plus the GPU.
- **Secret-looking files:** in the working folder, `.env`, `*.pem`, `id_rsa*` and similar do not open (`ENOENT`), unless the agent created them itself ([files.md](files.md#secret-looking-files)).
- **File-open interception:** a seccomp filter with notification sends every `open` to the supervisor. The supervisor opens the file itself, inside the sandbox root, checks the rules against the file it actually opened, and hands the descriptor to the process. Changing the path in memory after it was read changes nothing. Every open in the agent's folders goes to the log, and `file-deny` applies on the next open, without relaunching the agent.
- **By the kernel:** with the root helper and BPF LSM, the kernel decides and logs the tab's opens instead of the supervisor, by the inode of the marked files ([files.md](files.md#by-the-kernel)).

Details: [files.md](files.md), [config.md](config.md#paths-the-host-runs).

## Network

```text
sandbox                               supervisor (sandmux-run, on the host)
+---------------------------+         +-------------------------------+
| lo                        |  TUN    | gateway                       |
| sandmux0  10.0.2.100/24  -+-------->|   TCP and UDP redone by host  |
| DNS 10.0.2.3              |         |   DNS by the host resolver    |
+---------------------------+         |   policy and log              |
                                      +-------------------------------+
```

- **Own network namespace, unprivileged.** The sandbox's process 1 creates a TUN and passes the descriptor to the supervisor. There is no direct route out: every connection ends in a user-space TCP/IP stack and is made again from the host.
- **The gateway's DNS.** Each query is logged, and the answer binds the IP to the name for the next connections. A denied name gets NXDOMAIN.
- **Four modes:** `aberta` (open, the default: everything passes except the denylist, which ships with a catalog of exfiltration destinations), `inferencia` (only the inference providers), `custom` (only the allowlist) and `sem-rede` (no network). In every mode, cloud metadata, QUIC and ICMP are blocked. A new rule applies on the next connection.
- **Decryption only where there is a secret.** The gateway decrypts TLS only for domains that own secrets in the vault and the ones you ask for. Each launch generates its own CA, which never touches the disk. Everything else passes end-to-end encrypted.
- **Log.** Each connection, DNS query and decrypted request goes to `~/.local/state/sandmux/net/`, with the domain, the destination, the volume, the process and the block. For decrypted requests, also the method and the path; never headers or body.

Details: [network.md](network.md).

## Secrets

- **Vault:** a file encrypted with your password (Argon2id and XChaCha20-Poly1305), opened by the daemon and kept only in memory while unlocked. Optionally, the password stays in the system keyring ([vault.md](vault.md)).
- **Fake values:** each secret has a fake (`sandmux-fake-` and 32 characters). The fake is what goes into the harness environment and the synthetic HOME.
- **Swap in the traffic:** in a decrypted request, the gateway swaps the fake for the real value in the headers, only when the request's domain owns the secret. A fake sent to another destination stays fake. The real value stays in the gateway's memory only during the connection.
- **Git over SSH:** a session's own SSH agent signs with the chosen keys, kept in the vault, and outgoing SSH only reaches their servers ([git.md](git.md)).
- **Cloud credentials:** the agent receives a fake credential per GCP, Kubernetes or AWS identity, and the gateway swaps the token, or re-signs the request, on the way to the cloud ([cloud.md](cloud.md)).
- **Database:** the credentials stay in the vault, and sandmux's database MCP reaches the database through the daemon, which connects from outside the sandbox; the password never gets in ([database.md](database.md)).

Details: [vault.md](vault.md#key-swap-in-the-traffic).

## Protection levels

The launcher checks kernel support before each launch and refuses when something is missing. There is no option to launch without confinement.

```text
kernel with Landlock ABI 6 (6.12 or newer)
   +-- without the root helper --> full sandbox; opens through the supervisor
   +-- with the root helper ----> the same, plus: the tab's opens decided and
                                  logged by the kernel (BPF LSM), executions
                                  and connections observed by the kernel, and
                                  the private Docker confined by BPF LSM

kernel with Landlock ABI 4 or 5 (6.7 to 6.11)
   +-- with the root helper and BPF LSM active --> the helper completes the
   |                              abstract socket, signal and ioctl scope
   |                              that Landlock lacks
   +-- without them ------------> refuses to start the agent

without unprivileged user namespaces ---------> refuses to start the agent
```

The root helper is the only piece that runs as root. It only restricts, only the cgroups and namespaces of whoever asks, and only while the request's connection is open. When it restarts, a daemon session's tab stays frozen until the rules come back. `sandmux doctor` shows what the machine offers and what is missing.

Details: [helper.md](helper.md), [install.md](install.md#ubuntu-2404).

## Daemon and persistence

- **One daemon per user**, started by the systemd user manager, or as its own process without systemd.
- **Tabs survive** a closed terminal and a daemon restart: the session list lives in the data directory, and tabs come back with the same options. After a reboot, they come back waiting for the vault password when they need it.
- **Private Docker:** each working folder has its own rootless Docker, separate from the host's Docker, with the same file view and the same network as the sandbox ([docker.md](docker.md)).

Details: [sessions.md](sessions.md#how-the-daemon-starts).
