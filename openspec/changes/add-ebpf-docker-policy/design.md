## Context

Motivação em `proposal.md`. O que importa para o desenho:

- **Como o Docker privado sobe** (`daemon/docker.rs`), desde o change 69:
  - `systemd-run --user --scope` na slice `sandmux-docker-<id>.slice` → `rootlesskit --net=none --pidns --cgroupns --evacuate-cgroup2=motor` → `sandmux-run --view-file` (`exec_view`) → `dockerd`.
  - Os contêineres nascem em cgroups abaixo do escopo, dentro da slice.
  - Sem o systemd de usuário não há slice, e o Docker roda no cgroup do daemon.
- **O que a visão protege por montagem** (`compose_docker_view`, `git::protect`):
  - as coberturas (`Mount::Cover`) dos segredos, do `file-deny` e de `/etc`;
  - binds em somente leitura de `hooks`, `config` e `config.worktree` de cada diretório git (repositório e submódulos).

  Tudo isso cai para um contêiner com `CAP_SYS_ADMIN` no user namespace do `rootlesskit`.
- **A máquina de referência** (kernel 7.0, 2026-11-17):
  - `bpf` está na lista de LSMs ativos (`/sys/kernel/security/lsm`), com BTF em `/sys/kernel/btf/vmlinux`.
  - O BPF sem privilégio está desligado (`unprivileged_bpf_disabled = 2`).
  - O BTF tem os ganchos `bpf_lsm_file_open`, `inode_create`, `inode_link`, `inode_unlink`, `inode_symlink`, `inode_mkdir`, `inode_rmdir`, `inode_mknod`, `inode_rename` e `inode_setattr`.
  - Não há `clang` nem toolchain nightly do Rust.
- **O `sudo` pede senha.** Os testes que exigem root rodam por um script fixo que o usuário libera no `sudoers`, por decisão dele.

## Goals / Non-Goals

**Goals:**

- Fechar, onde há BPF LSM, o que as montagens não garantem contra um contêiner privilegiado: os segredos e o `file-deny` que existem quando o Docker sobe, a proteção do `.git` e o `.git/commondir`.
- Nenhum efeito fora do cgroup do Docker da pasta.
- Nenhuma dependência nova de toolchain para compilar o sandmux, e binários ainda estáticos (musl).
- O auxiliar root com a menor superfície possível: ele não abre caminhos, não lê configuração do usuário e só entende dois pedidos e uma consulta.

**Non-Goals:**

- O registro de arquivos dos contêineres, os segredos criados depois que o Docker sobe e a política ao vivo (change 71).
- Regras de montagem. Com a decisão pelo inode, uma montagem não abre caminho até um arquivo marcado.
- O Docker sem a slice dos tetos (máquina sem o systemd de usuário). Ele fica no nível do change 69, com o aviso. Criar um cgroup pelo auxiliar fica para o item dos tetos sem systemd.
- Esconder o nome de um arquivo negado no `ls`. É o mesmo limite do sandbox.

## Decisions

### 1. Um serviço root ativado por socket

O `sandmux-helper` roda pelas unidades `sandmux-helper.socket` e `sandmux-helper.service` do systemd do sistema.

- **Socket:** `/run/sandmux-helper.sock`, com o modo 0666. Quem pode pedir o quê é decidido pelo `SO_PEERCRED` de cada conexão.
- **Unidade endurecida:**
  - `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=yes` e `PrivateTmp`;
  - `CapabilityBoundingSet` só com o que carregar e prender programas LSM exige (`CAP_BPF`, `CAP_PERFMON`, `CAP_MAC_ADMIN`, `CAP_NET_ADMIN`, `CAP_SYS_ADMIN`, `CAP_SYS_RESOURCE`). A lista exata sai da tarefa de verificação.
- **Para os testes:** `--socket <caminho>` sobe o auxiliar sem o systemd. No daemon, a variável `SANDMUX_HELPER_SOCKET` troca o caminho.
- **Alternativa descartada:** o auxiliar só no `sandmux setup`, prendendo programas e mapas no bpffs para o daemon escrever direto. Escrever num mapa preso pede só a permissão do arquivo no bpffs, mas aí um usuário marcaria regras no cgroup de outro, e criar mapas por usuário exige `CAP_BPF`.

### 2. Protocolo

