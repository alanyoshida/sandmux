## MODIFIED Requirements

### Requirement: Cópia de mão única
`sandmux import <harness>` MUST copiar o estado do harness do HOME real para o HOME sintético, fora do sandbox. O comando MUST NOT alterar nenhum arquivo do HOME real, e nada do HOME sintético MUST voltar para o HOME real.

#### Scenario: Importação
- **WHEN** o usuário roda `sandmux import opencode` com o opencode já configurado no host
- **THEN** a configuração aparece no HOME sintético do opencode e os arquivos reais continuam idênticos

#### Scenario: Harness sem estado
- **WHEN** o harness nunca foi configurado no host
- **THEN** o comando informa que não há o que importar e termina com código zero

#### Scenario: Preset sem importação
- **WHEN** o usuário roda `sandmux import xyz`, com um nome que não é de preset
- **THEN** o comando termina com erro, listando os presets, que agora têm todos importação, inclusive o `shell`

### Requirement: Primeira execução sem terminal interativo
Sem terminal interativo, a primeira execução de um harness com estado real para importar MUST exigir `--import` ou `--no-import`. Sem uma das duas, o `run` MUST terminar com erro, sem lançar o sandbox e sem criar o HOME sintético. O preset `shell` é a exceção: sem uma das opções, ele MUST começar sem importar.

#### Scenario: Sem opção
- **WHEN** `sandmux run opencode` é chamado pela primeira vez por um script, sem terminal interativo e sem as opções
- **THEN** o comando termina com erro explicando as duas opções

#### Scenario: Com opção
- **WHEN** o script chama `sandmux run opencode --no-import`
- **THEN** o HOME sintético vazio é criado e o harness é lançado

#### Scenario: Opção numa execução posterior
- **WHEN** o HOME sintético já existe e o usuário passa `--import` ou `--no-import`
- **THEN** a opção é ignorada e o harness é lançado

#### Scenario: Shell sem terminal
- **WHEN** `sandmux run shell -- -c 'comando'` é chamado pela primeira vez por um script, com um `.bashrc` real
- **THEN** o shell roda o comando com o arquivo mínimo gerado, sem importar e sem erro

## ADDED Requirements

### Requirement: Importação do shell
O preset `shell` MUST ter importação, com a tabela do shell que o preset usa: o do usuário quando está nas pastas de sistema, e o `bash` caso contrário. Só os arquivos de inicialização desse shell e as pastas dos frameworks dele MUST ser copiados, sem as pastas `.git`, `cache` e `log` dos frameworks. Arquivos de outros programas, como o `starship.toml`, MUST NOT ser copiados.

#### Scenario: Bash
- **WHEN** o shell do usuário é o `bash` e ele roda `sandmux import shell`
- **THEN** o `.bashrc` e o `.oh-my-bash` vão para o HOME sintético do `shell`, sem o `.zshrc` e sem o `.git` do framework

#### Scenario: Programa de prompt
- **WHEN** o usuário tem `~/.config/starship.toml`
- **THEN** o arquivo não é copiado

### Requirement: Arquivos de cada shell
A tabela do `bash` MUST ter `.bashrc`, `.bash_aliases`, `.bash_profile`, `.bash_login`, `.bash_logout`, `.profile`, `.inputrc`, `.oh-my-bash` e `.bash_it`. A do `zsh` MUST ter `.zshrc`, `.zshenv`, `.zprofile`, `.zlogin`, `.zlogout`, `.p10k.zsh` e `.oh-my-zsh`. A do `fish` MUST ter `.config/fish` sem o `fish_variables`, `.config/omf` e `.local/share/omf`.

#### Scenario: Zsh
- **WHEN** o shell do usuário é o `zsh`
- **THEN** o `.zshrc` e o `.oh-my-zsh` são copiados, e o `.bashrc` não

#### Scenario: Variáveis universais do fish
- **WHEN** o shell do usuário é o `fish`
- **THEN** o `config.fish` é copiado, e o `fish_variables` não

### Requirement: Segredos nos arquivos do shell
Nos arquivos de inicialização do shell, toda atribuição de variável com nome de credencial MUST ter o valor trocado por um valor falso, com ou sem `export`, e no fish pelo `set`. Nos arquivos dos frameworks, MUST ser trocados os valores com formato de token.

#### Scenario: Token exportado
- **WHEN** o `.bashrc` real tem `export GITHUB_TOKEN=<token>`
- **THEN** o `.bashrc` importado tem um valor falso no lugar, e o resto do arquivo é mantido

#### Scenario: Variável no fish
- **WHEN** o `config.fish` real tem `set -gx OPENAI_API_KEY <chave>`
- **THEN** o arquivo importado tem um valor falso no lugar

### Requirement: Prompt marcado no shell importado
O arquivo principal de inicialização importado (`.bashrc`, `.zshrc` ou `config.fish`) MUST ganhar no fim um trecho que põe `[sandmux]` na frente do prompt do usuário.

#### Scenario: Prompt do usuário
- **WHEN** o `.bashrc` importado define um `PS1` próprio
- **THEN** o prompt dentro do sandbox é `[sandmux] ` seguido desse `PS1`

### Requirement: Primeira execução do shell com importação
Na primeira execução do `shell` com a importação aceita, a importação MUST substituir o arquivo mínimo gerado ao criar o HOME sintético. Num HOME sintético que já existe, o arquivo gerado MUST aparecer como diferente, e `sandmux import shell --overwrite` MUST substituí-lo.

#### Scenario: Primeira execução
- **WHEN** o usuário aceita a importação na primeira execução do `shell`
- **THEN** o `.bashrc` do HOME sintético é o real, com os segredos trocados e a marca no prompt

#### Scenario: HOME já criado
- **WHEN** o HOME sintético do `shell` já existe e o usuário roda `sandmux import shell`
- **THEN** o `.bashrc` aparece como diferente, e com `--overwrite` ele é substituído
