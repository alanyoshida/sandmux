# Git inside the sandbox

> Português: [git.md](../git.md)

The sandbox protects the points of the work folder's `.git` that the host's git executes or obeys. The agent makes commits, branches, merges and stashes normally; what it cannot do is plant a hook or change the repository's configuration.

## What is protected

| Path | Protection |
|---|---|
| `.git` | Mount point: cannot be renamed, deleted or replaced |
| `.git/hooks` | Read-only |
| `.git/config` | Read-only |
| `.git/modules/<submodule>` | The same treatment as `.git`: the folder, the hooks and the configuration of each submodule |
| A worktree's `.git` file | Read-only, so it cannot point to another git directory |

The commit goes out with the name and e-mail from your git: when creating the synthetic HOME, sandmux copies only `user.name` and `user.email`. Aliases, credential helpers and URLs with tokens are left out.

## Everyday operations

Result measured on 2026-11-12, with git 2.x, running inside the sandbox in a repository with a remote.

| Operation | Result |
|---|---|
| `git commit`, `git branch`, `git merge`, `git rebase`, `git stash` | Works |
| `git fetch`, `git pull` | Works |
| `git gc` | Works |
| `git worktree add` | Works |
| `git init` in a subfolder | Works, but the new repository has no protection at all (see limits) |
| `git config --global ...` | Works: writes to the synthetic HOME's `.gitconfig`, which applies only inside the sandbox |
| `git config <key> <value>` (local) | **Fails** with `could not write config file .git/config` |
| `git remote add` | **Fails** with the same message |
| `git checkout -b x origin/x` | Creates the branch, but **does not write the tracking**. Git shows the error and exits successfully |
| `git branch --set-upstream-to=...` | **Does not write**. Git shows the error, says it set up the tracking and exits successfully |
| `git push -u` | Pushes, but **does not write** the upstream, as above |
| `git submodule add` | Creates `.gitmodules`, the folder and `.git/modules/<name>`, but **does not write** the submodule to `.git/config` |

In the cases where git exits successfully without writing, the message `error: could not write config file .git/config` appears first. To configure branch tracking, remotes or submodules, do the operation on the host.

## Known limits

- **`.git/commondir`**: a `commondir` file created by the agent inside `.git` makes the host's git start using another git directory, with another configuration and other hooks. The mount protection does not prevent the creation of this file. Confirmed on 2026-11-12. The fix comes with the file interception of stage 6 (decision of 2026-11-13). Until then, before using git on the host after a run, check whether `.git/commondir` exists in a repository that is not a worktree and delete the file if the agent created it.
- **Repository created during the run**: a `git init` or a `git submodule add` done by the agent produces a git directory with no protection until the next launch. Whoever runs git on the host inside that folder obeys what the agent wrote there.
- **`core.hooksPath` pointing to a versioned folder** (the husky case): the hooks live in the work folder and remain writable.
- **Other files the host executes**: `Makefile`, `package.json` scripts, `.envrc`, editor tasks. What the agent writes there needs to be read before it is executed outside the sandbox.