As mensagens são linhas JSON numa conexão `SOCK_STREAM`. Os descritores vão junto, por `SCM_RIGHTS`, na mesma mensagem da linha. Cada pedido recebe uma resposta `{"ok": true, ...}` ou `{"ok": false, "error": "..."}`.

- `{"op": "status"}` responde a versão do auxiliar e se o BPF LSM está ativo. Não precisa de descritor.
- `{"op": "confine"}` traz um descritor do diretório do cgroup.
  - O auxiliar confere com `fstatfs` que é um cgroup2, e com `fstat` que o dono é o uid do `SO_PEERCRED` (o root pode qualquer um).
  - Depois carrega os programas e os prende ao cgroup.
  - Vale um por conexão.
- `{"op": "mark", "kind": "deny" | "protect" | "gitdir"}` traz até 200 descritores (`O_PATH`), e só vale depois do `confine`. Quem pede prova o acesso ao arquivo abrindo-o ele mesmo; o auxiliar nunca abre um caminho.

As linhas têm teto de 4 KiB. Uma linha inválida recebe erro e não derruba a conexão; uma conexão que manda lixo demais é fechada.

### 3. Programas presos ao cgroup (`BPF_LSM_CGROUP`)

Cada confinamento carrega o próprio conjunto de programas, com o tipo `BPF_PROG_TYPE_LSM` e o anexo `BPF_LSM_CGROUP`, presos ao diretório do cgroup da slice por `BPF_LINK_CREATE`.

- Rodam só para processos daquele cgroup e dos de baixo, que são o escopo, o `motor` e os contêineres. Não custam nada ao resto da máquina, e a decisão "isto é o Docker da pasta?" é do kernel.
- Os links ficam com o auxiliar. Fechar o descritor solta o programa.
- Um contêiner não tira um programa de cgroup: o `BPF_PROG_DETACH` exige capacidades no user namespace inicial.
- **Recuo, se a verificação mostrar que o `BPF_LSM_CGROUP` não serve (por um auxiliar que falta ou um gancho recusado):** anexo global (`BPF_LSM_MAC`). Cada programa começa comparando `bpf_get_current_ancestor_cgroup_id(nível)` com o id da slice, gravados como constantes na hora da carga.

### 4. Decisão pelo inode, com armazenamento por inode

Cada confinamento tem um mapa `BPF_MAP_TYPE_INODE_STORAGE` só dele. O valor é um conjunto de bits: `DENY`, `PROTECT` e `GITDIR`.

- **Marcar:** do espaço de usuário, a chave desse mapa é um descritor. O auxiliar grava a marca pelo descritor que recebeu e o mantém aberto, porque o armazenamento some quando o inode sai da memória.
- **Por que o inode:** desmontar, tirar o somente-leitura, renomear, criar um hard link ou montar em outro lugar não mudam o inode.
- **Alternativa descartada:** um mapa de hash com `(dispositivo, inode)`. No btrfs, o `stat` devolve o dispositivo do subvolume, e não o do superbloco que o kernel vê, e os números de inode se repetem entre subvolumes.
- **Mapa por confinamento:** um usuário não ocupa nem apaga as marcas de outro, e não há disputa de vagas num valor comum.

### 5. O que cada programa decide

`deny(x)`, `protect(x)` e `gitdir(x)` dizem se o inode `x` tem a marca. `acima(x, M)` diz se `x` ou algum diretório acima dele, seguindo `d_parent` até a raiz do sistema de arquivos, tem a marca `M`.

O `d_parent` segue a árvore real do sistema de arquivos, e não as montagens, por isso um bind ou uma cobertura desmontada não muda o resultado.

O laço tem teto de 64 níveis. Um arquivo mais fundo que isso é negado, por segurança. Na prática, só um caminho absurdo passa de 64 componentes.

| Gancho | Nega quando |
|---|---|
| `file_open` | `acima(arquivo, DENY)`; ou aberto com escrita e `acima(arquivo, PROTECT)` |
| `inode_create`, `inode_mkdir`, `inode_mknod`, `inode_symlink` | `acima(dir, DENY)` ou `acima(dir, PROTECT)`; ou `gitdir(dir)` e o nome é `commondir`, `hooks`, `config` ou `config.worktree` |
| `inode_link` | a origem tem `DENY` ou `PROTECT` por `acima`; ou o destino cai na regra de criação |
| `inode_unlink`, `inode_rmdir` | `acima(alvo, DENY ou PROTECT)` |
| `inode_rename` | a origem tem `acima(DENY ou PROTECT)`; o diretório de destino cai na regra de criação; ou o alvo substituído tem `acima(DENY ou PROTECT)` |
| `inode_setattr` | `acima(alvo, DENY ou PROTECT)` |

