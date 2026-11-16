# vt100 0.18.1, com o DECAWM

Cópia do crate [`vt100`](https://github.com/doy/vt100-rust) 0.18.1 (licença MIT, em `LICENSE`), usada pelo sandmux pelo `[patch.crates-io]` do workspace.

As mudanças são o modo DECAWM (`CSI ? 7 h` e `CSI ? 7 l`), a quebra automática de linha, que a versão publicada não implementa. As linhas mudadas estão em `src/screen.rs`, marcadas com `sandmux:`:

- o bit `MODE_NO_AUTOWRAP`, ligado por `?7l` e desligado por `?7h`;
- em `text()`, com a quebra desligada, o caractere que não cabe fica na última coluna, sobrescrevendo-a, sem passar para a linha de baixo, e o cursor fica na última coluna (sem quebra pendente);
- o estado formatado e a diferença de modos incluem o `?7l`, depois do conteúdo;
- `CSI I` (CHT) e `CSI Z` (CBT), a tabulação para a frente e para trás, que o renderizador usa para mover o cursor com menos bytes (`src/perform.rs`, `src/screen.rs` e `col_backtab` em `src/grid.rs`, com o `col_clamp` visível no crate). O CBT parte da última coluna quando há quebra pendente, como no tmux.

O renderizador do Antigravity CLI (`charmbracelet/ultraviolet`) desliga a quebra para escrever até a última coluna e move o cursor por tabulações; sem isso, a tela da aba se desalinhava (changes `fix-vt100-autowrap` e `fix-vt100-tabs` do sandmux). Quando o upstream tiver essas sequências, esta cópia sai e o `[patch.crates-io]` também.
