# Design

## Context

Hoje o único jeito de ver processos de uma aba é a contagem em "Recursos" (`pids.current`/`pids.max` do cgroup, lido em `sandmux-policy/src/limits.rs`) e, indiretamente, uma linha por execução no registro de arquivos (`file-access-log`), sem relação de pai/filho. Ver `proposal.md` para o porquê.

As execuções de uma aba chegam ao daemon por um caminho só: `daemon/execlog.rs` pede ao auxiliar root o `observe` do escopo da aba e lê os eventos (`{"ev": [...]}`) numa thread (`run`). O confinamento da aba pelo kernel (change 78, `tabfiles.rs`) também recebe as execuções, mas as descarta para não dobrar as linhas. Sem o auxiliar, o registro de execuções vem do supervisor do lançador, que grava direto no arquivo do registro (`Logger::record_exec` em `sandmux-launcher/src/files.rs`), sem nenhum canal ao vivo até o daemon.

O formato do evento do auxiliar (`crates/sandmux-ebpf/src/ring.rs`) é um cabeçalho fixo de 32 bytes (`EV_HEADER`) com `kind`, `hook`, `bits`, o modo (`read`/`write`, change 82), `ino` (u64) e `dev`, seguido do nome e, para `EV_EXEC`, dos argumentos. O pid de uma execução já viaja no campo `ino`. `EV_OPEN` (change 78) ocupa o `kind = 5`.

## Goals / Non-Goals

**Goals:**
- Montar e manter, por aba, uma árvore de processos ao vivo a partir das execuções e saídas vistas pelo kernel, com as mesmas garantias de "só observa" do registro de execuções.
- Mostrar essa árvore numa seção nova da página de configurações, com zoom de densidade, dobra e drill-down, sem travar nem arriscar o host.

**Non-Goals:**
- Árvore sem o auxiliar root: a seção diz por que não há árvore (decisão do usuário, 2026-11-21), em vez de um canal novo do supervisor até o daemon.
- Árvore de outras abas, do daemon inteiro ou do Docker privado (que roda noutro cgroup).
- Agir sobre um processo pela tela (matar, mudar prioridade): só observação.
- Tornar o limiar de dobra automática ou as colunas de cada nível de zoom configuráveis pelo `config.toml` nesta mudança.
- Histórico da árvore: o que já terminou sai dela.

## Decisions

### `ppid` e saída de processo sem crescer o cabeçalho do evento
O campo `bits` (offset 8) não tem uso em `EV_EXEC` e passa a levar o `ppid`. O campo do modo (offset 12) não serve, porque o change 82 o ocupou. Um evento `EV_EXIT` novo (`kind = 6`, porque o 5 é do `EV_OPEN`) usa só o cabeçalho, com o pid no lugar de `ino`, como em `EV_EXEC`.

**Alternativa descartada:** crescer `EV_HEADER` com um campo dedicado. Mexe em todo decodificador (`EV_DENY`, `EV_CREATE`, `EV_OPEN`, `EV_CONN`) para um dado que só dois tipos usam.

### Leitura do `ppid` e do pid de saída por campo BTF
`ppid` = `task->real_parent->tgid`; pid de saída = `task->tgid`, do `task_struct` que o tracepoint recebe. É o mesmo padrão de leitura direta de campo BTF que `task->mm` já usa em produção desde o change 72. O incidente do change 74 veio de outro tipo de leitura: um argumento de gancho LSM (`cred`) que o verificador trata como nunca nulo. `real_parent` nunca é nulo para um processo que executa ou termina.

### Saída só do líder do grupo de threads
O tracepoint `sched_process_exit` dispara para cada thread que termina. O programa só manda o evento quando `task->pid == task->tgid` (o líder): sem isso, o fim de uma thread apagaria da árvore um processo ainda vivo. O caso raro de um líder que termina antes das outras threads (`pthread_exit` no principal) sai da árvore antes da hora; aceito.

### Saída só no `observe`
O programa de saída é preso só pelo `observe` (`Observation::load`), que é por onde as execuções de uma aba chegam ao daemon. O `confine` (Docker e aba pelo kernel) continua só com o programa de execução: a árvore não usa os eventos dele, e assim o caminho do confinamento, onde estão os programas LSM, não muda.

