# Tasks

## 1. Conexões

- [x] 1.1 Segredo de tipo `db` no cofre, `DbAdd`/`DbList` (protocolo 17) e `sandmux db add|list|remove`; verificar com testes de unidade e de CLI
- [x] 1.2 Chave `db` e `--db`; a composição monta o socket e o `sandmux-db-mcp` e cadastra o MCP no opencode e no hermes; verificar com testes de unidade da composição

## 2. MCP

- [x] 2.1 Binário `sandmux-db-mcp` (repasse do stdio); verificar com teste de unidade
- [x] 2.2 Servidor MCP no daemon (`initialize`, `tools/list`, `tools/call` com `conexoes`, `tabelas`, `descrever` e `query`), PostgreSQL e MySQL, e o registro das queries; verificar com testes de unidade do JSON-RPC
- [x] 2.3 Testes de integração com `postgres:17-alpine` e `mysql:8.4` no Docker do sistema: uma sessão com `--db` faz `query` e `tabelas` pelo `sandmux-db-mcp`, a senha não está no sandbox, e a query vai para o registro

## 3. Documentação e verificação

- [x] 3.1 `sandmux db log` e a retenção; escrever `docs/database.md` e atualizar `docs/vault.md`, `docs/config.md`, `docs/mcp.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas (sob o vigia de memória) e verificar à mão o opencode real fazendo uma query
