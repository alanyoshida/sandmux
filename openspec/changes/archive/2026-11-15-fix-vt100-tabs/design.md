## Context

A busca binária na segunda captura do `agy` (83 KB, 176×47), comparando o emulador das abas com o tmux em modo raw, achou o primeiro byte divergente logo depois de `CSI 14 Z`. O `vt100` lista `CSI Z` e `CSI I` como não tratados. O renderizador `ultraviolet` escolhe o movimento de cursor mais curto, e as tabulações entram na conta porque o `xterm-256color` declara `ht` e `cbt`.

## Decisions

- **Paradas a cada 8 colunas,** as do `vt100` (`col_tab`) e as do `xterm-256color`. O `vt100` não implementa definir paradas (`ESC H`, `CSI g`), e o `?5W` do renderizador só as volta ao padrão.
- **CBT:** `col_backtab(n)` primeiro traz o cursor para a última coluna (`col_clamp`) quando há quebra pendente, e depois volta n vezes para a parada anterior, parando na coluna 0. Igual ao tmux nos casos testados, inclusive com a quebra pendente.
- **CHT:** n vezes o `col_tab` existente, limitado à largura. O tmux 3.6 não implementa o CHT, então a referência é o ECMA-48.
- **Fora daqui:** recuar com `CSI D` a partir da quebra pendente fica como no `vt100`, que é como o tmux faz. Um ajuste chegou a ser feito e foi desfeito na comparação.
