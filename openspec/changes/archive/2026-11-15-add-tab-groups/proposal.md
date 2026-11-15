## Why

Item "Sessões nomeadas" da lista depois da v1, escolhido pelo usuário (2026-11-15): várias sessões, cada uma com as suas abas, como no tmux.

Hoje há um conjunto só de abas por usuário, e todo cliente vê todas. Com vários projetos abertos, as abas se misturam na mesma barra.

Decisões do usuário (2026-11-15):

- **Nome:** o nível novo se chama **grupo**. Uma "sessão" continua sendo uma aba (`sandmux ls`, `attach <id>`), e nada do que existe muda de nome.
- **Padrão:** uma aba nova vai para o grupo que a TUI está mostrando. Pelo CLI, sem `--group`, vai para o grupo `principal`. As abas que já existem ficam no `principal`.

## What Changes

- **Grupo de cada aba:** toda sessão tem um grupo, guardado na lista de sessões e devolvido no `sandmux ls`. Um grupo existe enquanto tem abas, menos o `principal`, que sempre existe. Não há comando para criar ou apagar grupo: ele nasce com a primeira aba e some com a última.
- **CLI:**
  - `sandmux run --group <nome>`;
  - `sandmux ls` com a coluna GRUPO, e `--group <nome>` para filtrar;
  - `sandmux group list`: os grupos, com o número de abas;
  - `sandmux group move <id> <grupo>`: leva uma aba para outro grupo;
  - `sandmux group rename <grupo> <novo>`: renomeia um grupo;
  - `sandmux tui --group <nome>`: abre a TUI no grupo.
- **TUI:**
  - mostra só as abas do grupo atual, e a barra mostra o nome do grupo;
  - Ctrl+b `(` e `)` vão ao grupo anterior e ao próximo;
  - Ctrl+b `G` abre a lista de grupos: Enter troca, `n` cria um grupo novo (vazio até a primeira aba), `r` renomeia, `m` leva a aba ativa para o grupo escolhido;
  - uma aba nova vai para o grupo atual;
  - cada TUI tem o seu grupo, então duas janelas podem mostrar grupos diferentes ao mesmo tempo;
  - sem `--group`, a TUI abre no `principal`, ou, se ele está vazio, no primeiro grupo com abas.
- **Nomes de grupo:** de 1 a 32 caracteres, entre letras, dígitos, `-`, `_` e `.`.
- **Protocolo 30:** o grupo no `Create`, na lista de sessões, e as mensagens `SetGroup` e `RenameGroup`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `session-daemon`: o grupo de cada sessão, guardado e restaurado, e as mensagens de mover e renomear.
- `terminal-ui`: a TUI por grupo, a troca de grupo e a lista de grupos.

## Impact

- **`sandmux-proto`:** `group` em `Create` e em `SessionInfo`; `SetGroup` e `RenameGroup`; protocolo 30; a validação do nome do grupo.
- **Daemon:** o campo na sessão e no `SavedSession`, com o padrão `principal`; as mensagens novas.
- **CLI:** `run --group`, `ls --group` e a coluna nova, o subcomando `group` e `tui --group`.
- **TUI:**
  - o grupo atual, o filtro das abas e a troca de grupo (desanexa as abas que saem da tela);
  - as ações `group-prev`, `group-next` e `groups`;
  - o modal da lista de grupos;
  - o bloco do grupo na barra;
  - o item no menu.
- **Testes e documentação:** `docs/sessions.md`, `docs/tui.md`, README e os pares em inglês.
