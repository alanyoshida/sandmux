# harness-state-import Specification

## Purpose

Define como a configuração que o usuário já tem de um harness é levada para o HOME sintético do sandmux: o que é copiado, o que fica de fora, como os segredos são trocados por valores falsos e quando a importação é oferecida.

## Requirements

### Requirement: Cópia de mão única
`sandmux import <harness>` MUST copiar o estado do harness do HOME real para o HOME sintético, fora do sandbox. O comando MUST NOT alterar nenhum arquivo do HOME real, e nada do HOME sintético MUST voltar para o HOME real.

#### Scenario: Importação
- **WHEN** o usuário roda `sandmux import opencode` com o opencode já configurado no host
- **THEN** a configuração aparece no HOME sintético do opencode e os arquivos reais continuam idênticos

#### Scenario: Harness sem estado
- **WHEN** o harness nunca foi configurado no host
- **THEN** o comando informa que não há o que importar e termina com código zero

#### Scenario: Preset sem importação
- **WHEN** o usuário roda `sandmux import shell`
- **THEN** o comando termina com erro, informando que o preset `shell` não tem importação

### Requirement: Só o que está na tabela é copiado
A importação MUST copiar apenas os itens da tabela do harness. Qualquer arquivo ou pasta do estado do harness que não está na tabela MUST ficar de fora, inclusive os que uma versão futura do harness venha a criar.

#### Scenario: Arquivo desconhecido
- **WHEN** o estado real do harness contém um arquivo que a tabela não lista
- **THEN** o arquivo não é copiado e aparece no relatório como ignorado

### Requirement: Itens importados do opencode
Para o opencode, a importação MUST copiar de `~/.config/opencode` os arquivos de configuração (`opencode.json`, `opencode.jsonc`, `tui.jsonc`), as pastas `plugins` e `skills`, os demais scripts do usuário e os manifestos de pacotes, e MUST copiar `~/.local/share/opencode/auth.json` com os segredos trocados. Pastas de dependências instaladas MUST ficar de fora.

#### Scenario: MCPs cadastrados
- **WHEN** a configuração real do opencode tem servidores MCP cadastrados
- **THEN** os mesmos servidores aparecem cadastrados no opencode confinado

#### Scenario: Skills
- **WHEN** o usuário tem skills em `~/.config/opencode/skills`
- **THEN** elas existem no HOME sintético depois da importação

### Requirement: Itens importados do hermes
Para o hermes, a importação MUST copiar de `~/.hermes` o `config.yaml`, o `SOUL.md` e as pastas `skills`, `memories`, `plugins` e `hooks`, e MUST copiar `.env` e `auth.json` com os segredos trocados.

#### Scenario: Memórias
- **WHEN** o usuário tem memórias em `~/.hermes/memories`
- **THEN** o hermes confinado as encontra depois da importação

#### Scenario: Canais e agendamentos
- **WHEN** o estado real do hermes tem pareamento de canais de mensagem e tarefas agendadas
- **THEN** nada disso é copiado nesta fatia

### Requirement: Histórico não é copiado
A importação MUST NOT copiar histórico nem dados de execução: bancos de sessões, conversas, saídas de ferramenta, logs, caches, snapshots, filas de mensagens, arquivos de trava e o estado do gateway.

#### Scenario: Sessões do opencode
- **WHEN** o usuário importa o opencode
- **THEN** o banco de sessões, os logs, os snapshots e as saídas de ferramenta não existem no HOME sintético

#### Scenario: Sessões do hermes
- **WHEN** o usuário importa o hermes
- **THEN** as pastas de sessões, logs e caches e os arquivos do gateway não existem no HOME sintético

### Requirement: Segredos trocados por valores falsos
Nos arquivos importados, todo valor de credencial MUST ser trocado por um valor falso antes de ser gravado no HOME sintético. Um valor MUST ser tratado como credencial quando a chave tem nome de credencial ou quando o valor tem formato de token. Na dúvida, o valor MUST ser trocado.

#### Scenario: Arquivo de credenciais
- **WHEN** o `auth.json` real do harness contém uma API key
- **THEN** o `auth.json` do HOME sintético tem um valor falso no lugar e mantém a mesma estrutura

#### Scenario: Token dentro de configuração
- **WHEN** um servidor MCP cadastrado na configuração tem um token num header ou numa variável de ambiente
- **THEN** o token é trocado por um valor falso e o resto do cadastro é mantido

#### Scenario: Variáveis do hermes
- **WHEN** o `.env` real do hermes tem chaves de API e variáveis comuns
- **THEN** as chaves de API são trocadas e as variáveis que não são credenciais são mantidas

