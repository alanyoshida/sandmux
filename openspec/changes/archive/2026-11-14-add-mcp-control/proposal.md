# Proposal

## Why

Os harnesses rodam servidores MCP: os locais são processos dentro do sandbox, e os remotos são conexões que passam pelo gateway. A importação traz os MCPs cadastrados no host. O roadmap decidiu:

- o controle é por servidor (cada servidor habilitado ou desabilitado inteiro, sem controle por ferramenta);
- os MCPs importados ficam liberados e registrados, e o usuário desabilita os que não quiser.

Hoje o sandmux não mostra nem controla os MCPs.

## What Changes

- **Inventário:** o sandmux lê os servidores MCP de cada harness, com o nome, o tipo (local ou remoto), o comando ou a URL e a origem:
  - opencode: o `opencode.json`/`opencode.jsonc` do HOME sintético e o da pasta de trabalho;
  - hermes: o `mcp_servers` do `~/.hermes/config.yaml` do HOME sintético.
- **Allowlist e denylist por servidor:**
  - `mcp-deny` (nomes desabilitados) e `mcp-allow` (quando não vazia, só esses ficam habilitados), em qualquer camada, e `--mcp-deny`/`--mcp-allow` no `run`;
  - no opencode, os desabilitados vão como `"enabled": false` no `OPENCODE_CONFIG_CONTENT`, que vence as outras configurações;
  - no hermes, um `config.yaml` só daquela execução sem os desabilitados.
- **Consulta:** `sandmux mcp list [<id>]` lista os servidores (da sessão, ou do harness na pasta atual) com o estado e a origem.
- **TUI:**
  - o painel ganha a seção MCP;
  - Ctrl+b `m` abre a tela de MCPs da aba, em que espaço habilita ou desabilita o servidor escolhido na aba, valendo no próximo lançamento (o harness lê os MCPs ao iniciar).
- **Edição pela linha de comando:** `sandmux mcp deny|allow|forget <nome> <id>`.
- **MCPs remotos:** seguem a política de rede; a documentação orienta liberar o domínio no modo `custom`.

## Capabilities

### New Capabilities

- `mcp-control`: inventário dos servidores MCP, allowlist e denylist por servidor, consulta e a tela de MCPs.

### Modified Capabilities

- `config-profiles`: chaves `mcp-deny` e `mcp-allow`.

## Impact

- **`sandmux-policy`:** o inventário (JSONC e o `mcp_servers` do YAML do hermes), as chaves novas, a configuração do opencode e o `config.yaml` filtrado do hermes.
- **`sandmux-proto`:** `McpList` → `Mcps`; ações `mcp-deny`, `mcp-allow` e `mcp-forget` no `NetEdit`; `PROTOCOL` 16.
- **`sandmux`:** `sandmux mcp`, a seção do painel e a tela Ctrl+b `m`.
- **Documentação:** `docs/mcp.md` (novo), `docs/config.md`, `docs/tui.md`, `docs/sessions.md` e o README.
