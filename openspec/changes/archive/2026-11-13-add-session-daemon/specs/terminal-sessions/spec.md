# Spec Delta

## Purpose

Define as sessões que o daemon segura: cada uma é um harness confinado num terminal próprio que sobrevive ao cliente. Cobre como o usuário cria, encontra, anexa, desanexa, encerra e reinicia uma sessão, e o que o terminal dele recebe.

## ADDED Requirements

### Requirement: Sessão criada pelo run com terminal
Quando `sandmux run <harness>` é chamado com terminal interativo na entrada e na saída, o cliente MUST pedir ao daemon uma sessão nova com o harness, a pasta e as opções dadas, e anexar o terminal a ela. A sessão MUST receber um identificador numérico que não se repete na lista e um nome no formato `<harness>:<nome da pasta>`.

#### Scenario: Criar e anexar
- **WHEN** o usuário roda `sandmux run opencode` num terminal, na pasta `~/proj`
- **THEN** uma sessão `opencode:proj` é criada no daemon e a interface do opencode aparece no terminal

### Requirement: Verificações do run antes de criar a sessão
O `run` com terminal MUST fazer, antes de pedir a sessão, as mesmas verificações e perguntas do `run` direto: preset desconhecido, harness não instalado, pasta de trabalho perigosa, caminho inexistente em `--ro` e `--rw`, variável de `--pass-env` ausente e importação na primeira execução. Uma recusa MUST terminar o comando com a mesma mensagem e sem criar sessão. Os avisos do lançamento MUST aparecer no terminal que pediu a sessão.

#### Scenario: Pasta perigosa
- **WHEN** o usuário roda `sandmux run shell` num terminal, com o HOME como pasta atual
- **THEN** o comando termina com o erro de pasta perigosa e nenhuma sessão aparece em `sandmux ls`

#### Scenario: Aviso de variável passada
- **WHEN** o usuário roda `sandmux run opencode --pass-env OPENROUTER_API_KEY` num terminal
- **THEN** o aviso de que a variável fica ao alcance do agente aparece no terminal antes da interface do harness

### Requirement: Listar sessões
`sandmux ls` MUST listar as sessões do usuário com o identificador, o nome, o harness, a pasta de trabalho, o estado (viva, encerrada com o código de saída, ou parada com o motivo) e quantos clientes estão anexados. Sem sessões, o comando MUST dizer isso e terminar com código zero.

#### Scenario: Três estados
- **WHEN** existe uma sessão viva, uma encerrada com código 2 e uma parada por falta de variável
- **THEN** `sandmux ls` mostra as três, com os estados "viva", "encerrada (2)" e "parada" com o motivo

### Requirement: Anexar a uma sessão
`sandmux attach <id>` MUST anexar o terminal do usuário à sessão indicada. Sem identificador, o comando MUST anexar à única sessão viva, quando só existe uma, e caso contrário terminar com erro listando as sessões. Sem terminal interativo, o comando MUST terminar com erro.

#### Scenario: Reanexar depois de fechar o terminal
- **WHEN** o usuário fecha o terminal de uma sessão do opencode e roda `sandmux attach 1` em outro terminal
- **THEN** a interface do opencode aparece no novo terminal, no ponto em que estava

#### Scenario: Identificador inexistente
- **WHEN** o usuário roda `sandmux attach 99` e não existe a sessão 99
- **THEN** o comando termina com erro e mostra as sessões existentes

### Requirement: Tela redesenhada ao anexar
Ao anexar, o cliente MUST receber o histórico da sessão, para que a rolagem do próprio terminal o mostre, e em seguida a tela atual da sessão, com os modos de terminal que o harness ativou (tela alternativa, cursor, mouse, colagem). A tela mostrada MUST ser igual à que um cliente já anexado vê.

#### Scenario: Shell com histórico
- **WHEN** o usuário anexa a uma sessão do preset shell que já imprimiu 300 linhas
- **THEN** a rolagem do terminal mostra as 300 linhas e o prompt atual aparece embaixo

#### Scenario: Harness em tela cheia
- **WHEN** o usuário anexa a uma sessão do opencode
- **THEN** a interface aparece inteira, sem esperar o harness redesenhar