#### Scenario: Nenhum segredo real no HOME sintético
- **WHEN** a importação termina
- **THEN** nenhum dos valores de credencial dos arquivos reais aparece em qualquer arquivo do HOME sintético

### Requirement: Valor falso reconhecível
Cada valor falso MUST ter o prefixo `sandmux-fake-`, ser diferente para cada segredo trocado e não derivar do valor real.

#### Scenario: Identificação
- **WHEN** o usuário abre um arquivo importado
- **THEN** ele reconhece pelo prefixo quais valores foram trocados

### Requirement: Manifesto dos segredos trocados
A importação MUST gravar um manifesto que lista, para cada segredo trocado, o arquivo de origem, a chave e o valor falso correspondente, sem o valor real. O manifesto MUST ficar no diretório de dados do sandmux, fora do HOME sintético e fora do alcance do sandbox.

#### Scenario: Conteúdo do manifesto
- **WHEN** a importação troca três segredos
- **THEN** o manifesto tem três entradas e nenhum valor real

#### Scenario: Manifesto fora do sandbox
- **WHEN** um processo confinado procura o manifesto
- **THEN** o arquivo não existe dentro do sandbox

### Requirement: Relatório da importação
Ao terminar, a importação MUST mostrar o que foi copiado, o que foi ignorado e quais chaves tiveram o valor trocado. O relatório MUST NOT mostrar nenhum valor real de credencial.

#### Scenario: Saída do comando
- **WHEN** a importação termina
- **THEN** a saída lista os itens copiados, os ignorados e o nome de cada chave trocada, sem os valores

### Requirement: Reimportação copia só o que é novo
Quando o HOME sintético já existe, `sandmux import` MUST copiar apenas os arquivos que ainda não existem nele, MUST NOT sobrescrever nenhum arquivo e MUST listar os arquivos que diferem entre os dois lados. A opção `--overwrite` MUST substituir os arquivos do HOME sintético pelos importados.

#### Scenario: Skill nova
- **WHEN** o usuário cria uma skill no host depois da primeira importação e roda `sandmux import` de novo
- **THEN** a skill nova é copiada e os arquivos existentes ficam intactos

#### Scenario: Configuração alterada no host
- **WHEN** um arquivo de configuração foi alterado no host depois da primeira importação
- **THEN** o arquivo do HOME sintético não muda e ele aparece na lista de diferenças

#### Scenario: Ajuste feito dentro do sandbox
- **WHEN** o agente ou o usuário alterou um arquivo no HOME sintético e a importação é rodada de novo sem opções
- **THEN** a alteração é preservada

#### Scenario: Sobrescrita explícita
- **WHEN** o usuário roda `sandmux import opencode --overwrite`
- **THEN** os arquivos da tabela são substituídos pelos do host, com os segredos trocados de novo

### Requirement: Importação oferecida na primeira execução
Quando `sandmux run` é chamado para um harness que ainda não tem HOME sintético e existe estado real para importar, o `run` MUST perguntar uma vez se deve importar. Aceitando, ele MUST importar, mostrar o relatório e seguir para o lançamento. Recusando, ele MUST criar o HOME sintético vazio e não perguntar de novo.

#### Scenario: Aceite
- **WHEN** o usuário aceita a importação na primeira execução
- **THEN** a configuração é importada, o relatório é mostrado e o harness inicia já configurado

#### Scenario: Recusa
- **WHEN** o usuário recusa a importação
- **THEN** o harness inicia com o HOME sintético vazio e as execuções seguintes não perguntam de novo

#### Scenario: Importação posterior
- **WHEN** o usuário recusou na primeira execução e depois roda `sandmux import`
- **THEN** a importação é feita normalmente

#### Scenario: Nada a importar
- **WHEN** o harness não tem estado real no host
- **THEN** o `run` cria o HOME sintético vazio sem perguntar

### Requirement: Primeira execução sem terminal interativo
Sem terminal interativo, a primeira execução de um harness com estado real para importar MUST exigir `--import` ou `--no-import`. Sem uma das duas, o `run` MUST terminar com erro, sem lançar o sandbox e sem criar o HOME sintético.

#### Scenario: Sem opção
- **WHEN** `sandmux run opencode` é chamado pela primeira vez por um script, sem terminal interativo e sem as opções
- **THEN** o comando termina com erro explicando as duas opções

#### Scenario: Com opção
- **WHEN** o script chama `sandmux run opencode --no-import`
- **THEN** o HOME sintético vazio é criado e o harness é lançado

#### Scenario: Opção numa execução posterior
- **WHEN** o HOME sintético já existe e o usuário passa `--import` ou `--no-import`
- **THEN** a opção é ignorada e o harness é lançado
