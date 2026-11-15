## MODIFIED Requirements

### Requirement: Preset shell
O preset `shell` MUST lançar um shell interativo confinado, sem harness, com HOME sintético próprio. Ele MUST usar o shell do usuário quando esse shell está nas pastas de sistema, e o `bash` caso contrário, com um arquivo de configuração mínimo gerado ao criar o HOME sintético. Os arquivos de configuração de shell do HOME real MUST entrar só pela importação, com os segredos trocados por valores falsos.

#### Scenario: Início
- **WHEN** o usuário roda `sandmux run shell`
- **THEN** um shell interativo abre na pasta de trabalho, com as toolchains conhecidas no `PATH`

#### Scenario: Configuração real do shell
- **WHEN** o `.bashrc` real do usuário exporta um token
- **THEN** o token real não existe dentro do shell confinado: sem importação, a variável não existe; com importação, ela tem um valor falso

#### Scenario: Shell fora do sistema
- **WHEN** o shell do usuário está instalado dentro do HOME
- **THEN** o preset usa o `bash`

#### Scenario: Ajuste do usuário
- **WHEN** o usuário edita o arquivo de configuração do shell no HOME sintético
- **THEN** o ajuste vale nas execuções seguintes
