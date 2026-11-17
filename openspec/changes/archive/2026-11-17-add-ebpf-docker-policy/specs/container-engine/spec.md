## ADDED Requirements

### Requirement: Arquivos do Docker decididos pelo kernel
Quando o confinamento por eBPF vale para o Docker privado de uma pasta, o kernel MUST decidir o acesso de todo processo desse Docker (o `dockerd`, o `containerd`, o `runc` e cada contêiner) pelo arquivo, e não pelo caminho em que ele aparece. Desmontar uma cobertura, tirar o somente-leitura de um bind, renomear, criar um link ou montar a pasta em outro lugar MUST NOT mudar a decisão.

#### Scenario: Contêiner privilegiado desmonta a cobertura
- **WHEN** a pasta tem um `.env` do usuário, o confinamento vale, e o agente roda um contêiner com `--privileged` e a pasta montada que desmonta a cobertura do `.env` e lê o arquivo
- **THEN** a leitura falha com `EPERM`, e o `.env` do host não muda

#### Scenario: Link para um segredo
- **WHEN** um contêiner com a pasta montada cria um hard link para o `.env` com outro nome e lê o link
- **THEN** a criação do link ou a leitura falha com `EPERM`

### Requirement: Alcance do confinamento do Docker
As regras do confinamento por eBPF MUST valer antes de o socket do Docker ser entregue à primeira sessão. Elas MUST NOT alcançar processos fora do Docker da pasta: o agente, o host e o Docker de outra pasta.

#### Scenario: Agente não é afetado
- **WHEN** o confinamento vale no Docker da pasta e o agente lê, no sandbox, um segredo que ele mesmo criou
- **THEN** a leitura funciona como antes

#### Scenario: Outra pasta
- **WHEN** o confinamento vale no Docker da pasta `~/a` e um contêiner do Docker da pasta `~/b`, com `~/a` dentro da visão dele, lê um arquivo de `~/a` que não é segredo
- **THEN** a leitura segue só as regras do Docker de `~/b`

### Requirement: Arquivos negados ao Docker
Com o confinamento por eBPF, os arquivos e as pastas que a visão do Docker cobre MUST ser negados à leitura e à escrita para os processos do Docker da pasta. Isso inclui os segredos e o `file-deny` da pasta de trabalho, pela configuração da pasta, e as coberturas de `/etc`. Numa pasta negada, MUST ser negado também tudo o que existe dentro dela quando o Docker sobe, e os processos do Docker MUST NOT criar, apagar nem renomear nada dentro dela.

#### Scenario: Pasta negada, arquivo novo
- **WHEN** o perfil da pasta tem `file-deny = ["~/proj/privado"]`, o confinamento vale, e um contêiner com `--privileged` e `~/proj` montado desmonta a cobertura de `privado`, lê `privado/velho.txt` e cria `privado/novo.txt`
- **THEN** a leitura e a criação falham com `EPERM`, e `novo.txt` não aparece no host

#### Scenario: Criado no host depois
- **WHEN** o usuário cria `~/proj/privado/outro.txt` no host com o Docker já rodando
- **THEN** o arquivo fica fora do confinamento até o Docker da pasta subir de novo, como os segredos criados depois

#### Scenario: Segredo criado pelo agente
- **WHEN** o agente criou `chave.pem` numa sessão anterior e um contêiner com a pasta montada lê o arquivo
- **THEN** o contêiner lê o conteúdo

### Requirement: .git protegido no Docker
Com o confinamento por eBPF, em cada diretório git da pasta de trabalho (o do repositório e os dos submódulos), os processos do Docker da pasta MUST NOT:

- abrir com escrita, apagar, renomear, mudar atributos ou fazer link de `hooks`, `config` e `config.worktree`;
- criar, apagar ou renomear algo dentro de `hooks`;
- criar, por qualquer chamada, uma entrada chamada `commondir`, `hooks`, `config` ou `config.worktree` no diretório git.

A leitura desses arquivos MUST continuar funcionando.

#### Scenario: Hook gravado por contêiner privilegiado
- **WHEN** o confinamento vale e um contêiner com `--privileged` e a pasta montada tira o somente-leitura de `.git/hooks` e grava `.git/hooks/pre-commit`
- **THEN** a gravação falha com `EPERM`, e nenhum hook novo aparece no host

#### Scenario: commondir
- **WHEN** um contêiner com a pasta montada cria `.git/commondir`
- **THEN** a criação falha com `EPERM`

#### Scenario: git de leitura no contêiner
- **WHEN** um contêiner com a pasta montada roda `git log`
- **THEN** o comando funciona

### Requirement: Ativação do confinamento com recuo
Quando o Docker privado de uma pasta sobe, o daemon MUST ligar o confinamento por eBPF se o auxiliar root responde e o kernel tem o BPF LSM ativo. Sem um dos dois, ou com o Docker sem a slice dos tetos, o Docker MUST subir só com as proteções sem privilégio (o filtro seccomp, o `/dev` mínimo, os cgroups e as coberturas), e o motivo MUST aparecer no `sandmux doctor`, no `sandmux stats` da sessão e no log do daemon.

#### Scenario: Máquina sem o auxiliar
- **WHEN** o `sandmux-helper` não está instalado e o agente roda `docker ps`
- **THEN** o Docker privado sobe, e `sandmux stats <id>` mostra que o confinamento por eBPF está desligado, com o motivo e o comando que o instala

### Requirement: Queda do auxiliar com o Docker rodando
Se a conexão com o auxiliar root cair com o Docker privado de uma pasta rodando confinado, o daemon MUST parar o Docker da pasta e registrar o motivo no log. A subida seguinte MUST seguir a regra de ativação com recuo.

#### Scenario: Auxiliar para com o Docker rodando
- **WHEN** o confinamento vale e o serviço `sandmux-helper` é parado
- **THEN** o Docker privado da pasta para, e o próximo `docker` sobe outro, confinado de novo se o auxiliar voltar a responder (o socket do systemd o sobe na conexão seguinte), ou sem o confinamento e com o aviso
