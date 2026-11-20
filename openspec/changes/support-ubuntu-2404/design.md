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
- **Sinal** (`task_kill(p, info, sig, cred)`): se o processo atual está em S, o sinal só passa quando o alvo `p` também está em S ou abaixo e não é o processo 1 do sandbox. O sinal 0 também é negado, como no Landlock: ele revela se o processo existe. O processo 1 (a cópia do supervisor) está no user namespace do sandbox, mas fora do domínio do Landlock: o pedido leva o pid dele no host, e o programa o compara com o `tgid` do alvo (a suíte de fuga pegou esse caso na VM). Os auxiliares do lançador que entram no user namespace (nomes, montagem, conexão) não entram no namespace de pid do sandbox: o agente não os alcança por pid. O argumento `cred` só existe nos sinais de E/S assíncrona do USB, que passam. A notificação de E/S (`file_send_sigiotask`) fica de fora, como no Landlock com o dono do arquivo dentro do sandbox.
- **Argumentos que podem ser nulos:** o verificador trata os argumentos dos ganchos como ponteiros que nunca são nulos e apaga a conferência `!= 0` deles. O `cred` do `task_kill` é nulo em quase todo `kill()`, e a leitura de `cred->user_ns` derrubou o kernel do host duas vezes (no 7.0) e o da VM (no 6.8, com o *oops* no `dmesg`). Esse argumento é lido do contexto por `bpf_probe_read_kernel`, como número. Os campos alcançados por ponteiros sem a marca de confiável (`user_namespace.parent`, `sock.sk_socket`) são lidos com a proteção do verificador, e as conferências de nulo deles valem.
- **`ioctl` em dispositivo** (`file_ioctl(file, cmd, arg)`), só no ABI 4: se o processo atual está em S e o inode do arquivo é de caractere ou de bloco, o `ioctl` só passa quando o número do dispositivo (`i_rdev`) está na lista do pedido ou quando o comando está na lista fixa do Landlock (`FIOCLEX`, `FIONCLEX`, `FIONBIO`, `FIOASYNC`, `FIOQSIZE`, `FIFREEZE`, `FITHAW`, `FS_IOC_FIEMAP`, `FIGETBSZ`, `FIONREAD`, `FS_IOC_GETFLAGS` e afins). O `ioctl` de 32 bits (`file_ioctl_compat`, 6.10+) não existe no 6.8. No 6.8, o `compat_ioctl` também passa por `file_ioctl`.

A negação é `-EPERM` nos ganchos de socket e de sinal, e `-EACCES` no `ioctl`, como no Landlock.

**A lista de dispositivos** é a dos nós que o lançador monta no `/dev` do sandbox. O Landlock libera o `/dev` inteiro para escrita, `IOCTL_DEV` incluído, e a restrição do ABI 5 só pega os dispositivos de fora dele. O lançador lê o `st_rdev` de cada nó antes do pedido. Também entram:
- os terminais do `devpts` do sandbox, pelo número principal (136 a 143), o `tty` (5:0) e o `ptmx` (5:2);
- o TUN (10:200), que o filho configura antes de o comando rodar. O BPF vale desde o registro, e o Landlock só nos arquivos abertos depois das regras. O sandbox não abre o TUN, que não está no `/dev` dele;
- os consoles de texto (número principal 4), que chegam como a entrada herdada de um `sandmux run` num console.

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

Os programas BPF deste change só rodam na VM. No host, o primeiro teste com root derrubou a máquina do usuário duas vezes (decisão 4, os argumentos que podem ser nulos).

O roteiro `scripts/vm-ubuntu-2404.sh`:
- baixa a imagem de nuvem oficial do noble para `~/.cache/sandmux-vm/`, fora do repositório, e confere a soma pelo `SHA256SUMS` do mesmo lugar, por https;
- sobe a VM com QEMU/KVM e cloud-init (4 GB, 4 CPUs, ssh numa porta local), com um vigia que desliga a VM se a memória disponível do host cair abaixo de 6 GB;
- tem os passos `lsm-bpf` (o `bpf` no `lsm=` do GRUB, como o `doctor` manda) e `hwe` (o kernel HWE pelo `apt`), e `reset` para voltar ao disco da imagem.

