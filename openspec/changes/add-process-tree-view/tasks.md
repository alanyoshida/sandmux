# Tasks

## 1. Evento de execução e saída no eBPF

- [x] 1.1 Em `crates/sandmux-ebpf/src/ring.rs`, gravar o `ppid` no campo `bits` do cabeçalho de `EV_EXEC` (sem uso nesse tipo de evento); verificado com o teste de `decode` que cobre um `EV_EXEC` com `ppid` diferente de `pid`.
- [x] 1.2 Acrescentar `EV_EXIT` em `ring.rs` (`kind = 6`; o 5 é do `EV_OPEN` do change 78), só com o cabeçalho e o pid no lugar de `ino`; verificado com o teste de `decode` do evento novo.
- [x] 1.3 Em `crates/sandmux-ebpf/src/exec.rs`, ler o `ppid` (`task->real_parent->tgid`, leitura direta de campo BTF, como `task->mm`) no programa de `sched_process_exec`, e acrescentar o programa de `sched_process_exit` filtrado pelo mesmo cgroup, preso pelo `Observation::load` quando o kernel tem o tracepoint; verificado com o teste `programa_de_execucao_monta` estendido, que lê o BTF real do host sem carregar nada no kernel.
- [x] 1.4 No programa de saída, mandar o evento só quando a tarefa é o líder do grupo (`task->pid == task->tgid`), para o fim de uma thread não apagar um processo vivo; verificar com o teste `programa_de_execucao_monta`, conferindo a comparação antes da chamada do filtro do cgroup.

## 2. Eventos no auxiliar root

- [x] 2.1 Em `crates/sandmux/src/helper.rs`, incluir o `ppid` no `event_json` de `EV_EXEC` e tratar `EV_EXIT` (`{"kind": "exit", "pid": ...}`); verificar com um teste que decodifica os dois tipos e confere o JSON.
- [x] 2.2 Em `crates/sandmux/tests/docker_ebpf.rs`, um teste com root novo (`ppid_e_saida_no_kernel`): um `sh -c` que lança um filho e termina, conferindo o `ppid` da execução do filho e os eventos `exit` dos dois. Compila; só roda na VM (grupo 6).

## 3. Árvore no daemon

- [x] 3.1 Módulo novo `crates/sandmux/src/daemon/proctree.rs`: o mapa `pid -> nó` (ppid, `starttime`), com os eventos `exec` e `exit` de uma mensagem (lidos fora do lock em `changes`, aplicados em `commit`), a herança do pai na saída (órfãos) e o teto de 8192 nós; verificado com testes de unidade de uma árvore de 3 níveis montada por eventos e da saída de um nó do meio.
- [x] 3.2 A semente: ler `cgroup.procs` do escopo e das subpastas e os `/proc/<pid>/stat` (ppid e `starttime`); verificado com um teste sobre um cgroup e um `/proc` falsos.
- [x] 3.3 A resposta (`render`, fora do lock): reler `/proc/<pid>/stat`, `cmdline` e `cgroup` de cada nó, tirar quem sumiu, virou zumbi ou mudou de `starttime`, e montar os nós com nome, linha de comando, memória residente, CPU usado, início e cgroup; verificado com testes de pid morto, pid reusado e o processo do próprio teste no `/proc` real.
- [x] 3.4 Em `session.rs`, o campo `proc_tree` ao lado de `exec_obs`, zerado no relançamento e na queda da observação; em `execlog.rs`, a semente depois de ligar a observação e as mudanças de cada mensagem no laço de `run`; verificado com `cargo test` e `cargo clippy -D warnings` do crate.

## 4. Pedido da árvore no protocolo

- [x] 4.1 Em `crates/sandmux-proto`, `Msg::ProcessTree { id }` e `Msg::ProcessTreeOf { id, nodes, unavailable }`, com `ProcNode`, e a versão do protocolo em 45 (com `docs/sessions.md` e o par em inglês); verificado com o teste de ida e volta em JSON.
- [x] 4.2 Em `crates/sandmux/src/daemon/conn.rs`, responder `Msg::ProcessTree`: a árvore conferida no `/proc` (fora do lock) quando há `exec_obs`, ou nenhum nó e o motivo (`policy.exec`) sem ela; verificado pelo teste da TUI `processos_na_pagina_sem_o_auxiliar` no host (o motivo); o caminho com a árvore fica para o teste com root `arvore_de_processos_na_tui`, na VM (6.2).

## 5. Seção "Processos" na página de configurações

- [x] 5.1 Em `crates/sandmux/src/tui/settings.rs`, "Processos" em `SECTIONS`, depois de "Diagnóstico", com o índice exportado (como `VAULT` e `DOCTOR`); verificar que a seção aparece na barra lateral e é selecionável.
- [x] 5.2 O estado da tela em `crates/sandmux/src/tui/procs.rs` (a árvore recebida, a raiz atual com a migalha, a seleção, o zoom, as dobras manuais) e o pedido no tique de um segundo, só com a seção aberta; verificado com os testes de unidade do estado.
- [x] 5.3 Em `crates/sandmux/src/tui/render.rs`, a árvore em outline com a migalha no topo, os três níveis de zoom, ▸/▾/·, a dobra automática acima de 12 descendentes (menos as raízes da tela, que ficam abertas: com a raiz dobrando, um agente real caberia numa linha) e a mensagem quando não há árvore; verificado pelo teste da TUI no host (a mensagem) e, na VM, pelo `arvore_de_processos_na_tui` (a árvore, o recuo e o cartão do zoom 3).
- [x] 5.4 As teclas: ↑/↓ seleção, ←/→ e Espaço dobra, Enter entra no ramo, Esc/Backspace volta, `+`/`-` zoom, e o clique num segmento da migalha; verificar com testes de unidade do estado (entrar dois níveis e voltar um de cada vez; dobrar e desdobrar).
- [x] 5.5 Atualização a cada segundo preservando a seleção, a dobra manual e a raiz atual (e voltando à raiz quando o processo da raiz atual termina); verificar com um teste de unidade que troca a árvore recebida.
- [x] 5.6 A seção nova em `docs/tui.md` (onde a página de configurações é descrita) e o `ppid` e o fim de processo em `docs/helper.md`, com os pares em inglês; verificado com o teste `docs`.

## 6. Verificação

- [x] 6.1 `./scripts/check.sh` no host (sem BPF), sob a vigia de memória: fmt, clippy e 783 testes, nenhuma falha (2026-11-21).
- [ ] 6.2 Com a autorização do usuário, os testes com root na VM (`scripts/vm-ubuntu-2404.sh check-root`), no 7.0 e no 6.8, com o teste do grupo 2.2; nada de BPF no host.
- [ ] 6.3 Conferência à mão na TUI com uma aba viva (pelo usuário, no build local).
