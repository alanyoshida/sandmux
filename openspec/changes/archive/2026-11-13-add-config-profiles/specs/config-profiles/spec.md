# Spec Delta

## Purpose
Define o arquivo de configuração do sandmux e as camadas de política: onde o arquivo fica, o que ele pode ajustar, como ajustes globais, perfis nomeados, perfis de projeto e opções do `run` se combinam, como o usuário consulta e altera a configuração, e por que o agente nunca consegue alterá-la.

## ADDED Requirements

### Requirement: Arquivo de configuração
O sandmux MUST ler a configuração de `$XDG_CONFIG_HOME/sandmux/config.toml`, ou de `~/.config/sandmux/config.toml` quando a variável não está definida ou não é um caminho absoluto. O arquivo é opcional: sem ele, o comportamento MUST ser o mesmo de uma configuração vazia.

#### Scenario: Sem arquivo
- **WHEN** o arquivo de configuração não existe e o usuário roda `sandmux run shell`
- **THEN** o lançamento segue só com a baseline, o preset e as opções do `run`, sem aviso

#### Scenario: Pasta de configuração alternativa
- **WHEN** `XDG_CONFIG_HOME` é `/tmp/cfg` e existe `/tmp/cfg/sandmux/config.toml`
- **THEN** o sandmux usa esse arquivo e ignora `~/.config/sandmux/config.toml`

### Requirement: Seções do arquivo
O arquivo MUST aceitar três tipos de seção: `[global]`, que vale para toda execução; `[profile.<nome>]`, um perfil nomeado; e `[project."<pasta>"]`, o perfil de um projeto, ligado ao caminho absoluto de uma pasta. Nenhuma outra seção MUST ser aceita.

#### Scenario: As três seções
- **WHEN** o arquivo tem `[global]`, `[profile.web]` e `[project."/home/ana/proj"]`
- **THEN** o arquivo é aceito e cada seção é uma camada própria

#### Scenario: Seção desconhecida
- **WHEN** o arquivo tem uma seção `[rede]`
- **THEN** o arquivo é considerado inválido

#### Scenario: Projeto com caminho relativo
- **WHEN** o arquivo tem `[project."proj"]`
- **THEN** o arquivo é considerado inválido

### Requirement: Chaves de ajuste
Cada seção MUST aceitar só estas chaves: `ro` e `rw`, listas de caminhos absolutos ou iniciados por `~/`; `pass-env`, lista de nomes de variáveis; e `repo`, `kvm`, `gpu` e `clipboard`, valores verdadeiro ou falso. Cada chave MUST ter o mesmo efeito da opção correspondente do `run`: `--ro`, `--rw`, `--pass-env`, `--repo`, `--kvm`, `--no-gpu` (quando `gpu` é falso) e `--allow-clipboard`.

#### Scenario: Pasta extra no perfil
- **WHEN** o perfil do projeto tem `ro = ["~/docs"]` e o usuário roda `sandmux run shell` na pasta do projeto
- **THEN** o processo confinado lê `~/docs` e não consegue escrever nela

#### Scenario: Chave desconhecida
- **WHEN** a seção `[global]` tem `rede = "aberta"`
- **THEN** o arquivo é considerado inválido

#### Scenario: Caminho relativo
- **WHEN** a seção `[global]` tem `rw = ["saida"]`
- **THEN** o arquivo é considerado inválido

### Requirement: Arquivo inválido recusa o lançamento
Um arquivo de configuração malformado, com seção ou chave desconhecida, valor de tipo errado ou caminho relativo MUST fazer o `run` terminar com erro antes de lançar o sandbox, com o caminho do arquivo, a linha e o motivo na mensagem. O sandmux MUST NOT ignorar uma parte do arquivo e seguir com o resto.

#### Scenario: TOML malformado
- **WHEN** o arquivo tem um colchete sem fechar na linha 4 e o usuário roda `sandmux run shell`
- **THEN** o `run` termina com erro citando o arquivo e a linha 4 e não lança o sandbox

#### Scenario: Valor de tipo errado
- **WHEN** a seção `[global]` tem `kvm = "sim"`
- **THEN** o `run` termina com erro dizendo que `kvm` precisa ser verdadeiro ou falso

### Requirement: Variáveis passadas pela configuração
A chave `pass-env` MUST guardar só nomes de variáveis. Os valores MUST vir do ambiente de quem pede o lançamento, como na opção `--pass-env`, com o mesmo aviso a cada execução. Uma variável listada na configuração efetiva que não existe no ambiente MUST fazer o `run` terminar com erro.

