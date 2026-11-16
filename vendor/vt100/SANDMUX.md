# vt100 0.18.1, com o DECAWM

Cópia do crate [`vt100`](https://github.com/doy/vt100-rust) 0.18.1 (licença MIT, em `LICENSE`), usada pelo sandmux pelo `[patch.crates-io]` do workspace.

A única mudança é o modo DECAWM (`CSI ? 7 h` e `CSI ? 7 l`), a quebra automática de linha, que a versão publicada não implementa. As linhas mudadas estão em `src/screen.rs`, marcadas com `sandmux:`:

- o bit `MODE_NO_AUTOWRAP`, ligado por `?7l` e desligado por `?7h`;
- em `text()`, com a quebra desligada, o caractere que não cabe fica na última coluna, sobrescrevendo-a, sem passar para a linha de baixo, e o cursor fica na última coluna (sem quebra pendente);
- o estado formatado e a diferença de modos incluem o `?7l`, depois do conteúdo.

O renderizador do Antigravity CLI (`charmbracelet/ultraviolet`) desliga a quebra para escrever até a última coluna; sem o modo, a tela da aba se desalinhava (change `fix-vt100-autowrap` do sandmux). Quando o upstream tiver o modo, esta cópia sai e o `[patch.crates-io]` também.
