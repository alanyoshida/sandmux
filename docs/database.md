# Banco de dados

O sandmux tem um MCP de banco embutido, para PostgreSQL e MySQL. O agente consulta e altera o banco pelo MCP, e o usuário e a senha ficam no [cofre](vault.md): quem conecta é o daemon, fora do sandbox, e a senha nunca entra nele. Cada query vai para um registro.

## Conexões

```bash
sandmux db add app --kind postgres --host localhost --database app --user dev   # pede a senha, sem eco
sandmux db add loja --kind mysql --host 10.0.0.5 --port 3307 --database loja --user leitor
sandmux db list        # nome, tipo, usuário@host:porta/banco, sem a senha
sandmux db remove app
```

A porta padrão é 5432 (PostgreSQL) ou 3306 (MySQL). A conexão fica cifrada no cofre, como os outros segredos. Ela não tem valor falso nem entra na troca de tokens.

## Na sessão

A chave `db` da [configuração](config.md) (num perfil ou no perfil do projeto) e o `--db <nome>` do `run` escolhem as conexões que a sessão pode usar; `"*"` libera todas.

```bash
sandmux run opencode --db app
```

Com conexões escolhidas:

- **No sandbox:** o daemon abre um socket da sessão, montado em `/run/sandmux/db.sock` (`SANDMUX_DB_SOCKET`). O repassador `/run/sandmux/bin/sandmux-db-mcp` só copia o stdio do MCP para esse socket.
- **No harness:** o MCP aparece como `sandmux-db`, cadastrado sozinho no opencode (pelo `OPENCODE_CONFIG_CONTENT`) e no hermes (no `config.yaml` daquela execução). Ele pode ser desabilitado como os outros ([mcp.md](mcp.md)).
- **Cofre trancado:** as ferramentas devolvem um erro que pede `sandmux vault unlock`.

### Ferramentas

| Ferramenta | Argumentos | Faz |
|---|---|---|
| `conexoes` | | lista as conexões liberadas na sessão |
| `tabelas` | `conexao` | lista as tabelas |
| `descrever` | `conexao`, `tabela` | mostra as colunas e os tipos |
| `query` | `conexao`, `sql` | executa o SQL (leitura ou escrita) e devolve até 500 linhas, ou as linhas afetadas |

As consultas têm leitura e escrita por padrão. A proteção é a do usuário cadastrado no banco: cadastre um usuário com as permissões mínimas que o agente precisa.

## Registro

Cada chamada de `query`, `tabelas` e `descrever` vira uma linha em `$XDG_STATE_HOME/sandmux/db/AAAA-MM-DD.jsonl` (pasta 700, arquivo 600), com a etiqueta do lançamento, a conexão, o SQL, as linhas, a duração e o erro.

```bash
sandmux db log        # de hoje, de todas as sessões
sandmux db log 3 --days 7
```

A retenção segue a do registro de rede (`net-log-days` e `net-log-max`).

## Limites

- Só PostgreSQL e MySQL, conectados direto (sem TLS e sem túnel SSH).
- As queries no registro podem ter dados sensíveis; o registro fica fora do sandbox.
