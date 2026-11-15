## 1. Layout

- [x] 1.1 `Pane` em `sandmux-proto` (folhas, dividir, remover, retângulos e separadores, vizinho, redimensionar), com testes unitários; protocolo 32, `SetLayout`, layouts em `Sessions`
- [x] 1.2 Daemon: layouts no estado e no `sessions.json`, validação do `SetLayout`, poda ao gravar, layouts na listagem e na restauração

## 2. TUI

- [x] 2.1 Abas da barra (primeira folha ou sessão solta) nos três estilos, números, próximo e anterior, clique, `+N`
- [x] 2.2 Desenho dos painéis e separadores, tamanho por painel no `resize_all`, cursor e nota no painel em foco, ampliar
- [x] 2.3 Ações: dividir (divisão pendente no modal de nova aba e no `add_tab`), setas depois do prefixo, ampliar, aumentar e diminuir; clique e mouse por painel

## 3. Testes e documentação

- [x] 3.1 Testes: dividir lado a lado e em cima e embaixo, foco pelas setas, ampliar, fechar um painel, layout de volta ao reabrir a TUI e depois de reiniciar o daemon
- [x] 3.2 Documentação: `docs/tui.md`, `docs/sessions.md` e os pares em inglês
