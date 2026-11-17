# container-engine Specification

## Purpose
Define o Docker sem root privado de cada pasta de trabalho, que o agente usa de dentro do sandbox: a visão de arquivos restrita do Docker, o socket entregue ao sandbox, quando o Docker sobe e para, o que persiste, como as portas são publicadas, os tetos de recurso e o comportamento numa máquina sem suporte.

## Requirements

### Requirement: Docker privado por pasta de trabalho
Cada pasta de trabalho MUST ter o próprio Docker sem root, separado do Docker do host e do de outras pastas. Sessões na mesma pasta MUST usar o mesmo Docker. O socket do Docker do host MUST NOT entrar no sandbox.

#### Scenario: Duas sessões na mesma pasta
- **WHEN** duas sessões na pasta `~/proj` rodam `docker ps`
- **THEN** as duas veem os mesmos contêineres

#### Scenario: Pastas diferentes
- **WHEN** uma sessão em `~/proj` cria um contêiner e uma sessão em `~/outro` roda `docker ps -a`
- **THEN** o contêiner de `~/proj` não aparece, nem os contêineres do Docker do host

### Requirement: Socket do Docker no sandbox
Toda sessão do daemon MUST receber o socket do Docker privado da pasta em `/run/sandmux/docker.sock`, com `DOCKER_HOST=unix:///run/sandmux/docker.sock` no ambiente, de modo que `docker` e `docker compose` funcionem sem configuração.

#### Scenario: Docker compose
- **WHEN** um `compose.yaml` na pasta de trabalho define um serviço a partir de uma imagem local e o agente roda `docker compose up -d`
- **THEN** o serviço sobe e `docker compose ps` o mostra rodando

### Requirement: Montagens só alcançam a visão do sandbox
O Docker privado MUST resolver os caminhos de `-v`, `--mount` e dos volumes de `compose` numa visão de arquivos que contém só o sistema em somente leitura (com os arquivos sensíveis de `/etc` cobertos), a pasta de trabalho (com a proteção do `.git`) e a pasta de entrada. Um caminho fora dessa visão MUST chegar vazio ao contêiner ou fazer a criação falhar, inclusive quando alcançado por link simbólico.

#### Scenario: Pasta de trabalho
- **WHEN** o agente roda `docker run -v $PWD:/app <imagem> ls /app`
- **THEN** o contêiner lista os arquivos da pasta de trabalho, e o que ele grava em `/app` aparece no host

#### Scenario: Credencial do HOME real
- **WHEN** o agente roda `docker run -v ~/.ssh:/x <imagem> ls /x`
- **THEN** o contêiner não vê nenhum arquivo de `~/.ssh` do host

#### Scenario: Link simbólico plantado
- **WHEN** o agente cria na pasta de trabalho um link para `~/.ssh` e monta esse link num contêiner
- **THEN** o contêiner não vê nenhum arquivo de `~/.ssh` do host

#### Scenario: Hooks do git
- **WHEN** o agente monta a pasta de trabalho num contêiner e tenta gravar em `.git/hooks`
- **THEN** a gravação falha, como dentro do sandbox

### Requirement: Docker sob demanda e ligado às sessões
O Docker privado de uma pasta MUST subir na primeira conexão ao socket de uma sessão daquela pasta e MUST parar quando nenhuma sessão viva daquela pasta resta. Parar o daemon de sessões MUST parar todos os Docker privados.

#### Scenario: Sessão que não usa Docker
- **WHEN** uma sessão sobe e o agente nunca roda `docker`
- **THEN** nenhum Docker privado é iniciado

#### Scenario: Última sessão da pasta
- **WHEN** a única sessão viva da pasta `~/proj` termina
- **THEN** o Docker privado de `~/proj` para, e nenhum processo dele continua no host

### Requirement: Imagens e volumes persistem por pasta
Imagens, volumes e contêineres parados do Docker privado de uma pasta MUST ficar no diretório de dados do sandmux e voltar na próxima sessão da mesma pasta.

#### Scenario: Imagem construída antes
- **WHEN** o agente construiu uma imagem numa sessão em `~/proj`, a sessão terminou e outra sessão em `~/proj` roda `docker images`
- **THEN** a imagem aparece, sem ser construída de novo

### Requirement: Portas publicadas só no localhost
Uma porta publicada por um contêiner (`-p 8080:80`) MUST ficar acessível só no `127.0.0.1` do host, mesmo quando o pedido não indica o endereço.

#### Scenario: Servidor de desenvolvimento
- **WHEN** o agente roda `docker run -p 18080:80 <imagem com servidor>`
- **THEN** `http://127.0.0.1:18080` responde no host, e a porta não aceita conexões por outro endereço da máquina

### Requirement: Tetos do Docker privado
O Docker privado de uma pasta, com todos os contêineres, MUST rodar sob os tetos de memória e de processos resolvidos pela configuração para aquela pasta, separados dos tetos de cada sessão.

