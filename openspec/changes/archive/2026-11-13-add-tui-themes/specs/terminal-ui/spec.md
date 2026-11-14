# Spec Delta

## ADDED Requirements

### Requirement: Tema de cores
A chave `theme` da seção `[tui]` MUST escolher um tema embutido: `padrao`, que usa as cores do terminal e vídeo inverso e é o padrão; `escuro`; ou `claro`. A TUI MUST desenhar a barra, os avisos, as faixas de aba encerrada e os modais com as cores do tema. Um tema desconhecido MUST tornar o arquivo inválido.

#### Scenario: Tema escuro
- **WHEN** `[tui]` tem `theme = "escuro"` e o usuário abre a TUI
- **THEN** a aba ativa aparece com as cores de destaque do tema escuro, e não em vídeo inverso

#### Scenario: Tema desconhecido
- **WHEN** `[tui]` tem `theme = "neon"`
- **THEN** o arquivo é considerado inválido, com a linha e os temas aceitos na mensagem

### Requirement: Cor por elemento
A seção `[tui.colors]` MUST trocar, por cima do tema, a cor de cada elemento: `tab-active-fg`, `tab-active-bg`, `tab-fg`, `tab-bg`, `bar-bg`, `status-fg`, `note-fg`, `note-bg`, `modal-border`, `modal-title`, `selected-fg`, `selected-bg`, `error-fg`, `selection-fg` e `selection-bg`. Cada valor MUST ser `#rrggbb`, um índice de 0 a 255, um dos 16 nomes ANSI ou `default`. Um elemento ou valor desconhecido MUST tornar o arquivo inválido.

#### Scenario: Aba ativa vermelha
- **WHEN** `[tui.colors]` tem `tab-active-bg = "#aa0000"`
- **THEN** a aba ativa aparece com fundo `#aa0000`, e o resto segue o tema

#### Scenario: Cor inválida
- **WHEN** `[tui.colors]` tem `tab-bg = "roxo-claro"`
- **THEN** o arquivo é considerado inválido, com a linha e os formatos aceitos
