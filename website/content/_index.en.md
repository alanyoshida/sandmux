---
title: sandmux
description: terminal multiplexer that runs coding agents inside a sandbox
headline: Every agent in a confined tab.
lede: A tmux-style terminal multiplexer that runs opencode, Claude Code, hermes and other agents inside a sandbox on Linux. The agent sees the project folder and nothing else of your HOME, the network goes through a gateway that logs and filters, and no real credential ever gets inside.
install: curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
cta_primary: Install and get started

sees_title: What the agent sees
sees_text: You run `sandmux run opencode` in the project folder. The folder is mounted at the same path, read-write; the rest of the system is a different one.
sees_col_resource: Resource
sees_col_inside: Inside the sandbox
sees:
  - resource: Working folder
    inside: The real folder, read-write. `.git/hooks` and `.git/config` are read-only.
  - resource: HOME
    inside: A synthetic HOME per harness. Your real HOME does not exist in there.
  - resource: System (`/usr`, `/etc`)
    inside: Read-only, with the sensitive files covered.
  - resource: Toolchains (cargo, nvm, uv, go)
    inside: Read from the host and written to a private layer. `cargo install` and `npm install -g` work without changing the host.
  - resource: Files that look like secrets
    inside: "`.env`, `*.pem` and `id_rsa` in the folder do not open, unless the agent created them itself."
  - resource: Host sockets
    inside: ssh-agent, D-Bus, Docker, Wayland and X11 out of reach.
  - resource: API keys and tokens
    inside: Fake values only. The gateway swaps in the real one on the wire, and only when the destination owns the key.

gallery_title: The interface
gallery_text: Tabs as in tmux, with Ctrl+b. The side panel shows the protection applied, memory, and every connection of the tab, with the reason for each block. The screens below are recordings of the real sandmux.

layers_title: Kernel layers, no VM or container
layers_text: The sandbox uses what Linux already offers, with no root in normal use. Without Landlock and user namespaces, the agent does not start; there is no tab without a sandbox.
layers_sys: System calls
layers_net: Network traffic
layers_agent: agent
layers_agent_does: opencode, Claude Code, hermes, oh-my-pi, Antigravity or a shell
layers:
  - name: seccomp
    does: Denies mounting, `bpf`, modules, `kexec`, `io_uring` and the like, and sends every file open to the supervisor.
  - name: Landlock
    does: What can be read, written and executed; no abstract sockets, signals or `ioctl` to the outside.
  - name: mount namespace
    does: What exists for the agent. Synthetic HOME, `/etc` covered, minimal `/dev`.
  - name: user and pid namespaces
    does: Your UID, no capabilities, seeing only the sandbox's processes.
  - name: cgroup
    does: Memory and process caps; past the cap, the kernel kills only that sandbox.
net:
  - name: network namespace
    does: Only a TUN, no direct route out.
  - name: gateway
    does: DNS, domain policy, decryption only where there is a secret, key swap and logging.
  - name: internet and local network
    does: Cloud metadata, QUIC and ICMP blocked in every mode.
layers_more: Read the architecture

security_title: Built for people who must decide whether to trust it
security_text: The threat is an agent manipulated by prompt injection, trying to read secrets and send data out. The threat model, what each layer prevents and the limits are written down, with links to the specs.
stops_title: What the sandbox prevents
stops:
  - Reading `~/.ssh`, `~/.aws`, `~/.config/gh` and the rest of HOME
  - Stealing the provider's API key
  - Sending data to known exfiltration destinations
  - Planting a hook or a remote in `.git`
  - Talking to the host's ssh-agent, D-Bus or Docker
  - Seeing or signaling host processes
limits_title: What it does not promise
limits:
  - Surviving a kernel exploit
  - Protecting what you open on purpose, such as `--rw` or `--pass-env`
  - "In `open` mode, the default: stopping the code from going to a domain outside the denylist; it logs and highlights it"
  - Stopping the fake key from being used against its own owner, such as spending inference credits
security_more: Read the threat model and the limits

install_title: Installation
install_text: Static binaries for Linux `x86_64`, kernel 6.12 or newer. The script asks for `sudo`; without it, it installs to `~/.local/bin`. There are also `.deb`, `.rpm`, AUR, Homebrew and `cargo install`.
install_steps:
  - curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
  - sandmux doctor
  - "# in the project folder"
  - cd ~/projects/app
  - sandmux run opencode
install_more: Installation guide, including Ubuntu 24.04
---
