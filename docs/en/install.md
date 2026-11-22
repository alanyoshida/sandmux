# Installation

> Português: [install.md](../install.md)

sandmux ships as static binaries for Linux `x86_64` that do not depend on the distro's glibc: `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat` and `sandmux-db-mcp`. They live in the same folder, because the client finds the others next to it.

Requirements: kernel 6.12 or newer (Landlock ABI 6), unprivileged user namespaces and `/dev/net/tun`. A kernel with Landlock ABI 4 or 5 (6.7 to 6.11) also works with the [root helper](helper.md#sandbox-scope) and the BPF LSM on (see [Ubuntu 24.04](#ubuntu-2404)). The tested distros are Ubuntu 26.04, Ubuntu 24.04, Debian 13, current Fedora and Arch.

## Ubuntu 24.04

The original Ubuntu 24.04 kernel, 6.8, has Landlock ABI 4. It lacks the abstract socket and signal scoping the sandbox requires. `sandmux doctor` shows the two paths, with the commands:

1. **The HWE kernel (recommended):** `sudo apt install linux-generic-hwe-24.04` and reboot. The HWE kernel has the full Landlock (in 2026-11, it was 7.0), and sandmux runs as on any new kernel.
2. **The 6.8 kernel with the root helper:** the helper completes Landlock through the BPF LSM. The BPF LSM ships in the Ubuntu kernel, but outside the list of LSMs enabled at boot. To enable it:

   ```bash
   echo "GRUB_CMDLINE_LINUX_DEFAULT=\"\$GRUB_CMDLINE_LINUX_DEFAULT lsm=$(cat /sys/kernel/security/lsm),bpf\"" | sudo tee /etc/default/grub.d/99-sandmux-lsm.cfg
   sudo update-grub
   ```

   Then reboot and run `sandmux setup`, which installs the helper. `sandmux setup` does not change the boot. Without the helper or without the BPF LSM, the sandbox does not start on 6.8.

The private Docker needs `rootlesskit` 2.0 or newer, and Ubuntu 24.04 ships 1.1.1. It comes in the `docker-ce-rootless-extras` package from [Docker's repository](https://docs.docker.com/engine/install/ubuntu/), which replaces Ubuntu's. Without it, `sandmux doctor` shows the private Docker as unavailable, and the rest of sandmux works.

## Install script

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

The script:

1. Asks for `sudo` right at the start.
   - **With privilege** (or running as root): installs into `/usr/local/bin`.
   - **Without privilege** (`sudo` refused, missing or `--no-sudo`): installs into `~/.local/bin` and warns when that folder is not in `PATH`.
2. Downloads `sandmux-x86_64-linux.tar.gz` and its `.sha256` from the release and checks the sum. A wrong sum stops everything before installing.
3. With privilege and the [root helper](helper.md) already installed, restarts its service, so the running helper becomes the new one. Without this, on an update, the old helper would stay up.
4. Runs `sandmux setup` as your user (see below). When `sudo` was refused, setup runs with `--no-sudo`.

| Option | What it does |
|---|---|
| `--no-sudo` | Does not use `sudo`: installs into `~/.local/bin` |
| `--prefix DIR` | Folder for the binaries, instead of the default |
| `--from SOURCE` | Installs from an extracted folder or a local tarball, without downloading |
| `--version vX.Y.Z` | Installs that version, instead of the newest |
| `--no-setup` | Does not run `sandmux setup` |
| `--uninstall` | Disables the daemon and removes the binaries and the AppArmor profile |

The tarball also ships `install.sh`: inside the extracted folder, `sh install.sh` installs its binaries.

`--uninstall` keeps the configuration (`~/.config/sandmux`), the data (`~/.local/share/sandmux`, with the vault and the synthetic HOMEs) and the state (`~/.local/state/sandmux`, with the logs). Delete them if you want.

## `sandmux setup`

It is the configuration step for every channel: script, packages, Homebrew and `cargo install`.

1. **AppArmor profile.** When the kernel has AppArmor's user namespace restriction (`kernel.apparmor_restrict_unprivileged_userns`, on by default in Ubuntu), the launcher needs its own profile, which allows user namespaces only for it. Setup:
   - writes the profile to `/etc/apparmor.d/sandmux-run`, with the path of the installed `sandmux-run`;
   - loads the profile with `apparmor_parser -r`;
   - uses `sudo` after asking.

   With `--no-sudo`, without `sudo` or without a terminal, it shows the two commands to run as root. The profile applies to one path: if the binaries move, run `sandmux setup` again.
2. **Root helper.** When `sandmux-helper` sits next to `sandmux`, in a root-owned folder, it writes the `sandmux-helper.socket` and `sandmux-helper.service` units to `/etc/systemd/system` and enables the socket, with the same `sudo`. This is what turns on the private Docker's eBPF confinement ([helper.md](helper.md)). For a binary the user can change, as in `~/.local/bin`, it refuses and says why. With `--no-sudo`, it shows the commands.

   With the units already installed and the running helper older than sandmux, setup restarts its service (`systemctl try-restart`), with the same `sudo`, after asking. With `--no-sudo`, without `sudo` or without a terminal, it shows the command `sudo systemctl restart sandmux-helper.service`.
3. **Daemon.** Installs and enables the systemd user unit `sandmuxd.service`, which starts the daemon at login ([sessions.md](sessions.md)). To have it start at boot, without a login, run `sandmux daemon boot on` ([sessions.md](sessions.md#start-at-boot)). `--no-daemon` skips this step, and as root it never runs, because the daemon is per user.
4. **Diagnostics.** Ends with `sandmux doctor`.

## Packages

| Channel | File | Installs into | Afterwards |
|---|---|---|---|
| Debian and Ubuntu | `sandmux_<version>_amd64.deb` | `/usr/bin`, profile in `/etc/apparmor.d/sandmux-run`, loaded on install, and the `sandmux-helper` units in `/usr/lib/systemd/system`, with the socket enabled | `sandmux setup` as the user |
| Fedora | `sandmux-<version>-1.x86_64.rpm` | `/usr/bin`, profile in `/etc/apparmor.d/sandmux-run`, `sandmux-helper` units with the socket enabled | `sandmux setup` as the user |
| Arch (AUR) | `sandmux-bin` (`PKGBUILD` from the release) | `/usr/bin`, `sandmux-helper` units with the socket enabled | `sandmux setup` as the user |
| Homebrew (Linux) | `sandmux.rb` formula from the release | the Homebrew folder, without `sandmux-helper` | `sandmux setup` (Homebrew does not run `sudo`) |
| Tarball | `sandmux-<version>-x86_64-linux.tar.gz` | wherever `install.sh` puts it | `install.sh` already runs setup |

```sh
sudo apt install ./sandmux_0.1.33_amd64.deb
sudo dnf install ./sandmux-0.1.33-1.x86_64.rpm
```

## From source

Requires Rust 1.88 or newer.

```sh
cargo install --git https://github.com/alanyoshida/sandmux sandmux
sandmux setup
```

`cargo install` installs the binaries together into `~/.cargo/bin`.

To build a checkout and produce the same distribution as the release (static binaries, tarballs, `.deb`, `.rpm` when `rpmbuild` is present, `PKGBUILD` and formula in `dist/`):

```sh
rustup target add x86_64-unknown-linux-musl
./scripts/dist.sh
```

The C code in `ring` builds with the system gcc (`CC_x86_64_unknown_linux_musl=gcc`, which the script already sets), without `musl-gcc`.

## Release

A `v*` tag on GitHub runs `.github/workflows/release.yml`. The workflow:

1. builds the distribution with `scripts/dist.sh`, on an Ubuntu runner with `musl-tools` and `rpm`;
2. checks that the tag is the version in `Cargo.toml`;
3. creates the release as the latest (`--latest`, so that `/releases/latest/` serves `install.sh`) with the tarball, the sums, the `.deb`, the `.rpm`, `install.sh`, the `PKGBUILD`, the `SRCINFO` (the AUR `.SRCINFO`, without the dot, which GitHub would strip) and the formula.

Publishing to the AUR, to a Homebrew tap and to crates.io is up to the maintainer, with their own accounts. The files come out ready in `dist/aur/` and `dist/homebrew/`.

## Testing on the distros

`scripts/distro-test.sh` tests `dist/` in Debian 13, current Fedora and Arch containers on the system Docker. The options `seccomp=unconfined`, `apparmor=unconfined` and `systempaths=unconfined` are needed because Docker blocks the user namespace and the `/proc` mount that the sandbox uses. In each container:

- **as root:** installs the native package (on Arch, the tarball through `install.sh`, and checks the `.SRCINFO` with `makepkg`) and runs `sandmux --version`;
- **as a regular user:** `install.sh --no-sudo`, `sandmux doctor` and a sandbox without network;
- **at the end:** removes the package.

On Fedora, the script also builds the `.rpm` into `dist/`.

The kernel, Landlock and AppArmor are those of the machine running the containers: the test proves the binaries, the packages and each distro's user space.

Ubuntu 24.04 is tested in a VM, with its own kernel, by `scripts/vm-ubuntu-2404.sh`:
- the script downloads the official cloud image to `~/.cache/sandmux-vm/` and checks the sum;
- it boots the VM with QEMU/KVM (4 GB), with a guard that shuts it down if the host runs low on memory;
- it has the `doctor` steps: `lsm-bpf` turns on the BPF LSM at boot, and `hwe` installs the HWE kernel;
- it runs the root tests in there: `docker` prepares the rootless Docker, and `check-root` runs the tests (see [helper.md](helper.md#tests)).

New BPF programs are tested only in the VM: a bug in them takes down the whole kernel.
