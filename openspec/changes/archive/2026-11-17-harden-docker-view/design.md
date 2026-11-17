## Context

Motivação em `proposal.md`. O que importa para o desenho:

- **Como o Docker privado sobe hoje** (`daemon/docker.rs`, `docker.rs`): `systemd-run --user --scope` na slice `sandmux-docker-<id>.slice` → `rootlesskit --net=none --pidns` (user, mount e pid namespaces novos) → `sandmux-run --view-file` → `dockerd`.
- **O que o `exec_view` faz** (`sandmux-launcher/src/launch.rs`): cria o TUN, monta a raiz da especificação e executa o comando. Não aplica Landlock nem seccomp; a especificação da visão tem `seccomp` vazio.
- **O que a visão monta** (`compose_docker_view`): o sistema em somente leitura, a pasta de trabalho com a proteção do `.git`, a pasta de entrada, as pastas do Docker, e, do host inteiro e com escrita, o `/dev` (recursivo) e o `/sys/fs/cgroup`.
- **Por que não o Landlock nem o supervisor:** um processo sob Landlock não monta nada, e o `dockerd` e o `runc` montam. O supervisor de aberturas resolve caminhos numa raiz só, e cada contêiner tem a sua.
- **Quem o agente é dentro do Docker:** quem fala com a API do Docker manda no `dockerd`. Com `--privileged`, um contêiner tem `CAP_SYS_ADMIN` no user namespace do `rootlesskit`, que é o mesmo em que a visão foi montada.
- **Medido em 2026-11-16, num namespace de teste:** num mount namespace do mesmo user namespace, dá para tirar o somente-leitura de um bind e desmontar a cobertura de um arquivo. Num user namespace filho, as duas operações são recusadas (montagens trancadas). Não foi testado com o Docker real.

Consequência do último ponto: neste change, só é garantia o que não depende de uma montagem por cima de outra coisa. O seccomp herdado, a ausência de um dispositivo e o cgroup namespace são garantias. Uma cobertura é um obstáculo para contêineres comuns.

## Goals / Non-Goals

**Goals:**

- Paridade de chamadas de sistema com o sandbox, menos as de montagem, para todo processo do Docker privado.
- Nenhum dispositivo do host além dos que o sandbox já entrega.
- Nenhum cgroup do host fora da subárvore do Docker da pasta.
- Os segredos e os caminhos negados da pasta fora do alcance de um contêiner comum.
- Tudo sem privilégio, em toda máquina que já roda o Docker privado.

**Non-Goals:**

- Impedir que um contêiner privilegiado desfaça coberturas (as novas e as do `.git`). Depende de trancar a visão ou do eBPF.
- Política de arquivos ao vivo, `.git/commondir` e registro de arquivos dos contêineres (change 70).
- Restringir o que pode ser montado.
- Filtrar a API do Docker (recusar `--privileged`): o agente chega à raiz da visão por outros caminhos, como o socket do `containerd`, e a lista de recusas nunca fecha.

## Decisions

### 1. Filtro seccomp instalado pelo `exec_view`, herdado

`compose_docker_view` preenche `seccomp` com a lista do sandbox (`lists::DENY_SYSCALLS`) sem as dez chamadas de montagem, mais `DENY_IOCTLS`. O `exec_view` instala o filtro depois de montar a raiz e antes do `exec`.

- **Por que herdado:** um filtro seccomp passa pelo `fork` e pelo `exec` e não pode ser removido. O `--privileged` e o `seccomp=unconfined` só deixam de acrescentar o filtro do Docker.
- **Sem `no_new_privs`:** o `exec_view` roda como root do user namespace do `rootlesskit`, com `CAP_SYS_ADMIN` nele, e o kernel aceita o filtro sem `no_new_privs`. Ligar o `no_new_privs` passaria a todo contêiner e quebraria o `su` e o `sudo` das imagens.
- **Alternativa descartada:** entregar um perfil ao `dockerd` (`--seccomp-profile`). O agente o desliga com `--security-opt seccomp=unconfined`.
- **Alternativa descartada:** BPF LSM para as chamadas de sistema. Exige root e não acrescenta nada aqui.

