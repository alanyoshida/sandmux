## Why

Na primeira visão da barra Powerline (change 34), o Konsole do usuário mostrou os separadores e os ícones quebrados: eles só existem nas fontes Nerd Font, e o perfil dele não usa uma. O usuário pediu também (2026-11-14):
- um menu;
- uma página de configurações, com as opções do `config.toml` como interface visual e toggles para ligar e desligar funcionalidades, que atualize o arquivo quando algo muda;
- uma interface bonita e moderna;
- temas prontos: Catppuccin, Rosé Pine, Tokyo Night, Kanagawa, Gruvbox e Dracula.

## What Changes

- **Barra que funciona em qualquer fonte:** o estilo padrão passa a ser `"blocos"`: blocos coloridos com espaço em volta, sem símbolos especiais. `"powerline"` (separadores inclinados e ícones) fica como opção para quem tem uma Nerd Font. `"simples"` continua.
- **Menu (Ctrl+b `:`):** todas as ações da TUI numa lista, com setas e Enter, e também por clique no `≡` do começo da barra.
- **Configurações (Ctrl+b `e`, ou pelo menu):** uma página de tela cheia com seções à esquerda e itens à direita:
  - **Aparência:** tema, estilo e posição da barra, painel;
  - **Atalhos:** a tecla de prefixo;
  - **Sandbox:** repo, kvm, gpu, clipboard, memória, processos;
  - **Rede:** modo, denylist, allowlist, catálogos, decifração, portas de entrada e portas do host;
  - **Arquivos:** listas de arquivos;
  - **Assistente:** modelos de chat e de comando, lista de modelos;
  - **Registro:** retenção da rede.

  Os itens são toggles (`● ligado` e `○ desligado`), escolhas (`‹ valor ›`), textos e listas (com acrescentar e tirar). Um seletor no topo escolhe onde a mudança vale: global, ou o projeto da aba ativa.
- **Gravação na hora:** cada mudança é gravada no `config.toml` com o editor de configuração já usado pelo `sandmux config`, que valida o arquivo inteiro antes de gravar. Uma mudança recusada mostra o motivo e não grava. O que vale para as sessões segue as regras de sempre (algumas valem ao vivo, outras no próximo lançamento), e a página diz isso por item.
- **Temas prontos:** `catppuccin`, `rose-pine`, `tokyo-night`, `kanagawa`, `gruvbox` e `dracula`, além de `padrao`, `escuro` e `claro`. Eles pintam a barra, os modais, o painel e a página de configurações, e na página o tema muda na hora ao passar por ele, com amostras de cor.
- **Harness e pasta separados:** à direita da barra, o harness e a pasta da aba ativa ficam em blocos de cores diferentes (elemento `chip-folder-bg`), pedido do usuário depois de ver a barra.
- **Painel lateral (pedido do usuário, 2026-11-14):** seções mais separadas, cada uma com uma faixa de título e um resumo à direita; rótulos num tom mais apagado; medidores de memória e de processos; cores de estado na rede e nos MCPs. Depois (pedido do usuário, 2026-11-14): um clique no título de uma seção a recolhe ou expande, e as setas `↑` e `↓` da faixa mudam a ordem, gravadas em `[tui]` (`panel-order`, `panel-collapsed`). E (escolha do usuário entre cinco desenhos, 2026-11-14) o conteúdo das seções na "mistura": pontos de estado na Proteção, colunas alinhadas com medidores em Recursos, tabela sem bordas na Rede.
- **Visual:** modais e página com bordas arredondadas (caracteres de caixa comuns) e cores do tema.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: o estilo padrão da barra, o menu, a página de configurações e os temas.
- `config-profiles`: a gravação de chaves de `[tui]`, `[assistant]` e das chaves só de `[global]` pelo editor de configuração.

## Impact

- **`sandmux-policy`:**
  - `config.rs`: o estilo `blocos`, os temas novos, os elementos `chip-folder-bg`, `accent`, `muted-fg`, `ok-fg`, `page-bg`, `page-fg` e `surface-bg`, e as ações `menu` e `settings`;
  - `config/edit.rs`: `set_value`, o valor numa seção qualquer, validado.
- **`sandmux`:**
  - `tui/settings.rs` novo: o modelo da página, os itens e a gravação;
  - `tui/render.rs`: a barra, o menu, a página e as bordas;
  - `tui/mod.rs`: as ações `menu` e `settings`.
- **Documentação:** `docs/tui.md`, `docs/config.md`, README.
