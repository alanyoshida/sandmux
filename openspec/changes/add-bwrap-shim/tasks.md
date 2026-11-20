Os testes rodam no host, sem BPF, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`.

## 1. O substituto

- [ ] 1.1 O leitor de opções do bubblewrap (com `--args`), em grupos aplicadas, convertidas e ignoradas, e o erro para a desconhecida. Verificar com testes unitários, inclusive com a linha de comando real do Claude Code
- [ ] 1.2 A execução: o Landlock empilhado pelas montagens, os namespaces sem montagem, o processo 1 do `--unshare-pid`, o ambiente, o `--seccomp` e os descritores de sincronização. Verificar com testes unitários num processo filho
- [ ] 1.3 O `sandmux-run` chamado como `bwrap` e a montagem em `/run/sandmux/bin/bwrap` (e sobre o `/usr/bin/bwrap` da visão). Verificar com um teste de `sandmux run` que roda `bwrap --ro-bind / / --bind $PWD $PWD` e confere o que escreve

## 2. Harnesses

- [ ] 2.1 O Claude Code real na aba com o sandbox do bash ligado: um comando roda, e uma escrita fora do que ele libera falha. Conferir o opencode e os outros presets
- [ ] 2.2 Documentação: o bubblewrap no sandbox, os limites, e os pares em inglês; o roadmap com o modo de montagens e o podman em "Depois da v1"

## 3. Fechamento

- [ ] 3.1 Rodar o `./scripts/check.sh` inteiro sob o vigia

## Workflow follow-up

- Arquivar, PR, merge e release nova (ordem do usuário, 2026-11-20).
