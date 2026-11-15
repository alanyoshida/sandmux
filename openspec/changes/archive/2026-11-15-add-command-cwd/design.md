## Context

A sessão no daemon guarda o lado mestre do PTY (`Live.master`). O shell da aba roda no sandbox, com mount, PID e user namespaces próprios, e a pasta de trabalho montada no mesmo caminho. Um teste nesta máquina confirmou que o `readlink /proc/<pid>/cwd` lido de fora, pelo mesmo usuário, de um bash do sandbox que deu `cd src/api`, devolve o caminho completo da pasta.

## Goals / Non-Goals

**Goals:**
- O modelo recebe a pasta em que o usuário está.
- Ver no modal qual pasta vai.

**Non-Goals:**
- Detectar o shell pelo processo em primeiro plano. O shell continua o de hoje.
- OSC 7 (o shell avisando a pasta). Nem todo shell manda, e o PTY basta.

## Decisions

- **No daemon,** que tem o PTY: `tcgetpgrp` no mestre dá o grupo de processos em primeiro plano, já no espaço de PIDs do daemon, e `/proc/<pgid>/cwd` dá a pasta. Sem sessão viva, ou com erro em qualquer passo, a resposta é `None`.
- **Uma pasta fora da pasta de trabalho** (um `cd /tmp` no sandbox) vai como o caminho que o processo vê. Ela só vai ao modelo como contexto e não dá acesso a nada.
- **A pasta é lida quando o modal abre,** guardada no estado do modal e mostrada na linha `pasta`. A pergunta usa essa pasta. Abrir o modal de novo lê de novo.
- **Protocolo 33:** `Cwd { id }` → `CwdOf { path: Option<PathBuf> }`.

## Risks / Trade-offs

- [Um programa em primeiro plano que não é o shell (o `vim`, o opencode)] → Vai a pasta desse programa, que em geral é a mesma do shell que o abriu.
