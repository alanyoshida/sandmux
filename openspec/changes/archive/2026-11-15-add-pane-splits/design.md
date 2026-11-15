## Context

A TUI tem uma lista plana de `Tab`, uma por sessão, cada uma com a sua conexão e o seu parser. A aba ativa (`active`) é desenhada na área inteira e recebe a entrada. A barra, os números, o próximo e o anterior contam as `Tab`. O daemon guarda as sessões em `sessions.json` e as devolve em `Sessions`.

## Goals / Non-Goals

**Goals:**
- Painéis como os do tmux, com o layout guardado no daemon.
- Mudar pouco: todas as ações que valem para "a aba ativa" passam a valer para o painel em foco, sem tocar nelas.

**Non-Goals:**
- Levar um painel para outra aba, ou juntar abas.
- Redimensionar com o mouse.
- O mesmo layout com tamanhos diferentes em duas TUIs de tamanhos diferentes: cada sessão tem um tamanho só, o do último cliente que mudou.

## Decisions

- **O layout em `sandmux-proto`:** `Pane::Leaf { id }` ou `Pane::Split { dir: Side | Stack, ratio, a, b }`, com `ratio` em por cento da primeira parte (de 10 a 90). Funções puras:
  - `leaves`;
  - `split(alvo, dir, novo)`;
  - `remove(id)`, que colapsa a divisão;
  - `rects(área)`, que devolve os retângulos e os separadores (uma célula entre as partes);
  - `neighbor(id, direção)`, pela geometria;
  - `resize(id, delta)`, que ajusta a divisão mais próxima que contém a folha.

  Os testes unitários ficam ali.
- **A aba é o layout;** a identidade dela é a primeira folha. O daemon guarda `Vec<Pane>`:
  - `SetLayout { pane }` troca os layouts que têm alguma das folhas dele, depois de validar que todas as sessões existem e não se repetem. Um layout de uma folha só apaga os que a contêm.
  - Ao gravar a lista, as folhas de sessões que não existem mais saem, e os layouts de uma folha somem. Assim nenhum caminho de fechar sessão precisa saber de layouts.
  - Eles vão em `Sessions { sessions, layouts }`, com `#[serde(default)]`, e em `Saved.layouts`.
- **TUI, lista plana:** `tabs` continua uma por sessão, e `active` é o painel em foco.
  - **Barra:** mostra as "abas da barra", que são as `Tab` que não são folha de um layout ou que são a primeira folha dele. Os renderizadores, os números, o próximo e o anterior e o clique na barra usam essa lista.
  - **Troca de aba:** ao trocar de aba, o foco vai à primeira folha do layout dela.
  - **Grupos:** um layout só vale se todas as folhas estão nas abas do grupo atual; senão, as sessões aparecem soltas.
- **Desenho:**
  - a área da aba é dividida pelos `rects`, e cada sessão é desenhada no seu retângulo com o `draw_screen` que já existe;
  - os separadores usam `│` e `─` na cor `muted-fg`, e no destaque nas células junto ao painel em foco;
  - o cursor e a nota de estado (encerrada, parada) valem para o painel em foco;
  - ampliado (`zoom`), só o painel em foco aparece, na área inteira.
- **Tamanho:** `resize_all` calcula o tamanho de cada sessão: o do retângulo dela no layout (ou a área inteira, sem layout ou ampliada). O parser e o `Resize` da conexão seguem esse tamanho.
- **Mouse:**
  - um clique fora do painel em foco, num painel do layout, só muda o foco;
  - dentro do painel em foco, o evento é deslocado para a origem do painel, e o caminho de hoje (repassar ao harness, rolar ou selecionar) segue igual.
- **Dividir:** `split-side` (`%`) e `split-stack` (`"`) guardam a divisão pendente, com o painel e a direção, e abrem o modal de nova aba com a pasta do painel. Quando a sessão nova é criada, por `add_tab` (aba, shell de dentro ou chat), a TUI divide o layout, manda `SetLayout` e põe o foco nela. Esc no modal descarta a divisão pendente.
- **Teclas:** as ações novas entram em `TUI_ACTIONS` e na ajuda: `split-side` `%`, `split-stack` `"`, `zoom` `z`, `pane-grow` `>`, `pane-shrink` `<`. As setas depois do prefixo são fixas e viram `pane-left`, `pane-right`, `pane-up` e `pane-down`.
- **Protocolo 32.**

## Risks / Trade-offs

- [Uma sessão aparece num painel numa TUI e solta em outra, se o grupo diferir] → O layout só vale com todas as folhas no grupo; não quebra nada.
- [Duas TUIs de tamanhos diferentes] → O último `Resize` vence, como hoje com a aba inteira.