### Requirement: Tecla de prefixo
Enquanto anexado, Ctrl+b seguido de `d` MUST desanexar o terminal, e Ctrl+b seguido de Ctrl+b MUST enviar um Ctrl+b ao harness. Ctrl+b seguido de qualquer outra tecla MUST ser descartado. Todas as outras teclas, colagens e eventos de mouse MUST ir para o harness sem alteração.

#### Scenario: Desanexar pela tecla
- **WHEN** o usuário aperta Ctrl+b e depois `d`
- **THEN** o terminal volta ao shell de onde o comando foi chamado e a sessão continua viva

#### Scenario: Ctrl+b para o harness
- **WHEN** o usuário aperta Ctrl+b duas vezes
- **THEN** o harness recebe um único Ctrl+b

### Requirement: Terminal fechado desanexa
Quando o terminal de um cliente anexado é fechado ou a conexão do cliente cai, a sessão MUST continuar viva, com o harness rodando, e o daemon MUST tratar o cliente como desanexado.

#### Scenario: Janela fechada
- **WHEN** o usuário fecha a janela do terminal em que a sessão estava anexada
- **THEN** o harness continua rodando e `sandmux ls` mostra a sessão viva com zero clientes

### Requirement: Desanexar de outro terminal
`sandmux detach <id>` MUST desanexar todos os clientes da sessão indicada, sem encerrá-la.

#### Scenario: Terminal esquecido em outra máquina
- **WHEN** a sessão 1 está anexada num terminal via SSH e o usuário roda `sandmux detach 1` em outro terminal
- **THEN** o cliente remoto volta ao shell dele e a sessão continua viva

### Requirement: Restauração do terminal do usuário
Ao desanexar, por qualquer motivo, o cliente MUST devolver o terminal do usuário ao estado anterior ao anexo: modo de linha, tela principal, cursor visível e modos de mouse e de colagem desligados.

#### Scenario: Desanexar de um harness em tela cheia
- **WHEN** o usuário desanexa de uma sessão do opencode
- **THEN** o terminal volta à tela principal, com o cursor visível, e o shell do usuário funciona normalmente

### Requirement: Vários clientes na mesma sessão
Vários terminais MUST poder se anexar à mesma sessão ao mesmo tempo. Todos MUST receber a mesma saída, e a entrada de qualquer um deles MUST chegar ao harness.

#### Scenario: Dois terminais
- **WHEN** a sessão 1 está anexada em dois terminais e o usuário digita no segundo
- **THEN** o harness recebe o texto e os dois terminais mostram o resultado

### Requirement: Tamanho da tela segue o menor terminal
O terminal da sessão MUST ter o menor número de colunas e o menor número de linhas entre os clientes anexados, e MUST ser ajustado quando um cliente se anexa, se desanexa ou muda de tamanho. Sem clientes anexados, a sessão MUST manter o último tamanho.

#### Scenario: Segundo terminal menor
- **WHEN** a sessão está anexada num terminal de 200x50 e outro de 100x30 se anexa
- **THEN** o harness passa a ver uma tela de 100x30

#### Scenario: O menor sai
- **WHEN** o terminal de 100x30 se desanexa
- **THEN** o harness volta a ver 200x50

### Requirement: Terminal próprio de cada sessão
Cada sessão MUST ter um pseudoterminal próprio como terminal de controle do harness, com controle de tarefas funcionando dentro do sandbox. Um processo confinado MUST NOT conseguir inserir caracteres na entrada desse terminal.

#### Scenario: Ctrl+C no shell
- **WHEN** o usuário roda `sleep 100` numa sessão do preset shell e aperta Ctrl+C
- **THEN** o `sleep` é interrompido e o shell continua

### Requirement: Harness encerrado
Quando o harness de uma sessão termina, a sessão MUST continuar na lista como encerrada, com o código de saída e o histórico. Cada cliente anexado MUST mostrar que a sessão terminou, com o código, e o `run` ou o `attach` daquele cliente MUST terminar com o mesmo código.

#### Scenario: Shell encerrado
- **WHEN** o usuário digita `exit 3` numa sessão do preset shell criada por `sandmux run shell`
- **THEN** o terminal mostra que a sessão terminou com código 3, `sandmux run` termina com código 3 e `sandmux ls` mostra a sessão encerrada (3)

