## Context

O daemon guarda um mapa de sessões (abas), e cada cliente vê todas. A TUI tem a lista de abas (`tabs`) e anexa uma conexão por aba. A lista de sessões é gravada em `sessions.json` (`SavedSession`). Um `Create` leva as opções do `run` e o tamanho.

## Goals / Non-Goals

**Goals:**
- Separar as abas em conjuntos com nome, como as sessões do tmux, sem mudar o que já existe.
- Cada TUI no seu grupo.

**Non-Goals:**
- Grupos com configuração própria (perfil, rede). O grupo só organiza as abas.
- Grupos vazios guardados no daemon. Um grupo novo e vazio só existe na TUI que o criou, até a primeira aba.
- Notificações com o nome do grupo. Fica para depois, se fizer falta.

## Decisions

- **O grupo é da sessão, não das opções do `run`:** um campo `group` em `Session` e em `SavedSession`, com `#[serde(default)]` para `principal`, e `group` em `SessionInfo`. O `Create` ganha `group: Option<String>`, com `None` virando `principal`. Ele fica fora de `LaunchOptions` porque muda com o `move` e não é opção de lançamento: um `restart` mantém o grupo da sessão.
- **Grupos implícitos:** a lista de grupos é a dos grupos das sessões, mais o `principal`.
  - `SetGroup { id, group }` muda uma sessão.
  - `RenameGroup { from, to }` muda todas as do grupo. Renomear para um grupo que já existe junta os dois.
  - Os dois gravam a lista de sessões.
- **Validação do nome** em `sandmux-proto` (`check_group`), usada pelo CLI, pela TUI e pelo daemon, que é quem decide.
- **TUI:**
  - `App.group` guarda o grupo atual;
  - o `sync` só cria e mantém abas cujo `SessionInfo.group` é o atual;
  - uma aba que mudou de grupo, por outra TUI ou pelo CLI, sai da tela no próximo `sync`, e a conexão dela é fechada;
  - trocar de grupo fecha as conexões das abas que saem, troca o grupo, refaz as abas pelo `sync` e anexa a primeira.

  A ordem dos grupos para `(` e `)` é a alfabética, e um grupo novo e vazio entra nela enquanto é o atual.
- **Teclas:** as ações novas são `group-prev` (`(`), `group-next` (`)`) e `groups` (`G`). Elas entram em `TUI_ACTIONS` e na ajuda, e podem ser trocadas em `[tui.keys]` como as outras. tmux usa `(` e `)` para o mesmo, e `G` não tem dono.
- **Modal da lista de grupos:**
  - cada linha tem o nome e o número de abas, com o grupo atual marcado;
  - `n` e `r` pedem o nome num campo de texto do próprio modal;
  - `m` manda `SetGroup` para a aba ativa.
- **Barra:** um bloco com o nome do grupo logo depois do `≡`, nos dois estilos. Sem ele, quem tem um grupo só não veria diferença.
- **`sandmux ls`:** a coluna GRUPO depois do NOME, e `--group` filtra.
- **Protocolo 30.**

## Risks / Trade-offs

- [Uma aba anexada por `attach <id>` fora da TUI] → Não muda: o `attach` é pelo identificador e ignora grupos.
- [Uma TUI antiga num daemon novo] → O número do protocolo recusa a conexão, como sempre.
