## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: Arquivos novos no Docker confinado
Enquanto o Docker privado de uma pasta roda com o confinamento por eBPF, o daemon MUST vigiar a pasta de trabalho e negar aos processos do Docker, em até 1 segundo depois da criação:

- um arquivo com cara de segredo criado depois, pela configuração da pasta, que o agente não criou;
- um arquivo ou uma pasta criados dentro de uma pasta negada.

Sem vigias de inotify disponíveis, o daemon MUST refazer a varredura da pasta a cada 10 segundos e mostrar o aviso no `sandmux stats`.

#### Scenario: .env criado no host
- **WHEN** o confinamento vale e o usuário cria um `.env` na pasta de trabalho com o Docker já rodando
- **THEN** depois de 1 segundo, um contêiner com `--privileged` e a pasta montada não lê o `.env`

#### Scenario: Segredo criado pelo agente depois
- **WHEN** o agente cria `chave.pem` na pasta com o Docker já rodando
- **THEN** um contêiner com a pasta montada lê o conteúdo

#### Scenario: Sem vigias
- **WHEN** o limite de vigias de inotify do usuário acabou e o Docker da pasta sobe confinado
- **THEN** `sandmux stats <id>` mostra que os arquivos novos são vistos por varredura a cada 10 segundos

### Requirement: Política de arquivos ao vivo no Docker
Com o confinamento por eBPF, uma mudança no `file-deny`, no `file-allow`, no `secret-files` ou no `secret-files-off` da configuração da pasta MUST valer no Docker que já está rodando, sem subir de novo:

- um caminho que passa a ser negado MUST ser negado na operação seguinte;
- um caminho que deixa de ser negado MUST deixar de ser negado no kernel.

A cobertura por montagem feita na subida MUST continuar até o Docker da pasta subir de novo.

#### Scenario: Negar ao vivo
- **WHEN** o confinamento vale, um contêiner lê `dados.csv`, e o usuário acrescenta `file-deny = ["~/proj/dados.csv"]` à configuração da pasta
- **THEN** a leitura seguinte de `dados.csv` por um contêiner falha com `EPERM`

#### Scenario: Liberar ao vivo
- **WHEN** o confinamento vale, a pasta tem um `.env` negado, e o usuário acrescenta `file-allow = ["~/proj/.env"]`
- **THEN** um contêiner que abre o `.env` recebe o arquivo coberto, vazio, até o Docker da pasta subir de novo, quando lê o conteúdo
