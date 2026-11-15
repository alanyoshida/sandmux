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
- the inbox folder, read-only.

These do not enter:

- the HOME, neither the real nor the synthetic one, and therefore `~/.ssh`, `~/.aws` and `~/.cache`;
- the extra folders from the sessions' `--ro` and `--rw`, because the Docker belongs to the folder and each session may have different options.

A path outside the view fails or arrives empty in the container, even when reached through a symbolic link planted in the work folder. To give a container a file from outside, put it inside the work folder.

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

- **Why a slice:** rootless `dockerd` runs without managing cgroups (`Cgroup Driver: none`). runc puts each container in a cgroup next to the Docker's scope, in its parent. A cap only on the scope would leave the containers out; in the slice, the scope and the containers are inside.
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

## Limits

- A container runs with Docker's seccomp and in the `rootlesskit` user namespace, not with sandmux's Landlock and seccomp. The file view is the same as the sandbox's, and the unprivileged user namespace prevents `bpf`, modules and mounts on the host.
- `--privileged` gives no real privilege: the container's root is the user, mapped.
