# Design

## Context

Hoje o único jeito de ver processos de uma aba é a contagem em "Recursos" (`pids.current`/`pids.max` do cgroup, lido em `sandmux-policy/src/limits.rs`) e, indiretamente, uma linha por execução no registro de arquivos (`file-access-log`), sem relação de pai/filho. Essa linha já vem de dois caminhos (`crates/sandmux-ebpf/src/exec.rs` + `crates/sandmux/src/helper.rs`, pedido `observe`/`confine`, quando o auxiliar root está presente; ou o supervisor de aberturas por seccomp, nos outros casos), cada evento marcado com a fonte (`via`: `kernel`/`supervisor`). Ver `proposal.md` para o porquê.

O formato do evento do auxiliar (`crates/sandmux-ebpf/src/ring.rs`) é um cabeçalho fixo de 32 bytes (`EV_HEADER`) com `kind`, `hook`, `bits`, `write`, `ino` (u64) e `dev`, seguido do nome e, para `EV_EXEC`, dos argumentos. O pid de uma execução já viaja hoje no campo `ino` (reaproveitado, sem campo dedicado) — ver `event_json` em `helper.rs:606`.

## Goals / Non-Goals

**Goals:**
- Montar e manter, por aba, uma árvore de processos ao vivo, com a mesma fonte (kernel/supervisor) e as mesmas garantias de "só observa" que o registro de execuções já tem.
- Mostrar essa árvore numa seção nova da página de configurações, com zoom de densidade, dobra e drill-down, sem travar nem arriscar o host.

**Non-Goals:**
- Árvore de outras abas ou do daemon inteiro (só a aba ativa; ver proposal.md).
- Agir sobre um processo pela tela (matar, mudar prioridade): só observação.
- Tornar o limiar de dobra automática ou as colunas de cada nível de zoom configuráveis pelo `config.toml` nesta mudança — ficam com um valor fixo, revisável depois se incomodar.
- Histórico da árvore (o que já terminou continua fora dela, como hoje no registro de execuções).

## Decisions

### Carregar `ppid` e saída de processo sem crescer o cabeçalho do evento
O cabeçalho de 32 bytes já tem dois campos de 4 bytes sem uso em `EV_EXEC` (`bits` e `write`, que só fazem sentido para `EV_DENY`/`EV_CREATE`). Em vez de aumentar `EV_HEADER` — o que muda o layout que todo evento já gravado respeita e todo decodificador já lê — o `ppid` passa a viajar num desses campos livres, só para `EV_EXEC`. Um evento `EV_EXIT` novo (`kind = 5`) usa o mesmo cabeçalho mínimo, com o pid no lugar que `ino` já ocupa para `EV_EXEC`, sem nome nem argumentos.

**Alternativa descartada:** crescer `EV_HEADER` com um campo dedicado de `ppid`. Mexe em todo `decode()`/`encode` existente (`EV_DENY`, `EV_CREATE`, `EV_CONN` incluídos) para um dado que só dois tipos de evento usam — risco maior para um ganho que o reaproveitamento já dá.

### Caminho do supervisor (sem auxiliar) não precisa de um mecanismo novo
O supervisor de aberturas (`sandmux-launcher/src/seccomp.rs`) já intercepta `execve`/`execveat` e lê o processo pela memória dele nesse instante — ler `/proc/<pid>/stat` para o `ppid` no mesmo instante é uma leitura extra, não um novo caminho de observação. A saída do processo observado também não é nova informação: o processo que lança a aba já precisa saber quando um filho dela termina (reaproveitar, não criar um novo laço de verificação).

### Estado da árvore fica só em memória, no daemon, por sessão
Um mapa `pid -> nó` por sessão, atualizado pelos eventos de execução e saída. Um processo cujo pai monitorado já saiu, mas que continua vivo, fica pendurado no ascendente vivo mais próximo (ou na raiz) — sem isso, a saída de um processo do meio cortaria a árvore em dois pedaços. Nada é persistido: ao reiniciar o daemon, a árvore de cada sessão recomeça vazia e se preenche pelos eventos seguintes.

