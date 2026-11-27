---
name: sandmux
description: Site do sandmux, multiplexador de terminal que sobe cada agente numa aba confinada.
colors:
  sand: "#e8b04b"
  sand-hover: "#f0c062"
  on-sand: "#1d1505"
  sand-ink: "#8f6210"
  logo-sand: "#9a6a12"
  paper: "#faf9f6"
  paper-raised: "#ffffff"
  paper-sunken: "#f1efe9"
  paper-line: "#e2dfd6"
  paper-line-strong: "#cdc9bd"
  paper-code: "#f3f1ec"
  ink: "#16181d"
  ink-2: "#464b55"
  ink-muted: "#646a76"
  graphite: "#141414"
  graphite-raised: "#1b1b1b"
  graphite-sunken: "#212121"
  graphite-line: "#2c2c2c"
  graphite-line-strong: "#3d3d3d"
  chalk: "#ececea"
  chalk-2: "#c0bfbb"
  chalk-muted: "#9c9b96"
  limit-rust: "#a3441b"
  limit-coral: "#f08a5d"
  gruvbox-bg: "#282828"
  gruvbox-bar: "#1d2021"
  gruvbox-line: "#3c3836"
  gruvbox-line-dark: "#32302f"
  gruvbox-fg: "#ebdbb2"
  gruvbox-fg2: "#d5c4a1"
  gruvbox-gray: "#a89984"
  gruvbox-dot: "#504945"
  gruvbox-yellow: "#fabd2f"
typography:
  display:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(2.6rem, 5.4vw, 5rem)"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(1.75rem, 3vw, 2.4rem)"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  headline-doc:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(2rem, 3.4vw, 2.6rem)"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.03em"
  title:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.55rem"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  title-sm:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.15rem"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  lede:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.1rem"
    fontWeight: 400
    lineHeight: 1.65
  body:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.65
  label:
    fontFamily: "Clear Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.8rem"
    fontWeight: 500
    letterSpacing: "0.04em"
  mono:
    fontFamily: "Fira Code, ui-monospace, DejaVu Sans Mono, monospace"
    fontSize: "0.9rem"
    fontWeight: 400
    lineHeight: 1.7
    fontFeature: "\"liga\" 0, \"calt\" 0"
rounded:
  focus: "4px"
  code: "5px"
  rail: "6px"
  control: "8px"
  block: "10px"
  window: "12px"
spacing:
  gutter: "24px"
  gutter-mobile: "16px"
  max: "1200px"
  doc-max: "1320px"
  header: "64px"
  band: "96px"
  band-mobile: "72px"
  band-head: "36px"
components:
  button-primary:
    backgroundColor: "{colors.sand}"
    textColor: "{colors.on-sand}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 20px"
    height: "44px"
  button-primary-hover:
    backgroundColor: "{colors.sand-hover}"
    textColor: "{colors.on-sand}"
  button-secondary:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 20px"
    height: "44px"
  terminal-window:
    backgroundColor: "{colors.gruvbox-bg}"
    textColor: "{colors.gruvbox-fg}"
    rounded: "{rounded.window}"
  terminal-window-bar:
    backgroundColor: "{colors.gruvbox-bar}"
    textColor: "{colors.gruvbox-gray}"
    height: "34px"
    padding: "0 14px"
  install-line:
    backgroundColor: "{colors.gruvbox-bg}"
    textColor: "{colors.gruvbox-fg}"
    typography: "{typography.mono}"
    rounded: "{rounded.control}"
    padding: "11px 16px"
  layer-stack:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.window}"
    padding: "16px 20px"
  layer-stack-agent:
    backgroundColor: "{colors.gruvbox-bg}"
    textColor: "{colors.gruvbox-yellow}"
  nav-link:
    textColor: "{colors.ink-2}"
    rounded: "{rounded.control}"
    padding: "6px 10px"
  nav-link-current:
    backgroundColor: "{colors.paper-sunken}"
    textColor: "{colors.ink}"
  sidebar-link:
    textColor: "{colors.ink-2}"
    rounded: "{rounded.rail}"
    padding: "5px 10px"
  code-inline:
    backgroundColor: "{colors.paper-sunken}"
    textColor: "{colors.ink}"
    rounded: "{rounded.code}"
    padding: "0.08em 0.36em"
---

# Design System: sandmux

## Overview

**Creative North Star: "A bancada do terminal"**

