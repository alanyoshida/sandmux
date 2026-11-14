# session-daemon Specification

## Purpose
Define o `sandmuxd`, o processo de longa duração de cada usuário que segura as sessões confinadas: como ele começa a rodar, quem pode falar com ele, como cliente e daemon de versões diferentes se tratam e como as sessões voltam depois de um reboot.

## Requirements

### Requirement: Um daemon por usuário
Cada usuário MUST ter no máximo um `sandmuxd` rodando ao mesmo tempo. Um segundo `sandmuxd` iniciado pelo mesmo usuário MUST terminar com erro, sem afetar o daemon que já roda nem as sessões dele.

#### Scenario: Segunda instância
- **WHEN** o daemon já está rodando e o usuário inicia outro `sandmuxd`
- **THEN** o novo processo termina com erro dizendo que o daemon já está ativo, e as sessões existentes continuam

### Requirement: Socket privado do usuário
O daemon MUST ouvir num socket Unix dentro de uma pasta só do usuário em `$XDG_RUNTIME_DIR`, acessível apenas por ele. O daemon MUST recusar uma conexão de processo com UID diferente do seu. Sem `$XDG_RUNTIME_DIR`, o daemon MUST usar uma pasta do usuário em `/tmp` com as mesmas permissões e MUST recusar uma pasta que já existe e não pertence ao usuário.

#### Scenario: Permissões
- **WHEN** o daemon está rodando
- **THEN** a pasta do socket tem permissão só para o dono, e o socket só aceita conexões do mesmo UID

#### Scenario: Pasta de outro dono
- **WHEN** a pasta do socket já existe e pertence a outro usuário
- **THEN** o daemon termina com erro sem criar o socket

### Requirement: Sandbox fora do alcance do daemon
Um processo confinado MUST NOT conseguir se conectar ao socket do daemon, nem por caminho nem por nome abstrato.

#### Scenario: Tentativa de dentro do sandbox
- **WHEN** um processo confinado tenta se conectar ao caminho do socket do daemon ou a um socket abstrato dele
- **THEN** a conexão falha

### Requirement: Início pelo systemd de usuário
Quando o cliente precisa do daemon e o systemd de usuário está disponível, o cliente MUST instalar a unidade `sandmuxd.service` em `~/.config/systemd/user`, habilitá-la para o login e iniciá-la, avisando o usuário na primeira vez que faz isso. `sandmux daemon disable` MUST parar o daemon, desabilitar e remover a unidade.

#### Scenario: Primeiro uso com systemd
- **WHEN** o usuário roda `sandmux run shell` num terminal, sem daemon ativo e sem a unidade instalada
- **THEN** o cliente grava e habilita a unidade, avisa que fez isso, inicia o daemon pelo systemd e segue com a sessão

#### Scenario: Login seguinte
- **WHEN** o usuário faz login depois de a unidade ter sido habilitada
- **THEN** o daemon é iniciado pelo systemd sem nenhum comando do usuário

#### Scenario: Desabilitar
- **WHEN** o usuário roda `sandmux daemon disable`
- **THEN** o daemon para, a unidade é desabilitada e o arquivo dela é removido

### Requirement: Início pelo cliente sem systemd
Quando o systemd de usuário não está disponível, o cliente MUST iniciar o daemon como processo próprio, desligado do terminal, e seguir com o pedido assim que o socket responde.

#### Scenario: Máquina sem systemd de usuário
- **WHEN** o usuário roda `sandmux run shell` num terminal, sem daemon ativo e sem systemd de usuário
- **THEN** o cliente inicia o daemon, a sessão é criada, e fechar o terminal não encerra o daemon

### Requirement: Ambiente do lançamento vem do cliente
O daemon MUST lançar cada sessão com a pasta de trabalho, as variáveis da lista segura e os valores de `--pass-env` enviados pelo cliente que pediu a sessão, e não com os do ambiente do próprio daemon. Os valores de `--pass-env` MUST ficar só na memória do daemon.

#### Scenario: Pasta e terminal do cliente
- **WHEN** o usuário roda `sandmux run shell` na pasta `~/proj` com `TERM=xterm-kitty`
- **THEN** o shell confinado começa em `~/proj` com `TERM=xterm-kitty`, mesmo que o daemon tenha sido iniciado pelo systemd em outra pasta

#### Scenario: Valor de variável passada
- **WHEN** a sessão foi criada com `--pass-env OPENROUTER_API_KEY`
- **THEN** o valor está no ambiente do harness e não aparece em nenhum arquivo gravado pelo daemon

### Requirement: Verificação de versão do protocolo
Cliente e daemon MUST trocar a versão do protocolo antes de qualquer outro pedido. Com versões diferentes, o cliente MUST terminar com erro mostrando as duas versões e indicando `sandmux daemon restart`, e o daemon MUST continuar rodando com as sessões intactas.

#### Scenario: Cliente atualizado com daemon antigo
- **WHEN** o usuário atualiza o sandmux e roda `sandmux ls` com o daemon antigo ainda ativo
- **THEN** o cliente explica que as versões diferem, indica `sandmux daemon restart` e nenhuma sessão é afetada

