## 1. Barra e temas

- [x] 1.1 Estilo `blocos` como padrão (sem símbolos especiais), `powerline` opcional, o `≡` do menu na barra
- [x] 1.2 Temas `catppuccin`, `rose-pine`, `tokyo-night`, `kanagawa`, `gruvbox` e `dracula`, elementos `chip-folder-bg`, `accent`, `muted-fg`, `ok-fg`, `page-bg`, `page-fg` e `surface-bg`, modais com bordas arredondadas e o painel com o tema

## 2. Configurações

- [x] 2.1 `config::edit::set_value` para chaves de `[tui]`, `[assistant]` e `[global]`, validado, com testes
- [x] 2.2 Modelo da página: seções, itens, valor atual por alvo, gravação e mensagens, com testes
- [x] 2.3 Desenho da página, teclas, mouse, prévia do tema, editor de lista e de texto
- [x] 2.4 Menu (Ctrl+b `:`) e as ações `menu` e `settings`

## 3. Painel lateral

- [x] 3.1 Faixas de título com resumo, rótulos apagados, medidores de memória e de processos, cores de estado na rede e nos MCPs
- [x] 3.2 Clique no título recolhe ou expande, `↑` e `↓` mudam a ordem, gravados em `[tui]` (`panel-order`, `panel-collapsed`), com teste
- [x] 3.3 Conteúdo na "mistura": pontos de estado na Proteção, colunas alinhadas com medidores em Recursos, tabela sem bordas na Rede

## 4. Verificação e documentação

- [x] 4.1 Testes da TUI: menu, toggle gravado, valor recusado, lista, tema
- [x] 4.2 `docs/tui.md`, `docs/config.md`, README
