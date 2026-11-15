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
O daemon MUST lançar cada sessão com a pasta de trabalho, as variáveis da lista segura e os valores de `--pass-env` enviados pelo cliente que pediu a sessão, e não com os do ambiente do próprio daemon. Os valores de `--pass-env` MUST ficar só na memória do daemon e, quando o cofre está desbloqueado, cifrados no cofre; MUST NOT ir em claro para nenhum arquivo.

#### Scenario: Pasta e terminal do cliente
- **WHEN** o usuário roda `sandmux run shell` na pasta `~/proj` com `TERM=xterm-kitty`
- **THEN** o shell confinado começa em `~/proj` com `TERM=xterm-kitty`, mesmo que o daemon tenha sido iniciado pelo systemd em outra pasta

#### Scenario: Valor de variável passada
- **WHEN** a sessão foi criada com `--pass-env OPENROUTER_API_KEY`
- **THEN** o valor está no ambiente do harness e não aparece em claro em nenhum arquivo gravado pelo daemon

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
Ao subir, o daemon MUST relançar cada sessão que estava viva, com as mesmas opções, resolvendo as camadas com a configuração do momento. Uma sessão com `pass-env` cujos valores estão no cofre MUST ficar "aguardando cofre" e subir sozinha, com eles, no desbloqueio. A sessão do gateway do hermes e as sessões de chat MUST ficar "aguardando cofre" enquanto o cofre existe e está trancado, e subir sozinhas no desbloqueio. MUST ficar parada, com o motivo em `sandmux ls`, uma sessão com `pass-env` sem os valores no cofre, com perfil que não existe mais ou com configuração inválida. Uma sessão encerrada MUST voltar encerrada.

#### Scenario: Sessão sem variáveis passadas
- **WHEN** a máquina reinicia com uma sessão viva do `opencode` em `~/proj`, criada sem `--pass-env`
- **THEN** depois do login o daemon relança o `opencode` em `~/proj` sem nenhum comando do usuário

#### Scenario: Sessão com variável no cofre
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY` enquanto o cofre estava desbloqueado
- **THEN** a sessão aparece "aguardando cofre" em `sandmux ls` e sobe sozinha, com o valor, quando o usuário desbloqueia o cofre

#### Scenario: Sessão com variável passada
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY` sem cofre
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

#### Scenario: Gateway do hermes com o cofre trancado
- **WHEN** a máquina reinicia com a sessão do gateway do hermes viva e o cofre existe
- **THEN** a sessão aparece "aguardando cofre" em `sandmux ls` e sobe sozinha quando o usuário desbloqueia o cofre

#### Scenario: Aba de chat depois do reboot
- **WHEN** a máquina reinicia com uma aba de chat viva e o cofre existe
- **THEN** a aba de chat aparece "aguardando cofre" e volta, com a conversa, quando o usuário desbloqueia o cofre

### Requirement: Notificações do desktop
Com `[global] notify` ligado, que é o padrão, o daemon MUST mandar uma notificação ao desktop, pelo serviço `org.freedesktop.Notifications` do barramento de sessão, nestes casos:

- um pedido de rede de uma aba é bloqueado;
- uma chave real do HOME sintético é movida ao cofre, ou passa a esperar o cofre ou um domínio;
- o harness de uma aba termina sem o usuário ter pedido;
- depois de a lista de sessões ser restaurada, há abas esperando o cofre e ele continua trancado.


#### Scenario: Bloqueio com a TUI fechada
- **WHEN** nenhuma TUI está aberta e o agente de uma aba tenta um domínio da denylist
- **THEN** o desktop mostra uma notificação com o nome da aba, o domínio e o motivo do bloqueio

#### Scenario: Aba encerrada pelo usuário
- **WHEN** o usuário encerra uma aba com `sandmux kill`
- **THEN** nenhuma notificação de aba encerrada é mandada

### Requirement: Texto e falhas das notificações
O texto de cada notificação MUST dizer a aba e o que aconteceu, sem valores de segredo. Sem o serviço de notificações, o daemon MUST seguir sem erro. Com `[global] notify = false`, o daemon MUST NOT mandar notificações.

#### Scenario: Desligadas
- **WHEN** `[global] notify = false` e um pedido é bloqueado
- **THEN** nenhuma notificação é mandada