#### Scenario: Nome no perfil
- **WHEN** o perfil `ia` tem `pass-env = ["OPENROUTER_API_KEY"]` e o usuário roda `sandmux run opencode --profile ia` com a variável definida
- **THEN** o harness recebe o valor, o aviso de `--pass-env` aparece e o valor não está em nenhum arquivo do sandmux

#### Scenario: Valor no arquivo
- **WHEN** o arquivo tem `pass-env = ["OPENROUTER_API_KEY=sk-123"]`
- **THEN** o arquivo é considerado inválido, porque o item não é um nome de variável

### Requirement: Camadas de política
A política de um lançamento MUST ser composta por camadas, da menos para a mais específica: baseline embutida, preset do harness, `[global]`, perfil nomeado, perfil do projeto e opções do `run`. Uma camada que não define uma chave MUST deixar valer o que vem das camadas anteriores.

#### Scenario: Ajuste global
- **WHEN** `[global]` tem `gpu = false` e nenhuma outra camada define `gpu`
- **THEN** nenhum dispositivo de GPU existe dentro do sandbox

#### Scenario: Camada sem a chave
- **WHEN** `[global]` tem `kvm = true` e o perfil do projeto não define `kvm`
- **THEN** `/dev/kvm` existe dentro do sandbox lançado na pasta do projeto

### Requirement: Precedência nas chaves de liga e desliga
Nas chaves `repo`, `kvm`, `gpu` e `clipboard`, MUST valer o valor da camada mais específica que definiu a chave.

#### Scenario: Projeto afrouxa o global
- **WHEN** `[global]` tem `gpu = false` e o perfil do projeto tem `gpu = true`
- **THEN** o sandbox lançado na pasta do projeto enxerga a GPU

#### Scenario: Opção do run vence o perfil
- **WHEN** o perfil do projeto tem `kvm = true` e o usuário roda `sandmux run shell --no-kvm` na pasta do projeto
- **THEN** `/dev/kvm` não existe dentro do sandbox

### Requirement: Listas acumuladas entre camadas
Nas chaves `ro`, `rw` e `pass-env`, os itens de todas as camadas MUST se somar. Quando o mesmo caminho aparece em `ro` numa camada e em `rw` noutra, MUST valer a camada mais específica. O mesmo caminho em `ro` e `rw` da mesma camada MUST tornar a configuração inválida.

#### Scenario: Soma
- **WHEN** `[global]` tem `ro = ["~/docs"]` e o perfil do projeto tem `ro = ["~/ref"]`
- **THEN** o sandbox lançado na pasta do projeto enxerga `~/docs` e `~/ref`

#### Scenario: Projeto restringe uma pasta global
- **WHEN** `[global]` tem `rw = ["~/saida"]` e o perfil do projeto tem `ro = ["~/saida"]`
- **THEN** no sandbox lançado na pasta do projeto, `~/saida` é somente leitura

#### Scenario: Conflito na mesma camada
- **WHEN** `[global]` tem `ro = ["~/docs"]` e `rw = ["~/docs"]`
- **THEN** o arquivo é considerado inválido

### Requirement: Opções negativas no run
`sandmux run` MUST aceitar `--no-kvm`, `--gpu`, `--no-allow-clipboard` e `--no-repo`, que definem a chave correspondente como desligada (ou, em `--gpu`, ligada) na camada das opções do `run`. Uma opção e a sua negativa na mesma chamada MUST fazer o `run` terminar com erro.

#### Scenario: Desligar o clipboard liberado no perfil
- **WHEN** o perfil `web` tem `clipboard = true` e o usuário roda `sandmux run shell --profile web --no-allow-clipboard`
- **THEN** a escrita no clipboard fica bloqueada naquela sessão

#### Scenario: Opções contraditórias
- **WHEN** o usuário roda `sandmux run shell --kvm --no-kvm`
- **THEN** o `run` termina com erro e não lança o sandbox

### Requirement: Perfil nomeado
`sandmux run --profile <nome>` MUST aplicar a seção `[profile.<nome>]` como camada. Um nome sem seção MUST fazer o `run` terminar com erro, listando os perfis existentes. Sem `--profile`, nenhum perfil nomeado MUST ser aplicado.

#### Scenario: Perfil existente
- **WHEN** existe `[profile.vm]` com `kvm = true` e o usuário roda `sandmux run shell --profile vm`
- **THEN** `/dev/kvm` existe dentro do sandbox

