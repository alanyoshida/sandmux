## ADDED Requirements

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
Um contêiner do Docker privado MUST enxergar só os cgroups do Docker da própria pasta. Ele MUST NOT conseguir ler nem alterar o cgroup que guarda os tetos da pasta, nem os cgroups de outras sessões, do daemon ou do resto do usuário. Numa máquina sem suporte aos tetos, o contêiner MUST NOT ter nenhum cgroup gravável.

#### Scenario: Afrouxar o teto
- **WHEN** o perfil da pasta tem `memory = "512M"` e o agente roda um contêiner com `-v /sys/fs/cgroup:/c` que procura o teto da pasta para aumentá-lo e depois aloca 2 GB
- **THEN** o contêiner não acha o arquivo do teto, e o processo que aloca é morto por falta de memória

#### Scenario: Cgroups do usuário
- **WHEN** o agente roda `docker run -v /sys/fs/cgroup:/c <imagem> ls /c`
- **THEN** não aparecem `user.slice`, o cgroup do daemon nem os das outras sessões

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