Mais duas regras:

- **Nomes:** o nome vem do `d_name` da `dentry` nova, comparado como bytes com os quatro nomes.
- **Retorno:** no `BPF_LSM_CGROUP`, o programa devolve 1 para seguir e 0 para negar; o kernel transforma o 0 em `EPERM`.

### 6. Instruções montadas em Rust, com o BTF lido na carga

O crate novo `sandmux-ebpf` tem três partes:

- **Leitor do BTF:** lê `/sys/kernel/btf/vmlinux` e acha:
  - o id de cada `bpf_lsm_<gancho>` (o `attach_btf_id`);
  - a posição de cada parâmetro pelo nome, no `FUNC_PROTO`;
  - os deslocamentos dos campos usados (`file.f_inode`, `file.f_mode`, `file.f_path.dentry`, `dentry.d_parent`, `dentry.d_inode`, `dentry.d_name.name`, `dentry.d_name.len`).

  Cumpre o papel do CO-RE sem o `clang`.
- **Montador:** escreve as instruções BPF (cerca de 15 opcodes e desvios com rótulos). Segue o mesmo estilo do filtro seccomp montado à mão em `sandmux-launcher/src/seccomp.rs`.
- **Chamadas:** a chamada `bpf()` por `libc::syscall`, para `MAP_CREATE`, `MAP_UPDATE_ELEM`, `PROG_LOAD` (com o log do verificador devolvido no erro) e `LINK_CREATE`.

Alternativas descartadas:

- **`aya-ebpf`** (programas em Rust): exige o toolchain nightly e o `bpf-linker`, que liga ao LLVM.
- **C com `clang` e `libbpf` (ou o carregador do `aya`):** exige o `clang` para compilar o sandmux a partir do código. Commitar o objeto compilado tira essa exigência, mas deixa um binário no repositório que não se confere de forma reprodutível.

Os ponteiros lidos pela árvore (`d_parent`, `d_inode`) chegam ao `bpf_inode_storage_get`. Se o verificador recusar um ponteiro obtido assim, o recuo é este:

- o daemon marca também cada arquivo e pasta que existe dentro das pastas negadas, pela varredura que ele já faz;
- o `acima()` olha só o próprio inode e o diretório pai direto.

O recuo perde "arquivo criado depois numa pasta negada". Se ele for usado, o spec e o roadmap passam a registrar isso.

### 7. O daemon

Depois que o socket do Docker responde, e antes de entregar o socket à sessão, `start()` faz isto:

1. Com a slice, abre o diretório do cgroup dela e pede o `confine`.
2. Abre com `O_PATH`, como o usuário, cada caminho marcado e manda os `mark` em lotes de 200:
   - **`DENY`:** as coberturas (`Mount::Cover`) da visão cujo arquivo existe no host.
   - **`PROTECT`:** os binds em somente leitura da visão cuja origem fica dentro da pasta de trabalho. São os `hooks`, `config` e `config.worktree` de `git::protect`.
   - **`GITDIR`:** cada diretório git devolvido por `git::protect`.
3. Guarda a conexão no `Engine`. Uma thread espera o fim da conexão; se ela fechar antes de o Docker parar, a thread para o Docker da pasta e grava o motivo no log.

Se o `status` ou o `confine` falharem, o Docker segue sem o confinamento, e o motivo fica no `Engine`. Se um `mark` falhar depois do `confine`, o Docker é parado e a subida falha com o erro. Meia regra não vale como regra.

As marcas saem da especificação da visão, e não de uma segunda varredura. Assim, o que a montagem cobre e o que o kernel nega são sempre a mesma lista.

### 8. Onde o estado aparece

