# Auxiliar root (`sandmux-helper`)

> English: [helper.md](en/helper.md)

O `sandmux-helper` é o único pedaço do sandmux que roda como root. Ele existe para uma coisa: prender ao cgroup do [Docker privado](docker.md) de uma pasta os programas BPF LSM que decidem, no kernel, o que os contêineres podem abrir e alterar. Sem ele, o resto do sandmux funciona igual, e o Docker privado sobe só com as proteções sem privilégio. Num kernel com o Landlock abaixo do ABI 6 (o Ubuntu 24.04 com o 6.8), ele também completa o confinamento de cada aba ([escopo do sandbox](#escopo-do-sandbox)), e aí o sandbox só sobe com ele.

## O que ele faz

O daemon pede o confinamento quando o Docker privado de uma pasta sobe, e o auxiliar faz duas coisas:

1. Carrega os programas e os prende à slice `sandmux-docker-<id>.slice` daquela pasta.
   - Eles valem para o `dockerd`, o `containerd`, o `runc` e todo contêiner da pasta.
   - Não valem para nenhum outro processo da máquina, nem para o agente.
2. Marca os arquivos que o daemon manda:
   - **negados:** os segredos e o `file-deny` da pasta, com tudo o que existe dentro das pastas negadas, e as coberturas de `/etc`;
   - **protegidos contra escrita:** `.git/hooks`, `.git/config` e `.git/config.worktree`;
   - **diretórios git:** neles não dá para criar `commondir`, `hooks`, `config` nem `config.worktree`.

A decisão é pelo arquivo (o inode), e não pelo caminho. Por isso, desmontar uma cobertura, tirar o somente-leitura de um bind, renomear ou criar um link não abrem caminho até o arquivo. Os detalhes do que fica negado estão em [docker.md](docker.md#confinamento-por-ebpf).

## Quem pode pedir o quê

- O socket é `/run/sandmux-helper.sock`, e qualquer usuário conecta. O auxiliar sabe quem pede pelas credenciais do socket (`SO_PEERCRED`), e não por algo que o pedido diga.
- Um usuário só confina um cgroup cujo diretório pertence a ele. O root pode qualquer um.
- Uma regra só nega. Ela nunca dá acesso, e só vale para os processos do cgroup pedido. Marcar um arquivo do sistema, como `/etc/hostname`, só impede o Docker daquele usuário de abri-lo.
- O auxiliar nunca abre um caminho. Os arquivos chegam como descritores abertos pelo próprio usuário, que prova assim que alcança o arquivo.
- Um usuário só pede o escopo de um user namespace que pertence a ele, e nunca o do próprio auxiliar.
- **Limites por usuário:** 16 confinamentos ativos, cada um com até 16384 marcas, e 256 escopos de sandbox. Um pedido acima disso é recusado, sem afetar os outros.

## Ao vivo e eventos

- **Desmarcar:** o daemon também pede `unmark`, quando a política de arquivos da pasta muda e um caminho deixa de ser negado. O auxiliar tira a marca e solta o descritor do arquivo.
- **Eventos:** os programas mandam um evento por uma fila do kernel quando negam uma operação e quando um contêiner cria um arquivo no sistema de arquivos da pasta. O auxiliar lê a fila e manda os eventos ao daemon pela mesma conexão do confinamento. O evento tem o tipo, o gancho, o inode e o nome do arquivo criado, nunca o conteúdo de um arquivo. Com a fila cheia, os eventos a mais são descartados e contados, e os contêineres não esperam.

## Observação das execuções e das conexões

O pedido `observe` liga só a observação das execuções num cgroup do usuário (o escopo de uma aba), sem regras de arquivos nem marcas. Um programa no tracepoint `sched_process_exec` manda o caminho executado, o processo e os argumentos pela mesma conexão. Dois programas `fexit`, na saída de `tcp_v4_connect` e `tcp_v6_connect`, mandam cada conexão TCP que um processo do cgroup abre: o pid, a porta de origem e o destino. O daemon usa esses eventos para o [processo de cada conexão](network.md#processo-de-cada-conexão) no registro de rede. O confinamento do Docker (`confine`) inclui a mesma observação. São até 64 observações ativas por usuário, além dos 16 confinamentos. A observação só registra: nenhuma execução ou conexão é negada nem atrasada.

## Escopo do sandbox

Num kernel com o Landlock ABI 4 ou 5, o Landlock não tem o escopo de sockets abstratos e de sinais e, no ABI 4, a restrição de `ioctl` em dispositivos. O lançador (`sandmux-run`) pede então ao auxiliar o `scope` do sandbox, antes de o comando rodar, com:
- o user namespace que cada lançamento cria;
- o processo 1 do sandbox;
- a lista dos dispositivos do `/dev` do sandbox.

Programas BPF LSM globais, carregados no primeiro escopo e soltos quando não resta nenhum, negam a um processo desse user namespace (e dos de baixo):
- a conexão a um socket abstrato criado fora do sandbox;
- um sinal a um processo de fora, ou ao processo 1 do sandbox;
- um `ioctl` num dispositivo fora da lista, com os mesmos comandos liberados pelo Landlock.

O escopo vale enquanto a conexão do lançador está aberta, isto é, enquanto o sandbox vive. Sem o auxiliar ou sem o BPF LSM, o lançamento é recusado, e o `sandmux doctor` mostra os caminhos ([Ubuntu 24.04](install.md#ubuntu-2404)). Num kernel com o ABI 6, o lançador não pede o escopo.

## Quanto tempo as regras duram

As regras e as marcas valem enquanto a conexão do daemon com o auxiliar estiver aberta. Elas somem quando:

- o Docker da pasta para;
- o daemon termina, mesmo morto por `SIGKILL`;
- o auxiliar para.

Se o auxiliar parar com o Docker rodando, o daemon para o Docker da pasta, que não segue sem as regras. O próximo `docker` sobe outro Docker e liga o confinamento de novo, se o auxiliar responder. O socket do systemd sobe o auxiliar de volta na conexão seguinte.

## Instalar

O auxiliar precisa de duas coisas:

- o kernel com o BPF LSM ativo, isto é, `bpf` na lista de `/sys/kernel/security/lsm`;
- o binário numa pasta do root.

**`sandmux setup`** grava as unidades `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system` e liga o socket. Ele usa `sudo` depois de perguntar, como faz com o perfil do AppArmor ([install.md](install.md#sandmux-setup)).

Ele recusa instalar quando o binário, ou uma pasta acima dele, pode ser alterado por um usuário que não seja o root. É o caso de `~/.local/bin`, onde o `install.sh` põe os binários sem `sudo`. O motivo: o serviço roda como root, e quem altera o binário ganharia root.

Com `--no-sudo`, o setup mostra os comandos para rodar como root:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now sandmux-helper.socket
```

**Pacotes:** o `.deb`, o `.rpm` e o pacote do AUR trazem as unidades em `/usr/lib/systemd/system` e ligam o socket na instalação. A fórmula do Homebrew não traz o auxiliar, porque o Homebrew não roda como root.

**Kernel sem `bpf` na lista:** acrescente `bpf` ao parâmetro `lsm=` do boot, mantendo os que já estão em `/sys/kernel/security/lsm`. Depois de reiniciar, `sandmux doctor` mostra a verificação "confinamento do Docker por eBPF".

## Desligar

```bash
sudo systemctl disable --now sandmux-helper.socket
```

O Docker de cada pasta volta às proteções sem privilégio na próxima vez que subir. `install.sh --uninstall` para e remove as unidades.

## Como saber se vale

- `sandmux doctor` mostra a verificação "confinamento do Docker por eBPF", com o motivo e a correção quando falta algo.
- `sandmux stats <id>` mostra, na seção do Docker privado, "confinamento eBPF: ligado" ou "desligado:" com o motivo. O painel da TUI mostra a linha "eBPF".

## O que o serviço pode fazer

A unidade do serviço tem as restrições do systemd:

- **capacidades:** só `CAP_BPF` e `CAP_PERFMON`, as que carregar e prender programas BPF LSM a um cgroup exige;
- **sistema de arquivos:** `ProtectSystem=strict` e `ProtectHome=yes`;
- **rede:** nenhuma (`PrivateNetwork=yes`), e só sockets Unix.

Os programas são montados pelo próprio sandmux, a partir do BTF do kernel que está rodando (`/sys/kernel/btf/vmlinux`), sem `clang` e sem código compilado de fora.

## Testes

Os testes que exigem root (`crates/sandmux/tests/docker_ebpf.rs`) ficam fora do `check.sh`. Eles carregam programas BPF no kernel, e um erro num programa global derruba a máquina inteira. Por isso o padrão é rodá-los numa VM do Ubuntu 24.04:

```bash
scripts/vm-ubuntu-2404.sh up
scripts/vm-ubuntu-2404.sh docker
scripts/vm-ubuntu-2404.sh check-root
```

- `up` sobe a VM, e o `docker` instala nela o Docker sem root, uma vez.
- `check-root` compila os testes e o auxiliar em musl e os copia para a VM, no mesmo caminho. Depois sobe o auxiliar como root lá dentro, reiniciando-o quando um teste pede, e roda os testes como o usuário da VM. Um filtro de teste pode vir depois do `check-root`.

O `scripts/check-root.sh` roda os mesmos testes no kernel desta máquina:

```bash
sudo scripts/check-root.sh
```

Ele avisa e pergunta antes de carregar BPF no host; `--host` dispensa a pergunta. O script sobe o auxiliar como root num socket temporário e roda os testes como o usuário que chamou o `sudo`, com binários compilados por esse usuário. Por isso, liberá-lo no `sudoers` sem senha dá root a quem roda como esse usuário. Se fizer isso, faça só enquanto precisa.
