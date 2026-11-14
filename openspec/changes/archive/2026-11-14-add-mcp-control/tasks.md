# Tasks

## 1. Inventário e estado

- [x] 1.1 Inventário dos MCPs do opencode (JSONC global e do projeto) e do hermes (`mcp_servers`); verificar com testes de unidade
- [x] 1.2 Chaves `mcp-deny`/`mcp-allow` e as opções do `run`; aplicar no `OPENCODE_CONFIG_CONTENT` e no `config.yaml` filtrado do hermes; verificar com testes de unidade da composição

## 2. Consulta, edição e TUI

- [x] 2.1 `McpList`/`Mcps` e as ações `mcp-*` no `NetEdit` (protocolo 16); `sandmux mcp list|deny|allow|forget`; verificar com teste de CLI
- [x] 2.2 Seção MCP no painel e tela Ctrl+b `m`; verificar com teste de PTY

## 3. Documentação e verificação

- [x] 3.1 Escrever `docs/mcp.md` e atualizar `docs/config.md`, `docs/tui.md`, `docs/sessions.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas e conferir à mão um MCP desabilitado no opencode real
