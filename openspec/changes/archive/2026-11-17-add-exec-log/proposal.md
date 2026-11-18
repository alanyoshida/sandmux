## Why

O registro de arquivos cobre as aberturas nas pastas do agente, mas não diz o que o agente executa. Um `curl` de `/usr/bin` lançado por uma ferramenta, um script baixado e rodado, um compilador: nada disso deixa linha. Foi a primeira das "Outras lacunas do backend eBPF" do roadmap (seção 5). O change 71 deixou pronta a base para fechá-la: programas presos a um cgroup, uma fila de eventos e o auxiliar root que repassa os eventos ao daemon.

Pedido do usuário (2026-11-17): o registro de execuções agora; depois o processo de cada conexão (change 73) e o Ubuntu 24.04 (change 74), nessa ordem.

Decisões do usuário (2026-11-17), ao propor:

- os dois mecanismos: o programa eBPF carregado pelo auxiliar, que lê o que o kernel de fato executou, e o supervisor do sandbox como recuo nas máquinas sem ele, com a linha marcando a fonte;
- as execuções vão para o registro de arquivos, com o acesso `exec`, sem registro nem comando novos;
- o alcance são as abas (o agente e os shells) e o Docker privado da pasta.

## What Changes

- **Programa de execução:** um programa `tp_btf` no tracepoint `sched_process_exec`, carregado pelo auxiliar e filtrado pelo cgroup pedido. Ele manda pela fila de eventos:
  - o caminho que o kernel executou;
  - o pid;
  - os argumentos (até 1024 bytes, lidos da memória do processo novo, já com o executável carregado).
- **Auxiliar:** um pedido novo, `observe`. Ele carrega só o programa de execução num cgroup do usuário, sem as regras de arquivos, para as abas do agente.
  - O confinamento do Docker (`confine`) ganha o mesmo programa.
  - Os eventos vão pela conexão, como no change 71.
- **Recuo pelo supervisor:** o filtro seccomp das abas passa a mandar também `execve` e `execveat` ao supervisor de aberturas.
  - Ele lê o caminho e os argumentos na memória do processo, grava a linha e deixa o kernel seguir (`SECCOMP_USER_NOTIF_FLAG_CONTINUE`).
  - Ele registra desde o início da aba e para quando o daemon avisa, pela política ao vivo, que o auxiliar assumiu.
  - Essa fonte pode ser enganada: outra thread pode trocar o caminho ou os argumentos entre a leitura e a execução. A linha diz a fonte.
- **Daemon:**
  - quando acha o escopo de uma aba, pede o `observe` e grava cada execução no registro de arquivos, com a etiqueta do lançamento;
  - as execuções do Docker vão com a etiqueta `docker-<id>`.
- **Registro:** linhas com o acesso `exec`, o caminho do executável e dois campos novos: `args` e `via` (`kernel` ou `supervisor`). `sandmux files <id>` e a tela de arquivos da TUI mostram os argumentos.
- **Estado:** a proteção do lançamento, no `sandmux stats` e no painel, ganha a linha "execuções: pelo kernel" ou "pelo supervisor: <motivo>". A versão do protocolo muda.

Limites, escritos na proposta:

- Numa máquina sem o auxiliar, o registro vem só do supervisor, que pode ser enganado.
- No Docker, sem o auxiliar, não há registro de execuções, porque os contêineres não passam pelo supervisor.
- Na passagem do supervisor para o kernel, uma execução pode sair nas duas fontes.
- Só observa: nenhuma execução é negada.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `file-access-log`: o registro ganha as execuções das abas e do Docker privado, e o supervisor passa a ver as execuções.
- `root-helper`: os pedidos aceitos ganham o `observe`.

## Impact

- `crates/sandmux-ebpf`: o programa `tp_btf` de `sched_process_exec`, com `bpf_ringbuf_reserve` e `bpf_ringbuf_submit`, as leituras `bpf_probe_read_kernel_str` e `bpf_probe_read_user`, e o filtro por `bpf_get_current_ancestor_cgroup_id`. Também o anexo `BPF_TRACE_RAW_TP` e o evento `EV_EXEC`.
- `crates/sandmux/src/helper.rs`: o pedido `observe` e o programa de execução no `confine`.
- `crates/sandmux-launcher` (`seccomp.rs` e `files.rs`) e `crates/sandmux-policy`: `execve` e `execveat` no filtro de notificação, a leitura dos argumentos e a chave `exec_log` da política ao vivo.
- `crates/sandmux/src/daemon/session.rs` e `dockerfiles.rs`: o pedido ao achar o escopo, as linhas do registro e o motivo quando não há registro.
- `crates/sandmux-spec` (`FileEntry.args`) e `crates/sandmux-proto` (`PolicySummary`): a versão do protocolo muda.
- `crates/sandmux/src/filescmd.rs` e a tela de arquivos da TUI.
- `docs/files.md`, `docs/helper.md` e os pares em inglês.
