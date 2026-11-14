# Design

## Context

- **opencode:**
  - lê a configuração global (`~/.config/opencode/opencode.json` ou `.jsonc`) e a do projeto (`opencode.json` na pasta de trabalho);
  - lê também o `OPENCODE_CONFIG_CONTENT`, que tem precedência sobre as duas e que o sandmux já usa para os provedores;
  - os MCPs ficam em `"mcp": {"<nome>": {"type": "local"|"remote", "command": [...], "url": ..., "enabled": ...}}`.
- **hermes:** lê `~/.hermes/config.yaml`; os MCPs ficam no mapa `mcp_servers`. O sandmux já monta um `.env` só da execução por cima do guardado.
- **Importação:** copia os MCPs cadastrados no host para o HOME sintético, com os segredos trocados por valores falsos.

## Goals / Non-Goals

**Goals:**

- Ver os servidores MCP de cada harness e habilitá-los ou desabilitá-los por nome.

**Non-Goals:**

- Controle por ferramenta (decisão do roadmap).
- Desabilitar com o harness rodando: o harness lê os MCPs ao iniciar, então a mudança vale no próximo lançamento.
- Impedir que um harness relançado pelo próprio agente leia um MCP novo criado no meio da sessão: a lista é montada no lançamento.

## Decisions

### 1. Inventário

- **Tipo:** `McpServer { name, kind (local|remote), target, origin }`.
  - `target` é o comando (local) ou a URL (remoto);
  - `origin` é `global` (HOME sintético) ou `projeto` (pasta de trabalho, só no opencode).
- **opencode:** o `opencode.json` e o `opencode.jsonc` de `~/.config/opencode` do HOME sintético e o `opencode.json`/`opencode.jsonc` da pasta de trabalho, com os comentários e as vírgulas sobrando do JSONC tirados antes do `serde_json`. Um nome repetido no projeto vence o global.
- **hermes:** a leitura do `mcp_servers` do YAML por indentação. Cada chave filha direta é um servidor; `command`/`url` dentro dele dão o tipo e o alvo.

### 2. Estado

- **Chaves:** `mcp-deny` e `mcp-allow` (listas de nomes, somadas entre as camadas) e `--mcp-deny`/`--mcp-allow` no `run`.
- **Habilitado:** quando não está em `mcp-deny` e, se `mcp-allow` não está vazia, está nela.
- **Aplicação no opencode:** cada servidor do inventário desabilitado vai como `{"enabled": false}` dentro de `"mcp"` no `OPENCODE_CONFIG_CONTENT`. A mesclagem do opencode junta isso à definição.
- **Aplicação no hermes:**
  - `Mount::File` em `~/.hermes/config.yaml` com o texto guardado sem os blocos dos servidores desabilitados;
  - o arquivo guardado não muda;
  - sem servidores desabilitados, nada é montado.

### 3. Consulta e edição

- **Pedido:** `McpList { id }` → `Mcps { list: Vec<McpEntry { name, kind, target, origin, enabled }> }` (protocolo 16). Na sessão, o daemon monta o inventário pelo harness e pela pasta da sessão, com o estado pela configuração efetiva.
- **Sem sessão:** `sandmux mcp list` sem `id` monta o inventário da pasta atual, para o harness de `--harness` (padrão `opencode`).
- **Edição:** as ações `mcp-deny`, `mcp-allow` e `mcp-forget` no `NetEdit` mudam a camada da aba e respondem com o aviso de que valem no próximo lançamento.

### 4. TUI

- **Painel:** a seção MCP lista os nomes, com `✓` habilitado e `✗` desabilitado.
- **Tela:** Ctrl+b `m` (ação `mcp`) lista os servidores com o tipo, a origem e o estado:
  - espaço alterna o escolhido na aba (desabilitar põe em `mcp-deny`; habilitar tira de `mcp-deny` e, com `mcp-allow` em uso, põe nela);
  - Esc fecha;
  - a tela avisa que vale no próximo lançamento (Ctrl+b `r` relança).

## Risks / Trade-offs

- **[MCP plantado no projeto]** O agente pode escrever um `opencode.json` na pasta de trabalho com um MCP novo. → O inventário lê a pasta de trabalho a cada lançamento, e com `mcp-allow` em uso o novo nome fica desabilitado; o MCP local roda dentro do sandbox de qualquer forma.
- **[YAML lido à mão]** Um `config.yaml` com estilo incomum pode não ser entendido. → Sem entender, nada é filtrado e o lançamento avisa.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 16). Sem `mcp-deny` e `mcp-allow`, tudo continua habilitado, como hoje.
