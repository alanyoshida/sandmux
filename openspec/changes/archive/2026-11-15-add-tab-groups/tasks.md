## 1. Daemon e protocolo

- [x] 1.1 Protocolo 30: `group` em `Create` e `SessionInfo`, `SetGroup`, `RenameGroup`, `check_group`; campo na sessão e no `SavedSession` (padrão `principal`); as mensagens no daemon, gravando a lista

## 2. CLI

- [x] 2.1 `run --group`, `ls --group` e a coluna GRUPO, `sandmux group list|move|rename`, `tui --group`; textos com `tr!`/`trf!` e a ajuda em inglês

## 3. TUI

- [x] 3.1 Grupo atual, filtro das abas no `sync`, troca de grupo (desanexa as que saem), grupo inicial (`--group`, `principal` ou o primeiro com abas), aba nova no grupo atual
- [x] 3.2 Ações `group-prev`, `group-next` e `groups` (teclas `(`, `)`, `G`), modal da lista de grupos (Enter, `n`, `r`, `m`), bloco do grupo na barra, item no menu

## 4. Testes e documentação

- [x] 4.1 Testes: `run --group` e `ls`, restauração com grupos, `group move`/`rename`/`list`, nome inválido, TUI só com as abas do grupo, troca com `)`, aba nova no grupo atual, `m` no modal
- [x] 4.2 Documentação: `docs/sessions.md`, `docs/tui.md`, README e os pares em inglês
