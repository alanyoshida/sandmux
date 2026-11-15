# Installation

> Português: [install.md](../install.md)

sandmux ships as static binaries for Linux `x86_64` that do not depend on the distro's glibc: `sandmux`, `sandmuxd`, `sandmux-run`, `sandmux-chat` and `sandmux-db-mcp`. They live in the same folder, because the client finds the others next to it.

Requirements: kernel 6.12 or newer (Landlock ABI 6), unprivileged user namespaces and `/dev/net/tun`. The tested distros are Ubuntu 26.04, Debian 13, current Fedora and Arch.

## Install script

```sh
curl -fsSL https://github.com/alanyoshida/sandmux/releases/latest/download/install.sh | sh
```

The script:

1. Asks for `sudo` right at the start.
   - **With privilege** (or running as root): installs into `/usr/local/bin`.
   - **Without privilege** (`sudo` refused, missing or `--no-sudo`): installs into `~/.local/bin` and warns when that folder is not in `PATH`.
2. Downloads `sandmux-x86_64-linux.tar.gz` and its `.sha256` from the release and checks the sum. A wrong sum stops everything before installing.
3. Runs `sandmux setup` as your user (see below). When `sudo` was refused, setup runs with `--no-sudo`.

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
2. **Daemon.** Installs and enables the systemd user unit `sandmuxd.service`, which starts the daemon at login ([sessions.md](sessions.md)). `--no-daemon` skips this step, and as root it never runs, because the daemon is per user.
3. **Diagnostics.** Ends with `sandmux doctor`.

## Packages

| Channel | File | Installs into | Afterwards |
|---|---|---|---|
| Debian and Ubuntu | `sandmux_<version>_amd64.deb` | `/usr/bin`, profile in `/etc/apparmor.d/sandmux-run`, loaded on install | `sandmux setup` as the user |
| Fedora | `sandmux-<version>-1.x86_64.rpm` | `/usr/bin`, profile in `/etc/apparmor.d/sandmux-run` | `sandmux setup` as the user |
| Arch (AUR) | `sandmux-bin` (`PKGBUILD` from the release) | `/usr/bin` | `sandmux setup` as the user |
| Homebrew (Linux) | `sandmux.rb` formula from the release | the Homebrew folder | `sandmux setup` (Homebrew does not run `sudo`) |
| Tarball | `sandmux-<version>-x86_64-linux.tar.gz` | wherever `install.sh` puts it | `install.sh` already runs setup |

```sh
sudo apt install ./sandmux_0.1.7_amd64.deb
sudo dnf install ./sandmux-0.1.7-1.x86_64.rpm
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