### Estado da árvore no daemon, alimentado pelo `execlog.rs`
Um mapa `pid -> nó` por sessão (`Session.proc_tree`, ao lado de `exec_obs`), atualizado no laço de `execlog::run` junto das linhas do registro e das conexões, e zerado quando `exec_obs` é zerado. Na saída de um processo, os filhos dele passam a apontar para o pai dele (o ascendente vivo mais próximo), ou viram raiz. Ao ligar a observação, o daemon lê uma vez o `cgroup.procs` do escopo (e das subpastas) e os `/proc/<pid>/stat`, para a árvore não começar vazia quando a observação liga com a aba já rodando (por exemplo, depois de um reinício do daemon). Os eventos que chegam durante essa leitura ficam na fila e são aplicados depois, sem perda.

### Conferência no pedido, contra eventos perdidos e pid reusado
Cada nó guarda o `starttime` lido em `/proc/<pid>/stat` ao entrar. No pedido da árvore, o daemon relê `/proc/<pid>/stat` de cada nó: um pid que sumiu ou mudou de `starttime` sai da árvore. Isso cobre um `EV_EXIT` perdido (fila cheia) e um pid reaproveitado por outro processo. Na mesma leitura saem a memória residente, o CPU e o início de cada nó. O mapa tem um teto de 8192 nós.

### Mensagem pedida a cada segundo, como `Stats`
`Msg::ProcessTree { id }` e a resposta `Msg::ProcessTreeOf { .. }`, no mesmo molde de `Msg::Stats`/`StatsOf`, que o painel de "Recursos" pede uma vez por segundo. A TUI só pede enquanto a seção "Processos" está aberta, então o daemon só monta a resposta quando alguém olha. Sem a observação pelo kernel, a resposta vem sem nós e com o motivo (o mesmo `supervisor: <por quê>` da proteção do lançamento). A versão do protocolo vai a 45.

**Alternativa descartada:** pedir e cancelar uma assinatura com atualizações empurradas pelo daemon. Exige estado por cliente e limpeza na queda da conexão, para o mesmo efeito que o pedido periódico já tem no projeto.

### Limiar de dobra automática e colunas de cada zoom
Valores fixos para a primeira versão:
- dobra automática: um nó com mais de **12** descendentes nasce dobrado;
- zoom 1: nome + pid;
- zoom 2: zoom 1 + memória residente;
- zoom 3: cartão com a linha de comando, o CPU usado, a memória, o início (relativo, ex. "há 4min") e o cgroup.

Os pids são os do host (os do kernel e do `/proc` do daemon), não os de dentro do sandbox.

## Risks / Trade-offs

- **[Risco]** Programas BPF novos ou alterados já travaram a máquina do usuário duas vezes (change 74) → **Mitigação:** os programas mudados aqui são `tp_btf` de observação, presos ao cgroup pedido, sem poder de negar nada. Os testes que carregam BPF rodam só na VM (`scripts/vm-ubuntu-2404.sh check-root`), com a autorização do usuário antes; o `scripts/check-root.sh` no host já pede confirmação.
- **[Risco]** Processo que faz `fork` sem `exec` (um subshell, workers de Python por `fork`) não gera evento de execução e não aparece, a não ser que já existisse quando a observação ligou → aceito na primeira versão; o tracepoint `sched_process_fork` resolveria e fica para depois.
- **[Risco]** Processo órfão (pai monitorado já saiu) se perderia da árvore → **Mitigação:** os filhos herdam o pai do processo que saiu, com teste.
- **[Trade-off]** Sem o auxiliar root, não há árvore → aceito pelo usuário (2026-11-21); a seção mostra o motivo.

## Migration Plan

1. Estender `sandmux-ebpf` (ppid, `EV_EXIT` do líder) e o `event_json` do auxiliar, com testes que só montam o programa e decodificam eventos.
2. Árvore no daemon e mensagem de protocolo (versão 45), com testes de unidade.
3. Seção "Processos" na TUI, consumindo a mensagem.
4. Com a autorização do usuário, os testes com root na VM (`scripts/vm-ubuntu-2404.sh check-root`), com um teste novo que confere `ppid` e saída. Nada de BPF no host.

Sem dado persistido e sem mudança de formato em disco: reverter é não prender o programa de saída e remover a seção da página. Um auxiliar antigo manda execuções sem `ppid` e sem saídas; o daemon trata o `ppid` ausente como 0 (raiz) e confia na conferência do pedido para tirar quem terminou.
