## Why

O Ubuntu 24.04 é a LTS mais usada, e hoje o sandmux não sobe nele com o kernel original (6.8). O lançador exige o Landlock ABI 6 (kernel 6.12), o primeiro com o escopo de sockets abstratos e de sinais. É o terceiro item que o usuário pediu na etapa 15, e a lacuna "Ubuntu 24.04" da seção 5 do roadmap.

Achados do agente ao propor (2026-11-20):

- O kernel 6.8 do 24.04 tem o Landlock ABI 4. Faltam a restrição de `ioctl` em dispositivos (ABI 5) e os escopos de sockets abstratos e de sinais (ABI 6).
- O kernel HWE do 24.04 (`linux-generic-hwe-24.04`, 6.14 desde o 24.04.3) já tem o ABI 6, e deve rodar o sandmux como ele é hoje. Isso nunca foi testado.
- O BPF LSM vem compilado no kernel do 24.04 (`CONFIG_BPF_LSM=y`), mas fica desligado: a lista padrão é `CONFIG_LSM="landlock,lockdown,yama,integrity,apparmor"`, sem `bpf`. A informação vem da configuração do kernel do Ubuntu (`debian.master/config/annotations` do noble), e o kernel 6.11 é igual. Para ligar, o usuário põe `bpf` no parâmetro `lsm=` do boot e reinicia.

Decisões do usuário (2026-11-20), ao propor:

- os dois caminhos: o `sandmux doctor` e a instalação levam o 24.04 ao kernel HWE, e no 6.8 o que falta ao Landlock vem do BPF LSM, pelo auxiliar root;
- o teste num 24.04 de verdade: o agente baixa a imagem de nuvem oficial e roda uma VM com QEMU/KVM, no kernel 6.8 e no HWE.

## What Changes

- **Lançador:** com o Landlock ABI 4 ou 5, o lançador usa a maior ABI que o kernel oferece e pede ao auxiliar root o que falta, antes de o comando rodar. Sem o auxiliar, ou sem o `bpf` entre os LSMs ativos, o lançamento continua recusado, com a mensagem dizendo o que falta. Com o ABI 6, nada muda.
- **Auxiliar root:** um pedido novo, `scope`, com o user namespace do sandbox. O auxiliar carrega programas BPF LSM nos ganchos de conexão por socket Unix, de sinal e de `ioctl`, e eles negam a um processo de um sandbox registrado:
  - a conexão a um socket abstrato criado fora do sandbox;
  - um sinal a um processo de fora do sandbox;
  - um `ioctl` num dispositivo fora da lista do sandbox, com as mesmas exceções do Landlock.

  O registro dura enquanto a conexão do pedido está aberta, como os outros pedidos.
- **`sandmux doctor`:** no Ubuntu 24.04 com o kernel abaixo do 6.12, recomenda o kernel HWE e mostra o outro caminho: o auxiliar (`sandmux setup`) e o `bpf` no `lsm=` do GRUB, com os comandos.
- **Documentação:** o Ubuntu 24.04 entra nas distros testadas, com o kernel HWE ou com o 6.8 pelo auxiliar.

Decisões do agente:

- O `sandmux setup` não muda o GRUB. Trocar parâmetros do boot é uma mudança do sistema que o usuário faz, e o `doctor` mostra os comandos.
- Os programas desse pedido são BPF LSM globais (`BPF_LSM_MAC`), e não presos a um cgroup. Uma aba sem os tetos de recursos roda no cgroup do terminal, junto com outros processos do usuário. Por isso o filtro é o user namespace do sandbox, que cada lançamento cria.
- A restrição de `ioctl` em dispositivos (ABI 5) também vem do BPF no 6.8, para não ficar abaixo do que o Landlock garante nos kernels novos.

Limites, escritos na proposta:

- No 6.8, a aba só sobe com o auxiliar instalado e o `bpf` ligado no boot.
- O Docker privado no 6.8 depende da mesma verificação do lançador da visão. A VM mostra se ele sobe.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `sandbox-isolation`: a falha fechada sem suporte do kernel aceita o Landlock ABI 4 ou 5 quando o auxiliar root completa os escopos e a restrição de `ioctl`.
- `root-helper`: o pedido novo `scope`, que confina o user namespace de um sandbox.
- `installation`: o `sandmux doctor` no Ubuntu 24.04 com o kernel 6.8.

## Impact

- `crates/sandmux-ebpf`: os programas BPF LSM dos ganchos `unix_stream_connect`, `unix_may_send`, `task_kill` e `file_ioctl`, e o mapa dos user namespaces registrados.
- `crates/sandmux/src/helper.rs`: o pedido `scope`, a carga dos programas no primeiro registro e a remoção do registro quando a conexão fecha.
- `crates/sandmux-launcher`: a escolha da ABI do Landlock, a conexão com o auxiliar e o registro antes de liberar o filho.
- `crates/sandmux-launcher/src/support.rs` e `crates/sandmux/src/doctor.rs`: o diagnóstico novo.
- `docs/install.md`, `docs/helper.md`, `README.md` e os pares em inglês.
- Um roteiro de teste com uma VM do Ubuntu 24.04 (`scripts/`).