O site segue o padrão das páginas de ferramenta para desenvolvedores, com a régua de acabamento do Ghostty e do Zed: o produto é a estrela, a cópia é curta e verificável, e nada é enfeite. A peça central é a TUI real do sandmux, gravada pelo VHS, mostrada em janelas de terminal Gruvbox que ficam escuras nos dois temas. Em volta delas, uma superfície neutra e calma: grafite no escuro, papel morno no claro, texto em Clear Sans e um único destaque em cor de areia.

A densidade é a de documentação técnica bem cuidada. A página inicial alterna faixas largas com cabeçalho curto e conteúdo concreto (tabela do que o agente enxerga, galeria de telas, pilhas de camadas, o que o sandbox impede e o que não promete, a instalação num terminal). As páginas de docs usam três colunas: menu, texto e índice. A profundidade vem de tom e de linhas de 1px, não de sombra. O site não roda JavaScript e não carrega nada de outro endereço: fontes, imagens e CSS saem do próprio site.

O site recusa a fileira de cards iguais, o gradiente no título, o eyebrow acima do título e o neon no escuro.

**Key Characteristics:**
- Um só destaque, areia, usado em links, foco, seleção, botão principal, marca da pilha e marcadores.
- A TUI em Gruvbox (fundo `#282828`) é a imagem do site; o terminal não muda com o tema.
- Superfícies planas, separadas por linhas de 1px e por três tons de fundo (base, elevado, rebaixado).
- Clear Sans em tudo que é texto; Fira Code em tudo que é código ou terminal, sem ligaduras.
- Sem JavaScript, sem recursos de terceiros, tema só por `prefers-color-scheme`.

## Colors

Neutros quase sem croma, um destaque de areia e a paleta Gruvbox confinada às janelas de terminal.

