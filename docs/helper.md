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
- **Eventos:** os programas mandam um evento por uma fila do kernel quando negam uma operação e quando um contêiner cria um arquivo no sistema de arquivos da pasta. O auxiliar lê a fila e manda os eventos ao daemon pela mesma conexão do confinamento. O evento tem o tipo, o gancho, o inode e o nome do arquivo criado, nunca o conteúdo de um arquivo. Com a fila cheia, os eventos a mais são descartados e contados, e os contêineres não esperam (uma aba espera: veja abaixo).

## Arquivos de uma aba

O daemon também pede o `confine` para o escopo de uma aba, quando o lançador dela pede os [arquivos pelo kernel](files.md#pelo-kernel). O pedido leva, além do `dev` da pasta de trabalho, as montagens (`mnt_id`) das pastas do agente na visão do sandbox. Mudam três coisas em relação ao Docker:

- **Marcas novas:** `secret` dá `ENOENT` (o segredo some, como no supervisor) e `fdeny` dá `EACCES`, nas aberturas e nas operações dentro de uma pasta marcada.
- **Evento de abertura:** o programa do `file_open` manda, para cada arquivo aberto numa das montagens pedidas, o caminho (por `bpf_d_path`, na visão do sandbox, só com os bytes do caminho), o inode, a montagem e se a abertura lê e se grava (os bits `FMODE_READ` e `FMODE_WRITE` do arquivo, também no evento de uma abertura negada). As outras montagens não mandam nada. A fila desse confinamento tem 1 MiB.
- **Repetições:** um mapa LRU do confinamento (16384 entradas) guarda o arquivo (dispositivo, inode e geração), o modo e a hora do último evento. Uma abertura do mesmo arquivo e modo em menos de 1 s não manda evento. Os programas de renomear e de link tiram o arquivo do mapa. Um evento que não coube na fila não entra no mapa.
- **Fila cheia:** uma thread do auxiliar só esvazia a fila do kernel, e outra manda os eventos ao daemon. Quando a fila do kernel passa de 3/4, a do auxiliar passa de 32768 eventos ou há um descarte, o auxiliar congela o cgroup da aba (`cgroup.freeze`). Ele descongela quando a do auxiliar cai abaixo de 4096 e a do kernel abaixo de 1/4. Cada congelamento vai para o journal do auxiliar, com a duração ("aba congelada por N ms"). Depois do fim da conexão, ou na saída limpa, o auxiliar não descongela, porque as regras saem com a conexão; quem descongela é o daemon, ao refazer o confinamento.
- **Status:** o `status` diz `tab`: um auxiliar que confina abas. Com um auxiliar antigo, sem esse campo, a aba fica com o supervisor.

## Observação das execuções e das conexões

O pedido `observe` liga só a observação das execuções num cgroup do usuário (o escopo de uma aba), sem regras de arquivos nem marcas. Um programa no tracepoint `sched_process_exec` manda o caminho executado, o processo, o pai e os argumentos pela mesma conexão, e um no `sched_process_exit` manda o fim de cada processo (não de cada thread), quando o kernel tem o tracepoint. O daemon usa as execuções e os fins para a [árvore de processos](tui.md#processos) da aba. Dois programas `fexit`, na saída de `tcp_v4_connect` e `tcp_v6_connect`, mandam cada conexão TCP que um processo do cgroup abre: o pid, a porta de origem e o destino. O daemon usa esses eventos para o [processo de cada conexão](network.md#processo-de-cada-conexão) no registro de rede. O confinamento do Docker (`confine`) inclui a mesma observação das execuções, sem a dos fins. São até 64 observações ativas por usuário, além dos 16 confinamentos. A observação só registra: nenhuma execução ou conexão é negada nem atrasada.

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

Quando o auxiliar reinicia (uma atualização, um travamento), o escopo volta no auxiliar seguinte:

- **Numa sessão do daemon:** o lançador entrega ao daemon os parâmetros do escopo e o user namespace do sandbox, e o daemon vigia uma conexão com o auxiliar por aba. Na queda, ele congela a aba, registra de novo o escopo (e os [arquivos pelo kernel](files.md#pelo-kernel), quando a aba tem) e descongela. Nada da aba roda sem o escopo.
- **No `run` sem o daemon:** o próprio lançador registra de novo assim que o auxiliar seguinte responde. Nesse intervalo, o sandbox roda sem o escopo.

Sem o escopo de volta em 30 s, o sandbox termina. Um lançamento que encontra o auxiliar trocando tenta de novo por até 30 s. No 6.8, carregar e soltar os programas do escopo leva segundos; por isso, num kernel abaixo do ABI 6, o auxiliar os carrega ao subir, e eles ficam carregados por 60 s depois do último escopo.

## Quanto tempo as regras duram

As regras e as marcas valem enquanto a conexão do daemon com o auxiliar estiver aberta. Elas somem quando:

- o Docker da pasta para, ou a aba termina;
- o daemon termina, mesmo morto por `SIGKILL`;
- o auxiliar para.

Se o auxiliar parar com o Docker rodando, o daemon para o Docker da pasta, que não segue sem as regras. Uma aba com os arquivos pelo kernel fica congelada até o confinamento ser refeito com o auxiliar de volta, e para se ele não voltar em 30 s. No `SIGTERM` (o `systemctl restart`, uma atualização), o auxiliar fecha a escrita de cada conexão antes de sair: quem pediu fica sabendo com as regras ainda valendo, e elas saem quando ele fecha a conexão (ou em 2 s). As conexões que chegam nesse meio tempo fecham sem resposta, e quem pediu tenta no auxiliar seguinte. O próximo `docker` sobe outro Docker e liga o confinamento de novo, se o auxiliar responder. O socket do systemd sobe o auxiliar de volta na conexão seguinte.

## Instalar

O auxiliar precisa de duas coisas:

- o kernel com o BPF LSM ativo, isto é, `bpf` na lista de `/sys/kernel/security/lsm`;
- o binário numa pasta do root.

**`sandmux setup`** grava as unidades `sandmux-helper.socket` e `sandmux-helper.service` em `/etc/systemd/system` e liga o socket. Ele usa `sudo` depois de perguntar, como faz com o perfil do AppArmor ([install.md](install.md#sandmux-setup)).

**Depois de atualizar o sandmux**, o auxiliar em execução precisa ser reiniciado: ele só troca ao ser reiniciado, e gravar os binários novos não basta. O `install.sh` com `sudo` e os pacotes `.deb` e `.rpm` o reiniciam na instalação, e o `sandmux setup` o reinicia quando acha um auxiliar mais antigo que o sandmux. À mão: `sudo systemctl restart sandmux-helper.service`. Enquanto o auxiliar antigo está de pé, o que depende da versão nova pode faltar ou sair incompleto. O `sandmux doctor` avisa disso com `[aviso] versão do auxiliar root`, mostrando a versão do auxiliar e a do sandmux, e a seção Processos da TUI mostra o mesmo aviso. É um aviso, não uma falha: o código de saída do `doctor` não muda.

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
