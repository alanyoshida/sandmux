# Docker privado

Cada pasta de trabalho tem o próprio Docker sem root, que o agente usa de dentro do sandbox com `docker` e `docker compose`. O socket do Docker do host nunca entra no sandbox, porque equivale a root na máquina.

## Como o agente usa

Toda sessão do daemon recebe o socket em `/run/sandmux/docker.sock`, com `DOCKER_HOST=unix:///run/sandmux/docker.sock` no ambiente. O `docker` e o plugin `docker compose` do sistema funcionam sem configuração.

O `run` sem terminal, de scripts e CI, não tem Docker privado: não há daemon para mantê-lo.

## O que os contêineres enxergam

O `dockerd` roda no user namespace do `rootlesskit`, numa raiz montada pela mesma composição do sandbox. Por isso um `-v`, um `--mount` ou um volume do `compose` só alcança:

- o sistema em somente leitura, com os arquivos sensíveis de `/etc` cobertos;
- a pasta de trabalho, com escrita e com a proteção do `.git` (hooks e config em somente leitura);
- a pasta de entrada, em somente leitura;
- o `/dev` do sandbox: os nós `null`, `zero`, `full`, `random`, `urandom` e `tty`, pseudoterminais próprios e um `/dev/shm` próprio. A GPU e o `/dev/kvm` entram conforme as chaves `gpu` e `kvm` da [configuração](config.md) da pasta, sem as opções de `run` das sessões.

Não entram:

- o HOME, nem o real nem o sintético, e portanto `~/.ssh`, `~/.aws` e `~/.cache`;
- as pastas extras de `--ro` e `--rw` das sessões, porque o Docker é da pasta e cada sessão pode ter opções diferentes;
- os outros dispositivos do host: os terminais do usuário, a câmera, o áudio, `/dev/net/tun` e `/dev/fuse`. Um `--device` de um deles falha, porque o dispositivo não existe na visão, e um `-v /dev:/d` mostra só o `/dev` acima.

Um caminho fora da visão falha ou chega vazio ao contêiner, inclusive quando alcançado por um link simbólico plantado na pasta de trabalho. Para dar a um contêiner um arquivo de fora, coloque-o dentro da pasta de trabalho.

## Segredos e caminhos negados

Os contêineres não passam pelo supervisor de aberturas do sandbox ([files.md](files.md)). Para que um `-v $PWD:/w` não leia o que o agente não lê, o Docker da pasta, quando sobe, percorre a pasta de trabalho com a mesma política de arquivos e cobre:

- os arquivos com cara de segredo, menos os que o próprio agente criou;
- os arquivos e as pastas de `file-deny`; uma pasta negada é coberta inteira.

Um caminho coberto chega vazio e em somente leitura ao contêiner, e o arquivo do host não muda. `file-allow` e `secret-files-off` valem como para o agente. A política usada é a da configuração da pasta, sem a camada de aba das sessões, como os tetos e os dispositivos.

Com mais de 4096 caminhos a cobrir, o Docker privado não sobe, e todo `docker` mostra a contagem e o teto. Reduza a lista com `secret-files-off` ou `file-allow` na configuração da pasta.

## Confinamento por eBPF

As coberturas acima são montagens, e um contêiner com `--privileged` consegue desfazê-las (veja os limites). Com o [auxiliar root](helper.md) instalado e o BPF LSM ativo no kernel, o Docker de cada pasta sobe também com programas BPF LSM presos à slice dele. Eles decidem pelo arquivo, e não pelo caminho:

- **Negados à leitura e à escrita:** os caminhos cobertos (os segredos e o `file-deny` da pasta e as coberturas de `/etc`). Numa pasta negada, tudo o que existe dentro dela quando o Docker sobe; os contêineres também não criam, apagam nem renomeiam nada dentro dela.
- **Protegidos contra escrita:** `hooks`, `config` e `config.worktree` de cada diretório git da pasta, com os submódulos. Ficam negados abrir com escrita, apagar, renomear, mudar atributos, fazer link e criar algo dentro de `hooks`. A leitura continua.
- **Nomes reservados:** nos diretórios git, criar `commondir`, `hooks`, `config` ou `config.worktree`. O próprio diretório git não pode ser apagado nem renomeado.

Desmontar uma cobertura, tirar o somente-leitura, renomear, criar um hard link ou montar a pasta noutro lugar não mudam a decisão, que segue o inode. A operação negada falha com `EPERM`. As regras valem antes de o socket chegar à sessão e só para os processos do Docker da pasta: o agente, o host e o Docker de outra pasta não são afetados.

Sem o auxiliar, sem o `bpf` na lista de LSMs do kernel ou sem a slice dos tetos, o Docker sobe só com as proteções sem privilégio. `sandmux stats <id>` mostra a linha "confinamento eBPF" com o motivo, e o `sandmux doctor` mostra como ligar. Se o auxiliar parar com o Docker rodando, o Docker da pasta para, e o próximo `docker` sobe outro.

## Quando sobe e quando para

