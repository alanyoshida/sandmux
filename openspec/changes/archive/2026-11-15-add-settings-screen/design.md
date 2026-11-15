## Context

A TUI é `ratatui` sobre o emulador `vt100` de cada aba, com modais desenhados em `render.rs`.

O editor de configuração (`config::edit::apply`) faz `Set`, `SetLimit`, `Unset`, `Add` e `Remove` nas camadas (`[global]`, `[profile.*]` e `[project."…"]`):
- preserva comentários e ordem (`toml_edit`);
- revalida o arquivo inteiro antes de gravar.

O tema é uma lista de elementos nomeados com cores.

A barra Powerline (change 34) usou separadores e ícones da área de uso privado do Unicode, que só as Nerd Fonts têm.

## Goals / Non-Goals

**Goals:**
- Barra bonita em qualquer fonte.
- Toda opção do `config.toml` editável pela TUI, com a gravação validada.
- Temas prontos com um tom moderno: bordas arredondadas, cores consistentes.

**Non-Goals:**
- Perfis nomeados na página, que ficam com a linha de comando. O seletor tem global e o projeto da aba ativa.
- Editar as teclas de cada ação e as cores de cada elemento na página. Fica no arquivo e em `docs/tui.md`; a página mostra o tema e a tecla de prefixo.
- O cadastro de provedores próprios do assistente (`[assistant.provider.*]`), que fica no arquivo.

## Decisions

### Estilo `blocos`

A barra começa pelo ` ≡ ` do menu, num bloco `accent`. Cada aba é ` N ` num bloco (`tab-index-*`) e ` nome ` em seguida, com fundo `tab-bg`, e as abas ficam separadas por um espaço no fundo da barra. A aba ativa usa o bloco `tab-active-index-*`, e o nome dela vem na cor desse bloco, em negrito. Os blocos da direita (harness, pasta, cofre, alertas e tempo) são ` texto ` com fundo colorido, sem ícones, separados por um espaço.

Nenhum caractere fora do Latin e dos desenhos de caixa: `•`, `✕`, `‖` e `↻` já estavam na barra antes e são comuns. O `"powerline"` continua igual e passa a ser opcional.

### Gravação fora das camadas

`config::edit::set_value(path, section: &[&str], key, Option<toml_edit::Value>)` grava ou apaga uma chave numa tabela qualquer (`["tui"]`, `["assistant"]`, `["global"]`):
- cria a tabela quando falta, e apaga a tabela que ficou vazia;
- valida o texto novo com `Config::parse` antes de gravar, como o `apply`.

As listas de `[assistant] models` usam o mesmo caminho, com o array inteiro.

### Modelo da página (`tui/settings.rs`)

Cada item tem a seção, o rótulo, a ajuda curta, a chave e o tipo:

| Tipo | O que é |
|---|---|
| `Toggle` | uma chave de liga e desliga das camadas, via `Op::Set` e `Unset` |
| `Choice(&[valores])` | modo de rede, tema, estilo e posição da barra |
| `Limit` e `Text` | memória e processos; modelos, `net-log-days`, prefixo |
| `List` | as listas das camadas, via `Add` e `Remove`, e `[assistant] models` |

**Valor atual:** vem do `Config` relido a cada abertura e a cada gravação. As chaves de camada leem a camada do alvo, e, sem valor nela, mostram o efetivo com a marca "(padrão)" ou "(herdado)".

**Alvo:** `Target::Global` ou `Target::Project(pasta canônica da aba ativa)`. Itens de `[tui]`, `[assistant]` e `[global]`-só valem só em global, e o seletor fica fixo neles.

### Teclas e mouse

- ↑ ↓ movem a seleção;
- Tab troca entre as seções e os itens;
- ← → mudam uma escolha, Espaço ou Enter alterna um toggle, Enter edita um texto ou abre uma lista;
- `g` alterna o alvo;
- Esc volta ou fecha.

O clique numa seção ou num item o seleciona, e o clique num toggle o alterna.

