# Proposal

## Why

A TUI do change 5 desenha a barra, os avisos e os modais com o vídeo inverso e as cores do terminal do usuário. O pedido original do usuário inclui temas de cores para abas, componentes, janelas e botões, e o roadmap decidiu que eles ficam no `config.toml`. Este change dá à TUI um tema de cores, com temas embutidos e a troca da cor de cada elemento.

## What Changes

- **Temas embutidos**, escolhidos por `theme` na seção `[tui]`:
  - `padrao`, o atual, com as cores do terminal e vídeo inverso;
  - `escuro`, com fundo escuro e destaque azul;
  - `claro`, com fundo claro e destaque azul-escuro.
- **Cor por elemento** em `[tui.colors]`, por cima do tema. Os elementos são:
  - a aba ativa e as outras abas (frente e fundo);
  - o fundo da barra;
  - o aviso da barra;
  - a faixa de aba encerrada ou parada;
  - a borda e o título dos modais;
  - o item selecionado nas listas e no campo ativo dos modais;
  - o erro nos modais;
  - a seleção de texto.
- **Valores de cor:** `#rrggbb`, um índice de 0 a 255, um dos 16 nomes ANSI (`black`, `red`… `bright-white`) ou `default`, que é a cor do terminal.
- **Validação:** um tema, um elemento ou uma cor desconhecidos tornam o arquivo inválido, com a linha.
- O tema vale ao abrir a TUI. Mudar o arquivo vale na próxima abertura.

## Capabilities

### New Capabilities
- Nenhuma.

### Modified Capabilities
- `terminal-ui`: requisitos novos "Tema de cores" e "Cor por elemento".

## Impact

- **`sandmux-policy`:** `TuiConfig` ganha o tema e as cores, com a leitura e a validação de `theme` e `[tui.colors]`.
- **`sandmux` (TUI):** o desenho passa a usar o tema resolvido, sem cores fixas no código.
- **Documentação:** `docs/tui.md`, com os temas, os elementos e os formatos de cor.
