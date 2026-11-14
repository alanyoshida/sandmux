# Tasks

## 1. Tema no config

- [x] 1.1 Implementar `ThemeColor`, `Theme`, os temas embutidos (`padrao`, `escuro`, `claro`) e a leitura de `theme` e `[tui.colors]` no `sandmux-policy`, com a linha nos erros; verificar com testes de unidade dos cenários "Tema desconhecido" e "Cor inválida", dos formatos de cor e das cores por cima do tema

## 2. Desenho

- [x] 2.1 Fazer o desenho da TUI usar o tema (barra, abas, aviso, faixa, modais, selecionado, erro, seleção de texto), mantendo o `padrao` igual ao de hoje; verificar com teste de PTY que, com `theme = "escuro"` e `tab-active-bg = "#aa0000"`, a célula da aba ativa tem fundo `#aa0000`, e que sem tema a aba ativa usa vídeo inverso

## 3. Documentação e verificação

- [x] 3.1 Documentar os temas, os elementos e os formatos de cor em `docs/tui.md`; verificar com o teste de documentação, que confere os elementos contra o código
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas
