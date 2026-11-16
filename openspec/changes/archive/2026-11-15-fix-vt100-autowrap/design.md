## Context

As abas usam o crate `vt100` 0.18.1, na TUI e no daemon. Ele trata os modos privados 1, 6, 9, 25, 47, 1000-1006, 1049 e 2004, e manda o resto a um callback de "não tratado". A captura de uma resposta do `agy` (renderizador `charmbracelet/ultraviolet`) tinha 55 pares de `?7l`/`?7h`. Um teste mínimo (`?7l`, 25 caracteres numa tela de 20 colunas) mostrou o `vt100` quebrando a linha e o tmux mantendo tudo na primeira linha, com o último caractere na última coluna. O código do `ultraviolet` confirma que ele desliga a quebra ao escrever até a última coluna.

## Decisions

- **Cópia local do crate,** com `[patch.crates-io] vt100 = { path = "vendor/vt100" }` e o `vendor` fora dos membros do workspace, para o `fmt` e o `clippy` do projeto não passarem por ele. Não há versão publicada do `vt100` com o modo. A cópia guarda a licença MIT e um `SANDMUX.md` com o que mudou, para devolver ao upstream ou trocar pela versão publicada quando ele tiver o modo.
- **Bit de modo `MODE_NO_AUTOWRAP`:** `?7l` liga e `?7h` desliga. Em `text()`, com o bit ligado e o caractere sem caber, a coluna volta para `largura da tela - largura do caractere` e não há quebra, nem marcação de linha quebrada. O `RIS` (`ESC c`) recria a tela, com a quebra ligada.
- **Estado reenviado:** com a quebra desligada, o estado formatado (`state_formatted`, usado pelo daemon ao anexar um cliente) e a diferença de modos incluem `?7l`.

## Risks / Trade-offs

- [Manter uma cópia de dependência] → Ela muda só no que o `SANDMUX.md` descreve. A troca pela versão publicada é uma linha do `Cargo.toml`.
