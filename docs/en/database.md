# Database

> Português: [database.md](../database.md)

sandmux has a built-in database MCP, for PostgreSQL and MySQL. The agent queries and changes the database through the MCP, and the user and password stay in the [vault](vault.md): the one that connects is the daemon, outside the sandbox, and the password never enters it. Every query goes to a log.

## Connections

```bash
sandmux db add app --kind postgres --host localhost --database app --user dev   # asks for the password, without echo
sandmux db add loja --kind mysql --host 10.0.0.5 --port 3307 --database loja --user leitor
sandmux db list        # name, kind, user@host:port/database, without the password
sandmux db remove app
```

The default port is 5432 (PostgreSQL) or 3306 (MySQL). The connection is stored encrypted in the vault, like the other secrets. It has no fake value and does not take part in the token swap.

## In the session

The `db` key of the [configuration](config.md) (in a profile or in the project profile) and the `--db <name>` of `run` choose the connections the session may use; `"*"` allows all of them.

```bash
sandmux run opencode --db app
```

With connections chosen:

- **In the sandbox:** the daemon opens a socket for the session, mounted at `/run/sandmux/db.sock` (`SANDMUX_DB_SOCKET`). The forwarder `/run/sandmux/bin/sandmux-db-mcp` only copies the MCP's stdio to that socket.
- **In the harness:** the MCP shows up as `sandmux-db`, registered automatically in opencode (through `OPENCODE_CONFIG_CONTENT`) and in hermes (in that run's `config.yaml`). It can be disabled like the others ([mcp.md](mcp.md)).
- **Locked vault:** the tools return an error that asks for `sandmux vault unlock`.

### Tools

| Tool | Arguments | Does |
|---|---|---|
| `conexoes` | | lists the connections allowed in the session |
| `tabelas` | `conexao` | lists the tables |
| `descrever` | `conexao`, `tabela` | shows the columns and the types |
| `query` | `conexao`, `sql` | runs the SQL (read or write) and returns up to 500 rows, or the affected rows |

Queries have read and write access by default. The protection is that of the user registered in the database: register a user with the minimum permissions the agent needs.

## Log

Every call to `query`, `tabelas` and `descrever` becomes a line in `$XDG_STATE_HOME/sandmux/db/YYYY-MM-DD.jsonl` (folder 700, file 600), with the launch tag, the connection, the SQL, the rows, the duration and the error.

```bash
sandmux db log        # from today, from all sessions
sandmux db log 3 --days 7
```

Retention follows that of the network log (`net-log-days` and `net-log-max`).

## Limits

- Only PostgreSQL and MySQL, connected directly (no TLS and no SSH tunnel).
- The queries in the log may contain sensitive data; the log stays outside the sandbox.