### Primary
- **Areia** (`sand`, #e8b04b): o destaque único. No escuro é a cor dos links e do texto de destaque; nos dois temas é o fundo do botão principal, o fundo da seleção de texto, o anel de foco, o ponto do selo de situação e a borda ativa do menu lateral.
- **Areia acesa** (`sand-hover`, #f0c062): só o hover do botão principal.
- **Tinta sobre areia** (`on-sand`, #1d1505): texto sobre qualquer fundo de areia (botão, seleção, link de pular).
- **Areia escura** (`sand-ink`, #8f6210): o destaque como texto no claro. Links, marcadores de lista positiva e o traço do logo em linha usam esta cor no tema claro, porque passa de 4,5:1 sobre o papel.
- **Areia do favicon** (`logo-sand`, #9a6a12): só o traço do `favicon.svg` no claro. Não é cor de texto.

### Neutral
- **Papel** (`paper`, #faf9f6): fundo da página no claro. Morno, nunca branco puro.
- **Papel elevado** (`paper-raised`, #ffffff): legendas das janelas, pilhas de camadas, botão secundário, gaveta do menu no celular.
- **Papel rebaixado** (`paper-sunken`, #f1efe9): hover e item atual da navegação, código em linha, citações, cabeçalho de tabela.
- **Código no papel** (`paper-code`, #f3f1ec): blocos de código dos docs no claro.
- **Linha** (`paper-line`, #e2dfd6) e **linha forte** (`paper-line-strong`, #cdc9bd): divisórias de 1px; a forte contorna janelas, pilhas, botões e o seletor de idioma.
- **Tinta** (`ink`, #16181d), **tinta 2** (`ink-2`, #464b55) e **tinta apagada** (`ink-muted`, #646a76): texto principal, texto secundário e rótulos no claro.
- **Grafite** (`graphite`, #141414): fundo da página no escuro, da mesma família neutra do fundo Gruvbox.
- **Grafite elevado** (`graphite-raised`, #1b1b1b), **grafite rebaixado** (`graphite-sunken`, #212121), **linha** (`graphite-line`, #2c2c2c) e **linha forte** (`graphite-line-strong`, #3d3d3d): os mesmos papéis do claro, no escuro. O grafite elevado também é o fundo dos blocos de código no escuro.
- **Giz** (`chalk`, #ececea), **giz 2** (`chalk-2`, #c0bfbb) e **giz apagado** (`chalk-muted`, #9c9b96): texto principal, secundário e rótulos no escuro.

### Tertiary
- **Ferrugem** (`limit-rust`, #a3441b no claro) e **coral** (`limit-coral`, #f08a5d no escuro): só o traço que marca cada item da lista "O que ele não promete". É cor de sinal, não de destaque; não aparece em link, botão nem fundo.

### Terminal (Gruvbox)
- **Fundo Gruvbox** (`gruvbox-bg`, #282828): fundo das janelas, da linha de instalação e da camada "agente" da pilha, nos dois temas. É o tema `GruvboxDark` das gravações do VHS.
- **Barra** (`gruvbox-bar`, #1d2021), **bolinhas** (`gruvbox-dot`, #504945) e **título da barra** (`gruvbox-gray`, #a89984, também comentário de shell): a barra da janela.
- **Divisória do terminal** (`gruvbox-line`, #3c3836 no claro; `gruvbox-line-dark`, #32302f no escuro).
- **Texto do terminal** (`gruvbox-fg`, #ebdbb2) e **texto 2** (`gruvbox-fg2`, #d5c4a1).
- **Amarelo do prompt** (`gruvbox-yellow`, #fabd2f): o `$ ` dos comandos e o nome da camada "agente". Vive só dentro do terminal.

O realce de código dos docs (`chroma.css`) usa o estilo `github` no claro e o `gruvbox` no escuro, gerados pelo Hugo; essas cores são do realce, não do sistema.

### Named Rules
**The One Sand Rule.** Areia é o único destaque fora do terminal. Toda cor de ação, foco ou seleção é areia; não existe segundo destaque, azul de link nem cor de marca alternativa.

**The Terminal Stays Gruvbox Rule.** Janelas de terminal são Gruvbox escuro nos dois temas. O tema claro muda a página em volta, nunca o terminal; as cores Gruvbox não saem da janela.

**The Text-Sand Split Rule.** Areia clara (#e8b04b) é fundo ou texto sobre grafite; sobre papel, texto de destaque usa a areia escura (#8f6210).

## Typography

**Display Font:** Clear Sans (com ui-sans-serif, system-ui, sans-serif)
**Body Font:** Clear Sans (com ui-sans-serif, system-ui, sans-serif)
**Label/Mono Font:** Fira Code (com ui-monospace, DejaVu Sans Mono, monospace)

**Character:** Uma sans humanista de leitura técnica, sem personalidade de vitrine, com uma mono de terminal de verdade. As duas são servidas pelo próprio site em woff2, com as licenças em `website/static/fonts/`: Clear Sans em 400, 400 itálico, 500 e 700; Fira Code em 400 e 500.

### Hierarchy
- **Display** (700, clamp(2.6rem, 5.4vw, 5rem), 1, -0.035em): só o título da página inicial, em duas linhas.
- **Headline** (700, clamp(1.75rem, 3vw, 2.4rem), 1.2, -0.02em): título de cada faixa da página inicial. O título de uma página de docs usa clamp(2rem, 3.4vw, 2.6rem) com -0.03em.
- **Title** (700, 1.55rem, 1.2): `h2` dos docs. O `h3` é 1.2rem e o `h4` 1.02rem; os títulos de bloco da página inicial (pilhas, listas) são 1.15rem.
- **Lede** (400, 1.1rem, 1.65): o resumo ao lado do título. O texto sob o título de faixa é 1.08rem.
- **Body** (400, 17px, 1.65; 16px abaixo de 860px): texto corrido. Parágrafos e itens dos docs param em 70ch; a coluna de texto em 48rem.
- **Label** (500, 0.8rem, 0.04em, maiúsculas): só os nomes de grupo do menu lateral e o "Nesta página" do índice. O cabeçalho da tabela da página inicial é 0.85rem, 500, sem maiúsculas.
- **Mono** (400, 0.9rem, 1.7 no terminal; 0.86rem e 1.6 nos blocos de código dos docs; 0.86em em linha): comandos, código, nomes de arquivo, barra da janela (500, 12px).

Todos os títulos usam `text-wrap: balance`; parágrafos e itens, `text-wrap: pretty`.

### Named Rules
**The No Ligatures Rule.** Código, `pre` e `kbd` usam `font-variant-ligatures: none`. Num site de segurança, `->`, `!=` e `==` aparecem como foram digitados.

**The Two Families Rule.** Clear Sans para texto e títulos, Fira Code para código e terminal. Nenhuma terceira família, nenhuma fonte de display.

## Layout

A página inicial é uma coluna de até 1200px (`--max`) com margem lateral de 24px, que cai para 16px abaixo de 860px. A primeira dobra é uma grade de 7fr/5fr: o título à esquerda e o resumo com o selo de situação à direita, alinhados pela base; abaixo, numa linha só, o comando de instalação inteiro e o botão principal; depois a janela da TUI na largura toda. Abaixo de 980px a grade vira uma coluna, e a linha de instalação passa a ocupar a largura toda acima do botão.

As faixas seguintes têm 96px de respiro acima (72px no celular), um cabeçalho de até 40rem e 36px até o conteúdo. A galeria põe a primeira tela na largura toda e as outras em duas colunas, que viram uma abaixo de 860px. As camadas usam 3fr/2fr e a seção de segurança duas colunas iguais; as duas viram coluna única abaixo de 980px.

As docs usam uma grade de até 1320px: menu de 240px, texto e índice de 220px, com 48px entre colunas. O menu e o índice ficam presos com `position: sticky` a 88px do topo, abaixo do cabeçalho de 64px, que também é sticky. Abaixo de 1180px o índice some e o menu cai para 220px; abaixo de 860px o menu vira uma gaveta `<details>` acima do texto.

Abaixo de 600px a navegação do cabeçalho desce para uma segunda linha sem quebra, e o selo de situação empilha os itens. Linhas de terminal e blocos `pre` do terminal quebram linha abaixo de 860px, para o comando de instalação caber no celular sem rolagem lateral.

## Elevation & Depth

O sistema é plano. A profundidade vem de três tons de fundo (base, elevado e rebaixado), de linhas de 1px e do contraste entre a página e o terminal Gruvbox, que funciona como o plano mais fundo. Não há sombra de elevação em cartão, janela, botão nem menu.

### Shadow Vocabulary
- **Halo do selo** (`box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent-fill) 22%, transparent)`): só o ponto de 8px do selo de situação. É um anel de cor, não uma sombra.
- **Marca do item atual** (`box-shadow: inset 2px 0 0 var(--accent-fill)`): a borda de areia do item atual no menu lateral dos docs.

### Named Rules
**The Flat Tones Rule.** Separe superfícies por tom e por linha de 1px, nunca por sombra projetada. Se algo precisa parecer acima, use o fundo elevado e a linha forte.

## Shapes

Cantos suaves e proporcionais ao tamanho do objeto: 12px nas peças grandes (janela de terminal, pilha de camadas), 10px nos blocos dos docs (código, citação, imagem, gaveta do menu), 8px nos controles (botões, links da navegação, seletor de idioma, linha de instalação, link de pular), 6px nos links do menu lateral, 5px no código em linha e 4px no anel de foco. Círculos só nas bolinhas da barra da janela e no ponto do selo.

As bordas são sempre de 1px: linha forte em volta de janelas, pilhas, botões e seletor de idioma; linha comum entre itens, linhas de tabela, cabeçalho e rodapé. Peças com cantos de 12px cortam o conteúdo com `overflow: hidden`, para a tela gravada e as divisórias seguirem o canto.

Os marcadores das listas da seção de segurança são desenhados em CSS: um tique de areia (borda de 2px girada) para o que o sandbox impede e um traço de 12x2px em ferrugem para o que ele não promete.

## Components

### Buttons
Diretos e sem efeito: a cor faz o trabalho.
- **Shape:** cantos de 8px, altura mínima de 44px, 0 20px de padding, peso 500.
- **Primary:** fundo areia (#e8b04b) com texto em tinta sobre areia, nos dois temas. Há um só por tela, ao lado do comando de instalação ("Instalar e começar").
- **Hover / Focus:** o fundo e a borda vão para areia acesa em 160ms com `cubic-bezier(0.16, 1, 0.3, 1)`; o foco é o anel global de 2px em areia, a 3px de distância.
- **Secondary:** fundo elevado, borda de linha forte e texto principal; no hover a borda escurece para a cor apagada. Existe no CSS, mas a página atual não usa.

### Janela de terminal
A peça que define o site.
- **Moldura:** borda de 1px em linha forte, cantos de 12px, fundo Gruvbox.
- **Barra:** 34px, fundo #1d2021, três bolinhas de 10px e o nome do programa em Fira Code 500 de 12px. Nas telas da galeria a barra é omitida.
- **Conteúdo:** a gravação do VHS (PNG ou WebM em loop), sempre como link para o PNG em tamanho real; ou um `pre` com comandos, prompt `$ ` em amarelo e comentários em cinza.
- **Legenda:** faixa de fundo elevado abaixo da tela, separada por linha de 1px, em 0.92rem.

### Linha de instalação
O comando de instalação inteiro numa caixa de terminal de uma linha: fundo Gruvbox, cantos de 8px, borda de linha forte, Fira Code em 0.9rem, rolagem lateral fina no desktop e quebra de linha no celular. O prompt `$ ` não é selecionável.

### Pilha de camadas
- **Moldura:** lista numerada com borda de 1px em linha forte, cantos de 12px e fundo elevado.
- **Itens:** 16px 20px, nome em 700 numa coluna de 12rem e descrição em 0.95rem; divisória de 1px entre itens.
- **Camada do agente:** o primeiro item é um pedaço de terminal: fundo Gruvbox, nome em Fira Code amarelo, descrição em #d5c4a1. Ele mostra que o agente está do lado de dentro.
- **Variante de rede:** uma coluna só, nome acima da descrição. No celular todas as pilhas ficam assim.

### Selo de situação
Uma linha em 0.9rem na cor apagada, logo abaixo do resumo: ponto de areia de 8px com halo, "Em desenvolvimento, antes da v1" e "Linux x86_64 · MIT", separados por divisórias verticais de 1px. Abaixo de 600px cada item vai para a própria linha. É a única informação de estado na primeira dobra.

### Tabela do que o agente enxerga
Sem moldura: linhas de 1px no topo de cada linha, cabeçalho apagado em 0.85rem, recurso em 700 numa coluna de 34% e a descrição em texto secundário.

### Navigation
- **Cabeçalho:** sticky, 64px, fundo da página e linha de 1px embaixo. Logo e nome em 700 à esquerda; links em 0.95rem e texto secundário à direita, com 6px 10px e cantos de 8px. Hover e página atual ganham o fundo rebaixado. O seletor de idioma tem borda de linha forte e leva ao par da página no outro idioma.
- **Menu lateral dos docs:** grupos com rótulo em maiúsculas apagado; links em 0.92rem com cantos de 6px; hover em fundo rebaixado; o item atual ganha fundo rebaixado, peso 500 e uma borda interna de 2px em areia à esquerda.
- **Índice da página:** à direita, links apagados sem fundo, um nível de recuo de 12px para subtítulos, e o hover só troca a cor do texto.
- **Celular:** o menu lateral vira uma gaveta `<details>` com borda de 1px, cantos de 10px e fundo elevado, acima do texto; o índice não aparece.

### Código nos docs
- **Em linha:** Fira Code em 0.86em, fundo rebaixado, borda de 1px, cantos de 5px, quebra em qualquer ponto.
- **Bloco:** fundo de código, borda de 1px, cantos de 10px, 16px 18px, 0.86rem e altura de linha 1.6, com rolagem lateral.

### Logo
O prompt dentro da caixa: um quadrado de cantos arredondados com traço de areia, uma janela com o sinal `>` em cor de texto e um cursor de areia. Inline no cabeçalho (28px) e no rodapé (22px), herdando as cores do tema; o `favicon.svg` repete o desenho com cores próprias e troca pelo `prefers-color-scheme`.

### Motion
A única animação é a gravação WebM da TUI em loop (`autoplay loop muted playsinline`), com o PNG como poster. Com `prefers-reduced-motion: reduce`, o CSS esconde o vídeo e mostra o PNG parado. Fora isso, só a transição de 160ms de cor do botão.

## Do's and Don'ts

### Do:
- **Do** usar areia (#e8b04b) como o único destaque: links no escuro, fundo do botão principal, seleção, foco e marca do item atual.
- **Do** usar a areia escura (#8f6210) para texto de destaque sobre o papel (#faf9f6).
- **Do** mostrar o produto em janelas de terminal Gruvbox (#282828) com cantos de 12px e borda de 1px, iguais nos dois temas.
- **Do** separar superfícies com tom e linhas de 1px; agrupar itens relacionados numa moldura única com divisórias, como a pilha de camadas.
- **Do** escrever código e comandos em Fira Code com as ligaduras desligadas.
- **Do** trocar toda animação por imagem parada com `prefers-reduced-motion`.
- **Do** servir fontes, imagens e CSS do próprio site e manter tudo funcionando sem JavaScript.

### Don't:
- **Don't** usar JavaScript nem carregar recurso de terceiros (CDN, web font remota, analytics, embed).
- **Don't** pôr gradiente em título.
- **Don't** pôr eyebrow (rótulo pequeno em maiúsculas) acima de título; rótulo em maiúsculas só nomeia grupo de navegação.
- **Don't** montar fileira de cartões iguais; use tabela, lista ou uma moldura com divisórias.
- **Don't** usar neon nem brilho no escuro, nem sombra projetada para dar profundidade.
- **Don't** criar um segundo destaque fora do terminal; ferrugem e coral marcam só limites.
- **Don't** usar Gruvbox fora das janelas de terminal, nem clarear o terminal no tema claro.
