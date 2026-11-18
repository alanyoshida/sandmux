# Private Docker

> Português: [docker.md](../docker.md)

Each work folder has its own rootless Docker, which the agent uses from inside the sandbox with `docker` and `docker compose`. The host's Docker socket never enters the sandbox, because it is equivalent to root on the machine.

## How the agent uses it

Every daemon session receives the socket at `/run/sandmux/docker.sock`, with `DOCKER_HOST=unix:///run/sandmux/docker.sock` in the environment. The system's `docker` and the `docker compose` plugin work without configuration.

`run` without a terminal, from scripts and CI, has no private Docker: there is no daemon to keep it.

## What the containers see

`dockerd` runs in the `rootlesskit` user namespace, in a root mounted by the same composition as the sandbox. That is why a `-v`, a `--mount` or a `compose` volume only reaches:

- the system, read-only, with the sensitive files of `/etc` covered;
- the work folder, writable and with the `.git` protection (hooks and config read-only);
- the inbox folder, read-only;
- the sandbox's `/dev`: the `null`, `zero`, `full`, `random`, `urandom` and `tty` nodes, its own pseudo-terminals and its own `/dev/shm`. The GPU and `/dev/kvm` enter according to the `gpu` and `kvm` keys of the folder's [configuration](config.md), without the sessions' `run` options.

These do not enter:

- the HOME, neither the real nor the synthetic one, and therefore `~/.ssh`, `~/.aws` and `~/.cache`;
- the extra folders from the sessions' `--ro` and `--rw`, because the Docker belongs to the folder and each session may have different options;
- the host's other devices: the user's terminals, the camera, audio, `/dev/net/tun` and `/dev/fuse`. A `--device` for one of them fails, because the device does not exist in the view, and a `-v /dev:/d` shows only the `/dev` above.

A path outside the view fails or arrives empty in the container, even when reached through a symbolic link planted in the work folder. To give a container a file from outside, put it inside the work folder.

## Secrets and denied paths

Containers do not go through the sandbox's open supervisor ([files.md](files.md)). So that a `-v $PWD:/w` does not read what the agent does not read, the folder's Docker, when it starts, walks the work folder with the same file policy and covers:

- the secret-looking files, except those the agent itself created;
- the `file-deny` files and folders; a denied folder is covered whole.

A covered path reaches the container empty and read-only, and the host's file does not change. `file-allow` and `secret-files-off` apply as they do for the agent. The policy used is the folder's configuration, without the sessions' tab layer, like the caps and the devices.

With more than 4096 paths to cover, the private Docker does not start, and every `docker` shows the count and the cap. Shorten the list with `secret-files-off` or `file-allow` in the folder's configuration.

## eBPF confinement

The covers above are mounts, and a container with `--privileged` can undo them (see the limits). With the [root helper](helper.md) installed and the BPF LSM active in the kernel, each folder's Docker also starts with BPF LSM programs attached to its slice. They decide by file, not by path:

