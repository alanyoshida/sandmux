## Why

Pedidos do usuário (2026-11-15): depois de ligar a janela na tela de configurações, o Ctrl+b `r` "não parecia funcionar", e ele pediu um aviso de que as abas precisam ser recarregadas. O Ctrl+b `r` só relança abas encerradas ou paradas: numa aba viva, ele recusava com um aviso curto na barra. A tela de configurações dizia "vale no próximo lançamento da aba (Ctrl+b r)", que indicava um caminho que não funcionava numa aba viva. Decisão do usuário (2026-11-15): o Ctrl+b `r` reinicia também uma aba viva, com confirmação.

## What Changes

- **Reiniciar uma aba viva:** na TUI, Ctrl+b `r` numa aba viva pergunta "Reiniciar a aba? O harness é encerrado. s/n". No sim, o daemon encerra o sandbox (TERM, e KILL depois de 5 s) e relança na mesma pasta, com as mesmas opções e a configuração de agora. Na linha de comando, `sandmux restart <id> --force`.
- **Sem efeitos de aba encerrada:** o encerramento do reinício não dispara a notificação de aba encerrada, nem o relançamento automático do gateway do hermes.
- **Aviso na configuração:** uma mudança que vale no lançamento (as seções Sandbox, Rede, Arquivos e MCP) mostra quantas abas vivas continuam com o valor antigo e que Ctrl+b `r` as reinicia. O `sandmux config set` mostra o mesmo aviso quando o daemon tem sessões vivas.
- **Janela no painel:** pedido do usuário (2026-11-15): a seção Proteção do painel lateral mostra se a janela está ligada (restrita ou direta), e o `sandmux stats` também.
- **Protocolo 35:** `force` no pedido `restart`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-sessions`: `sandmux restart <id> --force` reinicia uma sessão viva.
- `terminal-ui`: Ctrl+b `r` reinicia uma aba viva com confirmação; a tela de configurações avisa quais abas vivas usam o valor antigo; o painel mostra a janela.

## Impact

- **`sandmux-proto`:** `force` no `Restart`; protocolo 35.
- **Daemon:** parar a sessão para o reinício, marcada como reiniciando.
- **TUI:** o modal de confirmação e o aviso na tela de configurações.
- **CLI:** `--force` no `restart`; o aviso no `config set`.
- **Testes e documentação:** `docs/sessions.md`, `docs/tui.md`, `docs/config.md` e os pares em inglês.