### Semente única ao assinar, não um caminho de dados novo
Sem uma leitura inicial, a árvore de uma sessão já em execução ficaria vazia até a próxima execução dentro dela — o caso comum, já que a maioria das aberturas de seção acontece com a aba já rodando. Ao primeiro cliente assinar a árvore de uma sessão, o daemon lê uma vez `cgroup.procs` do cgroup da sessão e os `/proc/<pid>/stat` correspondentes, só para montar o estado inicial; dali em diante, só os eventos atualizam. Essa leitura não substitui a fonte combinada na exploração (kernel/supervisor): ela só evita a tela vazia no primeiro segundo.

### Mensagem de protocolo por assinatura, não embutida nas estatísticas
A árvore é pedida só enquanto a seção "Processos" está aberta para aquela aba — diferente de "Recursos", que já está sempre visível no painel lateral. Uma mensagem de assinatura dedicada (pedir/cancelar a árvore de uma sessão) evita montar e mandar a árvore quando nenhum cliente está olhando para ela.

### Limiar de dobra automática e colunas de cada zoom
Valores fixos para a primeira versão, ajustáveis depois sem mudar a spec:
- dobra automática: um nó com mais de **12** descendentes nasce dobrado;
- zoom 1: nome + pid;
- zoom 2: zoom 1 + memória residente;
- zoom 3: cmdline completa, CPU, memória, início (relativo, ex. "há 4min") e cgroup.

## Risks / Trade-offs

- **[Risco]** Programas BPF novos ou alterados já travaram a máquina do usuário duas vezes (LSM global, change 74) → **Mitigação:** o programa estendido aqui é um `tp_btf` de observação (como o `EV_EXEC` já em produção), preso ao cgroup pedido, sem poder de negar nada — categoria bem menos arriscada que um hook LSM global. Ainda assim, testar só numa VM (QEMU/KVM) antes de qualquer teste no host, e só prosseguir para o host com autorização explícita do usuário depois do teste em VM.
- **[Risco]** Verificador do BPF pode tratar um ponteiro ou argumento como nunca nulo e remover uma checagem (foi a causa da pane de change 74) → **Mitigação, corrigida na implementação:** a causa em `task_kill` era um argumento de gancho (`cred`) que o verificador trata como ponteiro confiável mesmo sendo nulo na maioria das chamadas. Aqui o `ppid` e o pid de saída vêm de `task->real_parent->tgid` e `task->tgid`, o mesmo padrão de leitura direta de campo BTF que `task->mm` já usa em produção desde o change 72 (não um argumento de gancho, e `real_parent` nunca é nulo para um processo que executa ou termina). Por isso o design não usa `bpf_probe_read_kernel` para esses dois campos, como a primeira versão deste documento previa; o teste `programa_de_execucao_monta`, que lê o BTF real deste host, confirma que os dois deslocamentos resolvem.
- **[Risco]** Processo órfão (pai monitorado já saiu) pode se perder da árvore se o reparenting não for tratado → **Mitigação:** regra explícita na spec (`process-tree`: "Órfão herda o ascendente vivo mais próximo") e teste dedicado a esse caminho.
- **[Trade-off]** A árvore do supervisor (sem auxiliar) pode ser enganada do mesmo jeito que o registro de execuções já pode (outra thread troca caminho/argumentos entre a leitura e a execução) → aceito, porque é a mesma limitação já documentada e aceita em `file-access-log` (`Fonte das execuções`); a tela marca a fonte como `supervisor` nesse caso.
- **[Trade-off]** Sem o auxiliar, a saída de um processo pode chegar um pouco depois do real fim dele, limitada à granularidade com que o supervisor já reage a filhos terminando → aceito, mesma limitação de latência que o resto do caminho supervisor já tem.

## Migration Plan

1. Estender `sandmux-ebpf`/`helper.rs` (ppid, `EV_EXIT`) e testar numa VM — nunca no host — até a extensão rodar limpa num kernel de teste.
2. Pedir autorização explícita do usuário antes de qualquer teste da extensão no host dele, mesmo depois de validada em VM.
3. Estender o caminho do supervisor (sem auxiliar) em paralelo — não depende da extensão em BPF.
4. Montar a árvore no daemon e a mensagem de protocolo (versão nova), com o teste dos dois caminhos (kernel e supervisor) e do caso de órfão.
5. Seção "Processos" na TUI, por último, consumindo a árvore já pronta.

Sem dado persistido e sem mudança de formato em disco, não há passo de rollback de dados: reverter é só não carregar a extensão nova (volta a `observe`/`confine` sem `ppid`/`EV_EXIT`) e remover a seção da página.
