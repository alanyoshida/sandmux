Os testes com o Docker real, o `./scripts/check.sh` e o `scripts/check-root.sh` rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Os testes com root precisam da linha do `sudoers` de `scripts/check-root.sh`.

## 1. Programa de execução (com root primeiro)

- [x] 1.1 Montar o programa `tp_btf` de `sched_process_exec` (o filtro pelo cgroup, o `bpf_ringbuf_reserve`, o caminho, os argumentos por `mm->arg_start` e `mm->arg_end`, o `bpf_ringbuf_submit` e o contador), o evento `EV_EXEC` e a carga com o anexo `BPF_TRACE_RAW_TP`. Verificar com testes de montagem e de leitura do evento, e com um teste com root em que o `observe` de um cgroup de teste recebe o `ls -la` com os argumentos. Se o `BPF_LINK_CREATE` recusar, aplicar o recuo da decisão 1 e anotar no design. Feito: o kernel aceitou o programa e o anexo; o teste `observacao_de_execucoes_no_kernel` recebe o `ls -la /`

## 2. Auxiliar

- [x] 2.1 Acrescentar o pedido `observe` (a conferência do dono, o nível e o id do cgroup, a observação com a fila, os eventos pela conexão, o limite de 64 por usuário) e o programa de execução no `confine`. Verificar com testes sem root, pela carga falsa: `observe` de um cgroup alheio recusado, o limite, os eventos de execução chegando ao cliente

## 3. Recuo pelo supervisor

- [x] 3.1 Acrescentar `files.exec_log` à especificação e `execve` e `execveat` ao filtro de notificação quando ele vale. No supervisor, a leitura do caminho e do `argv` em `/proc/<pid>/mem`, a linha com `via` = `supervisor` quando a política ao vivo tem `exec_log`, e a resposta `CONTINUE`. Verificar com um teste de `sandmux run` sem o auxiliar: `ls -la` sai no registro com os argumentos e a fonte, e um programa que não existe não trava o supervisor
- [x] 3.2 Acrescentar `exec_log` à política ao vivo do lançamento, com `true` por padrão. Verificar com um teste em que o daemon regrava `false` e o supervisor para de registrar

## 4. Daemon, registro e estado

- [x] 4.1 Quando o `monitor` acha o escopo, pedir o `observe` (com o auxiliar disponível), gravar as execuções com a etiqueta do lançamento, regravar `exec_log` como `false` e, se a conexão cair com a aba viva, como `true`. As execuções do Docker vão com `docker-<id>`. Verificar com testes unitários da tradução dos eventos e com os testes com root da seção 5
- [x] 4.2 Acrescentar `args` e `via` ao `FileEntry`; mostrar os argumentos no `sandmux files` e na tela de arquivos da TUI. Acrescentar `exec` ao `PolicySummary`, subir a versão do protocolo e mostrar a linha "execuções" no `sandmux stats` e no painel. Verificar com os testes de protocolo, de `filescmd` e de `stats`

## 5. Verificações com root

- [x] 5.1 Teste com o auxiliar real: numa aba, `curl --version` um segundo depois de abrir sai com `via` = `kernel`, o caminho e os argumentos; a proteção do lançamento mostra "execuções: pelo kernel"; um contêiner que roda `sh -c 'ls /'` gera as linhas `docker-<id>` do `sh` e do `ls`. Mudado: o `ls` do contêiner não aparece porque o busybox o roda sem `exec`; o teste confere o `sh -c` do contêiner, e o cenário do spec foi ajustado
- [x] 5.2 Teste com o auxiliar real: parar o auxiliar com a aba viva devolve o registro ao supervisor (`via` = `supervisor`) na execução seguinte
- [x] 5.3 Medir o custo do programa no resto da máquina (execuções fora do cgroup, por exemplo 2000 `/bin/true` num shell do host, com e sem uma aba observada) e numa aba (um `cargo build` pequeno), e anotar no roadmap. Feito em 2026-11-17: 2000 `/bin/true` no host em 1963 a 2002 ms sem observação e 1876 a 2024 ms com uma aba observada (sem custo mensurável); numa aba, 2410 ms (o filtro do supervisor ainda manda o `execve` a ele, que só deixa seguir). O `cargo build` não foi medido

## 6. Documentação e fechamento

- [x] 6.1 Atualizar `docs/files.md` (as linhas `exec`, as fontes, os limites), `docs/helper.md` (o `observe`), `docs/sessions.md` (o protocolo) e os pares em inglês, com os testes de `docs.rs`
- [x] 6.2 Rodar `./scripts/check.sh` e `scripts/check-root.sh` inteiros sob o vigia

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Depois: o change 73 (processo de cada conexão) e o 74 (Ubuntu 24.04), nessa ordem (pedido do usuário, 2026-11-17).
- Lembrar o usuário de tirar a linha do `sudoers` de `scripts/check-root.sh` quando não precisar mais dela.
