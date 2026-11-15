## Context

A TUI marca uma aba como encerrada por dois caminhos:

- pelo evento `Exited` da conexão anexada;
- pela lista de sessões (`sync`, a cada segundo), para abas sem conexão.

A aba encerrada já tem `restart` (Ctrl+b r) e o fechamento (`close_tab`, sem confirmação desde o change 28).

## Goals / Non-Goals

**Goals:**
- Perguntar uma vez, no momento em que o usuário vê a aba terminar.

**Non-Goals:**
- Reiniciar sem perguntar.
- Perguntar sobre abas paradas: a parada tem um motivo que o usuário precisa ler.

## Decisions

### A marca `ask_restart` na aba

`Tab.ask_restart` liga quando a TUI vê a passagem de viva para encerrada:

- no `Exited`;
- no `sync`, quando o estado anterior era viva.

Uma aba criada pelo `sync` (já encerrada ao abrir a TUI) começa sem a marca.

Depois de cada evento, se a aba ativa tem a marca e não há modal aberto, a TUI:
1. abre o `Modal::Ended { id, code }`;
2. desliga a marca.

A pergunta não interrompe outro modal: ela espera ele fechar.

### Teclas

- **Enter ou `r`:** chama o `restart` da aba.
- **`x`:** chama o `close_tab`.
- **Esc:** fecha o modal.

O modal só age se a aba ainda é a do `id` e continua encerrada.

## Risks / Trade-offs

- **[Um harness que sai de propósito pergunta toda vez]** Esc custa uma tecla, e a aba fica como era.