#### Scenario: Contêiner que aloca demais
- **WHEN** o perfil da pasta tem `memory = "512M"` e um contêiner tenta alocar 2 GB
- **THEN** o processo do contêiner é morto por falta de memória, e o Docker privado e as sessões continuam

### Requirement: Máquina sem Docker sem root
Quando a máquina não tem o necessário para o Docker sem root (`rootlesskit`, `dockerd`, `newuidmap` e uma faixa do usuário em `/etc/subuid` e `/etc/subgid`), a sessão MUST subir normalmente e o socket MUST responder a todo pedido com um erro que diz o que falta e indica `sandmux doctor`.

#### Scenario: Sem rootlesskit
- **WHEN** a máquina não tem `rootlesskit` e o agente roda `docker ps`
- **THEN** o `docker` mostra o erro dizendo que o Docker privado não está disponível, o que falta e `sandmux doctor`

### Requirement: Run sem terminal sem Docker privado
O `run` sem terminal MUST NOT ter Docker privado nem definir `DOCKER_HOST`, porque não há daemon para manter o Docker.

#### Scenario: Script
- **WHEN** um script roda `sandmux run shell -- -c 'echo ${DOCKER_HOST:-sem}'`
- **THEN** a saída é `sem`

### Requirement: Rede do Docker pelo gateway
O Docker privado de uma pasta MUST rodar num namespace de rede sem rede própria, com um TUN ligado a um gateway do sandmux, de modo que todo tráfego dos contêineres e do `dockerd` passe pelo gateway, com o DNS do gateway, os bloqueios fixos e o registro, com a etiqueta `docker-<id>`.

#### Scenario: Contêiner registrado
- **WHEN** um contêiner do Docker privado acessa `host.sandmux.internal`
- **THEN** o registro de rede tem a linha da conexão com a etiqueta do Docker da pasta

### Requirement: Política do Docker
A política de rede do Docker privado MUST ser a mais restrita entre as sessões vivas da pasta, na ordem `sem-rede`, `inferencia`, `custom` e `aberta`. Entre sessões com o mesmo modo, a denylist MUST ser a soma e a allowlist MUST ser a interseção. A política MUST ser recompilada quando uma sessão da pasta sobe ou termina e quando o arquivo de configuração muda.

#### Scenario: Aba restrita na pasta
- **WHEN** a pasta tem uma sessão no modo aberta e outra no modo `inferencia`, e um contêiner consulta `host.sandmux.internal`
- **THEN** a consulta recebe NXDOMAIN, e o registro tem o bloqueio "fora do modo só inferência"

### Requirement: Cache de imagens entre pastas
Quando a máquina tem o binário `registry` (ou `docker-registry`), o daemon MUST subir um espelho pull-through do Docker Hub no `127.0.0.1` do host, com as camadas numa pasta comum do diretório de dados, e cada Docker privado MUST usá-lo como espelho. O gateway do Docker MUST decidir a conexão ao espelho como acesso a `registry-1.docker.io`. Sem o binário, cada pasta MUST baixar as próprias imagens, e o `sandmux doctor` MUST mostrar como instalar o espelho.

#### Scenario: Mesma imagem em outra pasta
- **WHEN** a pasta `~/a` baixou `alpine` e o agente da pasta `~/b` roda `docker pull alpine`
- **THEN** as camadas vêm do espelho, sem nova consulta ao Docker Hub pelas camadas

#### Scenario: Sem o binário
- **WHEN** a máquina não tem `registry` nem `docker-registry` e o usuário roda `sandmux doctor`
- **THEN** a saída diz que o cache de imagens está desligado e como instalar o pacote

### Requirement: Chamadas de sistema negadas no Docker privado
O `dockerd` e todo contêiner do Docker privado MUST ter negadas `bpf`, `perf_event_open`, a carga de módulos, `kexec`, o chaveiro do kernel, `userfaultfd` e `io_uring`, e os comandos de `ioctl` que injetam teclas no terminal. A negação MUST valer também para um contêiner com `--privileged` ou `--security-opt seccomp=unconfined`, e MUST devolver erro sem encerrar o processo. As chamadas de montagem MUST continuar disponíveis ao Docker.

#### Scenario: Contêiner privilegiado e sem o seccomp do Docker
- **WHEN** o agente roda um contêiner com `--privileged --security-opt seccomp=unconfined` e o processo dele chama `bpf`, `io_uring_setup` ou `keyctl`
- **THEN** cada chamada devolve erro, e o processo continua rodando

#### Scenario: Uso comum do Docker
- **WHEN** o agente roda `docker build`, `docker run` e `docker compose up` na pasta de trabalho
- **THEN** os três funcionam como antes

#### Scenario: Binário setuid dentro do contêiner
- **WHEN** um processo de usuário comum dentro de um contêiner roda um binário setuid da imagem, como o `su`
- **THEN** o binário ganha o privilégio dentro do contêiner, como antes

