# Design

## Context

- **Cofre (changes 11 e 14):** segredos cifrados, com tipos (`token`, `ssh`); o daemon guarda o conteúdo aberto na memória.
- **Agente SSH (change 14):** o daemon abre um socket por lançamento, a composição o monta em `/run/sandmux/` com uma variável, e cada pedido lê o cofre na hora. O mesmo modelo serve aqui.
- **MCPs (change 17):** o opencode recebe MCPs pelo `OPENCODE_CONFIG_CONTENT`, e o hermes por um `config.yaml` só da execução.
- **MCP por stdio:** o harness inicia um comando local e troca mensagens JSON-RPC 2.0, uma por linha.

## Goals / Non-Goals

**Goals:**

- O agente consulta e altera um banco PostgreSQL ou MySQL cadastrado, sem a senha no sandbox, com cada query registrada.

**Non-Goals:**

- Modo só leitura e confirmação por query (o roadmap decidiu leitura e escrita por padrão).
- Outros bancos (SQLite, MongoDB).
- Túnel SSH até o banco.

## Decisions

### 1. Conexão no cofre

- **Segredo:** tipo `db`. O valor é um JSON com `kind`, `host`, `port`, `database`, `user` e `password`; tudo fica cifrado.
- **Fora das outras trocas:** sem valor falso, sem ferramentas e fora da decifração do TLS, como as chaves SSH.
- **Cadastro:** `sandmux db add` lê a senha sem eco (ou da entrada padrão) e manda `DbAdd` ao daemon. Os padrões de porta são 5432 e 3306.
- **Lista:** `sandmux db list` mostra o nome, o tipo, `usuário@host:porta/banco`, nunca a senha.

### 2. Conexões da sessão

- **Escolha:** a chave `db` (lista de nomes, `"*"` para todas, somada entre as camadas) e `--db <nome>` no `run`.
- **Sem conexões:** não há socket nem MCP.

### 3. O MCP

- **No sandbox:**
  - `sandmux-db-mcp` é um binário pequeno, sem dependências, que conecta ao socket de `SANDMUX_DB_SOCKET` e copia o stdin para o socket e o socket para o stdout;
  - ele é montado em somente leitura em `/run/sandmux/bin/sandmux-db-mcp`, a partir do binário ao lado do `sandmux-run`.
- **No daemon:** um socket por lançamento (`$XDG_RUNTIME_DIR/sandmux/db/<etiqueta>.sock`, pasta 700, socket 600), com uma thread por conexão, atende o JSON-RPC:
  - `initialize` responde a versão do protocolo pedida e o servidor `sandmux-db`;
  - `tools/list` lista as ferramentas;
  - `tools/call`:
    - `conexoes`: os nomes e tipos das conexões da sessão;
    - `tabelas` (`conexao`): as tabelas;
    - `descrever` (`conexao`, `tabela`): as colunas e os tipos;
    - `query` (`conexao`, `sql`): executa e devolve até 500 linhas em JSON, ou as linhas afetadas.
  - Notificações (sem `id`) não têm resposta.
- **Senha:** lida do cofre a cada chamada. Com o cofre trancado, a ferramenta devolve um erro que pede `sandmux vault unlock`.
- **Drivers:** `tokio-postgres` (sem TLS, ou TLS pelo `rustls` já presente) e `mysql_async`, num runtime tokio do daemon. Os valores vão para o JSON como texto, número, booleano ou `null`.

### 4. Cadastro no harness

- **opencode:** `"mcp": {"sandmux-db": {"type": "local", "command": ["/run/sandmux/bin/sandmux-db-mcp"], "enabled": true}}`, somado ao `OPENCODE_CONFIG_CONTENT`.
- **hermes:** um bloco `sandmux-db:` com `command:` em `mcp_servers`, no `config.yaml` da execução (o mesmo `Mount::File` do change 17).
- **Variável:** `SANDMUX_DB_SOCKET=/run/sandmux/db.sock` no ambiente.

### 5. Registro

- **Linha:** `ts`, `tag`, `workdir`, `connection`, `sql`, `rows`, `ms` e `error`, em `$XDG_STATE_HOME/sandmux/db/AAAA-MM-DD.jsonl` (pasta 700, arquivo 600).
- **Consulta:** `sandmux db log [<id>]`.
- **Retenção:** a da rede (`net-log-days`, `net-log-max`).

### 6. Testes

Os testes de integração sobem no Docker do sistema (`docker run -d --rm -m 512m -p 127.0.0.1::5432 postgres:17-alpine` e `mysql:8.4`), esperam o banco aceitar conexões e removem o contêiner no fim. Sem o Docker do sistema ou sem as imagens, os testes são pulados.

## Risks / Trade-offs

- **[Leitura e escrita]** O agente pode apagar dados. → A proteção é a do usuário cadastrado no banco (risco aceito); a documentação recomenda um usuário com as permissões mínimas.
- **[SQL no registro]** As queries podem ter dados sensíveis. → O registro fica fora do sandbox, com permissões 600.
- **[Dependências]** Os dois drivers aumentam o binário. → Ficam no daemon e no cliente `sandmux`, não no lançador.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 17). Sem conexões cadastradas, nada muda.
