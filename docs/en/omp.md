# oh-my-pi

> Português: [omp.md](../omp.md)

The `omp` preset runs the oh-my-pi installed on your machine inside the sandbox, like opencode, hermes and Claude Code.

```sh
sandmux run omp
```

## Installation

sandmux uses the host's `~/.local/bin/omp` binary, read-only: to update, update it outside the sandbox. Without the binary, `run` refuses and says where it should be. On the first run, oh-my-pi extracts its native library into `~/.omp/natives` in the synthetic HOME.

## Keys

Provider keys come from the vault, as in opencode and hermes. The providers with a secret in the vault come in through environment variables (`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY` and the like) with the fake value, and the gateway swaps in the real one in the traffic. To register a key:

```sh
sandmux vault add openrouter --domain openrouter.ai
```

oh-my-pi keeps its logins in a SQLite database, `~/.omp/agent/agent.db`. That database is not imported, and a login made inside the sandbox stays in the synthetic HOME's database, with the real value. Prefer keys through the vault.

## Import

On the first run, sandmux asks whether to import the configuration you already have. It copies from `~/.omp/agent`:

- `config.yml`;
- the `extensions/`, `agents/`, `rules/`, `managed-skills/`, `skills/`, `commands/` and `prompts/` folders.

Left out:

- `agent.db`, with the credentials;
- the other databases (`history.db`, `models.db`, `skill-descriptions.db`);
- the sessions, the caches and the logs.