- **Sobe** na primeira conexão ao socket. Uma sessão que nunca roda `docker` não sobe Docker nenhum. A primeira chamada espera de 1 a 3 segundos.
- **Para** quando a última sessão viva daquela pasta termina, e também no `sandmux daemon stop`. Os contêineres que estavam rodando param junto.

## O que persiste

Imagens, volumes e contêineres parados de cada pasta ficam em `~/.local/share/sandmux/docker/<identificador>`, com o caminho da pasta no arquivo `pasta`. Voltam na próxima sessão da mesma pasta e ficam separados do Docker do host e das outras pastas. O cache de downloads de imagem entre pastas vem com o gateway de rede (change 8).

Os arquivos das imagens pertencem aos IDs da faixa subordinada (`/etc/subuid`), e um `rm -rf` comum não os apaga. Para apagar os dados de uma pasta, use uma destas formas:

```bash
rootlesskit --net=host -- rm -rf ~/.local/share/sandmux/docker/<identificador>
```

- `docker system prune -a --volumes`, de dentro de uma sessão da pasta.

## Portas e rede

- **Portas:** uma porta publicada (`-p 8080:80`) fica só no `127.0.0.1` do host. O `dockerd` sobe com `--ip 127.0.0.1`, e a porta nunca escuta em `0.0.0.0`.
- **De dentro do sandbox:** o sandbox tem rede própria (veja [network.md](network.md)), e o `127.0.0.1` dele não é o do host. A porta publicada é alcançada por `host.sandmux.internal`, como `http://host.sandmux.internal:8080`.
- **Lista:** o `sandmux stats <id>` e a seção Portas do painel da TUI mostram as portas publicadas pelos contêineres da pasta, como `80 → 127.0.0.1:8080 (web)`.
- **Rede pelo gateway:** o Docker de cada pasta roda num namespace de rede sem rede própria (`rootlesskit --net=none`; não precisa do `slirp4netns`, do `pasta` nem do `gvisor-tap-vsock`). A visão do Docker cria uma interface TUN e a entrega ao daemon, que roda um gateway igual ao do sandbox: o mesmo DNS, os mesmos bloqueios fixos e o mesmo [registro de rede](network.md), com a etiqueta `docker-<id>`.
- **Política:** a do Docker é a mais restrita entre as sessões vivas da pasta, na ordem `sem-rede`, `inferencia`, `custom` e `aberta`. Entre sessões com o mesmo modo, a denylist é a soma e a allowlist fica com os itens que todas têm. Ela muda quando uma sessão da pasta sobe ou termina, e quando a configuração muda. Um contêiner de uma pasta com uma aba em `sem-rede` fica sem rede para fora.

## Registries privados

