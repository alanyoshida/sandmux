## MODIFIED Requirements

### Requirement: Negar e liberar arquivos na hora
Uma abertura de arquivo cujo caminho, na visão do agente, casa com um padrão de `file-deny` MUST falhar com `EACCES` e ser registrada como bloqueada. Um padrão de `file-allow` MUST vencer `file-deny` e o esconder dos segredos. Uma mudança na política MUST valer na abertura seguinte, sem relançar o agente; numa aba confinada pelo kernel, MUST valer em até 1 s.

#### Scenario: Negar com a sessão viva
- **WHEN** o agente lê `dados.sql` na pasta de trabalho e o usuário roda `sandmux files deny dados.sql 1`
- **THEN** a leitura seguinte de `dados.sql` falha com `EACCES`, e o registro tem a linha com `file-deny`

#### Scenario: Pasta negada
- **WHEN** a configuração tem `file-deny = ["~/proj/privado"]`
- **THEN** nenhum arquivo dentro de `~/proj/privado` abre

### Requirement: Arquivos com cara de segredo escondidos
Na pasta de trabalho, um arquivo cujo nome casa com a lista de segredos MUST NOT abrir (`ENOENT`), a menos que o próprio agente o tenha criado. A lista padrão MUST ser `.env` e `.env.*` (menos `.env.example`, `.env.sample` e `.env.template`), `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.kdbx`, `id_rsa*`, `id_ecdsa*` e `id_ed25519*` (menos os `.pub`), `.npmrc`, `.pypirc`, `.netrc`, `credentials*.json`, `*.tfstate` e `.git-credentials`, ajustável por `secret-files` e `secret-files-off`.

#### Scenario: .env do usuário
- **WHEN** a pasta de trabalho tem um `.env` do usuário e o agente roda `cat .env`
- **THEN** o `cat` falha com "No such file or directory", e o registro tem a linha com `segredo`

#### Scenario: Segredo criado pelo agente
- **WHEN** o agente cria `chave.pem` e depois a lê, inclusive num lançamento seguinte
- **THEN** a leitura funciona

#### Scenario: Exemplo continua visível
- **WHEN** a pasta de trabalho tem `.env.example`
- **THEN** o agente lê o arquivo

#### Scenario: Segredo criado no host com a aba confinada pelo kernel
- **WHEN** a aba é confinada pelo kernel e o usuário cria `nova.pem` na pasta de trabalho, no host, com a sessão aberta
- **THEN** depois de no máximo 1 s, o agente não abre `nova.pem` (`ENOENT`)

