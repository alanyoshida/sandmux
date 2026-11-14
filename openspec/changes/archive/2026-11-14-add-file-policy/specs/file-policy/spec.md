# Spec Delta

## Purpose
Define a política de arquivos que muda com o agente em execução: negar e liberar arquivos e pastas, esconder os arquivos com cara de segredo da pasta de trabalho, fechar a brecha do `.git/commondir`, e editar e acompanhar os acessos pela linha de comando e pela TUI.

## ADDED Requirements

### Requirement: Negar e liberar arquivos na hora
Uma abertura de arquivo cujo caminho, na visão do agente, casa com um padrão de `file-deny` MUST falhar com `EACCES` e ser registrada como bloqueada. Um padrão de `file-allow` MUST vencer `file-deny` e o esconder dos segredos. Uma mudança na política MUST valer na abertura seguinte, sem relançar o agente.

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

### Requirement: Sem .git/commondir criado pelo agente
Criar `.git/commondir` sob a pasta de trabalho MUST falhar com `EACCES`, por abertura, rename, link ou symlink. As chamadas `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` e `symlinkat` MUST ser feitas pelo supervisor no lugar do processo, conferindo as regras de escrita do Landlock nas pastas de origem e de destino, e o `file-deny` no destino.

#### Scenario: Rename para commondir
- **WHEN** o agente cria `x` e roda `mv x .git/commondir` num repositório na pasta de trabalho
- **THEN** o `mv` falha, e `.git/commondir` não existe no host

#### Scenario: Rename comum
- **WHEN** o agente roda `mv a.txt b.txt` na pasta de trabalho
- **THEN** o arquivo é renomeado

### Requirement: Edição da política de arquivos pela linha de comando
`sandmux files deny|allow|forget <padrão> <id>` MUST mudar a política de arquivos da aba da sessão `id`, com efeito na abertura seguinte. Um padrão inválido MUST ser recusado com a mensagem do formato aceito.

#### Scenario: Liberar de novo
- **WHEN** o usuário negou `dados.sql` na sessão 1 e roda `sandmux files forget dados.sql 1`
- **THEN** a abertura seguinte de `dados.sql` funciona

### Requirement: Tela de arquivos
Na TUI, Ctrl+b `f` MUST abrir a tela de arquivos da aba ativa, com os acessos recentes da sessão e os caminhos nunca vistos nesta pasta de trabalho marcados. A tecla `b` MUST negar o caminho escolhido na aba (o caminho exato), e `l` MUST liberá-lo.

#### Scenario: Bloquear da tela
- **WHEN** o agente leu `dados.sql`, o usuário abre a tela, escolhe o caminho e aperta `b`
- **THEN** a leitura seguinte de `dados.sql` falha com `EACCES`