- **`DockerStats`** ganha `ebpf: Option<String>`: `None` quando o confinamento vale, ou o motivo quando não vale. A versão do protocolo sobe.
- **`sandmux stats`** e a seção do Docker do painel da TUI mostram uma linha "Confinamento eBPF: ligado" ou "desligado: <motivo>".
- **`sandmux doctor`** ganha a verificação "Confinamento do Docker por eBPF". Ela lê o `bpf` em `/sys/kernel/security/lsm`, pede o `status` ao auxiliar e diz como instalar o que falta: o `sandmux setup` com `sudo`, ou o parâmetro `lsm=` do kernel.

### 9. Instalação

- **`sandmux setup`:** grava as duas unidades, com o `ExecStart` apontando para o `sandmux-helper` ao lado do `sandmux`, roda `systemctl daemon-reload` e `systemctl enable --now sandmux-helper.socket`, tudo pelo mesmo `sudo` do perfil do AppArmor.
  - Antes, confere o binário e cada pasta acima dele: o dono é o root, e nenhum é gravável pelo grupo ou pelos outros.
  - Um binário em `~/.local/bin` daria root ao usuário pelo serviço, e a instalação é recusada com o motivo.
- **Pacotes:** o `.deb`, o `.rpm` e o `PKGBUILD` levam as unidades em `/usr/lib/systemd/system`, com o caminho `/usr/bin/sandmux-helper`, e ligam o socket na instalação. O Homebrew não leva as unidades.
- **`install.sh --uninstall`:** para e desliga o socket e o serviço e remove as unidades.

### 10. Testes com root

`scripts/check-root.sh` é o único comando que o usuário libera no `sudoers`, de forma temporária.

1. Confere que roda como root e que `SUDO_UID` existe.
2. Sobe o `target/debug/sandmux-helper --socket <temporário>` como root.
3. Roda `cargo test -p sandmux --test docker_ebpf` como o usuário de `SUDO_UID` (`setpriv`), com `XDG_RUNTIME_DIR`, `HOME`, o barramento do usuário e `SANDMUX_HELPER_SOCKET`.
4. Para o auxiliar e devolve o código dos testes.

Os testes sem root ficam no `check.sh`:

- o leitor do BTF contra o `vmlinux` da máquina;
- o montador;
- o protocolo e a autorização do auxiliar, com uma carga falsa;
- a recusa do setup a um binário gravável;
- a lista de marcas tirada da visão.

O script inteiro roda dentro do vigia de memória.

## Risks / Trade-offs

- **[Verificador recusa ponteiros da árvore no armazenamento por inode]** → recuo da decisão 6, registrado no spec.
- **[O `BPF_LSM_CGROUP` sem algum gancho ou auxiliar]** → recuo da decisão 3, com o anexo global e o id do cgroup como constante.
- **[Código root lendo pedidos de qualquer usuário]** → protocolo de três operações, teto de tamanho, nenhum caminho aberto pelo auxiliar, limites por usuário, unidade endurecida.
- **[Um binário do usuário apontado pela unidade]** → o setup confere o dono e as permissões do caminho inteiro.
- **[`sudo` sem senha para o script de testes]** → equivale a root para quem roda como o usuário, porque o script roda binários compilados por ele. O usuário escolheu assim, de forma temporária. O roadmap lembra de tirar a linha do `sudoers` quando o change acabar.
- **[Custo em cada abertura dos processos do Docker]** → até 64 consultas ao mapa por abertura, só no cgroup do Docker. É medido numa tarefa com um `docker build` grande; se pesar, o teto de níveis cai.
- **[Descritores `O_PATH` mantidos pelo auxiliar]** → impedem desmontar o sistema de arquivos da pasta enquanto o Docker roda. O limite de 16384 marcas por confinamento segura o total.
- **[Segredo criado com o Docker rodando]** → continua visível até o Docker subir de novo, como no change 69, até o change 71.
- **[Arquivo a mais de 64 níveis de profundidade]** → negado ao Docker. Documentado.

## Migration Plan

- O protocolo entre o cliente e o daemon muda (o `DockerStats`): `sandmux daemon restart` depois de atualizar.
- O `sandmux setup` instala o auxiliar numa instalação existente com `sudo`. Sem ele, nada muda além do aviso.
- Para voltar atrás: `systemctl disable --now sandmux-helper.socket`. O Docker volta ao nível do change 69 na subida seguinte.

## Open Questions

- A lista mínima de capacidades da unidade do serviço. Sai da verificação com root e não muda o comportamento.