### Requirement: Estado do daemon
`sandmux daemon status` MUST mostrar se o daemon está rodando, a versão do protocolo, o PID, como ele foi iniciado (systemd ou cliente) e quantas sessões existem. Sem daemon ativo, o comando MUST dizer isso e terminar com código diferente de zero, sem iniciar o daemon.

#### Scenario: Daemon parado
- **WHEN** o usuário roda `sandmux daemon status` sem daemon ativo
- **THEN** o comando diz que o daemon não está rodando, termina com código diferente de zero e não inicia nada

### Requirement: Daemon no diagnóstico
`sandmux doctor` MUST mostrar se o daemon está rodando, se a unidade systemd de usuário está instalada e habilitada e onde fica o socket, sem iniciar o daemon.

#### Scenario: Unidade instalada e daemon parado
- **WHEN** a unidade está habilitada, o daemon foi parado com `sandmux daemon stop` e o usuário roda `sandmux doctor`
- **THEN** o diagnóstico mostra a unidade habilitada, o daemon parado e o caminho do socket, e nenhum daemon é iniciado

### Requirement: Parada do daemon
`sandmux daemon stop` MUST encerrar todos os harnesses e parar o daemon. A lista de sessões gravada MUST continuar valendo para a próxima vez que o daemon subir.

#### Scenario: Parar
- **WHEN** o usuário roda `sandmux daemon stop` com duas sessões vivas
- **THEN** os dois harnesses e os sandboxes são encerrados e o daemon termina

### Requirement: Reinício do daemon
`sandmux daemon restart` MUST parar o daemon como `sandmux daemon stop` e iniciá-lo de novo com o binário instalado, que restaura as sessões como depois de um reboot. O histórico de tela das sessões MUST ser perdido.

#### Scenario: Reinício depois de atualizar
- **WHEN** o usuário atualizou o sandmux e roda `sandmux daemon restart` com sessões vivas
- **THEN** o daemon novo sobe, relança as sessões nas mesmas pastas e o cliente novo passa a funcionar com ele

### Requirement: Lista de sessões guardada sem segredos
O daemon MUST manter no diretório de dados do sandmux um arquivo com a lista de sessões: para cada uma, o identificador, o nome, o harness, a pasta de trabalho, as opções dadas no `run`, o perfil nomeado escolhido, os nomes das variáveis de `pass-env` do último lançamento e se ela estava viva ou encerrada. O arquivo MUST NOT conter valores de variáveis, a política resolvida nem saída das sessões.

#### Scenario: Conteúdo do arquivo
- **WHEN** existe uma sessão criada com `--pass-env OPENROUTER_API_KEY`
- **THEN** o arquivo da lista tem o nome `OPENROUTER_API_KEY` e não tem o valor dele nem nenhuma linha da saída da sessão

#### Scenario: Perfil guardado pelo nome
- **WHEN** existe uma sessão criada com `--profile vm`
- **THEN** o arquivo da lista guarda o nome `vm`, e não as chaves do perfil

#### Scenario: Lista da versão anterior
- **WHEN** o daemon novo sobe com uma lista gravada pela versão anterior do sandmux
- **THEN** as sessões são lidas com as opções que tinham, sem perfil nomeado, e a lista é regravada no formato novo

### Requirement: Restauração depois de reboot
Ao subir, o daemon MUST ler a lista de sessões e relançar cada sessão que estava viva, na mesma pasta, com o mesmo harness, as mesmas opções e o mesmo perfil, resolvendo as camadas com a configuração do momento. MUST ficar parada, com o motivo em `sandmux ls`, até o usuário reiniciá-la, uma sessão cuja política efetiva tem `pass-env`, cujo perfil nomeado não existe mais ou cuja configuração é inválida. Uma sessão encerrada MUST voltar encerrada, com o código de saída.

#### Scenario: Sessão sem variáveis passadas
- **WHEN** a máquina reinicia com uma sessão viva do `opencode` em `~/proj`, criada sem `--pass-env`
- **THEN** depois do login o daemon relança o `opencode` em `~/proj` sem nenhum comando do usuário

#### Scenario: Sessão com variável passada
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY`
- **THEN** a sessão aparece parada em `sandmux ls`, com o motivo, e só sobe quando o usuário roda `sandmux restart` de um terminal que tem a variável

#### Scenario: Pasta de trabalho apagada
- **WHEN** a pasta de trabalho de uma sessão viva não existe mais quando o daemon sobe
- **THEN** a sessão fica parada com o motivo, e as demais são restauradas

#### Scenario: Variável acrescentada pelo perfil
- **WHEN** uma sessão viva foi criada sem `--pass-env` e, antes do reboot, o perfil do projeto dela ganhou `pass-env = ["OPENROUTER_API_KEY"]`
- **THEN** depois do login a sessão aparece parada, com o motivo citando a variável

#### Scenario: Perfil apagado
- **WHEN** uma sessão viva foi criada com `--profile vm` e o perfil `vm` foi apagado antes do reboot
- **THEN** depois do login a sessão aparece parada, com o motivo dizendo que o perfil não existe

#### Scenario: Configuração inválida
- **WHEN** o arquivo de configuração está inválido quando o daemon sobe
- **THEN** todas as sessões que estavam vivas aparecem paradas, com o motivo citando o arquivo e a linha, e o daemon continua rodando
