## Context

- **O daemon** filtra a saída de cada sessão (`OscFilter`): o OSC 52 de gravação só passa com `clipboard` liberado, e o de leitura nunca passa.
- **No anexo cru** (`sandmux run` sem TUI), a sequência liberada chega ao terminal do usuário.
- **Na TUI**, a saída de cada aba vai para um `vt100::Parser`, que interpreta o OSC 52 e o descarta: o `vt100` chama o callback `copy_to_clipboard`, que não faz nada por padrão.
- **A cópia da seleção da TUI** escreve OSC 52 no terminal do usuário.
- **O `arboard`** já é dependência (colar imagem, Ctrl+b v), com o suporte a Wayland por data-control.

## Goals / Non-Goals

**Goals:**
- A cópia liberada do harness chega ao usuário na TUI.
- A cópia da TUI e a do harness vão também ao clipboard do sistema.

**Non-Goals:**
- Mudar o bloqueio padrão ou avisar quando ele age. O usuário escolheu só a gravação no clipboard do sistema.
- O clipboard do sistema no anexo cru. Lá a sequência já chega ao terminal, e o cliente cru não interpreta a saída.
- Ler o clipboard pelo harness. A leitura continua sempre bloqueada.

## Decisions

### O emulador da aba com callbacks

`Tab.parser` passa a ser `vt100::Parser<Clip>`. O `Clip` guarda os pedidos de `copy_to_clipboard` (o tipo e os dados em base64). Depois de cada `process`, a TUI esvazia a fila e copia cada texto.

Só chegam aqui os pedidos que o daemon deixou passar, então a TUI não refaz a política.

O `Clip` ignora pedidos maiores que 1 MiB (base64) e os que não decodificam.

### `copy_text`

Um ponto só para copiar, que faz duas coisas:

1. escreve o OSC 52 no terminal do usuário, como hoje;
2. grava o texto com `arboard::Clipboard::set_text`.

O `Clipboard` fica guardado no `App` durante a vida da TUI. No X11, o dono da seleção é o processo, e o conteúdo some se o objeto for destruído. A criação é preguiçosa e, se falhar (sem Wayland nem X11, como numa sessão SSH), a gravação no sistema é pulada em silêncio: o OSC 52 continua.

A seleção da TUI e a cópia do harness passam por ele.

## Risks / Trade-offs

- **[Com o clipboard liberado, o harness grava no clipboard do sistema]** É o mesmo efeito do OSC 52 num terminal que o aceita. Por isso continua desligado por padrão.
- **[Wayland sem data-control (o GNOME)]** Com `DISPLAY` definido, o `arboard` recua para o X11 do XWayland, que o compositor sincroniza. Sem os dois, vale só o OSC 52.