### Requirement: Agrupamento das notificações
Eventos do mesmo tipo na mesma aba, a menos de 60 segundos um do outro, MUST atualizar a notificação anterior, com a contagem, em vez de abrir outra.

#### Scenario: Vários bloqueios seguidos
- **WHEN** o agente tenta três domínios bloqueados em poucos segundos
- **THEN** o desktop mostra uma notificação só, que termina dizendo três bloqueios

### Requirement: Supressão pelo foco
O daemon MUST NOT mandar notificação sobre uma aba que uma TUI aberta está mostrando, quando essa TUI informou que está com o foco do terminal. O foco MUST ser guardado por processo da TUI, e o de uma TUI que terminou MUST ser descartado.

#### Scenario: TUI em foco na aba
- **WHEN** a TUI está em foco mostrando a aba 1, e um pedido da aba 1 é bloqueado
- **THEN** nenhuma notificação é mandada, e o aviso fica na TUI

#### Scenario: TUI sem foco
- **WHEN** a TUI perdeu o foco do terminal, e um pedido da aba que ela mostra é bloqueado
- **THEN** a notificação é mandada

### Requirement: Grupo de cada sessão
Toda sessão MUST pertencer a um grupo. Uma sessão criada sem grupo MUST ir para o grupo `principal`. O grupo MUST ser guardado na lista de sessões, voltar na restauração e aparecer na listagem. Uma sessão gravada antes dos grupos MUST voltar no `principal`. O nome de um grupo MUST ter de 1 a 32 caracteres, entre letras, dígitos, `-`, `_` e `.`, e um nome fora da regra MUST ser recusado com a regra na mensagem.

#### Scenario: Run com grupo
- **WHEN** o usuário roda `sandmux run shell --group estudos`
- **THEN** a sessão fica no grupo `estudos`, e `sandmux ls` mostra `estudos` na coluna GRUPO

#### Scenario: Restauração
- **WHEN** o daemon reinicia com sessões em dois grupos
- **THEN** cada sessão volta no seu grupo

### Requirement: Mover e renomear grupos
`sandmux group move <id> <grupo>` MUST levar a sessão para o grupo dado, criando-o se preciso. `sandmux group rename <grupo> <novo>` MUST levar todas as sessões do grupo para o nome novo. `sandmux group list` MUST listar os grupos com o número de sessões, com o `principal` sempre presente.

#### Scenario: Renomear
- **WHEN** o grupo `estudos` tem duas sessões e o usuário roda `sandmux group rename estudos rust`
- **THEN** as duas sessões ficam no grupo `rust`, e `estudos` some da lista

### Requirement: Início no boot
`sandmux daemon boot on` MUST ligar o linger do usuário pelo `loginctl`, sem sudo, e deixar a unidade systemd de usuário do daemon habilitada. `sandmux daemon boot off` MUST desligar o linger e avisar que outros serviços de usuário também deixam de subir no boot. `sandmux daemon boot`, sem argumento, `sandmux daemon status` e `sandmux doctor` MUST dizer se o daemon sobe no boot.

#### Scenario: Ligar
- **WHEN** o usuário roda `sandmux daemon boot on`
- **THEN** o `loginctl enable-linger` roda para o usuário, e `sandmux daemon boot` diz que o daemon sobe no boot

#### Scenario: loginctl recusa
- **WHEN** o `loginctl` recusa ligar o linger
- **THEN** o comando termina com erro e a mensagem do `loginctl`

### Requirement: Espera do login
Quando o daemon sobe sem nenhuma sessão de login do usuário, as sessões que seriam relançadas MUST NOT ser relançadas: elas MUST ficar no estado "aguardando o login", guardado na lista de sessões. Nesse estado, o daemon MUST NOT consultar o keyring. Quando o usuário faz login, o daemon MUST relançar essas sessões, tentar abrir o cofre pelo keyring e avisar as sessões que esperam o cofre. Com o usuário logado ao subir, a restauração MUST seguir sem espera.

#### Scenario: Boot sem login
- **WHEN** o daemon sobe no boot, sem login, e havia duas abas vivas
- **THEN** `sandmux ls` mostra as duas como aguardando o login, e nenhum harness roda

#### Scenario: Primeiro login
- **WHEN** o usuário faz login
- **THEN** as duas abas voltam vivas, sem comando do usuário

#### Scenario: Reinício antes do login
- **WHEN** o daemon reinicia de novo antes do login
- **THEN** as abas continuam aguardando o login, e voltam no primeiro login
