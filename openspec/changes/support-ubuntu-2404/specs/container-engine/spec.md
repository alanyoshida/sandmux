## MODIFIED Requirements

### Requirement: Máquina sem Docker sem root
Quando a máquina não tem o necessário para o Docker sem root (`rootlesskit` 2.0 ou mais novo, com a rede `none`, `dockerd`, `newuidmap` e uma faixa do usuário em `/etc/subuid` e `/etc/subgid`), a sessão MUST subir normalmente e o socket MUST responder a todo pedido com um erro que diz o que falta e indica `sandmux doctor`.

#### Scenario: Sem rootlesskit
- **WHEN** a máquina não tem `rootlesskit` e o agente roda `docker ps`
- **THEN** o `docker` mostra o erro dizendo que o Docker privado não está disponível, o que falta e `sandmux doctor`

#### Scenario: rootlesskit sem a rede none
- **WHEN** a máquina tem o `rootlesskit` 1.1.1 do Ubuntu 24.04 e o usuário roda `sandmux doctor`
- **THEN** o Docker privado aparece como indisponível, com a versão do `rootlesskit` e o pacote `docker-ce-rootless-extras` do repositório do Docker como solução

### Requirement: Arquivos negados ao Docker
Com o confinamento por eBPF, os arquivos e as pastas que a visão do Docker cobre MUST ser negados à leitura e à escrita para os processos do Docker da pasta. Isso inclui os segredos e o `file-deny` da pasta de trabalho, pela configuração da pasta, e as coberturas de `/etc`. Numa pasta negada, MUST ser negado também tudo o que existe dentro dela, e os processos do Docker MUST NOT criar, apagar nem renomear nada dentro dela.

#### Scenario: Pasta negada, arquivo novo
- **WHEN** o perfil da pasta tem `file-deny = ["~/proj/privado"]`, o confinamento vale, e um contêiner com `--privileged` e `~/proj` montado desmonta a cobertura de `privado`, lê `privado/velho.txt` e cria `privado/novo.txt`
- **THEN** a leitura e a criação falham com `EPERM`, e `novo.txt` não aparece no host

#### Scenario: Criado no host depois
- **WHEN** o usuário cria `~/proj/privado/outro.txt` no host com o Docker já rodando
- **THEN** um contêiner com `--privileged` e `~/proj` montado não lê `privado/outro.txt`, nem depois de desmontar a cobertura

#### Scenario: Segredo criado pelo agente
- **WHEN** o agente criou `chave.pem` numa sessão anterior e um contêiner com a pasta montada lê o arquivo
- **THEN** o contêiner lê o conteúdo

#### Scenario: Arquivo que o usuário não lê
- **WHEN** o confinamento vale e uma cobertura de `/etc` é um arquivo que o usuário não lê, como o `/etc/gshadow`, com os binários estáticos das releases
- **THEN** o arquivo é marcado e o Docker privado sobe
