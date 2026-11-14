# Spec Delta

## MODIFIED Requirements

### Requirement: Verificações do run antes de criar a sessão
O `run` com terminal MUST fazer, antes de pedir a sessão, as mesmas verificações e perguntas do `run` direto: arquivo de configuração inválido, perfil inexistente, preset desconhecido, harness não instalado, pasta de trabalho perigosa, caminho inexistente em `--ro`, `--rw` ou nas listas da configuração efetiva, variável de `pass-env` ausente e importação na primeira execução. Uma recusa MUST terminar o comando com a mesma mensagem e sem criar sessão. Os avisos do lançamento MUST aparecer no terminal que pediu a sessão.

#### Scenario: Pasta perigosa
- **WHEN** o usuário roda `sandmux run shell` num terminal, com o HOME como pasta atual
- **THEN** o comando termina com o erro de pasta perigosa e nenhuma sessão aparece em `sandmux ls`

#### Scenario: Aviso de variável passada
- **WHEN** o usuário roda `sandmux run opencode --pass-env OPENROUTER_API_KEY` num terminal
- **THEN** o aviso de que a variável fica ao alcance do agente aparece no terminal antes da interface do harness

#### Scenario: Configuração inválida
- **WHEN** o arquivo de configuração tem uma chave desconhecida e o usuário roda `sandmux run shell` num terminal
- **THEN** o comando termina com o erro do arquivo, com a linha, e nenhuma sessão aparece em `sandmux ls`

#### Scenario: Perfil inexistente
- **WHEN** o usuário roda `sandmux run shell --profile nada` num terminal e não existe esse perfil
- **THEN** o comando termina com erro listando os perfis e nenhuma sessão é criada

### Requirement: Reiniciar uma sessão
`sandmux restart <id>` MUST relançar uma sessão encerrada ou parada com o mesmo harness, a mesma pasta, as mesmas opções do `run` e o mesmo perfil nomeado, resolvendo as camadas com a configuração do momento, e anexar o terminal quando houver um. Os valores das variáveis de `pass-env` da política efetiva MUST vir do ambiente do terminal que pediu o reinício, e uma variável ausente MUST impedir o reinício com erro. Uma sessão viva MUST NOT ser reiniciada.

#### Scenario: Reinício de sessão encerrada
- **WHEN** o harness da sessão 2 terminou e o usuário roda `sandmux restart 2`
- **THEN** o harness sobe de novo na mesma pasta, o terminal se anexa e o estado passa a viva

#### Scenario: Variável ausente
- **WHEN** a sessão 3 foi criada com `--pass-env OPENROUTER_API_KEY` e o usuário roda `sandmux restart 3` num terminal sem a variável
- **THEN** o comando termina com erro citando a variável e a sessão continua parada

#### Scenario: Sessão viva
- **WHEN** o usuário roda `sandmux restart 1` e a sessão 1 está viva
- **THEN** o comando termina com erro dizendo que a sessão está viva, sem afetá-la

#### Scenario: Configuração mudou
- **WHEN** a sessão 2 foi criada com `--profile vm`, o usuário tira `kvm = true` do perfil `vm` e roda `sandmux restart 2`
- **THEN** o harness relançado não tem `/dev/kvm`

#### Scenario: Perfil apagado
- **WHEN** a sessão 2 foi criada com `--profile vm`, o perfil `vm` foi apagado e o usuário roda `sandmux restart 2`
- **THEN** o comando termina com erro dizendo que o perfil não existe, e a sessão continua como estava

### Requirement: Escrita no clipboard bloqueada por padrão
O daemon MUST retirar da saída da sessão, antes de enviá-la aos clientes, as sequências que pedem ao terminal do usuário para gravar no clipboard (OSC 52). Quando a política efetiva liga `clipboard`, por `sandmux run --allow-clipboard` ou pela configuração, a gravação MUST passar para aquela sessão. A política MUST ser resolvida de novo no reinício e na restauração.

#### Scenario: Padrão
- **WHEN** o harness de uma sessão sem a opção escreve uma sequência OSC 52 de gravação
- **THEN** nenhum cliente anexado recebe a sequência e o clipboard do usuário não muda

#### Scenario: Liberado
- **WHEN** a sessão foi criada com `--allow-clipboard` e o harness escreve uma sequência OSC 52 de gravação
- **THEN** os clientes anexados recebem a sequência

#### Scenario: Liberado pelo perfil
- **WHEN** o perfil do projeto tem `clipboard = true`, a sessão foi criada na pasta do projeto sem opções e o harness escreve uma sequência OSC 52 de gravação
- **THEN** os clientes anexados recebem a sequência
