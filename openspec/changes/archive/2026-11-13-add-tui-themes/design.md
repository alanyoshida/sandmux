# Design

## Context

O desenho da TUI (`tui/render.rs`) usa estilos fixos: vídeo inverso e negrito na aba ativa, `DIM` nas outras, itálico no aviso, vermelho no erro e vídeo inverso na faixa de aba encerrada e na seleção. A seção `[tui]` é lida por `tui_section` no `sandmux-policy`, que valida `prefix` e `[tui.keys]` com a linha nos erros.

## Goals / Non-Goals

**Goals:**

- Todo estilo da barra, dos avisos, das faixas e dos modais vem de um `Theme`. O desenho não tem cor fixa.
- O tema `padrao` desenha exatamente como hoje.

**Non-Goals:**

- Mudar as cores do conteúdo das abas, que são do harness.
- Recarregar o tema com a TUI aberta.

## Decisions

### 1. Tipos no `sandmux-policy`

- **`ThemeColor`:** `Default`, `Indexed(u8)` ou `Rgb(u8, u8, u8)`. Os 16 nomes ANSI viram os índices 0 a 15.
- **`Theme`:** um `ThemeColor` opcional por elemento, mais dois marcadores, `reverse_active` e `reverse_note`, para o tema `padrao`.
- **`TuiConfig`:** ganha `theme: Theme`, resolvido na leitura. Primeiro o tema embutido, depois as cores de `[tui.colors]` por cima.

O `sandmux-policy` não depende do `ratatui`. A conversão para o estilo do `ratatui` fica no binário.

### 2. Temas embutidos

- **`padrao`:** sem cores, com vídeo inverso na aba ativa, na faixa e na seleção, como hoje.
- **`escuro`:**
  - barra `#1e1e2e`;
  - aba ativa `#89b4fa` sobre `#1e1e2e`;
  - abas `#a6adc8`;
  - aviso `#f9e2af`;
  - faixa `#1e1e2e` sobre `#f38ba8`;
  - modal com borda `#89b4fa`;
  - selecionado `#1e1e2e` sobre `#89b4fa`;
  - erro `#f38ba8`;
  - seleção de texto `#1e1e2e` sobre `#f9e2af`.
- **`claro`:** a mesma estrutura, com fundo `#eff1f5` e destaque `#1e66f5`.

### 3. Validação

`theme` aceita `padrao`, `escuro` e `claro`. Os elementos de `[tui.colors]` e os formatos de cor seguem a spec, com a linha no erro, pelo mesmo `ErrFn` da leitura.

## Risks / Trade-offs

- **[Terminal sem cor verdadeira]** Um tema com `#rrggbb` num terminal de 256 cores aparece aproximado pelo próprio terminal. → O `padrao` não usa cor nenhuma.
