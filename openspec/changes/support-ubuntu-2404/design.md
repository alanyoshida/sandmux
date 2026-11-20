## Context

Motivação e achados em `proposal.md`. O que importa para o desenho:

- **O lançador (`sandmux-run`)** confere o kernel em `check_kernel()`, antes de qualquer montagem, e exige o Landlock ABI 6 (`landlock_rules::REQUIRED_ABI`). As regras são aplicadas com `CompatLevel::HardRequirement` e `ABI::V6`: os direitos de arquivo, o `IOCTL_DEV` (ABI 5) e os dois escopos (ABI 6).
- **O filho do lançador** espera no `sync_r` até o lançador escrever os mapas de uid e liberar (`sync_w`). Só depois ele monta a raiz, aplica o Landlock e o seccomp e executa o comando. Cada lançamento cria um user namespace novo (`CLONE_NEWUSER`), e também namespaces de pid, de montagem, de IPC e de UTS. O namespace de rede só existe quando há rede (`spec.network`).
- **O auxiliar root** atende pedidos em JSON por um socket Unix (`/run/sandmux-helper.sock`), com descritores por `SCM_RIGHTS`, e solta tudo de um pedido quando a conexão fecha. Hoje os programas dele são presos a um cgroup (`BPF_LSM_CGROUP` no confinamento do Docker, `tp_btf` e `fexit` filtrados pelo id do cgroup na observação).
- **A montagem do BPF** é feita à mão, sem clang (`crates/sandmux-ebpf`: `asm.rs`, deslocamentos pelo BTF do kernel, chamadas `bpf()` diretas). O verificador só aceita ponteiros confiáveis dos argumentos do gancho e dos campos que o BTF marca como confiáveis. O resto se lê com `bpf_probe_read_kernel`.
- **No Ubuntu 24.04 com o kernel 6.8**, o Landlock está no ABI 4, e o BPF LSM está compilado mas fora da lista padrão de LSMs (proposta).

## Goals / Non-Goals

**Goals:**

- No 6.8 com o auxiliar e o `bpf` ligado, a mesma garantia do ABI 6: sockets abstratos, sinais e `ioctl` em dispositivos.
- Falha fechada: sem o auxiliar ou sem o BPF LSM, o 6.8 continua recusado, com a mensagem certa.
- Nada muda nos kernels com o ABI 6.
- O 24.04 testado de verdade numa VM, no kernel 6.8 e no HWE.

**Non-Goals:**

- Kernels abaixo do 6.8, ou sem o Landlock ABI 4.
- Mudar o boot pelo `sandmux setup`.
- Usar o caminho pelo auxiliar quando o kernel tem o ABI 6, mesmo com o auxiliar instalado.

## Decisions

### 1. A ABI do Landlock escolhida pelo kernel

`check_kernel()` passa a devolver o nível:

- **ABI 6 ou mais:** como hoje.
- **ABI 4 ou 5:** o lançador precisa do auxiliar. Ele usa a ABI do kernel em `landlock_rules::apply` (`ABI::V4` ou `ABI::V5`), ainda com `HardRequirement`, e o auxiliar completa o que falta. Com o ABI 5, o `IOCTL_DEV` fica no Landlock e o auxiliar dá só os escopos.
- **Abaixo do 4, ou sem Landlock:** recusa, como hoje.

A variável de teste `SANDMUX_RUN_REQUIRE_LANDLOCK_ABI` continua só endurecendo. Uma variável nova, só para testes, faz o lançador agir como num kernel com uma ABI menor (`SANDMUX_RUN_LANDLOCK_ABI`, que só pode baixar a ABI usada). Assim o caminho do auxiliar é testado com root nesta máquina, com o kernel 7.0.

Alternativa descartada: pedir o auxiliar sempre que ele existe. Isso juntaria dois mecanismos onde um basta, e o Landlock é o que vale sem root.

### 2. O registro antes de liberar o filho

Com o ABI 4 ou 5, o lançador abre a conexão com o auxiliar depois do `clone` e antes de escrever no `sync_w`:

