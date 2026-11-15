# MCPs

> Português: [mcp.md](../mcp.md)

Harnesses use MCP servers: local ones are processes that run inside the sandbox, and remote ones are connections that go through the network gateway. sandmux shows each harness's servers and enables or disables each one as a whole (no per-tool control). MCPs brought in by the import stay enabled until the user disables them.

## Where they come from

- **opencode:** the `"mcp"` in `opencode.json` or `opencode.jsonc` in `~/.config/opencode` of the synthetic HOME (origin `global`) and the one in the work folder (origin `projeto`), which wins on a repeated name.
- **hermes:** the `mcp_servers` map in `~/.hermes/config.yaml` of the synthetic HOME.

Each server has a name, a type (`local` or `remote`), the command or the URL, the origin and the state.

```bash
sandmux mcp list 3                     # for session 3
sandmux mcp list --harness hermes      # for hermes, in the current folder
```

## Enabling and disabling

| Key | `run` option | Effect |
|---|---|---|
| `mcp-deny` | `--mcp-deny` | disables the server |
| `mcp-allow` | `--mcp-allow` | when not empty, only its servers stay enabled |

The lists add up across the layers of the [configuration](config.md). For a session's tab:

```bash
sandmux mcp deny github 3     # disables
sandmux mcp allow fs 3        # puts it on the tab's allowlist
sandmux mcp forget github 3   # removes it from the tab's rules
```

Harnesses read their MCPs at startup: a change takes effect at the session's next launch (`sandmux restart <id>`, or Ctrl+b `r` in the TUI).

- **opencode:** each disabled server goes as `"enabled": false` in `OPENCODE_CONFIG_CONTENT`, which beats the global and the project configuration.
- **hermes:** the session gets a `~/.hermes/config.yaml` for that run only, without the disabled servers; the stored file does not change. During that run, hermes cannot write `config.yaml`. If the file cannot be understood, nothing is filtered, and the launch warns.

## In the TUI

- **Panel:** the MCP section lists the tab's servers, with `✓` for enabled and `✗` for disabled.
- **Screen:** Ctrl+b `m` opens the tab's MCP screen, with each server's type, origin and target. Space enables or disables the selected one in the tab, and Esc closes it ([tui.md](tui.md)).

## Database MCP

With database connections chosen for the session (`db` or `--db`), sandmux registers the `sandmux-db` MCP, which queries the database without the password entering the sandbox ([database.md](database.md)).

## Remote and local MCPs

- **Remote:** the connection goes through the gateway and follows the [network policy](network.md). In `custom` mode, allow the server's domain with `allow`.
- **Local:** they run inside the sandbox, with the same view of files, the same network and the same caps as the agent.

## Limits

- A new MCP written by the agent into the work folder's `opencode.json` enters the inventory at the next launch. With `mcp-allow` in use, it stays disabled.
- A harness relaunched by the agent itself inside the session reads the configuration again; the list of disabled servers is the one from the launch.