### Requirement: Dispositivos da visão do Docker
A visão do Docker privado MUST ter um `/dev` próprio, com os nós `null`, `zero`, `full`, `random`, `urandom` e `tty`, pseudoterminais novos e um `/dev/shm` próprio. Os dispositivos de GPU e o `/dev/kvm` MUST entrar conforme as chaves `gpu` e `kvm` da configuração da pasta. Nenhum outro dispositivo do host MUST ficar ao alcance de um contêiner.

#### Scenario: Terminais do usuário
- **WHEN** o usuário tem outros terminais abertos e o agente roda `docker run -v /dev:/d <imagem> ls /d/pts`
- **THEN** nenhum pseudoterminal do host aparece

#### Scenario: Câmera do host
- **WHEN** a máquina tem `/dev/video0` e o agente roda `docker run --device /dev/video0 <imagem>`
- **THEN** a criação do contêiner falha, porque o dispositivo não existe na visão

#### Scenario: Contêiner com terminal
- **WHEN** o agente roda `docker run -t <imagem> tty`
- **THEN** o contêiner recebe um pseudoterminal próprio e o comando funciona

#### Scenario: GPU desligada na pasta
- **WHEN** o perfil da pasta tem `gpu = false` e o agente roda `docker run -v /dev:/d <imagem> ls /d`
- **THEN** nenhum dispositivo de GPU aparece

### Requirement: Cgroups do Docker privado
Um contêiner do Docker privado MUST enxergar só os cgroups do Docker da própria pasta. Ele MUST NOT conseguir ler nem alterar o cgroup que guarda os tetos da pasta, nem os cgroups de outras sessões, do daemon ou do resto do usuário. Numa máquina sem suporte aos tetos, o contêiner MUST enxergar só o cgroup em que o Docker da pasta roda e os abaixo dele, e o Docker MUST continuar funcionando.

#### Scenario: Afrouxar o teto
- **WHEN** o perfil da pasta tem `memory = "512M"` e o agente roda um contêiner com `-v /sys/fs/cgroup:/c` que procura o teto da pasta para aumentá-lo e depois aloca 2 GB
- **THEN** o contêiner não acha o arquivo do teto, e o processo que aloca é morto por falta de memória

#### Scenario: Cgroups do usuário
- **WHEN** o agente roda `docker run -v /sys/fs/cgroup:/c <imagem> ls /c`
- **THEN** não aparecem `user.slice`, o cgroup do daemon nem os das outras sessões

#### Scenario: Máquina sem os tetos
- **WHEN** a máquina não tem suporte aos tetos e o agente roda `docker run -v /sys/fs/cgroup:/c <imagem> ls /c`
- **THEN** o contêiner roda, e `user.slice` não aparece

### Requirement: Arquivos da pasta cobertos no Docker
Quando o Docker privado de uma pasta sobe, os arquivos da pasta de trabalho que casam com a lista de segredos e os caminhos negados por `file-deny`, pela configuração da pasta, MUST chegar vazios e em somente leitura aos contêineres. Os segredos que o agente criou e os caminhos de `file-allow` MUST continuar visíveis. O arquivo no host MUST NOT ser alterado.

#### Scenario: .env do usuário
- **WHEN** a pasta de trabalho tem um `.env` do usuário e o agente roda `docker run -v $PWD:/w <imagem> cat /w/.env`
- **THEN** a saída é vazia, e o `.env` do host continua com o conteúdo dele

#### Scenario: Exemplo continua visível
- **WHEN** a pasta de trabalho tem `.env.example` e um contêiner com a pasta montada lê o arquivo
- **THEN** o contêiner lê o conteúdo

#### Scenario: Segredo criado pelo agente
- **WHEN** o agente criou `chave.pem` numa sessão anterior e um contêiner com a pasta montada lê o arquivo
- **THEN** o contêiner lê o conteúdo

#### Scenario: Pasta negada
- **WHEN** o perfil da pasta tem `file-deny = ["~/proj/privado"]` e um contêiner com `~/proj` montado lista `privado`
- **THEN** a pasta aparece vazia, e gravar nela falha

#### Scenario: Arquivo criado depois
- **WHEN** o usuário cria um `.env` na pasta com o Docker privado já rodando
- **THEN** os contêineres leem o arquivo até o Docker privado da pasta subir de novo, quando ele passa a chegar vazio

### Requirement: Teto de coberturas
Com mais de 4096 arquivos e pastas a cobrir numa pasta de trabalho, o Docker privado MUST NOT subir, e todo pedido ao socket MUST receber um erro com a contagem, o teto e as chaves `secret-files-off` e `file-allow`, que reduzem a lista.

#### Scenario: Pasta com segredos demais
- **WHEN** a pasta de trabalho tem 5000 arquivos `*.pem` e o agente roda `docker ps`
- **THEN** o `docker` mostra o erro com a contagem e o teto, e nenhum Docker privado sobe
