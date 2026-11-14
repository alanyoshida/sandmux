# Spec Delta

## Purpose
Define como o sandmux mostra e controla os servidores MCP dos harnesses: o inventário, a allowlist e a denylist por servidor, a consulta, a edição e a tela de MCPs.

## ADDED Requirements

### Requirement: Inventário dos servidores MCP
O sandmux MUST listar os servidores MCP de cada harness com o nome, o tipo (local ou remoto), o comando ou a URL, a origem e o estado: no opencode, os do `opencode.json`/`opencode.jsonc` do HOME sintético e os da pasta de trabalho; no hermes, os do `mcp_servers` do `config.yaml` do HOME sintético. `sandmux mcp list [<id>]` MUST mostrar essa lista.

#### Scenario: MCPs importados
- **WHEN** a importação trouxe para o opencode os MCPs `github` (remoto) e `fs` (local), e o usuário roda `sandmux mcp list 1`
- **THEN** a saída tem `github` e `fs`, com o tipo, o alvo, a origem `global` e o estado habilitado

### Requirement: Habilitar e desabilitar por servidor
Um servidor em `mcp-deny`, ou fora de uma `mcp-allow` não vazia, MUST ficar desabilitado no lançamento seguinte do harness: no opencode, pelo `OPENCODE_CONFIG_CONTENT`; no hermes, por um `config.yaml` só daquela execução sem o servidor, sem alterar o arquivo guardado. O controle MUST ser do servidor inteiro.

#### Scenario: Desabilitar no opencode
- **WHEN** a configuração tem `mcp-deny = ["github"]` e o usuário lança o opencode
- **THEN** o `OPENCODE_CONFIG_CONTENT` tem `github` com `"enabled": false`, e o `fs` continua habilitado

#### Scenario: Allowlist no hermes
- **WHEN** a configuração tem `mcp-allow = ["fs"]` e o hermes tem `fs` e `web` em `mcp_servers`
- **THEN** o `config.yaml` da execução só tem o `fs`, e o guardado continua com os dois

### Requirement: Edição dos MCPs
`sandmux mcp deny|allow|forget <nome> <id>` MUST mudar os MCPs da aba da sessão, com o aviso de que vale no próximo lançamento. Na TUI, Ctrl+b `m` MUST abrir a tela de MCPs da aba ativa, em que espaço habilita ou desabilita o servidor escolhido. O painel MUST ter a seção MCP com o estado de cada servidor.

#### Scenario: Desabilitar pela tela
- **WHEN** o usuário abre a tela, escolhe `github` e aperta espaço
- **THEN** `github` aparece desabilitado, e o próximo lançamento da aba o desabilita no harness