### Temas

Os presets saem de uma paleta base por tema (fundo, superfície, destaque, segundo destaque, vermelho, verde, amarelo, ciano) e definem todos os elementos, inclusive os novos:

- `chip-folder-bg`: o bloco da pasta, com a cor de `accent`;
- `accent`: o `≡`, a borda e os títulos da página;
- `muted-fg`: o texto secundário;
- `ok-fg`: o toggle ligado e o aviso de gravação;
- `page-bg` e `page-fg`: o fundo e o texto da página;
- `surface-bg`: as seções e o item escolhido.

Os presets:

| Tema | Variante |
|---|---|
| Catppuccin | Mocha |
| Rosé Pine | principal |
| Tokyo Night | Night |
| Kanagawa | Wave |
| Gruvbox | escuro |
| Dracula | — |

As cores vêm das paletas públicas de cada um. Os modais ganham bordas arredondadas e um espaço de cada lado. Com o tema `padrao` (as cores do terminal), a página usa as cores do `escuro`.

**Prévia:** na página, passar pelos temas troca o `cfg.theme` da TUI na hora e grava `theme` em `[tui]`. Esc não desfaz, porque cada passo já foi gravado. As amostras são blocos com as cores de destaque do tema.

### Painel lateral

Cada seção começa por uma faixa com fundo `surface-bg`, `▌` e o nome em `accent`, e um resumo à direita em `muted-fg`. Sem `muted-fg` no tema (o `padrao`), o texto secundário fica esmaecido (DIM).

Os medidores de memória e de processos são `█` e `░`, que o terminal desenha na altura da célula:
- verde (`ok-fg`) até 75%;
- amarelo (`status-fg`) até 90%;
- vermelho (`error-fg`) acima.

O painel ganha um espaço de margem depois da borda.

**Conteúdo das seções (a "mistura", escolhida pelo usuário):**
- Proteção: os itens do nível (namespaces, Landlock, seccomp), GPU, KVM, clipboard e `pass-env` em pontos de estado numa grade de duas colunas (`●` na cor do estado, `○` apagado); pasta, extras, camada de escrita e as variáveis do `pass-env` em colunas alinhadas. A rede sai da Proteção e fica só na seção Rede.
- Recursos: colunas alinhadas (rótulo apagado à esquerda, valor à direita), com os medidores embaixo da memória e dos processos.
- Rede: o modo numa linha, e as conexões numa tabela sem bordas (domínio, `↑`, `↓`, status); domínio longo numa linha própria, com os números embaixo.
- Valor que não cabe na linha do rótulo vai para a linha de baixo.

**Ordem e recolher:** cada seção é desenhada à parte, a faixa e depois o corpo, com a altura do corpo quebrado medida por `Paragraph::line_count` (o recurso `unstable-rendered-line-info` do ratatui). Assim a TUI sabe a linha de cada faixa e guarda as áreas de clique:
- o título recolhe ou expande;
- `↑` e `↓` trocam a seção com a vizinha.

A mudança vale na hora e é gravada com `set_value` em `[tui]` (`panel-order`, `panel-collapsed`); a ordem padrão e a lista vazia apagam a chave. Uma gravação recusada aparece como aviso na barra, e a mudança vale até fechar a TUI.

### Menu

O `Modal::Menu(sel)` lista o `HELP` (ação, descrição, tecla), sem o próprio menu; o `HELP` passa a ter também arquivos, pastas, MCP e configurações. Enter chama `action(nome)`, e a tecla de uma ação, sem o prefixo, também a executa. O `≡` fica no começo da barra, nos estilos `blocos` e `powerline`; o `simples` não tem.

## Risks / Trade-offs

- **[Muitos itens numa página]** As seções mantêm cada lista curta. As opções raras ficam com a ajuda, que diz onde editar no arquivo.
- **[Gravar a cada passo do tema]** Grava algumas vezes ao passar pelos temas. O arquivo é pequeno, e a gravação é atômica.
