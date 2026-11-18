## Context

Motivação em `proposal.md`. O que importa para o desenho:

- **A base do change 71:** cada confinamento do Docker já tem uma fila `RINGBUF`, um contador de descartados e a thread do auxiliar que manda os eventos pela conexão (`{"ev": [...]}`). O cliente separa os eventos das respostas (`take_events`), e o daemon grava `FileEntry` no registro do dia (`dockerfiles.rs`, `write_entry`).
- **O escopo de cada aba:** o `monitor` (`daemon/session.rs`) acha o cgroup `.scope` do lançamento, criado pelo `systemd-run --user --scope`, e o guarda em `s.cgroup`. O diretório é do usuário, o que o auxiliar exige.
- **O supervisor de aberturas** (`sandmux-launcher/src/files.rs`) recebe por `SECCOMP_RET_USER_NOTIF` as aberturas do agente:
  - lê o caminho em `/proc/<pid>/mem`;
  - relê a política ao vivo (o arquivo que o daemon regrava);
  - grava o registro de arquivos.
- **O kernel 7.0 tem o que é preciso:**
  - o tracepoint `sched_process_exec` com BTF (`btf_trace_sched_process_exec`: a tarefa, o pid antigo e o `linux_binprm`);
  - os auxiliares `bpf_ringbuf_reserve` e `bpf_ringbuf_submit`, `bpf_probe_read_user` e `bpf_get_current_ancestor_cgroup_id`;
  - o tipo `BPF_PROG_TYPE_TRACING` com o anexo `BPF_TRACE_RAW_TP`.

## Goals / Non-Goals

**Goals:**

- Cada execução das abas e do Docker no registro de arquivos, com o caminho, os argumentos e a fonte.
- O kernel como fonte sempre que o auxiliar existe, e o supervisor como recuo em toda máquina, sem buraco no começo da aba.
- Nenhum atraso perceptível numa execução.

**Non-Goals:**

- Negar execuções.
- O caminho absoluto quando o programa foi chamado por um caminho relativo (`./script`): o registro guarda o que foi passado ao `execve`.
- O ambiente (`envp`) da execução, que pode ter segredos.
- O registro de execuções do Docker sem o auxiliar.

## Decisions

### 1. Programa no tracepoint `sched_process_exec`

O programa é `BPF_PROG_TYPE_TRACING`, com o anexo `BPF_TRACE_RAW_TP`, e o `attach_btf_id` é o do `typedef btf_trace_sched_process_exec`. Ele é preso por `BPF_LINK_CREATE` (ou `BPF_RAW_TRACEPOINT_OPEN`, se o primeiro recusar).

- **Por que o tracepoint, e não o gancho LSM `bprm_check_security`:** no gancho, os argumentos ainda estão nas páginas da memória nova, que o programa não alcança. No tracepoint, o processo já está com a memória nova, e os argumentos ficam entre `mm->arg_start` e `mm->arg_end`, legíveis com `bpf_probe_read_user`.
- **Filtro pelo cgroup:** o tracepoint vale para a máquina inteira, então o programa começa comparando `bpf_get_current_ancestor_cgroup_id(nível)` com o id do cgroup pedido. Os dois são gravados como constantes na carga, como no recuo global do change 70. Fora do cgroup, o programa sai na terceira instrução.
- **O evento** (`EV_EXEC`, 1312 bytes) é reservado na própria fila (`bpf_ringbuf_reserve`), porque não cabe na pilha de 512 bytes. Ele traz:
  - o cabeçalho de 32 bytes do change 71, com o pid no lugar do inode, o tamanho dos argumentos no lugar do `dev` e o tamanho do caminho;
  - o caminho (`bprm->filename`, até 256 bytes, por `bpf_probe_read_kernel_str`);
  - os argumentos (até 1024 bytes, separados por NUL).

  Sem espaço na fila, o contador de descartados soma 1.

### 2. O pedido `observe` e o confinamento

