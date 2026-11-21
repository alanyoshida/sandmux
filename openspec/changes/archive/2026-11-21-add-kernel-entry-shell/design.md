## Context

O motivo está em proposal.md, em "Why". O que importa para o desenho:

- O auxiliar de entrada (`crates/sandmux-launcher/src/enter.rs`) nasce do lançador antes das threads dele, com os namespaces do processo 1 abertos. A cada pedido, ele cria um processo que entra nos namespaces do sandbox, aplica o Landlock e o seccomp do comando, instala o filtro de aberturas e manda o descritor do filtro ao lançador. Uma thread do lançador cria um supervisor para cada shell.
- O modo de arquivos da aba é decidido no lançador, na thread `sandmux-files`: o processo 1 pergunta (`MODE_ASK`), o lançador pede ao daemon (`confine_tab`, até 10 s) e responde `MODE_KERNEL` ou `MODE_SUPERVISOR`. No modo kernel, o comando instala só o filtro das execuções, quando o registro delas é pelo supervisor (`(!kernel || exec).then(|| install_notify(!kernel, exec))`).
- O daemon prende os programas BPF ao escopo do systemd da aba, que é o cgroup do lançador (`own_scope`). O auxiliar de entrada é filho do lançador, e o `setns` não muda o cgroup: o shell já está no escopo e já passa pelas marcas e pela fila do kernel. Hoje ele passa pelo supervisor também.
- O kernel só manda os eventos das montagens registradas, que são as da visão do processo 1. O shell entra no mesmo mount namespace e vê as mesmas montagens.

## Goals / Non-Goals

**Goals:**

- O shell de entrada de uma aba pelo kernel tem os mesmos filtros seccomp que o comando, e as aberturas dele vão só pelo kernel.
- Nada muda numa aba pelo supervisor, nem no `sandmux run` sem sessão, que não tem shell de entrada.

**Non-Goals:**

- Separar no registro as aberturas do shell das do agente. Hoje elas também saem juntas, com a etiqueta da aba.
- Mudar o daemon, o auxiliar root ou o protocolo.

## Decisions

### 1. O modo vai com cada pedido de entrada

A thread dos arquivos guarda o modo decidido num valor compartilhado: desconhecido, kernel ou supervisor. A thread de entrada lê esse valor e manda o modo ao auxiliar de entrada junto com os dois descritores do pedido. O auxiliar passa o modo ao processo do shell.

- **Alternativa:** mandar o modo uma vez ao auxiliar de entrada, depois da decisão. O auxiliar teria de esperar por uma segunda mensagem, e um pedido que chegasse antes ficaria preso. Por pedido, o auxiliar segue sem estado.
- **Alternativa:** o shell descobrir sozinho, pelo cgroup ou pelo `/proc`. Dentro do sandbox, ele não sabe se os programas do daemon estão presos ao escopo, e um erro aí tiraria o supervisor sem o kernel no lugar.

### 2. Um pedido antes do modo espera

O socket de entrada existe desde a subida do lançador, e o modo só sai depois da resposta do daemon. Um pedido que chega antes espera o modo até 15 s (os 10 s do daemon e uma folga). Sem o modo nesse prazo, o shell fica com o supervisor.

O supervisor é seguro em qualquer caso: as marcas do kernel valem para o shell, porque ele está no escopo, e o supervisor aplica a política por cima. Errar para o supervisor só custa desempenho.

### 3. Os filtros do shell pela mesma regra do comando

Uma função só decide os filtros pelo modo e pelo registro das execuções, e o comando e o shell a usam. No modo kernel sem o registro das execuções pelo supervisor, o shell não instala filtro de notificação nem manda descritor ao lançador. A thread que espera os filtros dos shells não muda: ela só não recebe nada desse shell.

### 4. Nada no daemon nem no auxiliar root

O shell já está no escopo da aba. As regras, a fila, o congelamento da fila cheia (change 80) e a recuperação de uma queda do auxiliar (change 79) já valem para ele.

Prender o shell a outro cgroup, ou pedir um confinamento próprio ao daemon, daria um segundo conjunto de programas para o mesmo sandbox, sem ganho.

## Risks / Trade-offs

- **Uma aba lançada por uma versão anterior:** o lançador dela é o binário antigo, e o shell continua com o supervisor até a aba ser reiniciada. → Não muda a segurança, só o desempenho. A documentação diz isso.
- **O shell de uma aba que voltou ao supervisor no meio da sessão:** o modo é decidido uma vez por lançamento e não volta ao supervisor depois (uma queda do auxiliar congela a aba e refaz o confinamento, ou a aba morre). → O shell segue o modo do lançamento, como o comando.
- **Uma aba pelo kernel sem o registro das execuções pelo supervisor:** o shell fica sem nenhum filtro de notificação. → É o que o comando já tem nesse caso, com as execuções registradas pelo kernel.

## Migration Plan

Basta atualizar o sandmux e reiniciar a aba. O auxiliar root não muda, e não é preciso rodar `sandmux setup`. Para desfazer, basta voltar a versão anterior.
