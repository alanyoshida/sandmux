# Proposal

## Why

O roadmap pede acesso a banco de dados sem expor a credencial ao agente:

- um MCP embutido para PostgreSQL e MySQL;
- o cadastro de conexões;
- usuário e senha no cofre, injetados fora do sandbox;
- leitura e escrita por padrão;
- registro de cada query.

O usuário decidiu (2026-11-14) que o MCP fala com o daemon por um socket por sessão, montado no sandbox como o agente SSH.

## What Changes

- **Conexões no cofre:** `sandmux db add <nome> --kind postgres|mysql --host <h> --port <p> --database <d> --user <u>` pede a senha sem eco e guarda a conexão cifrada no cofre (um segredo de tipo `db`). `sandmux db list` lista sem a senha, e `sandmux db remove` apaga.
- **Conexões da sessão:** a chave `db` (nomes, ou `"*"`) e `--db <nome>` no `run` escolhem as conexões que a sessão pode usar.
- **MCP pelo socket da sessão:**
  - com conexões escolhidas, o daemon abre um socket por lançamento, montado em `/run/sandmux/db.sock`;
  - o binário `sandmux-db-mcp` é montado em `/run/sandmux/bin/`; ele só repassa o stdio do MCP para o socket;
  - o daemon atende o protocolo MCP (JSON-RPC por linhas) com as ferramentas `conexoes`, `tabelas`, `descrever` e `query`, conectando ao banco com a senha do cofre;
  - a senha nunca entra no sandbox.
- **Cadastro automático no harness:** o opencode recebe o servidor `sandmux-db` no `OPENCODE_CONFIG_CONTENT`, e o hermes no `config.yaml` da execução.
- **Registro de cada query:** uma linha JSONL em `$XDG_STATE_HOME/sandmux/db/AAAA-MM-DD.jsonl` com a etiqueta, a conexão, o SQL, as linhas, a duração e o erro. `sandmux db log [<id>]` mostra o registro, e a retenção segue a do registro de rede.
- **Leitura e escrita por padrão:** a proteção é a do usuário cadastrado no banco (risco aceito no roadmap).

## Capabilities

### New Capabilities

- `database-mcp`: conexões no cofre, conexões da sessão, o MCP pelo socket, o cadastro no harness e o registro das queries.

### Modified Capabilities

- `config-profiles`: chave `db`.

## Impact

- **`sandmux-vault`:** segredos de tipo `db`.
- **`sandmux-policy`:** chave `db`; a composição monta o socket e o binário, e cadastra o MCP no opencode e no hermes.
- **`sandmux`:**
  - o binário `sandmux-db-mcp`;
  - o servidor MCP no daemon (`tokio-postgres` e `mysql_async`);
  - `sandmux db add|list|remove|log`.
- **`sandmux-proto`:** `DbAdd`, `DbList`; `PROTOCOL` 17.
- **Dependências:** `tokio-postgres` e `mysql_async`.
- **Documentação:** `docs/database.md` (novo), `docs/vault.md`, `docs/config.md`, `docs/mcp.md` e o README.
