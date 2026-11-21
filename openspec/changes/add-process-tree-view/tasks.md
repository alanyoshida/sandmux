# Tasks

## 1. Evento de execução e saída no eBPF (só em VM)

- [x] 1.1 Em `crates/sandmux-ebpf/src/ring.rs`, gravar o `ppid` num campo livre do cabeçalho de `EV_EXEC` (`bits` ou `write`, sem uso nesse tipo de evento) e expor em `Event`; verificar com um teste de `decode`/`encode` que cobre um evento `EV_EXEC` com `ppid` diferente de `pid`.
- [x] 1.2 Acrescentar `EV_EXIT` (`kind = 5`) em `ring.rs`, reaproveitando o cabeçalho mínimo (pid no lugar de `ino`, sem nome nem argumentos); verificar com um teste de `decode`/`encode` do evento novo.
- [x] 1.3 Em `crates/sandmux-ebpf/src/exec.rs`, ler o `ppid` do processo no tracepoint `sched_process_exec` (encadeando `task->real_parent->tgid` por leituras diretas do BTF, o mesmo padrão já usado para `task->mm`, e não por um argumento de gancho não confiável como no caso do `task_kill`) e mandar no evento; acrescentar o programa/evento de `sched_process_exit` filtrado pelo mesmo cgroup. Verificado com o teste já existente do módulo (`programa_de_execucao_monta`) estendido para cobrir os dois — passou neste host lendo o BTF real (`/sys/kernel/btf/vmlinux`), sem carregar nada no kernel (só monta os bytes do programa em memória).
- [ ] 1.4 Carregar e exercitar a extensão numa VM (QEMU/KVM) — nunca no host — lançando processos filhos e encerrando-os dentro do cgroup de teste; verificar pelo `dmesg` da VM sem oops e pelos eventos recebidos (`ppid` correto, `EV_EXIT` para cada saída).
- [ ] 1.5 Pedir e registrar a autorização explícita do usuário antes de qualquer teste da extensão fora da VM; só então seguir para as tarefas que a exercitam no host do usuário.

## 2. Auxiliar root (`observe`/`confine`)

- [ ] 2.1 Em `crates/sandmux/src/helper.rs`, incluir o `ppid` no `event_json` de `EV_EXEC` e tratar `EV_EXIT` (evento `{"kind": "exit", "pid": ...}`); verificar com um teste que decodifica os dois tipos e confere o JSON produzido.
- [ ] 2.2 Confirmar que `observe` e `confine` continuam sem negar nem atrasar nada com os dois eventos novos na fila; verificar com o teste de integração do auxiliar já existente, estendido para abrir, executar um filho, encerrá-lo e checar os dois eventos na ordem certa.

## 3. Caminho do supervisor (sem auxiliar root)

- [ ] 3.1 Em `crates/sandmux-launcher/src/seccomp.rs`, ler `/proc/<pid>/stat` para o `ppid` no mesmo instante em que já lê o caminho e os argumentos de `execve`/`execveat`; verificar com um teste que simula uma execução e confere o `ppid` reportado.
- [ ] 3.2 Reaproveitar o acompanhamento de saída de filhos que o lançador da aba já faz para gerar o mesmo formato de evento de saída usado pelo caminho do auxiliar; verificar com um teste que encerra um processo filho e confere o evento.

## 4. Árvore no daemon e protocolo

- [ ] 4.1 Nova capability `process-tree`: em `crates/sandmux/src/daemon/session.rs`, manter um mapa `pid -> nó` por sessão, atualizado pelos eventos de execução e saída dos dois caminhos (2 e 3); verificar com um teste que monta uma árvore de 3 níveis a partir de uma sequência de eventos.
- [ ] 4.2 Tratar o processo órfão (pai monitorado já saiu, filho ainda vivo): prendê-lo ao ascendente vivo mais próximo, ou à raiz; verificar com um teste que encerra um nó do meio da árvore e confere que os descendentes continuam presentes, só reposicionados.
- [ ] 4.3 Semente única ao primeiro cliente assinar a árvore de uma sessão: ler `cgroup.procs` do cgroup da sessão e os `/proc/<pid>/stat` correspondentes uma vez, sem repetir depois; verificar com um teste que assina a árvore de uma sessão já em execução, sem eventos ainda recebidos, e confere que a árvore não está vazia.
- [ ] 4.4 Mensagem nova em `crates/sandmux-proto` para assinar e cancelar a árvore de processos de uma sessão, com a versão do protocolo revisada; verificar com um teste de (de)serialização da mensagem e com a verificação de versão de protocolo existente ainda passando.
- [ ] 4.5 No daemon, mandar o estado atual ao assinar e as atualizações seguintes amortizadas a 1 por segundo, parando ao cancelar a assinatura ou a conexão cair; verificar com um teste que assina, gera várias mudanças em menos de um segundo e confere uma atualização só, e que cancelar para o envio.

## 5. Seção "Processos" na página de configurações

- [ ] 5.1 Em `crates/sandmux/src/tui/settings.rs`, acrescentar "Processos" a `SECTIONS`, depois de "Diagnóstico", com o índice exportado (como `VAULT`/`DOCTOR`); verificar que a seção aparece na barra lateral da página e é selecionável.
- [ ] 5.2 Em `crates/sandmux/src/tui/render.rs`, desenhar a árvore em outline (nome, pid) com a migalha de pão no topo, assinando a árvore da aba ativa ao entrar na seção e cancelando ao saída dela; verificar manualmente com uma aba viva com processos filhos.
- [ ] 5.3 Zoom de densidade (tecla dedicada, 3 níveis, aplicado à árvore inteira, sem mudar dobra nem seleção); verificar manualmente alternando os 3 níveis sobre a mesma árvore.
- [ ] 5.4 Dobra manual por nó (▸/▾) e dobra automática por tamanho do ramo (limiar de 12 descendentes); verificar manualmente com um processo com mais de 12 filhos diretos nascendo dobrado, e dobrando/desdobrando outro manualmente.
- [ ] 5.5 Drill-down (entrar re-enraíza no nó selecionado e estende a migalha; voltar, pelo atalho ou clicando num segmento anterior, re-enraíza nele e descarta os segmentos depois); verificar manualmente entrando dois níveis e voltando um de cada vez.
- [ ] 5.6 Atualização ao vivo a 1 segundo preservando seleção, dobra manual e o nó re-enraizado atual; verificar manualmente lançando e encerrando um processo dentro da aba enquanto a seção está aberta.
- [ ] 5.7 Atualizar `docs/config.md` e `docs/en/config.md` com a seção nova da página de configurações.

## Workflow follow-up

- Arquivar a mudança depois que a extensão do eBPF (grupo 1) tiver sido validada em VM e autorizada pelo usuário para o host, e os grupos 2 a 5 estiverem implementados e verificados.
