## Why

O Docker privado de uma pasta só é limitado pela visão de arquivos: o `dockerd` e os contêineres rodam sem o Landlock, sem o seccomp e sem o supervisor de aberturas do sandbox. A exploração do backend eBPF (2026-11-16) achou, pela leitura do código, quatro caminhos pelos quais um contêiner alcança o que o agente não alcança:

- **Chamadas de sistema:** com `--privileged` ou `--security-opt seccomp=unconfined`, o contêiner recupera `bpf`, `perf_event_open`, o chaveiro do kernel, `userfaultfd` e `io_uring`, que o sandbox nega.
- **Dispositivos:** a visão monta o `/dev` do host inteiro, com escrita. Um `-v /dev:/d` alcança os pseudoterminais dos outros terminais do usuário, a câmera, o áudio e o `/dev/shm` do host.
- **Cgroups:** a visão monta o `/sys/fs/cgroup` do host com escrita. Um contêiner pode afrouxar o teto de memória da própria slice ou matar outros cgroups do usuário.
- **Arquivos da pasta:** os arquivos com cara de segredo (`.env`, `*.pem`) e os caminhos de `file-deny`, que o agente não abre, abrem num contêiner com a pasta de trabalho montada.

Os três primeiros fecham sem privilégio e por completo. O quarto só fecha por completo com o backend eBPF (change 70); aqui entra um paliativo.

Decisões do usuário (2026-11-16): dois changes, com o endurecimento sem privilégio primeiro; os cgroups entram neste change; o paliativo dos segredos entra neste change.

## What Changes

- **Seccomp herdado:** o `dockerd` nasce com um filtro seccomp que todo contêiner herda, mesmo com `--privileged` ou `--security-opt seccomp=unconfined`. Ele nega `bpf`, `perf_event_open`, a carga de módulos, `kexec`, o chaveiro do kernel, `userfaultfd`, `io_uring`, `TIOCSTI` e `TIOCLINUX`. As chamadas de montagem continuam liberadas, porque o `dockerd` e o `runc` precisam delas.
- **`/dev` mínimo:** a visão do Docker passa a ter o mesmo `/dev` do sandbox (os nós básicos, pseudoterminais novos e `/dev/shm` próprio), com a GPU e o `/dev/kvm` conforme a configuração da pasta. O `/dev` do host sai da visão.
- **Cgroups próprios:** o Docker roda num cgroup namespace e só enxerga a subárvore dele. O teto fica na slice, acima do que os contêineres veem.
- **Segredos cobertos (paliativo):** quando o Docker sobe, os arquivos da pasta de trabalho com cara de segredo e os caminhos negados por `file-deny` na configuração da pasta são cobertos, e chegam vazios e em somente leitura aos contêineres. Os segredos que o agente criou continuam visíveis.
  - Decisão do agente: o `file-deny` entra junto, porque a mesma função decide os dois.
  - Decisão do agente: acima de 4096 coberturas, o Docker privado não sobe, com uma mensagem que diz como reduzir.
- **Limite conhecido, documentado:** uma cobertura por montagem cai para um contêiner com `CAP_SYS_ADMIN` no user namespace do Docker (`--privileged`): ele desmonta a cobertura ou tira o somente-leitura. Isso vale para as coberturas novas e para a proteção do `.git` que já existe na visão. O paliativo não cobre arquivos criados depois, mudanças ao vivo na política, nem o `.git/commondir`.
- **BREAKING (comportamento):** um contêiner deixa de alcançar os dispositivos do host além dos da lista (por exemplo `/dev/net/tun`, `/dev/fuse` e `/dev/video0`) e deixa de ler os segredos da pasta que já existiam.

Fora deste change:

- A paridade completa dos arquivos (segredos, `file-deny` ao vivo, `.git/commondir`, registro), que é o change 70.
- Trancar as montagens da visão contra um contêiner privilegiado. Fica como decisão em aberto no roadmap.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `container-engine`: as chamadas de sistema negadas ao Docker privado, os dispositivos da visão, os cgroups que os contêineres enxergam e os arquivos da pasta cobertos.

## Impact

- `crates/sandmux-spec`: o erro `ENOSYS` no filtro seccomp (para o chaveiro) e a montagem de um cgroup2 novo.
- `crates/sandmux-launcher`: `exec_view` instala o filtro; a montagem do cgroup2; uma cobertura cujo destino sumiu é pulada.
- `crates/sandmux-policy`: `compose_docker_view` com o `/dev` mínimo, a lista de chamadas negadas, o cgroup2 e as coberturas; a varredura da pasta com a política de arquivos.
- `crates/sandmux/src/docker.rs` e `daemon/docker.rs`: `--cgroupns` no `rootlesskit`, os dispositivos e as coberturas pela configuração da pasta, a recusa acima do teto de coberturas.
- `crates/sandmux/tests/docker.rs`: testes do modo de visão e com o Docker real.
- `docs/docker.md`, `docs/files.md`, `docs/launch-spec.md` e os pares em inglês.
- Sem mudança de protocolo. O Docker privado de uma pasta pega a visão nova quando sobe de novo.