Com um segredo `docker` no [cofre](vault.md#ferramentas) (por exemplo `sandmux vault add registry --domain registry.empresa.com --tool docker`), o `docker pull` de um registry privado funciona sem o token real no sandbox:

- a sessão recebe um `~/.docker/config.json` só daquela execução com o valor falso em `auths`;
- o gateway do Docker da pasta tem uma CA própria, criada quando o Docker sobe, e decifra os domínios donos dos segredos `docker`;
- a visão do Docker tem o pacote de CAs do sistema com essa CA, e o `dockerd` confia no gateway;
- a cada conexão decifrada, o gateway pede os segredos ao daemon e troca o falso pelo real no `Authorization` (inclusive dentro do `Basic` do login) do pedido ao domínio dono.

Um desbloqueio do cofre depois que o Docker subiu vale na conexão seguinte. Com o cofre trancado, o pedido sai com o falso e o registry recusa.

## Cache de imagens entre pastas

Com o binário `registry` do projeto distribution instalado (no Debian e no Ubuntu, o pacote `docker-registry`, que instala `docker-registry`), o daemon sobe um espelho do Docker Hub no `127.0.0.1` do host e cada Docker privado o usa por `--registry-mirror`. As camadas ficam em `~/.local/share/sandmux/registry-cache`, e uma imagem baixada numa pasta não é baixada de novo em outra.

- Só o Docker Hub passa pelo espelho; outros registries (`ghcr.io`, `quay.io`) são baixados por pasta.
- Para a política, a conexão ao espelho vale como acesso a `registry-1.docker.io`: num Docker em modo `custom` sem o grupo `docker` liberado, o espelho também é bloqueado.
- Sem o binário, cada pasta baixa as próprias imagens, e o `sandmux doctor` mostra como instalar.

```bash
sudo apt install docker-registry
```

## Tetos e consumo

O Docker de cada pasta, com todos os contêineres, roda numa slice própria do systemd (`sandmux-docker-<id>.slice`) com os tetos de memória e de processos daquela pasta, resolvidos pela [configuração](config.md) e separados dos tetos de cada sessão. Um contêiner que passa do teto é morto, e o Docker continua.

- **Por que uma slice:** o teto fica acima de tudo o que os contêineres enxergam. O Docker roda num cgroup namespace próprio, cuja raiz é o escopo dele (`sandmux: docker <pasta>`), dentro da slice: o `dockerd` e os outros processos ficam no subgrupo `motor`, e o runc cria cada contêiner ao lado dele. Um contêiner com `-v /sys/fs/cgroup:/c` vê só o escopo e os cgroups do Docker da pasta; não vê a slice com o teto, nem `user.slice`, nem os cgroups das sessões e do daemon. O escopo não tem teto próprio, porque os contêineres o alcançam.
- **Sem os tetos:** numa máquina sem o systemd de usuário, o Docker roda no mesmo cgroup namespace, com raiz no cgroup em que ele sobe, e os contêineres não veem o resto da árvore.
- **Quem morre no teto:** os processos dos contêineres ficam com `oom_score_adj` 900, e o `dockerd`, o `containerd` e os shims ficam com o valor da sessão. Assim, quando a pasta passa do teto, o kernel mata um contêiner, e não o Docker. Quem faz isso é um `runc` próprio da visão do Docker, na frente do `PATH`: ele sobe o valor antes de criar o contêiner e o põe no `config.json` dele, porque o `dockerd` sem root escreve ali o valor da sessão. Um contêiner pode ser levado junto quando o teto estoura.
- **`docker run -m`:** não limita um contêiner sozinho; o teto que vale é o da pasta.
- **Fim:** a slice é parada junto com o Docker da pasta. As que ficarem vazias, de um daemon derrubado, são paradas quando o daemon sobe de novo.

O `sandmux stats <id>` mostra a seção "Docker privado da pasta", com a memória, os processos, os tetos e quantos contêineres estão rodando.

## Sem Docker sem root

O Docker privado precisa de três coisas:

- `rootlesskit`, `dockerd`, `containerd`, `newuidmap` e `newgidmap` no `PATH`;
- uma faixa de 65536 IDs do usuário em `/etc/subuid`;
- uma faixa igual em `/etc/subgid`.

Sem isso, a sessão sobe normalmente, e todo `docker` mostra:

```
Error response from daemon: o Docker privado não está disponível nesta máquina: <o que falta>; veja `sandmux doctor`
```

## Chamadas de sistema negadas

O `dockerd` nasce com um filtro seccomp do sandmux, e todo processo do Docker da pasta o herda: o `containerd`, o `runc` e cada contêiner. Um filtro herdado não pode ser removido, então ele vale também com `--privileged` e com `--security-opt seccomp=unconfined`, que só deixam de acrescentar o filtro do Docker.

- **Negadas, com `EPERM`:** as mesmas que o sandbox do agente nega, menos as de montagem: `bpf`, `perf_event_open`, a carga de módulos, `kexec`, `userfaultfd`, `io_uring` e os `ioctl` `TIOCSTI` e `TIOCLINUX`, que injetam teclas no terminal.
- **Chaveiro do kernel, com `ENOSYS`:** `keyctl`, `add_key` e `request_key` respondem como se o kernel não tivesse chaveiro. É o único erro que o `runc` aceita ao criar um contêiner; com outro, nenhum contêiner sobe.
- **Liberadas:** as chamadas de montagem, porque o `dockerd` e o `runc` montam a raiz de cada contêiner.
- **Setuid:** o filtro não liga o `no_new_privs`, e o `su` e o `sudo` das imagens funcionam.

## Limites

- Um contêiner roda no user namespace do `rootlesskit`, sem o Landlock e sem o supervisor de aberturas do sandmux. O que o limita é a visão de arquivos, o filtro seccomp acima e o user namespace sem privilégio, que impede módulos e montagens no host.
- `--privileged` não dá privilégio real: o root do contêiner é o usuário, mapeado.
- Um contêiner que precisa de um dispositivo do host fora da lista, como uma VPN com `/dev/net/tun` ou um sistema de arquivos com `/dev/fuse`, não funciona no Docker privado.
- **Sem o confinamento por eBPF, as coberturas caem para um contêiner privilegiado.** Com `--privileged`, um contêiner tem `CAP_SYS_ADMIN` no user namespace em que a visão foi montada, e consegue desmontar uma cobertura ou tirar o somente-leitura de um bind. Isso vale para os segredos e os caminhos negados cobertos e para a proteção do `.git` (hooks e config em somente leitura). Um contêiner comum não consegue. Com o [confinamento por eBPF](#confinamento-por-ebpf), o kernel nega pelo arquivo, e desfazer a montagem não adianta.
- **Arquivos criados depois não são cobertos.** Um segredo criado na pasta com o Docker já rodando, um arquivo que você cria dentro de uma pasta negada, ou uma mudança na política de arquivos, só vale quando o Docker da pasta sobe de novo (depois de `sandmux daemon restart`, ou quando a última sessão da pasta termina). Com o confinamento por eBPF, os próprios contêineres não criam nada dentro de uma pasta negada.
- **Sem o resto da política de arquivos:** os contêineres não têm o registro de arquivos nem a política ao vivo. Sem o confinamento por eBPF, também não têm a recusa do `.git/commondir`.
