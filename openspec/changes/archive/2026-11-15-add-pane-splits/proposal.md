## Why

Pedido do usuário (2026-11-15): dividir a tela da TUI em painéis, lado a lado e em cima e embaixo, como no tmux.

Hoje cada aba é uma sessão que ocupa a área inteira, e só a aba ativa aparece.

Decisões do usuário (2026-11-15):

- **Modelo:** aba com painéis, como no tmux. Uma aba da barra pode ter vários painéis, cada um uma sessão.
- **Novo painel:** abre o modal de nova aba, já com a pasta do painel atual. A escolha `chat` também serve.
- **Layout:** guardado no daemon. Ele volta ao reabrir a TUI, aparece em outra janela e sobrevive ao reboot.

## What Changes

- **Dividir:**
  - Ctrl+b `%` divide o painel ativo lado a lado;
  - Ctrl+b `"` divide em cima e embaixo.

  O modal de nova aba abre com a pasta do painel atual, e a sessão criada ocupa a metade nova.
- **Painéis:**
  - Ctrl+b com as setas move o foco para o painel vizinho;
  - um clique num painel também o põe em foco;
  - Ctrl+b `z` amplia o painel ativo para a aba inteira, e de novo volta;
  - Ctrl+b `<` e `>` diminuem e aumentam o painel ativo em 5%.
- **Fechar:** Ctrl+b `x` fecha o painel ativo (encerra a sessão dele), e o vizinho ocupa o espaço. Com um painel só, a aba volta a ser comum.
- **Barra:** uma entrada por aba. O nome é o do primeiro painel, com `+N` para os outros, e os números de Ctrl+b `1`..`9` contam as abas.
- **Separadores** entre os painéis, na cor secundária, e os do painel ativo na cor de destaque.
- **Tamanho:** cada sessão recebe o tamanho do seu painel.
- **Daemon:** guarda os layouts (árvore de divisões com as sessões nas folhas) na lista de sessões e os devolve com a listagem. Uma sessão encerrada sai do layout.
- **Protocolo 32:** `SetLayout`, e os layouts na resposta `Sessions`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: painéis na aba (dividir, foco, ampliar, redimensionar, fechar) e a barra por aba.
- `session-daemon`: os layouts guardados e restaurados com as sessões.

## Impact

- **`sandmux-proto`:** o tipo do layout (árvore de divisões, com os retângulos, o vizinho e a remoção de uma folha), `SetLayout`, os layouts em `Sessions` e o protocolo 32.
- **Daemon:** os layouts no estado e no `sessions.json`, a validação no `SetLayout` e a limpeza das sessões encerradas.
- **TUI:**
  - a barra por aba;
  - o desenho dos painéis e dos separadores;
  - o tamanho por painel;
  - as ações (`split-side`, `split-stack`, `zoom`, `pane-grow`, `pane-shrink`) e as setas depois do prefixo;
  - o clique e o mouse por painel;
  - a divisão pendente no modal de nova aba.
- **Testes:** dividir lado a lado e em cima e embaixo, foco pelas setas, ampliar, fechar um painel, o layout de volta depois de reabrir a TUI e de reiniciar o daemon, testes unitários da árvore.
- **Documentação:** `docs/tui.md`, `docs/sessions.md` e os pares em inglês.
