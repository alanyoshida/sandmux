# MCPs

Os harnesses usam servidores MCP: os locais são processos que rodam dentro do sandbox, e os remotos são conexões que passam pelo gateway de rede. O sandmux mostra os servidores de cada harness e habilita ou desabilita cada um inteiro (sem controle por ferramenta). Os MCPs trazidos pela importação ficam habilitados até o usuário desabilitar.

## De onde vêm

- **opencode:** o `"mcp"` do `opencode.json` ou `opencode.jsonc` em `~/.config/opencode` do HOME sintético (origem `global`) e o da pasta de trabalho (origem `projeto`), que vence num nome repetido.
- **hermes:** o mapa `mcp_servers` do `~/.hermes/config.yaml` do HOME sintético.

Cada servidor tem o nome, o tipo (`local` ou `remote`), o comando ou a URL, a origem e o estado.

```bash
sandmux mcp list 3                     # da sessão 3
sandmux mcp list --harness hermes      # do hermes, na pasta atual
```

## Habilitar e desabilitar

| Chave | Opção do `run` | Efeito |
|---|---|---|
| `mcp-deny` | `--mcp-deny` | desabilita o servidor |
| `mcp-allow` | `--mcp-allow` | quando não está vazia, só os servidores dela ficam habilitados |

As listas se somam entre as camadas da [configuração](config.md). Para a aba de uma sessão:

```bash
sandmux mcp deny github 3     # desabilita
sandmux mcp allow fs 3        # põe na allowlist da aba
sandmux mcp forget github 3   # tira das regras da aba
```

Os harnesses leem os MCPs ao iniciar: uma mudança vale no próximo lançamento da sessão (`sandmux restart <id>`, ou Ctrl+b `r` na TUI).

- **opencode:** cada servidor desabilitado vai como `"enabled": false` no `OPENCODE_CONFIG_CONTENT`, que vence a configuração global e a do projeto.
- **hermes:** a sessão recebe um `~/.hermes/config.yaml` só daquela execução, sem os servidores desabilitados; o arquivo guardado não muda. Nessa execução, o hermes não consegue gravar o `config.yaml`. Se o arquivo não for entendido, nada é filtrado, e o lançamento avisa.

## Na TUI

- **Painel:** a seção MCP lista os servidores da aba, com `✓` habilitado e `✗` desabilitado.
- **Tela:** Ctrl+b `m` abre a tela de MCPs da aba, com o tipo, a origem e o alvo de cada servidor. Espaço habilita ou desabilita o escolhido na aba, e Esc fecha ([tui.md](tui.md)).

## MCP de banco

Com conexões de banco escolhidas na sessão (`db` ou `--db`), o sandmux cadastra o MCP `sandmux-db`, que consulta o banco sem a senha entrar no sandbox ([database.md](database.md)).

## MCPs remotos e locais

- **Remotos:** a conexão passa pelo gateway e segue a [política de rede](network.md). No modo `custom`, libere o domínio do servidor com `allow`.
- **Locais:** rodam dentro do sandbox, com a mesma visão de arquivos, a mesma rede e os mesmos tetos do agente.

## Limites

- Um MCP novo escrito pelo agente no `opencode.json` da pasta de trabalho entra no inventário do próximo lançamento. Com `mcp-allow` em uso, ele fica desabilitado.
- Um harness relançado pelo próprio agente dentro da sessão lê a configuração de novo; a lista de desabilitados é a do lançamento.