#### Scenario: Perfil inexistente
- **WHEN** o usuário roda `sandmux run shell --profile nada` e não há `[profile.nada]`
- **THEN** o `run` termina com erro, lista os perfis existentes e não lança o sandbox

### Requirement: Perfil do projeto pela pasta
O perfil de projeto MUST ser aplicado sem opção quando a pasta de trabalho, com os links resolvidos, é a pasta da seção ou uma subpasta dela. Quando mais de uma seção casa, MUST valer só a da pasta mais próxima da pasta de trabalho.

#### Scenario: Subpasta do projeto
- **WHEN** existe `[project."/home/ana/proj"]` e o usuário roda `sandmux run shell` em `/home/ana/proj/api`
- **THEN** o perfil de `/home/ana/proj` é aplicado

#### Scenario: Projetos aninhados
- **WHEN** existem `[project."/home/ana/proj"]` e `[project."/home/ana/proj/api"]` e a pasta de trabalho é `/home/ana/proj/api/src`
- **THEN** só o perfil de `/home/ana/proj/api` é aplicado

#### Scenario: Pasta fora do projeto
- **WHEN** existe `[project."/home/ana/proj"]` e a pasta de trabalho é `/home/ana/proj2`
- **THEN** nenhum perfil de projeto é aplicado

### Requirement: Nada dentro da pasta de trabalho altera a política
O sandmux MUST NOT ler configuração de dentro da pasta de trabalho nem de um repositório que a contém. Um arquivo com qualquer nome que o agente possa criar ali MUST NOT mudar a política de nenhum lançamento.

#### Scenario: Arquivo plantado no projeto
- **WHEN** a pasta de trabalho tem `.sandmux.toml` e `.config/sandmux/config.toml` com `rw = ["~"]`
- **THEN** o lançamento seguinte na mesma pasta não monta o HOME e não usa nada desses arquivos

### Requirement: Configuração fora do alcance de escrita do agente
O `run` MUST recusar como pasta de trabalho, e em `--rw` ou na chave `rw`, a pasta de configuração do sandmux, uma pasta dentro dela ou uma pasta que a contém. Em somente leitura a pasta MUST poder entrar, porque não guarda segredo.

#### Scenario: Pasta de configuração com escrita
- **WHEN** o usuário passa `--rw ~/.config`
- **THEN** o `run` termina com erro, explica que o agente poderia alterar a configuração do sandmux e não lança o sandbox

#### Scenario: Pasta de configuração em somente leitura
- **WHEN** o usuário passa `--ro ~/.config`
- **THEN** o lançamento prossegue, e o processo confinado não consegue alterar `~/.config/sandmux/config.toml`

### Requirement: Mostrar a configuração efetiva
`sandmux config show` MUST mostrar o caminho do arquivo e, para cada chave, o valor efetivo para a pasta atual, marcado como "padrão" ou "modificada" e, quando modificada, com a camada de onde veio. `--workdir <pasta>` e `--profile <nome>` MUST mudar a pasta e o perfil considerados. Um arquivo inválido MUST fazer o comando terminar com erro, com a linha e o motivo.

#### Scenario: Sem ajustes
- **WHEN** o arquivo não existe e o usuário roda `sandmux config show`
- **THEN** cada chave aparece com o valor padrão, marcada como "padrão"

#### Scenario: Chave vinda do projeto
- **WHEN** o perfil do projeto atual tem `gpu = false` e o usuário roda `sandmux config show`
- **THEN** `gpu` aparece como falso, marcada como "modificada", com o perfil do projeto como origem

### Requirement: Alterar a configuração pela linha de comando
`sandmux config set <chave> <valor>` e `unset <chave>` MUST alterar uma chave de liga e desliga; `add <chave> <item>` e `remove <chave> <item>` MUST alterar um item de lista. Cada comando MUST exigir um alvo explícito: `--global`, `--profile <nome>` ou `--project [<pasta>]`, que sem pasta usa a atual. O comando MUST validar o resultado antes de gravar e MUST recusar o que tornaria o arquivo inválido.

#### Scenario: Ligar o KVM num perfil
- **WHEN** o usuário roda `sandmux config set kvm true --profile vm` e não existe `[profile.vm]`
- **THEN** o arquivo passa a ter `[profile.vm]` com `kvm = true`

#### Scenario: Sem alvo
- **WHEN** o usuário roda `sandmux config set kvm true`
- **THEN** o comando termina com erro pedindo `--global`, `--profile` ou `--project`, e o arquivo não muda