### Requirement: Encerrar uma sessão
`sandmux kill <id>` MUST encerrar o harness da sessão, desfazer o sandbox e tirar a sessão da lista. O harness MUST receber primeiro um sinal de término e, se não terminar em poucos segundos, ser encerrado à força. Os clientes anexados MUST ser desanexados com uma mensagem.

#### Scenario: Harness que ignora o término
- **WHEN** o usuário roda `sandmux kill 1` e o harness ignora o sinal de término
- **THEN** o harness é encerrado à força, nenhum processo do sandbox continua no host e a sessão some de `sandmux ls`

### Requirement: Reiniciar uma sessão
`sandmux restart <id>` MUST relançar uma sessão encerrada ou parada com o mesmo harness, a mesma pasta e as mesmas opções, e anexar o terminal quando houver um. Os valores de `--pass-env` MUST vir do ambiente do terminal que pediu o reinício, e uma variável ausente MUST impedir o reinício com erro. Uma sessão viva MUST NOT ser reiniciada.

#### Scenario: Reinício de sessão encerrada
- **WHEN** o harness da sessão 2 terminou e o usuário roda `sandmux restart 2`
- **THEN** o harness sobe de novo na mesma pasta, o terminal se anexa e o estado passa a viva

#### Scenario: Variável ausente
- **WHEN** a sessão 3 foi criada com `--pass-env OPENROUTER_API_KEY` e o usuário roda `sandmux restart 3` num terminal sem a variável
- **THEN** o comando termina com erro citando a variável e a sessão continua parada

#### Scenario: Sessão viva
- **WHEN** o usuário roda `sandmux restart 1` e a sessão 1 está viva
- **THEN** o comando termina com erro dizendo que a sessão está viva, sem afetá-la

### Requirement: Histórico em memória
O daemon MUST guardar as últimas 10.000 linhas que saíram da tela principal de cada sessão, valor ajustável por uma opção do `sandmuxd`. Linhas mais antigas MUST ser descartadas. O histórico e a saída das sessões MUST NOT ser gravados em disco.

#### Scenario: Limite de linhas
- **WHEN** uma sessão imprime 12.000 linhas com o limite padrão
- **THEN** ao anexar, o histórico recebido tem as 10.000 linhas mais recentes

#### Scenario: Nada em disco
- **WHEN** uma sessão imprime um texto marcador
- **THEN** o marcador não aparece em nenhum arquivo do diretório de dados, do `$XDG_RUNTIME_DIR` ou da pasta de estado do sandmux

### Requirement: Escrita no clipboard bloqueada por padrão
O daemon MUST retirar da saída da sessão, antes de enviá-la aos clientes, as sequências que pedem ao terminal do usuário para gravar no clipboard (OSC 52). Com `sandmux run --allow-clipboard`, a gravação MUST passar para aquela sessão, e a opção MUST valer também no reinício e na restauração.

#### Scenario: Padrão
- **WHEN** o harness de uma sessão sem a opção escreve uma sequência OSC 52 de gravação
- **THEN** nenhum cliente anexado recebe a sequência e o clipboard do usuário não muda

#### Scenario: Liberado
- **WHEN** a sessão foi criada com `--allow-clipboard` e o harness escreve uma sequência OSC 52 de gravação
- **THEN** os clientes anexados recebem a sequência

### Requirement: Leitura do clipboard sempre bloqueada
As sequências que pedem ao terminal do usuário o conteúdo do clipboard MUST ser retiradas da saída de toda sessão, com ou sem `--allow-clipboard`.

#### Scenario: Consulta do clipboard
- **WHEN** o harness escreve uma consulta OSC 52 numa sessão criada com `--allow-clipboard`
- **THEN** nenhum cliente recebe a consulta e o harness não recebe o conteúdo do clipboard

### Requirement: Sandboxes terminam com o daemon
Quando o daemon termina, por parada, reinício ou falha, nenhum processo de sandbox das sessões dele MUST continuar rodando no host.

#### Scenario: Daemon morto à força
- **WHEN** o processo do daemon é morto com SIGKILL com duas sessões vivas
- **THEN** os harnesses e todos os processos dos dois sandboxes terminam