- **Denied for reading and writing:** the covered paths (the folder's secrets and `file-deny` and the `/etc` covers). In a denied folder, everything that exists inside it when the Docker starts; containers also cannot create, delete or rename anything inside it.
- **Write-protected:** `hooks`, `config` and `config.worktree` of each git directory in the folder, with the submodules. Opening for writing, deleting, renaming, changing attributes, linking and creating anything inside `hooks` are denied. Reading still works.
- **Reserved names:** in the git directories, creating `commondir`, `hooks`, `config` or `config.worktree`. The git directory itself cannot be deleted or renamed.

Unmounting a cover, removing the read-only flag, renaming, creating a hard link or mounting the folder elsewhere do not change the decision, which follows the inode. The denied operation fails with `EPERM`. The rules apply before the socket reaches the session and only to the folder's Docker processes: the agent, the host and another folder's Docker are not affected.

Without the helper, without `bpf` in the kernel's LSM list or without the caps slice, the Docker starts with the unprivileged protections only. `sandmux stats <id>` shows the "eBPF confinement" line with the reason, and `sandmux doctor` shows how to turn it on. If the helper stops with the Docker running, the folder's Docker stops, and the next `docker` starts another.

### Live

With the confinement on, the Docker that is already running follows the folder and the configuration:

- **New files:** the daemon watches the work folder with inotify. Within 1 second, it denies containers:
  - a secret-looking file created later, that the agent did not create;
  - everything that appears inside a denied folder, including what you create on the host.

  A secret the agent itself creates stays readable to containers. With no inotify watches available (`fs.inotify.max_user_watches`), the daemon rescans the folder every 10 seconds, and `sandmux stats` warns.
- **Live policy:** a change in the folder configuration's `file-deny`, `file-allow`, `secret-files` or `secret-files-off` applies on the next operation, without restarting the Docker.
  - Allowing a path removes the denial in the kernel, but the mount cover made at startup stays: an ordinary container sees the file empty until the Docker starts again.
- **Log:** every operation denied to a container and every file a container creates in the work folder go into the [file log](files.md#log), with the `docker-<id>` tag and the host path. Containers' ordinary reads and writes are not logged.

## When it starts and when it stops

- **Starts** on the first connection to the socket. A session that never runs `docker` starts no Docker at all. The first call waits 1 to 3 seconds.
- **Stops** when the last live session of that folder ends, and also on `sandmux daemon stop`. The containers that were running stop with it.

## What persists

Each folder's images, volumes and stopped containers stay in `~/.local/share/sandmux/docker/<identifier>`, with the folder's path in the `pasta` file. They come back in the next session of the same folder and are kept separate from the host's Docker and from the other folders. The image download cache across folders comes with the network gateway (change 8).

The image files belong to the IDs of the subordinate range (`/etc/subuid`), and a plain `rm -rf` does not delete them. To delete a folder's data, use one of these ways:

```bash
rootlesskit --net=host -- rm -rf ~/.local/share/sandmux/docker/<identifier>
```

- `docker system prune -a --volumes`, from inside a session of the folder.

## Ports and network

- **Ports:** a published port (`-p 8080:80`) stays only on the host's `127.0.0.1`. `dockerd` starts with `--ip 127.0.0.1`, and the port never listens on `0.0.0.0`.
- **From inside the sandbox:** the sandbox has its own network (see [network.md](network.md)), and its `127.0.0.1` is not the host's. The published port is reached through `host.sandmux.internal`, as in `http://host.sandmux.internal:8080`.
- **List:** `sandmux stats <id>` and the Ports section of the TUI panel show the ports published by the folder's containers, such as `80 → 127.0.0.1:8080 (web)`.
- **Network through the gateway:** each folder's Docker runs in a network namespace without its own network (`rootlesskit --net=none`; it does not need `slirp4netns`, `pasta` or `gvisor-tap-vsock`). The Docker view creates a TUN interface and hands it to the daemon, which runs a gateway like the sandbox's: the same DNS, the same fixed blocks and the same [network log](network.md), with the tag `docker-<id>`.
- **Policy:** the Docker's policy is the most restrictive among the folder's live sessions, in the order `sem-rede`, `inferencia`, `custom` and `aberta`. Among sessions with the same mode, the denylist is the union and the allowlist keeps the items they all have. It changes when a session of the folder starts or ends, and when the configuration changes. A container of a folder with a tab in `sem-rede` has no outbound network.

## Private registries

With a `docker` secret in the [vault](vault.md#tools) (for example `sandmux vault add registry --domain registry.empresa.com --tool docker`), `docker pull` from a private registry works without the real token in the sandbox:

- the session receives a `~/.docker/config.json` for that run only, with the fake value in `auths`;
- the folder's Docker gateway has its own CA, created when the Docker starts, and decrypts the domains that own the `docker` secrets;
- the Docker view has the system's CA bundle with that CA, and `dockerd` trusts the gateway;
- on each decrypted connection, the gateway asks the daemon for the secrets and swaps the fake for the real one in the `Authorization` (including inside the login's `Basic`) of the request to the owning domain.

Unlocking the vault after the Docker started applies on the next connection. With the vault locked, the request goes out with the fake and the registry refuses it.

## Image cache across folders

With the `registry` binary from the distribution project installed (on Debian and Ubuntu, the `docker-registry` package, which installs `docker-registry`), the daemon starts a Docker Hub mirror on the host's `127.0.0.1` and each private Docker uses it through `--registry-mirror`. The layers stay in `~/.local/share/sandmux/registry-cache`, and an image downloaded in one folder is not downloaded again in another.

- Only Docker Hub goes through the mirror; other registries (`ghcr.io`, `quay.io`) are downloaded per folder.
- For the policy, the connection to the mirror counts as access to `registry-1.docker.io`: in a Docker in `custom` mode without the `docker` group allowed, the mirror is blocked too.
- Without the binary, each folder downloads its own images, and `sandmux doctor` shows how to install it.

```bash
sudo apt install docker-registry
```

## Caps and usage

Each folder's Docker, with all its containers, runs in its own systemd slice (`sandmux-docker-<id>.slice`) with that folder's memory and process caps, resolved by the [configuration](config.md) and separate from each session's caps. A container that goes over the cap is killed, and the Docker keeps running.

- **Why a slice:** the cap sits above everything the containers see. The Docker runs in its own cgroup namespace, whose root is its scope (`sandmux: docker <folder>`), inside the slice: `dockerd` and the other processes stay in the `motor` subgroup, and runc creates each container next to it. A container with `-v /sys/fs/cgroup:/c` sees only the scope and the folder's Docker cgroups; it does not see the slice with the cap, nor `user.slice`, nor the sessions' and the daemon's cgroups. The scope has no cap of its own, because the containers reach it.
- **Without caps:** on a machine without the systemd user manager, the Docker runs in the same cgroup namespace, rooted at the cgroup it starts in, and the containers do not see the rest of the tree.
- **Who dies at the cap:** the containers' processes get `oom_score_adj` 900, and `dockerd`, `containerd` and the shims keep the session's value. That way, when the folder goes over the cap, the kernel kills a container, not the Docker. What does this is a `runc` of the Docker view itself, at the front of the `PATH`: it raises the value before creating the container and puts it in the container's `config.json`, because rootless `dockerd` writes the session's value there. One container may be taken along when the cap is exceeded.
- **`docker run -m`:** does not limit a container by itself; the cap that applies is the folder's.
- **End:** the slice is stopped together with the folder's Docker. Those left empty, from a daemon that was brought down, are stopped when the daemon starts again.

`sandmux stats <id>` shows the "Folder's private Docker" section, with the memory, the processes, the caps and how many containers are running.

## Without rootless Docker

The private Docker needs three things:

- `rootlesskit`, `dockerd`, `containerd`, `newuidmap` and `newgidmap` in the `PATH`;
- a range of 65536 of the user's IDs in `/etc/subuid`;
- an equal range in `/etc/subgid`.

Without them, the session starts normally, and every `docker` shows:

```
Error response from daemon: the private Docker is not available on this machine: <what is missing>; see `sandmux doctor`
```

## Denied system calls

`dockerd` starts with a sandmux seccomp filter, and every process of the folder's Docker inherits it: `containerd`, `runc` and every container. An inherited filter cannot be removed, so it also applies with `--privileged` and with `--security-opt seccomp=unconfined`, which only stop adding Docker's own filter.

- **Denied, with `EPERM`:** the same ones the agent's sandbox denies, minus the mount calls: `bpf`, `perf_event_open`, module loading, `kexec`, `userfaultfd`, `io_uring` and the `ioctl`s `TIOCSTI` and `TIOCLINUX`, which inject keys into the terminal.
- **Kernel keyring, with `ENOSYS`:** `keyctl`, `add_key` and `request_key` answer as if the kernel had no keyring. It is the only error `runc` accepts when creating a container; with any other, no container starts.
- **Allowed:** the mount calls, because `dockerd` and `runc` mount each container's root.
- **Setuid:** the filter does not turn on `no_new_privs`, and the images' `su` and `sudo` work.

## Limits

- A container runs in the `rootlesskit` user namespace, without sandmux's Landlock and without its open supervisor. What limits it is the file view, the seccomp filter above and the unprivileged user namespace, which prevents modules and mounts on the host.
- `--privileged` gives no real privilege: the container's root is the user, mapped.
- A container that needs a host device outside the list, such as a VPN with `/dev/net/tun` or a file system with `/dev/fuse`, does not work in the private Docker.
- **Without the eBPF confinement, covers fall to a privileged container.** With `--privileged`, a container has `CAP_SYS_ADMIN` in the user namespace where the view was mounted, and can unmount a cover or remove the read-only flag from a bind. This applies to the covered secrets and denied paths and to the `.git` protection (hooks and config read-only). An ordinary container cannot. With the [eBPF confinement](#ebpf-confinement), the kernel denies by file, and undoing the mount does not help.
- **Without the eBPF confinement, files created later are not covered.** A secret created in the folder with the Docker already running, a file you create inside a denied folder, or a change in the file policy, only applies when the folder's Docker starts again (after `sandmux daemon restart`, or when the folder's last session ends). With the confinement, they apply [live](#live), within 1 second.
- **Without the eBPF confinement, without the rest of the file policy:** containers have no file log, no live policy and no `.git/commondir` refusal. With it, the log has only the denials and the creations.
- **New-file window:** between a file's creation and the denial in the kernel, up to about 200 ms pass (the watcher's quiet wait), and a container that reads the file in that interval can read it.