1. abre `/proc/<filho>/ns/user`;
2. manda `{"op":"scope","ioctl_dev":[...],"ioctl":<bool>}` com o descritor do namespace;
3. espera a resposta. Com erro, mata o filho e termina com `LaunchError::Unsupported`, com a mensagem do auxiliar e a indicação do `sandmux doctor`.

O lançador guarda a conexão aberta até o fim do sandbox. O lançador é o supervisor e vive tanto quanto o sandbox. Quando ele morre, a conexão fecha, e o auxiliar tira o namespace do registro.

O cliente do auxiliar no lançador é pequeno (uma linha JSON e um descritor), no próprio `sandmux-launcher`. O lançador não depende do crate `sandmux`.

### 3. Programas globais filtrados pelo user namespace

Os programas são `BPF_LSM_MAC` (ligados por `BPF_LINK_CREATE` ou `raw_tracepoint_open`), carregados no primeiro `scope` e soltos quando o registro fica vazio. Um mapa `HASH` guarda os inums dos user namespaces registrados, com o valor das opções do pedido (o `ioctl` e a lista de dispositivos).

**Quem está num sandbox:** um processo está num sandbox registrado quando o user namespace dele, ou um ancestral até 8 níveis (`user_namespace.parent`), está no mapa. Os ancestrais cobrem um user namespace aninhado dentro do sandbox. O namespace vem de `bpf_get_current_task_btf()`, `task->cred->user_ns->ns.inum`. Se o verificador não aceitar o caminho como confiável, a leitura é por `bpf_probe_read_kernel`.

**Por que não o cgroup:** sem os tetos de recursos (sem o systemd de usuário delegado), a aba roda no cgroup de quem a chamou, junto com outros processos do usuário. O user namespace é criado por lançamento e só o sandbox está nele.

**Por que não o namespace de pid ou de rede:** o de rede não existe nas abas sem rede. Um processo pode trocar de namespace de pid só para os filhos, e o user namespace já é a fronteira do Landlock do sandbox.

### 4. Os ganchos

- **Socket abstrato** (`unix_stream_connect(sock, other, newsk)` e `unix_may_send(sock, other)`): se o processo atual está num sandbox S e o endereço de `other` é abstrato (`unix_sock.addr` com `sun_path[0] == 0`), o socket só passa quando quem o criou também está em S. A origem do socket é o user namespace das credenciais do arquivo do socket (`other->sk_socket->file->f_cred`). O mesmo teste do processo atual vale para esse namespace, que precisa ser S ou um descendente. Um socket por caminho passa: o caminho já é decidido pela visão de arquivos e pelo Landlock.
- **Sinal** (`task_kill(p, info, sig, cred)`): se o processo atual está em S, o sinal só passa quando o alvo `p` também está em S ou abaixo. O sinal 0 também é negado, como no Landlock: ele revela se o processo existe. A notificação de E/S (`file_send_sigiotask`) fica de fora, como no Landlock com o dono do arquivo dentro do sandbox.
- **`ioctl` em dispositivo** (`file_ioctl(file, cmd, arg)`), só no ABI 4: se o processo atual está em S e o inode do arquivo é de caractere ou de bloco, o `ioctl` só passa quando o número do dispositivo (`i_rdev`) está na lista do pedido ou quando o comando está na lista fixa do Landlock (`FIOCLEX`, `FIONCLEX`, `FIONBIO`, `FIOASYNC`, `FIOQSIZE`, `FIFREEZE`, `FITHAW`, `FS_IOC_FIEMAP`, `FIGETBSZ`, `FIONREAD`, `FS_IOC_GETFLAGS` e afins). O `ioctl` de 32 bits (`file_ioctl_compat`, 6.10+) não existe no 6.8. No 6.8, o `compat_ioctl` também passa por `file_ioctl`.

A negação é `-EPERM` nos ganchos de socket e de sinal, e `-EACCES` no `ioctl`, como no Landlock.

