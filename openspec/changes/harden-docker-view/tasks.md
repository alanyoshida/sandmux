Os testes com o Docker real e o `./scripts/check.sh` rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`.

## 1. Seccomp herdado

- [x] 1.1 Escrever em `tests/docker.rs` um teste do modo de visão (com `unshare`, como `modo_de_visao_monta_so_a_visao`) que roda o `sandmux-probe` dentro da visão e espera erro em `bpf`, `perf_event_open`, `userfaultfd` e `io_uring_setup`, `ENOSYS` em `keyctl`, e um `mount -t tmpfs` funcionando; verificar que ele falha antes da mudança
- [x] 1.2 Acrescentar `enosys_syscalls` a `SeccompRules`, com padrão vazio, e ao montador do filtro em `seccomp.rs`; verificar com um teste unitário do programa BPF (erro certo por chamada, número x32 negado) e com o teste de `launch_spec.rs`
- [x] 1.3 Preencher a lista em `compose_docker_view` (a do sandbox sem as dez chamadas de montagem, com o chaveiro em `ENOSYS`) e instalar o filtro no `exec_view`, sem `no_new_privs`; verificar que o teste 1.1 passa e atualizar `visao_do_docker_tem_so_o_necessario`, que hoje afirma o seccomp vazio
- [x] 1.4 Teste com o Docker real: um contêiner `--privileged --security-opt seccomp=unconfined` com o `sandmux-probe` montado recebe erro nas chamadas negadas, e um processo de usuário comum num contêiner roda um binário setuid com sucesso; verificar que os testes atuais de `run`, `build` e `compose` continuam passando. Se a criação do contêiner falhar pelo chaveiro, aplicar o recuo da decisão 2 do design e anotar no design
- [x] 1.5 Documentar o campo novo em `docs/launch-spec.md` e as chamadas negadas em `docs/docker.md`, com os pares em inglês; verificar com o teste de `docs.rs`

## 2. `/dev` mínimo

- [x] 2.1 Extrair de `compose` a montagem do `/dev` do sandbox para uma função comum; verificar que os testes de composição e de `baseline.rs` passam sem mudança
- [x] 2.2 Usar a função em `compose_docker_view`, com a GPU e o `/dev/kvm` pelas chaves `gpu` e `kvm` da configuração efetiva da pasta, passadas pelo daemon; verificar com testes unitários da composição (sem o bind de `/dev`; `gpu = false` sem nós de GPU) e com um teste do modo de visão em que `/dev/pts` não tem os pseudoterminais do host
- [x] 2.3 Teste com o Docker real: `docker run -t` recebe um terminal, `docker run -v /dev:/d` não mostra pseudoterminais do host nem `/d/video0`, e `docker info` mostra o driver de overlay (sem `fuse-overlayfs`); verificar que os testes atuais de `docker.rs` continuam passando
- [x] 2.4 Atualizar "O que os contêineres enxergam" e "Limites" em `docs/docker.md` com os dispositivos, e o par em inglês; verificar com o teste de `docs.rs`

## 3. Cgroups próprios

- [x] 3.1 Acrescentar o tipo de montagem `cgroup2` à especificação e ao lançador; verificar com o teste de validação da especificação e documentar em `docs/launch-spec.md` e no par em inglês
- [x] 3.2 Passar `--cgroupns` e `--evacuate-cgroup2` ao `rootlesskit`, trocar o `--cgroup-parent` do `dockerd` para `/conteineres`, e na visão montar o cgroup2 próprio quando há escopo e nenhum cgroup quando não há; verificar com os testes unitários de `rootlesskit_args`, `dockerd_command` e da composição. Mudado na implementação (decisão 4 do design): o `--cgroupns` e o cgroup2 valem sempre, e só o `--evacuate-cgroup2` depende do escopo; o `--cgroup-parent` ficou como estava, porque não tem efeito com o driver `none`; o escopo do Docker deixou de ter teto próprio
- [x] 3.3 Teste com o Docker real: um contêiner com `-v /sys/fs/cgroup:/c` não vê `user.slice` nem o arquivo do teto da pasta; verificar que `porta_so_no_localhost_teto_e_stats` continua matando o contêiner no teto e que o `sandmux stats` mostra o consumo. Se os contêineres não subirem dentro do namespace, aplicar o recuo da decisão 4 do design e anotar no design. Acrescentado: `sem_tetos_o_docker_so_ve_o_proprio_cgroup`, com o `systemd-run` fora do `PATH` do daemon
- [x] 3.4 Atualizar "Tetos e consumo" em `docs/docker.md` e o par em inglês; verificar com o teste de `docs.rs`

## 4. Coberturas dos arquivos da pasta

- [x] 4.1 Escrever em `sandmux-policy` a varredura que recebe a política de arquivos, os criados pelo agente e a pasta, e devolve as coberturas ou o erro do teto de 4096; verificar com testes unitários: `.env` coberto, `.env.example` não, segredo criado pelo agente não, `file-allow` vence, pasta negada coberta sem descer, link simbólico pulado, teto
- [x] 4.2 Fazer o lançador pular um `Cover` cujo destino não existe mais; verificar com um teste do modo de visão com um destino ausente
- [x] 4.3 No daemon, compilar a política da configuração da pasta, ler os criados e acrescentar as coberturas à visão; acima do teto, responder no socket do Docker com a mensagem da spec, em português e em inglês. Verificar com um teste do modo de visão: `.env` vazio e em somente leitura na visão, o do host intacto, pasta negada vazia
- [x] 4.4 Teste com o Docker real: `docker run -v $PWD:/w <imagem> cat /w/.env` sai vazio e o `.env.example` é lido; medir a varredura numa pasta com `node_modules` e anotar o tempo no roadmap
- [x] 4.5 Documentar em `docs/docker.md` os arquivos cobertos e os limites (contêiner privilegiado desfaz coberturas e a proteção do `.git`; arquivos criados depois; política ao vivo), apontar de `docs/files.md`, e atualizar os pares em inglês; verificar com o teste de `docs.rs`

## 5. Integração

- [x] 5.1 Rodar `./scripts/check.sh` inteiro e verificar que passa, inclusive a suíte de fuga
- [ ] 5.2 Conferir à mão numa sessão real do daemon, numa pasta com um `.env` criado no host: o `.env` chega vazio a um contêiner, `ls /d/pts` de um contêiner com `-v /dev:/d` não mostra os terminais do usuário, e `docker compose up` de um projeto de exemplo funciona
- [x] 5.3 Atualizar `openspec/roadmap.md`: a situação do change 69, os achados que passaram de inferidos a testados, e as verificações que ficarem pendentes

## Workflow follow-up

- Arquivar o change depois da revisão e sincronizar a spec `container-engine`.
- Decidir com o usuário se a visão trancada vira um change próprio antes do change 70.