Os testes vão para a VM como binários estáticos (musl), no mesmo caminho do host, porque os testes acham os binários pelo caminho da compilação. O fluxo do usuário também roda lá: o `.deb` do `scripts/dist.sh`, o `doctor`, o `lsm=`, o `sandmux setup` e um `sandmux run`.

### 8. O Docker privado no 24.04

A VM mostrou dois problemas, nenhum do escopo:

- **O `rootlesskit` 1.1.1 do Ubuntu** não tem a rede `none` (`unknown network mode: none`). O Docker privado usa essa rede porque o tráfego passa pelo gateway do sandmux. O `docker::support` lê a ajuda do `rootlesskit` (`--help`, a linha do `--net`) e, sem `none`, diz que falta o 2.0 ou mais novo, com o `docker-ce-rootless-extras` do repositório do Docker como solução. Com ele (o `rootlesskit` 3.3), o Docker privado sobe no 24.04.
- **O `O_PATH` no musl:** o `OpenOptions::custom_flags` do Rust tira os bits de `O_ACCMODE`, e no musl esse valor inclui o `O_PATH` (`O_SEARCH`). As marcas eram abertas para leitura, e uma cobertura que o usuário não lê (o `/etc/gshadow`) dava `EACCES` e parava o Docker confinado. Afeta os binários estáticos das releases em qualquer distro, desde o change 70. Os testes no host usam a glibc e não viam o erro. A abertura passa a ser pelo `open` direto (`helper::open_path`), assim como o `open_proc` do lançador. O `PathFd` do crate `landlock` tem o mesmo padrão: no musl, um caminho da allowlist que o usuário não lê é pulado, o que só tira acesso.

## Risks / Trade-offs

- **[Ponteiros que o verificador não aceita no 6.8]** → o caminho até o user namespace e até `f_cred` passa por `bpf_probe_read_kernel`, que o LSM permite. Um teste de montagem de cada programa roda no kernel desta máquina, e a VM confere a carga no 6.8.
- **[A lista fixa de comandos de `ioctl` do Landlock muda entre versões]** → a lista copiada é a do kernel 6.12, que define o ABI 5, e fica num teste unitário.
- **[O usuário liga o `bpf` mas esquece o auxiliar, ou o contrário]** → o lançamento falha fechado, e o `doctor` diz qual dos dois falta.
- **[Programas globais custam em todo processo da máquina]** → os ganchos são raros (conexão Unix, sinal, `ioctl`), e a primeira coisa de cada programa é procurar o namespace atual no mapa. O custo é medido na VM com um laço de `ioctl` num terminal.
- **[A imagem de nuvem muda]** → o roteiro usa a imagem `current` e confere a soma. O kernel HWE vem do `apt`, dentro da VM. Em 2026-11-20 ele era o 7.0, o mesmo do host.
- **[Um processo do sandbox fora do domínio do Landlock]** → o processo 1 é protegido pelo pid. O shell de Ctrl+b `t` entra no sandbox com outro domínio do Landlock, e no ABI 6 o agente não o sinaliza; pelo BPF, sinaliza (o mesmo user namespace). É o shell do próprio usuário dentro do mesmo sandbox.

## Migration Plan

- O lançador, o auxiliar e o daemon mudam juntos. A versão do protocolo do daemon não muda. O pedido novo é do auxiliar, que diz a versão no `status`. Um lançador novo com um auxiliar velho recebe "pedido desconhecido" e falha fechado no 6.8, com a mensagem de atualizar o auxiliar (`sandmux setup`).
- Nos kernels com o ABI 6, nada muda para o usuário.