**A lista de dispositivos** é a dos nós que o lançador monta no `/dev` do sandbox e que o Landlock libera para escrita. O lançador lê o `st_rdev` de cada um antes do pedido. Os terminais do `devpts` do sandbox entram pelo número principal (136 a 143) e pelo `ptmx` (5:2).

### 5. O auxiliar

- **Identidade:** o auxiliar confere que o dono do user namespace (`ioctl(NS_GET_OWNER_UID)`) é o uid de `SO_PEERCRED`, e que o descritor é mesmo um user namespace (`NS_GET_NSTYPE`). Senão recusa.
- **Estado:** o pedido `status` passa a dizer se os programas do escopo carregam (o `bpf` ativo e os ganchos presentes no BTF).
- **Limites:** até 256 escopos por usuário. Uma sessão com muitas abas tem um escopo por aba.

### 6. O `doctor` e o `support`

`support.rs` ganha o estado do auxiliar e do BPF LSM (o `/sys/kernel/security/lsm`). `sandmux doctor` mostra "Landlock com escopo..." como:

- ok pelo Landlock (ABI 6);
- ok pelo auxiliar root (ABI 4 ou 5, o `bpf` ligado e o auxiliar instalado);
- falha, com os dois caminhos.

No Ubuntu 24.04 (pelo `/etc/os-release`), o primeiro caminho é `sudo apt install linux-generic-hwe-24.04` e reiniciar. Fora dele, é "um kernel 6.12 ou mais novo". O segundo caminho é:

- pôr o `bpf` no fim da lista atual (lida de `/sys/kernel/security/lsm`) em `GRUB_CMDLINE_LINUX_DEFAULT`, como `lsm=<lista atual>,bpf`;
- `sudo update-grub` e reiniciar;
- `sandmux setup` para o auxiliar.

### 7. O teste no 24.04

Um roteiro (`scripts/vm-ubuntu-2404.sh`) baixa a imagem de nuvem oficial do noble para a pasta de cache do usuário (`~/.cache/sandmux-vm/`, fora do repositório) e confere o `SHA256SUMS` assinado do mesmo lugar. Depois sobe a VM com QEMU/KVM e cloud-init, copia os pacotes `.deb` do `scripts/dist.sh` e roda dentro da VM:

1. no 6.8 sem o `bpf`: o `doctor` mostra os dois caminhos, e o lançamento é recusado;
2. no 6.8 com `lsm=...,bpf` e o auxiliar: a suíte de fuga do sandbox (socket abstrato, sinal, `ioctl`) e uma sessão com o Docker privado;
3. no HWE: o `doctor` limpo e a mesma suíte, sem o auxiliar.

A VM roda sob o vigia de memória, com 4 GB.

## Risks / Trade-offs

- **[Ponteiros que o verificador não aceita no 6.8]** → o caminho até o user namespace e até `f_cred` passa por `bpf_probe_read_kernel`, que o LSM permite. Um teste de montagem de cada programa roda no kernel desta máquina, e a VM confere a carga no 6.8.
- **[A lista fixa de comandos de `ioctl` do Landlock muda entre versões]** → a lista copiada é a do kernel 6.12, que define o ABI 5, e fica num teste unitário.
- **[O usuário liga o `bpf` mas esquece o auxiliar, ou o contrário]** → o lançamento falha fechado, e o `doctor` diz qual dos dois falta.
- **[Programas globais custam em todo processo da máquina]** → os ganchos são raros (conexão Unix, sinal, `ioctl`), e a primeira coisa de cada programa é procurar o namespace atual no mapa. O custo é medido na VM com um laço de `ioctl` num terminal.
- **[A imagem de nuvem muda]** → o roteiro usa a imagem `current` e confere a soma. O kernel HWE vem do `apt`, dentro da VM.

## Migration Plan

- O lançador, o auxiliar e o daemon mudam juntos. A versão do protocolo do daemon não muda. O pedido novo é do auxiliar, que diz a versão no `status`. Um lançador novo com um auxiliar velho recebe "pedido desconhecido" e falha fechado no 6.8, com a mensagem de atualizar o auxiliar (`sandmux setup`).
- Nos kernels com o ABI 6, nada muda para o usuário.
