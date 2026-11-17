# Launch specification

> Português: [launch-spec.md](../launch-spec.md)

The launch specification is the contract between whoever composes the policy (the `sandmux` client and, in the future, the daemon) and the `sandmux-run` launcher. The launcher knows nothing about harnesses or policy: it mounts exactly what the specification describes and runs the command.

## How it is passed

The client writes the specification as JSON to an inherited file descriptor and runs:

```
sandmux-run --spec-fd <N>
```

The launcher reads the descriptor to the end, closes it and validates the content before creating any process.

## Exit codes

| Code | Meaning |
|---|---|
| The command's | The command ran; the code is its own, or 128 plus the number of the signal that ended it |
| 125 | The specification was refused or the confinement could not be set up. The command did not run |
| 127 | The confinement was set up, but the command could not be executed (not found, no permission) |

## Parsing rules

- The `version` field is checked before all the others. A version other than `1` is refused with a message that shows the received version and the supported one.
- An unknown field, at any level, makes parsing fail.
- All paths are absolute and cannot contain `..`.
- `command` cannot be empty. When the first item does not contain `/`, it is looked up in the `PATH` from `env`.

## Fields

| Field | Content |
|---|---|
| `version` | Format version: `1` |
| `mounts` | List of mounts, applied in order over a new, empty root |
| `landlock.read` | Paths with read and execute access |
| `landlock.write` | Paths with all access rights |
| `seccomp.deny_syscalls` | System calls that return a permission error |
| `seccomp.deny_ioctls` | `ioctl` commands that return a permission error (`TIOCSTI`, `TIOCLINUX`) |
| `seccomp.enosys_syscalls` | Optional. System calls that return `ENOSYS`, as if the kernel did not have them. A name in this list and in `deny_syscalls` gets `ENOSYS`. Used in the Docker view for the kernel keyring, because `runc` only accepts that error |
| `env` | The command's complete environment, as a list of `{name, value}`. Nothing is inherited |
| `command` | Program and arguments |
| `cwd` | Folder where the command starts, inside the sandbox |

## Mounts

In all of them, `dest` is the path inside the sandbox. `source`, `lower` and the layer paths are host paths. Mount points are created when missing, component by component, and a symbolic link in any component of the destination makes the launch fail.

| `type` | Fields | Effect |
|---|---|---|
| `tmpfs` | `dest`, `mode` | New, private in-memory file system. `mode` is numeric (for example `1023` for `0o1777`) |
| `bind` | `source`, `dest`, `writable` | Host folder or file, recursively; read-only when `writable` is false |
| `device` | `source`, `dest` | Host device node |
| `overlay` | `lower`, `dest`, `layer` | Host folder underneath, read-only, with a write layer on top |
| `cover` | `dest` | The path now appears empty and read-only. If it does not exist, or if it is a symbolic link, nothing is done |
| `file` | `dest`, `contents`, `mode` | Generated file, read-only |
| `symlink` | `dest`, `target` | Symbolic link created in the new root |
| `proc` | `dest` | New `/proc`, from the sandbox's pid namespace |
| `devpts` | `dest` | New pseudoterminal instance |
| `sysfs` | `dest` | New read-only `/sys`, of the mounting process's network namespace (Docker view) |
| `cgroup2` | `dest` | New writable cgroup2, whose root is the mounting process's cgroup namespace root. Only works inside its own cgroup namespace (Docker view) |

The layer of an `overlay` is `{"kind": "persistent", "upper": ..., "work": ...}`, stored on the host and kept between runs, or `{"kind": "temporary"}`, kept in memory and discarded when the sandbox ends.

After all mounts, the new root becomes read-only.

## Example

The example below is checked by a test in the project (`crates/sandmux/tests/docs.rs`).

```json
{
  "version": 1,
  "mounts": [
    {"type": "bind", "source": "/usr", "dest": "/usr", "writable": false},
    {"type": "symlink", "dest": "/bin", "target": "usr/bin"},
    {"type": "symlink", "dest": "/lib", "target": "usr/lib"},
    {"type": "symlink", "dest": "/lib64", "target": "usr/lib64"},
    {"type": "bind", "source": "/etc", "dest": "/etc", "writable": false},
    {"type": "cover", "dest": "/etc/shadow"},
    {"type": "file", "dest": "/etc/passwd", "contents": "ana:x:1000:1000::/home/ana:/bin/bash\n", "mode": 420},
    {"type": "proc", "dest": "/proc"},
    {"type": "tmpfs", "dest": "/dev", "mode": 493},
    {"type": "device", "source": "/dev/null", "dest": "/dev/null"},
    {"type": "devpts", "dest": "/dev/pts"},
    {"type": "tmpfs", "dest": "/tmp", "mode": 1023},
    {"type": "bind", "source": "/home/ana/.local/share/sandmux/homes/shell", "dest": "/home/ana", "writable": true},
    {"type": "overlay", "lower": "/home/ana/.cargo", "dest": "/home/ana/.cargo",
     "layer": {"kind": "persistent",
               "upper": "/home/ana/.local/share/sandmux/layers/shell/.cargo/upper",
               "work": "/home/ana/.local/share/sandmux/layers/shell/.cargo/work"}},
    {"type": "bind", "source": "/home/ana/projetos/app", "dest": "/home/ana/projetos/app", "writable": true}
  ],
  "landlock": {
    "read": ["/"],
    "write": ["/home/ana", "/home/ana/projetos/app", "/tmp", "/dev", "/proc"]
  },
  "seccomp": {
    "deny_syscalls": ["mount", "umount2", "pivot_root", "bpf", "keyctl", "io_uring_setup"],
    "deny_ioctls": ["TIOCSTI"]
  },
  "env": [
    {"name": "HOME", "value": "/home/ana"},
    {"name": "PATH", "value": "/home/ana/.cargo/bin:/usr/bin:/bin"}
  ],
  "command": ["bash"],
  "cwd": "/home/ana/projetos/app"
}
```