### 2. Chaveiro com `ENOSYS`

O `runc` cria um chaveiro de sessão novo por contêiner. Pelo que o agente conhece do código do `runc`, ele trata só o `ENOSYS` como "o kernel não tem chaveiro", e qualquer outro erro derruba a criação do contêiner. Por isso `keyctl`, `add_key` e `request_key` devolvem `ENOSYS` na visão do Docker, e as outras chamadas, `EPERM`.

Conferido na tarefa 1.4 (2026-11-17), com o `runc` 1.5.3 e o Docker 29.9.3: os contêineres sobem com o chaveiro em `ENOSYS`, e o recuo não foi preciso. Na mesma prova, sem o filtro, um processo da visão lia o identificador do chaveiro de sessão.

- `SeccompRules` ganha uma lista `enosys_syscalls`, com valor padrão vazio; o sandbox do agente não muda.
- **Recuo, se o `runc` instalado não tolerar o `ENOSYS`:** o `runc` da visão (o wrapper que já existe em `/run/sandmux/bin`) passa `--no-new-keyring` ao `create` e ao `run`, e as três chamadas voltam para `EPERM`.

### 3. `/dev` mínimo, compartilhado com o sandbox

O trecho de `compose` que monta o `/dev` do sandbox (tmpfs, os seis nós, `devpts` novo, `ptmx`, `/dev/shm`, os links de `fd`) vira uma função usada pelas duas composições. Na visão do Docker, a GPU e o `/dev/kvm` seguem as chaves `gpu` e `kvm` da configuração efetiva da pasta, sem opções de `run`, como os tetos já fazem.

- **Por que basta:** sem root real, o `runc` não cria nós de dispositivo; ele monta por bind os nós que acha no `/dev` da visão. Os seis nós são os que um contêiner padrão recebe.
- **O TUN não entra:** o `exec_view` já cria o TUN antes de trocar a raiz, com o `/dev/net/tun` do host à vista.
- **Garantia, não cobertura:** o `/dev` da visão é um tmpfs novo numa raiz nova. Desmontá-lo mostra uma pasta vazia, e não o `/dev` do host.

### 4. Cgroup namespace e cgroup2 próprio

O `rootlesskit` ganha `--cgroupns` (existe na versão 3.3 da máquina de referência). A visão troca o bind de `/sys/fs/cgroup` por um cgroup2 novo, montado pelo `exec_view` (tipo de montagem novo `cgroup2` na especificação). A raiz visível é o escopo do Docker; a slice com o teto fica acima dela.

- **Processos fora da raiz do escopo:** com `--evacuate-cgroup2=motor`, o `rootlesskit` leva os processos para um subgrupo. A raiz fica sem processos, como o cgroup v2 pede para delegar controladores.
- **Onde ficam os contêineres (medido em 2026-11-17):** o `dockerd` sem root fica com o driver de cgroup `none`, e o `--cgroup-parent` não tem efeito. O `runc` cria cada contêiner na raiz do namespace, que é o escopo, ao lado de `motor`. O `--cgroup-parent conteineres` antigo ficou como estava.
- **O escopo sem teto próprio:** o escopo recebia o mesmo teto da slice. Como ele é a raiz que os contêineres enxergam, um contêiner via o `memory.max` de 512M e podia afrouxá-lo (a slice ainda seguraria). O teto passa a ficar só na slice, e o escopo do Docker sobe sem `MemoryMax` nem `TasksMax`.
- **Sem escopo** (máquina sem os tetos), mudado na implementação: o desenho previa a visão sem cgroup nenhum, mas o `runc` não cria contêiner sem um cgroup montado ("no cgroup mount found in mountinfo"), e o Docker inteiro deixaria de funcionar nessas máquinas. O `--cgroupns` passa a valer sempre, e o cgroup2 da visão também; só o `--evacuate-cgroup2` depende do escopo, porque mover processos exige escrever no cgroup. A raiz que os contêineres enxergam é o cgroup em que o `rootlesskit` sobe (o do daemon), nunca a árvore do usuário. Numa máquina sem o systemd de usuário, esse cgroup não é delegado ao usuário, e nada nele é gravável; isso não foi testado numa máquina assim. O teste simula a falta dos tetos tirando o `systemd-run` do `PATH` do daemon, e confere que o Docker funciona e que a árvore do usuário não aparece.
- **Recuo, se o `runc` não criar os cgroups dos contêineres dentro do namespace:** não foi preciso.
- **Alternativa descartada:** manter o bind do host em somente leitura. O `runc` precisa escrever, e o somente-leitura é uma cobertura.

