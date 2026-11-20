Os testes rodam no host, sem BPF, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`.

## 1. O substituto

- [x] 1.1 O leitor de opções do bubblewrap (com `--args`), em grupos aplicadas, convertidas e ignoradas, e o erro para a desconhecida. Verificar com testes unitários, inclusive com a linha de comando real do Claude Code
- [x] 1.2 A execução: o Landlock empilhado pelas montagens, os namespaces sem montagem, o processo 1 do `--unshare-pid`, o ambiente, o `--seccomp` e os descritores de sincronização. Verificar com testes unitários num processo filho
- [x] 1.3 O `sandmux-run` chamado como `bwrap` e a montagem em `/run/sandmux/bin/bwrap` (e sobre o `/usr/bin/bwrap` da visão). Verificar com um teste de `sandmux run` que roda `bwrap --ro-bind / / --bind $PWD $PWD` e confere o que escreve

- [x] 1.4 Achado no teste com o Claude Code real: ele não subia em nenhuma aba (`EINVAL` ao abrir `/tmp/claude-<uid>`). O supervisor de arquivos tratava um `O_DIRECTORY` como `O_TMPFILE` (o `O_TMPFILE` inclui o bit do `O_DIRECTORY`), mantinha o `mode`, e o `openat2` recusava. Verificar com um teste de integração que abre um diretório por `openat` com `mode` (falha sem a correção)
## 2. Harnesses

- [x] 2.1 O Claude Code real na aba: ele sobe com a correção da 1.4. O teste do sandbox do bash dele com o login de verdade depende do cofre do usuário e fica como verificação à mão no roadmap. Conferir os outros presets: só o Claude Code usa o `bwrap` (o opencode, o omp, o hermes e o agy, não)
- [x] 2.2 Documentação: o bubblewrap no sandbox, os limites, e os pares em inglês; o roadmap com o modo de montagens e o podman em "Depois da v1"

## 3. Fechamento

- [x] 3.1 Rodar o `./scripts/check.sh` inteiro sob o vigia

## Workflow follow-up

- Arquivar, PR, merge e release nova (ordem do usuário, 2026-11-20).