O `observe` traz o descritor do cgroup, com a mesma conferência do dono. O auxiliar carrega a observação, que tem só o programa de execução, a fila e o contador, e manda os eventos pela conexão, com a thread do change 71.

- **O confinamento do Docker** (`confine`) carrega também o programa de execução, na mesma fila dos outros eventos, com o nível e o id da slice.
- **Limite:** até 64 observações por usuário, separadas dos 16 confinamentos, porque cada aba tem a sua.

### 3. O recuo pelo supervisor

O filtro seccomp das abas passa a mandar `execve` e `execveat` ao supervisor quando a especificação pede (`files.exec_log`).

1. **Leitura:** o supervisor lê o caminho e o vetor `argv` (os ponteiros e depois as strings, até 64 argumentos e 1024 bytes) em `/proc/<pid>/mem`.
2. **Registro:** se a política ao vivo tem `exec_log: true`, grava a linha com `via` = `supervisor`.
3. **Retorno:** responde com `SECCOMP_USER_NOTIF_FLAG_CONTINUE`, sem decidir nada pela leitura.

A política ao vivo (o arquivo da política de arquivos do lançamento) ganha a chave `exec_log`, com `true` por padrão.

- Quando o `observe` da aba dá certo, o daemon a regrava com `false`, e o supervisor para de registrar.
- Quando a conexão com o auxiliar cai e a aba continua viva, o daemon a volta para `true`.

**Por que não só o eBPF:** decisão do usuário. O supervisor cobre as máquinas sem o auxiliar e os primeiros milissegundos de toda aba, antes de o daemon achar o escopo.

**Risco aceito:** com o `CONTINUE`, outra thread pode trocar o caminho ou os argumentos entre a leitura e a execução. A linha diz a fonte.

### 4. As linhas do registro

O `FileEntry` ganha dois campos:

- `args: Vec<String>`, omitido quando vazio;
- `via: Option<String>`, com `kernel` ou `supervisor`.

As linhas `exec` são assim:

- `path`: o caminho executado;
- `access`: `exec`;
- `blocked`: `null`;
- `tag`: a etiqueta do lançamento (`s<id>-<n>`, a última da sessão quando o evento chega) ou `docker-<id>`.

Toda execução vira uma linha, sem tirar as repetidas: duas execuções do mesmo programa são duas ações.

- **Na linha de comando:** `sandmux files` mostra os argumentos depois do caminho, cortados em 80 caracteres na tabela.
- **Na TUI:** a tela de arquivos mostra o caminho e os argumentos na mesma coluna.

### 5. Estado e protocolo

O `PolicySummary` ganha `exec: Option<String>`:

- `kernel`;
- `supervisor: <motivo>`;
- ausente num lançamento sem supervisor.

O `sandmux stats` e o painel mostram a linha "execuções". A versão do protocolo sobe para 41.

## Risks / Trade-offs

- **[`bpf_probe_read_user` falha com as páginas dos argumentos fora da memória]** → o evento sai sem os argumentos (tamanho 0). É raro logo depois do `exec`, que acabou de escrever essas páginas.
- **[Muitas execuções num `docker build` ou num `cargo build`]** → uma linha por execução. A fila descarta e conta o que passar, e o registro é podado pela retenção que já existe (`net-log-days`, `net-log-max`).
- **[Uma execução nas duas fontes na passagem]** → aceito. A linha diz a fonte.
- **[O `BPF_LINK_CREATE` recusar o anexo `BPF_TRACE_RAW_TP`]** → o recuo para o `BPF_RAW_TRACEPOINT_OPEN` está na decisão 1, verificado na primeira tarefa com root.
- **[Custo no resto da máquina]** → o programa roda em toda execução da máquina, mas sai depois de uma chamada e uma comparação fora do cgroup. É medido numa tarefa.

## Migration Plan

- O auxiliar, o daemon e o lançador mudam juntos. Depois de atualizar: `sandmux setup` e `sandmux daemon restart`. As abas que já estavam abertas ganham o registro quando são lançadas de novo.
