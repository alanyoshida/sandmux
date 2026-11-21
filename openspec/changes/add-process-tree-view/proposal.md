# Proposal

## Why

A seção "Recursos" do painel lateral já mostra memória, CPU e a contagem de processos da aba ativa, mas só o número — não há como ver quais processos compõem esse número, como se relacionam (quem lançou quem) ou qual deles está pesando. Investigar isso hoje exige sair do sandmux e usar `ps`/`pstree` fora dele, perdendo o recorte por aba. Uma árvore de processos navegável, dentro da página de configurações, fecha essa lacuna sem precisar sair da TUI.

## What Changes

- **Nova seção "Processos" na página de configurações**, ao lado de "Diagnóstico" e "Cofre": mostra a árvore de processos lançada pela aba ativa, com conteúdo próprio (não um formulário de opções do `config.toml`).
  - Visão padrão: a árvore inteira em outline, com migalha de pão no topo (`raiz ▸ ...`).
  - Zoom (tecla dedicada, `+`/`-`): densidade de informação por linha, em 3 níveis — nome e pid; + métrica (memória); cartão de detalhe multi-linha (cmdline, CPU, memória, desde quando, cgroup) — aplicado na árvore inteira de uma vez.
  - Drill-down (`Enter`): re-enraíza a tela no nó escolhido quando um ramo não cabe nem no zoom mais aberto; a migalha de pão sobe um nível. `Esc`/clique num segmento anterior da migalha volta.
  - Dobra manual por nó (`▸`/`▾`), com a mesma marca visual do colapso que as seções do painel lateral já usam.
  - Dobra automática: um ramo com mais descendentes que um limiar nasce dobrado, para um processo com muitos filhos diretos não dominar a tela.
  - Atualização ao vivo, amortizada a 1 vez por segundo — a mesma cadência da seção "Recursos".
- **Observação das execuções (`root-helper`) ganha o `ppid` e a saída do processo**: o programa de execução do pedido `observe`/`confine` passa a mandar também o pid de quem lançou cada execução, e um evento novo quando um processo observado termina. Sem esses dois dados não é possível montar nem manter uma árvore — só uma lista plana de execuções, como hoje.
- **Nova capability `process-tree`**: o daemon monta e mantém, por aba, a árvore de processos vivos a partir desses eventos (kernel, pelo auxiliar root) ou, nas máquinas sem o auxiliar, a partir do que o supervisor de aberturas já acompanha para o registro de execuções (mesma fonte e a mesma marcação `kernel`/`supervisor` que o registro de execuções já usa), e expõe essa árvore ao cliente pelo protocolo.

## Capabilities

### New Capabilities
- `process-tree`: o daemon acompanha, por sessão, a árvore de processos vivos do seu sandbox (quem lançou quem, nome, pid, memória, cmdline), a partir dos eventos de execução e saída, com a fonte dos dados (kernel ou supervisor) marcada como no registro de execuções, e expõe essa árvore ao cliente da TUI.

### Modified Capabilities
- `root-helper`: a "Observação das execuções" passa a incluir o `ppid` no evento de execução e ganha um evento novo para a saída do processo.
- `terminal-ui`: a página de configurações ganha a seção "Processos", com a árvore de processos da aba ativa, zoom de densidade, drill-down e dobra de ramos.

## Impact

- `crates/sandmux-ebpf/src/exec.rs`: ler e mandar o `ppid` no evento de execução (`EV_EXEC`); programa `tp_btf` novo, ou evento novo no mesmo programa, para `sched_process_exit`.
- `crates/sandmux/src/helper.rs`: os pedidos `observe` e `confine` passam o `ppid` e o evento de saída pela conexão.
- `crates/sandmux-launcher/src/seccomp.rs` e `crates/sandmux-launcher/src/files.rs`: o caminho do supervisor (sem auxiliar) passa a acompanhar `ppid` e saída de processo, para alimentar a árvore na mesma fonte usada hoje pelo registro de execuções.
- `crates/sandmux/src/daemon/session.rs` (e o par do Docker privado): monta e mantém a árvore em memória a partir dos eventos; novo estado por sessão.
- `crates/sandmux-proto`: mensagem nova (ou campo novo numa existente) para o cliente pedir e receber a árvore; a versão do protocolo muda.
- `crates/sandmux/src/tui/settings.rs` e `crates/sandmux/src/tui/render.rs`: seção "Processos" (índice novo em `SECTIONS`), desenho customizado da árvore (outline, migalha de pão, zoom, drill-down, dobra), análogo ao que "Diagnóstico" e "Cofre" já fazem para seções sem formulário.
- `docs/config.md` (e `docs/en/config.md`): a seção nova na página de configurações.

## Fora do escopo

- Mostrar a árvore de processos do daemon inteiro ou de outras abas — só a da aba ativa.
- Agir sobre um processo pela árvore (matar, renomear prioridade): a tela só observa.
- Testar a extensão do eBPF fora de uma VM, ou no host, sem autorização explícita do usuário — ver `design.md`.
