# Security

> Português: [security.md](../security.md)

This document is for whoever needs to decide whether sandmux can be trusted: the threat model, what each layer prevents, what stays out of scope, and the known limits. How the layers work is in [architecture.md](architecture.md).

sandmux is in development, before v1. Everything here describes the behavior of the current specs (`openspec/specs/`), covered by tests.

## Threat model

**The threat:** a coding agent manipulated by prompt injection (in a project file, a web page, a tool's answer) that actively tries to:

- read secrets: SSH keys, cloud credentials, other programs' tokens, `.env`, API keys;
- send data out: the code, whatever secrets it finds, the contents of the disk;
- leave something planted that runs outside the sandbox: a git hook, a shell login script;
- reach the rest of the machine: other processes, host sockets, the host's Docker.

**What is assumed trusted:**

- the Linux kernel and the hardware;
- you, the user: what you allow on purpose holds;
- the host outside the sandbox: the sandmux binaries, the daemon and the programs that run as your user outside a tab;
- the destinations you allowed or that own a secret in the vault.

The agent runs with your UID. The barrier is not another user: it is the namespaces, Landlock, seccomp and the gateway.

## What each layer prevents

| The agent tries to | What prevents it | Details |
|---|---|---|
| Read `~/.ssh`, `~/.aws`, `~/.config/gh` and the rest of HOME | HOME is synthetic; from the real HOME only the working folder, the known toolchains (with their credential files empty) and what you add get in | [files.md](files.md) |
| Read `.env`, `*.pem` or `id_rsa` in the working folder | Secret-looking files do not open (`ENOENT`), unless the agent created them itself | [files.md](files.md#secret-looking-files) |
| Read `/etc/shadow`, host keys, VPN configurations | Those files show up empty; `passwd` and `group` are generated | [architecture.md](architecture.md#files) |
| Steal the provider's API key | The real key never gets in: the agent has a fake, swapped for the real one in the gateway only when the destination owns the key | [vault.md](vault.md#key-swap-in-the-traffic) |
| Send data to an exfiltration destination | The denylist ships with a catalog of known destinations, on; the `inferencia`, `custom` and `sem-rede` modes restrict more; every connection is logged, and a new domain is highlighted | [network.md](network.md#modes) |
| Reach cloud metadata (`169.254.169.254`) | Blocked in every mode, together with QUIC and ICMP | [network.md](network.md#fixed-blocks) |
| Plant a hook or a remote in `.git` | `.git/hooks` and `.git/config` are read-only, and creating `.git/commondir` is denied | [git.md](git.md#what-is-protected) |
| Plant something in the host's `.bashrc` or `~/.local/bin` | Those paths cannot be mounted with write access; neither can sandmux's configuration folder | [config.md](config.md#paths-the-host-runs) |
| Talk to the host's ssh-agent, D-Bus, Docker, Wayland or X11 | Host sockets do not exist in the sandbox, and Landlock denies abstract sockets from outside | [architecture.md](architecture.md#the-sandbox-layers) |
| See, signal or debug host processes | The pid namespace shows only the sandbox, and Landlock denies signals going out | [architecture.md](architecture.md#the-sandbox-layers) |
| Write into the user's terminal (inject commands) | Inserting characters into the terminal's input queue is denied | [architecture.md](architecture.md#the-sandbox-layers) |
| Gain privilege | No capabilities, `setuid` does not elevate, and mount, `bpf`, modules, `kexec`, `io_uring` and similar are denied by seccomp | [architecture.md](architecture.md#the-sandbox-layers) |
| Exhaust the machine's memory or processes | Memory and process caps per launch; the kernel kills only that sandbox's processes | [limits.md](limits.md) |
| Use the host's Docker | The host socket never gets in; each folder has its own rootless Docker, with the same file view and the same network as the sandbox | [docker.md](docker.md) |
| Read the database password | The daemon connects, outside the sandbox; the password never gets in | [database.md](database.md) |

## Out of scope

- **Kernel exploits.** sandmux uses the kernel as the barrier. An agent that exploits a kernel flaw leaves the sandbox. The GPU, allowed by default, and the private Docker, which uses mounts and networking in a user namespace, widen the part of the kernel exposed to the agent.
- **What you allow on purpose:**
  - `--rw` and `--ro` of a folder with credentials (`run` warns on every launch);
  - `--pass-env`, which puts a variable's real value in the sandbox when it has no secret in the vault;
  - `--decrypt`, which widens TLS decryption;
  - the `aberta` (open) mode, the default, where everything outside the denylist passes.
- **The host outside the sandbox.** A process of your user with the privilege to read other processes' memory reaches the open vault. `sandmux update` runs the harness's official updater on the host, outside the sandbox ([sessions.md](sessions.md#updating-the-harness)).
- **What happens at the destination.** sandmux controls where the traffic goes and swaps the keys; it does not decide what the provider, the cloud or the database does with an authorized request.
- **Files you run later.** Besides `.git/hooks` and `.git/config`, the working folder has files you run on the host: `Makefile`, `package.json` scripts, `.envrc`, editor tasks. What the agent writes there needs to be read first.

## Known limits

**Network:**

- In `aberta` (open) mode, the contents of the working folder can be sent to any domain outside the denylist. The defense is the log, the new-domain highlight and fast blocking. Whoever needs a guarantee uses the `custom` or `inferencia` modes.
- In `aberta` mode, a denylisted destination behind the same CDN as an allowed site is only caught by SNI or `Host`. A client that sends an empty SNI to a CDN IP gets through.
- In `aberta` mode, a direct connection by IP, without a DNS query, passes, logged as "no domain". In the restricted modes it is blocked.
- With the local network allowed in `aberta` mode, local services without a password are within the agent's reach.
- A new network rule applies to the next connections; connections already open are not cut.

**Secrets:**

- The fake is useful against the secret's owner: the agent can use the inference key through the gateway, and spend credits, by design. Each use is logged, with the secret's name.
- The swap is only in headers. Providers that ask for the key in the body, or programs that pin the server certificate, are not served.
- Cloud identities apply in every tab, and the only limit is IAM (or RBAC, on Kubernetes): an identity that can create keys or read a cloud secret store lets the agent get a real credential through the API itself ([cloud.md](cloud.md#what-the-agent-can-do)).
- In the Antigravity CLI login, the token has the `cloud-platform` scope: the agent in the `agy` tab can act on the user's Google Cloud through the gateway, with each use logged ([agy.md](agy.md#google-login)).
- With chosen SSH keys, the agent signs as many times as it wants while the tab lives; each signature is logged, and outgoing SSH only reaches those keys' servers.
- In the database MCP, the protection depends on the permissions of the user registered in the database.

**Files and state:**

- The agent can read, change and delete what is in the working folder. It is the real folder.
- The synthetic HOME is shared among the tabs of the same harness: one project can affect skills or memory used by another.
- In a git worktree, the agent reaches the history and branches of the other worktrees of the same repository.
- With the kernel path, a secret created on the host during the session is readable for up to about 1 s, until it is marked.
- What is put in the inbox folder (`~/inbox`) is visible to every agent until it is deleted.

**Private Docker:**

- A container runs in the `rootlesskit` user namespace, without Landlock and without the file-open supervisor. The file view is the same as the sandbox's, and with the root helper BPF LSM confines the containers ([docker.md](docker.md#ebpf-confinement)).
- Without eBPF confinement, the protections made by mounts (covered secrets, read-only `.git`) fall for a container with `--privileged`, and a file created while Docker is already running is not covered. With the confinement, the kernel denies by file, with a window of about 200 ms for new files ([docker.md](docker.md#limits)).

## How to verify

- **`sandmux doctor`:** what the machine offers (user namespaces, Landlock, BPF LSM, root helper, caps, Docker) and what is missing.
- **`sandmux enter <id>`:** a shell inside the sandbox of a live tab, with what the agent sees ([sessions.md](sessions.md#shell-inside-a-tabs-sandbox)).
- **`sandmux net <id>` and `sandmux files <id>`:** the tab's network and file log, with the blocks ([network.md](network.md#log), [files.md](files.md#log)).
- **`sandmux net rules <id>` and `sandmux config show`:** the effective policy and where each rule comes from.
- **The specs and the tests:** every behavior in this document is a requirement in `openspec/specs/`, and the test suite includes attempts to escape the sandbox.
