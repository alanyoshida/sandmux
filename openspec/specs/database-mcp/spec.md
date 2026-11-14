# database-mcp Specification

## Purpose
Define o MCP de banco de dados do sandmux: conexões PostgreSQL e MySQL no cofre, escolhidas por sessão, atendidas pelo daemon por um socket da sessão sem a senha entrar no sandbox, com cada query registrada.

## Requirements

### Requirement: Conexões de banco no cofre
`sandmux db add <nome> --kind postgres|mysql --host <h> [--port <p>] --database <d> --user <u>` MUST guardar a conexão cifrada no cofre, com a senha lida sem eco ou da entrada padrão. `sandmux db list` MUST mostrar o nome, o tipo, o usuário, o host, a porta e o banco, nunca a senha. `sandmux db remove <nome>` MUST apagar a conexão.

#### Scenario: Cadastro
- **WHEN** o usuário roda `sandmux db add app --kind postgres --host localhost --database app --user dev` e digita a senha
- **THEN** `sandmux db list` mostra `app  postgres  dev@localhost:5432/app`, e o arquivo do cofre não tem a senha em claro

### Requirement: MCP de banco na sessão
Uma sessão com conexões escolhidas (chave `db` ou `--db`) MUST ter o MCP `sandmux-db` cadastrado no harness, atendido pelo daemon por um socket montado em `/run/sandmux/db.sock` através do `sandmux-db-mcp`. As ferramentas MUST ser `conexoes`, `tabelas`, `descrever` e `query`, com leitura e escrita. A senha MUST NOT entrar no sandbox. Sem conexões escolhidas, a sessão MUST NOT ter o MCP.

#### Scenario: Query pelo MCP
- **WHEN** a sessão tem `--db app` e o agente chama `query` com `select 1 as x`
- **THEN** a resposta tem a linha com `x` igual a 1, e nenhum arquivo ou variável do sandbox tem a senha

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o agente chama `query`
- **THEN** a ferramenta devolve um erro que pede `sandmux vault unlock`

### Requirement: Registro das queries
Cada chamada de `query`, `tabelas` e `descrever` MUST virar uma linha em `$XDG_STATE_HOME/sandmux/db/AAAA-MM-DD.jsonl` (pasta 700, arquivo 600), com a etiqueta do lançamento, a conexão, o SQL, as linhas, a duração e o erro. `sandmux db log [<id>]` MUST mostrar o registro, e a retenção MUST seguir a do registro de rede.

#### Scenario: Query registrada
- **WHEN** o agente da sessão 1 roda `update itens set x = 2` pelo MCP
- **THEN** `sandmux db log 1` mostra a linha com a conexão, o SQL e as linhas afetadas
