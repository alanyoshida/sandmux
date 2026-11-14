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