#### Scenario: Projeto da pasta atual
- **WHEN** o usuário roda `sandmux config add ro ~/docs --project` na pasta `/home/ana/proj`
- **THEN** o arquivo passa a ter `[project."/home/ana/proj"]` com `~/docs` na lista `ro`

#### Scenario: Escrita recusada
- **WHEN** o usuário roda `sandmux config add rw ~/.config --global`
- **THEN** o comando termina com erro e o arquivo não muda

### Requirement: Só a diferença é gravada
Os comandos de escrita MUST gravar só as chaves definidas pelo usuário, nunca os valores padrão das camadas embutidas, e MUST preservar os comentários, a ordem e a formatação do resto do arquivo. `unset` e `remove` MUST apagar a chave que ficou sem valor e a seção que ficou vazia. A gravação MUST ser atômica.

#### Scenario: Comentário preservado
- **WHEN** o arquivo tem um comentário escrito à mão e o usuário roda `sandmux config set gpu false --global`
- **THEN** o comentário continua no arquivo, no mesmo lugar

#### Scenario: Seção esvaziada
- **WHEN** `[profile.vm]` só tem `kvm = true` e o usuário roda `sandmux config unset kvm --profile vm`
- **THEN** a seção `[profile.vm]` some do arquivo

#### Scenario: Nada de padrão no arquivo
- **WHEN** o usuário roda só `sandmux config add ro ~/docs --global` com o arquivo inexistente
- **THEN** o arquivo criado tem só `[global]` com `ro = ["~/docs"]`

### Requirement: Listar perfis
`sandmux config list` MUST mostrar os perfis nomeados e as pastas que têm perfil de projeto, cada um com as chaves que define. Uma pasta de projeto que não existe mais MUST aparecer marcada como inexistente.

#### Scenario: Perfis e projetos
- **WHEN** o arquivo tem `[profile.vm]` e `[project."/home/ana/proj"]`
- **THEN** `sandmux config list` mostra `vm` e `/home/ana/proj`, com as chaves de cada um

#### Scenario: Projeto apagado
- **WHEN** a pasta `/home/ana/velho` de um perfil de projeto foi apagada
- **THEN** `sandmux config list` mostra a pasta marcada como inexistente

### Requirement: Política resolvida a cada lançamento
Criar, reiniciar e restaurar uma sessão, e o `run` sem terminal, MUST resolver as camadas com o arquivo de configuração daquele momento, a partir das opções dadas no `run` e do perfil escolhido. Uma mudança no arquivo MUST valer no próximo lançamento de cada sessão e MUST NOT afetar uma sessão viva.

#### Scenario: Mudança antes do reinício
- **WHEN** a sessão 1 foi criada na pasta de um projeto, o usuário roda `sandmux config set gpu false --project` nessa pasta e depois `sandmux restart 1`
- **THEN** o harness relançado não enxerga a GPU

#### Scenario: Sessão viva
- **WHEN** a sessão 1 está viva e o usuário muda o perfil do projeto dela
- **THEN** o sandbox da sessão 1 continua com a política do lançamento, até ela ser relançada

### Requirement: Cliente e daemon com a mesma configuração
O daemon MUST usar o mesmo arquivo de configuração do cliente que fala com ele. Um cliente com outra pasta de configuração MUST ser recusado na saudação, com os dois caminhos na mensagem. A unidade systemd MUST levar `XDG_CONFIG_HOME` quando o cliente que a instalou o tinha definido.

#### Scenario: Pasta de configuração diferente
- **WHEN** o daemon foi iniciado com a configuração padrão e o usuário roda `XDG_CONFIG_HOME=/tmp/cfg sandmux ls`
- **THEN** o cliente termina com erro mostrando as duas pastas, e o daemon e as sessões continuam como estavam

#### Scenario: Unidade com a pasta do cliente
- **WHEN** o cliente instala a unidade systemd com `XDG_CONFIG_HOME=/home/ana/cfg`
- **THEN** a unidade define `XDG_CONFIG_HOME=/home/ana/cfg` para o daemon

### Requirement: Configuração no diagnóstico
`sandmux doctor` MUST mostrar o caminho do arquivo de configuração e dizer se ele não existe, é válido ou é inválido, com a linha e o motivo no último caso. O diagnóstico MUST NOT alterar o arquivo.

#### Scenario: Arquivo inválido
- **WHEN** o arquivo tem uma chave desconhecida e o usuário roda `sandmux doctor`
- **THEN** o diagnóstico mostra o caminho, diz que o arquivo é inválido e cita a chave e a linha