### 5. Coberturas pela política de arquivos da pasta

Quando o Docker de uma pasta sobe, o daemon:

1. compila a política de arquivos da configuração efetiva da pasta (`Effective::file_policy`), sem a camada de aba;
2. lê a lista de segredos criados pelo agente naquela pasta (`files/created/<hash>.json`);
3. percorre a pasta de trabalho sem seguir links e, para cada entrada, chama `FilePolicy::decide`, a mesma função do supervisor;
4. acrescenta à visão um `Mount::Cover` (que já existe) para cada arquivo com veredito de segredo ou de negação e para cada pasta negada, sem descer nela.

- **Links simbólicos são pulados:** não dá para montar por cima de um link, e o alvo, se estiver na pasta, é decidido pelo próprio nome.
- **Destino que sumiu:** entre a varredura e a montagem um arquivo pode ser apagado. O lançador pula a cobertura cujo destino não existe, em vez de falhar o lançamento.
- **Teto de 4096:** cada cobertura é uma montagem, copiada a cada contêiner criado. Acima do teto o Docker não sobe (falha fechada), com a mensagem da spec.
- **Só a configuração da pasta:** o Docker é da pasta, e as sessões podem ter camadas de aba diferentes. É a mesma regra das pastas extras e dos tetos.
- **Alternativa descartada:** refazer as coberturas ao vivo. Exigiria montar dentro de um Docker rodando e não alcançaria os contêineres já criados.

### 6. O limite fica escrito

`docs/docker.md` passa a dizer, na seção de limites, que as coberturas e a proteção do `.git` na visão do Docker valem para contêineres comuns e caem para um contêiner privilegiado, e que os segredos criados depois não são cobertos. `docs/files.md` aponta para lá. O roadmap registra o achado e a decisão em aberto de trancar a visão.

## Risks / Trade-offs

- [O paliativo passa uma sensação de proteção completa] → O limite vai para a documentação, e o roadmap mantém a lacuna aberta até o change 70 ou até a visão ser trancada.
- [Um contêiner privilegiado desfaz a proteção do `.git` e grava um hook, que roda no host quando o usuário usa o git] → Já é assim hoje. Este change não piora nem fecha; registra.
- [Contêineres que usavam dispositivos do host deixam de funcionar: `/dev/net/tun` (VPN), `/dev/fuse`, câmera, áudio] → É o objetivo. A GPU e o KVM continuam, pela configuração da pasta.
- [O `dockerd` cai para `fuse-overlayfs` sem `/dev/fuse`] → O kernel exigido (6.12 ou mais novo) monta overlay em user namespace. A primeira tarefa confere o driver em uso.
- [A varredura atrasa a subida do Docker em pastas grandes] → Uma passada por lançamento do Docker, sem abrir arquivos. A tarefa da varredura mede uma pasta com `node_modules`.
- [O `runc` ou o `dockerd` se comporta diferente com o chaveiro negado, o cgroup namespace ou o `/dev` mínimo] → A primeira tarefa é uma prova com o Docker real, e cada decisão tem o recuo escrito.
- [Os testes com o Docker real pesam na máquina] → Rodam sob o vigia de memória (`vigia.sh`), com `CARGO_BUILD_JOBS=16`, como os testes atuais de `docker.rs`.

## Migration Plan

- Sem mudança de protocolo nem de configuração.
- O Docker privado de uma pasta recebe a visão nova quando sobe de novo: depois de `sandmux daemon restart`, ou quando a última sessão da pasta termina.
- Para voltar atrás, basta a versão anterior; imagens e volumes não mudam de formato.
