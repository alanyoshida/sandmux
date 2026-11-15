## Context

A barra é desenhada em `tui/render.rs::draw`, na primeira linha, com o texto ` N:nome marcas ` por aba e o aviso à direita. O mouse trata a linha 1 como barra e desloca a linha da aba por 1. O tema (`[tui] theme` e `[tui.colors]`) tem elementos nomeados com cores. O usuário escolheu o estilo novo como padrão e os quatro blocos da direita.

## Goals / Non-Goals

**Goals:**
- O visual do tmux do usuário: barra embaixo, blocos com separadores inclinados, cores, informações à direita.
- Um recuo sem fonte especial (`style = "simples"`) e a barra no topo (`bar = "top"`).

**Non-Goals:**
- Detectar se o terminal tem a fonte. Não há como perguntar isso ao terminal.
- Mudar o painel lateral.

## Decisions

### Desenho de uma aba (Powerline)

Cada aba é desenhada como `` + ` N ` (o bloco do número) + `` + ` nome marcas ` + um espaço:

- os separadores são U+E0BA e U+E0BC, as diagonais do Powerline estendido;
- o bloco do número usa `tab-index-*`, ou `tab-active-index-*` na aba ativa;
- o nome da aba ativa vem em negrito, com `tab-active-fg`.

O ✕ de uma aba encerrada continua clicável, pela coluna dele.

### Blocos da direita

Cada bloco é `` + ` ícone texto `, com a cor de `chip-<nome>-bg` e o texto em `chip-fg`. Os ícones são do Nerd Font.

| Bloco | Ícone | Conteúdo |
|---|---|---|
| Harness | `` | o harness e a pasta da aba ativa, com `~` no lugar do HOME |
| Cofre | `` ou `` | "cofre aberto", "cofre trancado" ou "sem cofre", pelo `VaultStatus` a cada segundo |
| Alertas | `` | `! N` (abas com bloqueio) e `• N` (abas com domínio novo), só quando há |
| Tempo | `` | `Nd Nh`, `Nh Nm` ou `Nm`, pelo `started_at` da sessão |

Quando a barra não cabe, os blocos da direita saem da esquerda para a direita, e o tempo fica por último. Os avisos da TUI ficam entre as abas e os blocos.

### Cores

Os elementos novos do tema são `tab-index-fg`, `tab-index-bg`, `tab-active-index-fg`, `tab-active-index-bg`, `chip-fg`, `chip-harness-bg`, `chip-vault-bg`, `chip-alert-bg` e `chip-time-bg`. O tema `padrao` passa a ter cores na barra Powerline, parecidas com as do tmux do usuário:

- fundo `#1e1e2e`;
- número cinza `#45475a`;
- aba ativa lilás `#cba6f7`;
- blocos rosa `#f38ba8`, verde `#a6e3a1`, amarelo `#f9e2af` e ciano `#89dceb`.

No estilo simples, a barra continua sem cores no tema `padrao`.

### Posição e mouse

O `App` guarda a linha da barra e o deslocamento da área (`bar_row` e `area_dy`), calculados no desenho:

- **barra embaixo:** a última linha é a barra, e a área começa na linha 1, com deslocamento 0;
- **barra em cima:** como antes.

O mouse e a recodificação para o harness (`modes::encode_for`) usam esses dois valores.

### Protocolo 23

`SessionInfo.started_at: Option<u64>` (`serde default`) traz os segundos Unix do começo do lançamento atual. O daemon o grava ao lançar e o mantém na lista.

## Risks / Trade-offs

- **[Terminal sem a fonte]** Os separadores e os ícones aparecem como quadrados. A documentação e o README dizem como trocar para `style = "simples"`.
- **[Testes que liam ` 1:shell:app `]** Passam a usar um ajudante com o texto da aba no estilo novo.
