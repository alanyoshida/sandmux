# Spec Delta

## Purpose
Define a interface de terminal do sandmux: as abas, uma por sessão do daemon; a emulação de terminal de cada aba; a tecla de prefixo e os atalhos; os modais de nova aba, lista, renomear, fechar e ajuda; as abas encerradas e paradas; o mouse; e a colagem de imagem na pasta de entrada.

## ADDED Requirements

### Requirement: Abrir a TUI
`sandmux` sem subcomando, ou `sandmux tui`, com terminal interativo na entrada e na saída, MUST abrir a TUI, com uma aba para cada sessão do daemon, na ordem do identificador. Sem terminal interativo, o comando MUST terminar com erro dizendo que a TUI precisa de um terminal. Sem nenhuma sessão, a TUI MUST abrir o modal de nova aba.

#### Scenario: Sessões existentes
- **WHEN** o daemon tem as sessões 1 e 2 e o usuário roda `sandmux` num terminal
- **THEN** a barra de abas mostra as duas, e a área do terminal mostra a tela da aba ativa

#### Scenario: Sem terminal
- **WHEN** um script roda `sandmux tui` com a saída num pipe
- **THEN** o comando termina com erro e nada é desenhado

### Requirement: Barra de abas
A TUI MUST mostrar na primeira linha uma aba para cada sessão, com a posição (o número usado pelos atalhos de dígito), o nome e uma marca quando a sessão está encerrada ou parada, destacando a aba ativa. O identificador da sessão aparece na lista de abas. A área abaixo da barra MUST ser o terminal da aba ativa, e o tamanho dessa área MUST ser o tamanho do terminal da sessão.

#### Scenario: Aba ativa
- **WHEN** a aba 2 está ativa
- **THEN** a barra destaca a aba 2, e a área do terminal mostra a tela da sessão 2, com uma linha a menos que o terminal do usuário

### Requirement: Emulação de terminal da aba
Cada aba MUST emular o terminal da sua sessão, mostrando o texto, as cores (16, 256 e cor verdadeira), os atributos (negrito, itálico, sublinhado, inverso), a posição e a visibilidade do cursor e a tela alternativa. Trocar de aba MUST mostrar a tela atual da outra aba sem esperar o harness redesenhar.

#### Scenario: Cor verdadeira
- **WHEN** o harness da aba ativa escreve um texto com cor de frente `38;2;10;200;30`
- **THEN** a TUI mostra esse texto com a mesma cor

#### Scenario: Troca de aba
- **WHEN** a aba 1 mostra a interface do opencode e o usuário vai para a aba 2 e volta
- **THEN** a interface do opencode aparece de novo na hora, igual a antes

### Requirement: Entrada e modos da aba ativa
A entrada do usuário, exceto a tecla de prefixo e os eventos de mouse tratados pela TUI, MUST ir crua para a sessão da aba ativa. Os modos que o harness da aba ativa ligou (protocolo de teclado do kitty, colagem entre marcadores, eventos de foco) MUST ser ligados no terminal do usuário enquanto a aba está ativa e desligados ao trocar de aba e ao sair.

#### Scenario: Protocolo de teclado do kitty
- **WHEN** o opencode na aba ativa liga o protocolo de teclado do kitty e o usuário digita no campo de texto dele
- **THEN** o texto aparece no opencode como num terminal direto

#### Scenario: Saída restaura o terminal
- **WHEN** o usuário sai da TUI com uma aba que ligou o protocolo do kitty
- **THEN** o terminal do usuário volta sem tela alternativa, sem mouse e sem o protocolo do kitty

### Requirement: Tecla de prefixo e atalhos
A TUI MUST tratar a tecla de prefixo (Ctrl+b por padrão, como byte ou na forma do kitty) seguida de: `c` nova aba, `n` próxima, `p` anterior, `0` a `9` ir para a aba naquela posição, `w` lista de abas, `,` renomear, `r` reiniciar, `x` fechar, `v` colar imagem, `d` sair, `?` ajuda, e a própria tecla de prefixo, que envia o prefixo à sessão. Outra tecla depois do prefixo MUST ser descartada.

#### Scenario: Próxima aba
- **WHEN** a aba 1 está ativa e o usuário digita Ctrl+b n
- **THEN** a aba 2 fica ativa

#### Scenario: Prefixo para o harness
- **WHEN** o usuário digita Ctrl+b Ctrl+b
- **THEN** a sessão ativa recebe um Ctrl+b

### Requirement: Atalhos configuráveis
A seção `[tui]` do `config.toml` MUST aceitar `prefix` (`"C-<letra>"`) e, em `[tui.keys]`, a troca da tecla de cada ação (`new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help`) por um caractere. Duas ações na mesma tecla, ou uma ação desconhecida, MUST tornar o arquivo inválido.

#### Scenario: Prefixo trocado
- **WHEN** `[tui]` tem `prefix = "C-a"` e o usuário digita Ctrl+a n
- **THEN** a próxima aba fica ativa, e Ctrl+b vai cru para a sessão

#### Scenario: Atalho repetido
- **WHEN** `[tui.keys]` tem `next = "x"`, e `close` continua no `x` padrão
- **THEN** o arquivo é considerado inválido

### Requirement: Nova aba
O modal de nova aba MUST pedir o harness (entre os presets), a pasta de trabalho (por padrão, a pasta de onde a TUI foi aberta) e, opcionalmente, o perfil nomeado. A sessão criada MUST passar pelas mesmas verificações e pela mesma resolução de camadas do `sandmux run`, inclusive a pergunta de importação na primeira execução do harness, feita no próprio modal. Uma recusa MUST aparecer no modal, sem criar sessão.

#### Scenario: Shell em outra pasta
- **WHEN** o usuário cria uma aba com o preset `shell` e a pasta `~/outro`
- **THEN** uma aba `shell:outro` aparece ativa, com o shell confinado em `~/outro`

#### Scenario: Pasta perigosa
- **WHEN** o usuário informa o HOME como pasta de trabalho
- **THEN** o modal mostra o erro de pasta perigosa e nenhuma sessão é criada

### Requirement: Lista de abas
O modal de lista MUST mostrar todas as abas, com o identificador, o nome, o estado e a pasta, e MUST permitir escolher uma com as setas e Enter, ou fechar com Esc.

#### Scenario: Escolher pela lista
- **WHEN** o usuário abre a lista, desce até a aba 3 e aperta Enter
- **THEN** a aba 3 fica ativa

### Requirement: Renomear aba
O modal de renomear MUST trocar o nome da sessão da aba ativa pelo pedido de renomear do daemon, com as mesmas regras de nome de `sandmux rename`.

#### Scenario: Renomear
- **WHEN** o usuário renomeia a aba ativa para `api`
- **THEN** a barra mostra `api`, e `sandmux ls` também

### Requirement: Aba encerrada ou parada
Uma aba cuja sessão está encerrada MUST continuar aberta, com a última tela, o código de saída e as teclas de reiniciar (`r`) e fechar (`x`). Uma aba parada MUST mostrar o motivo. Reiniciar MUST relançar a sessão como `sandmux restart`, com as variáveis de `pass-env` do ambiente da TUI.

#### Scenario: Shell encerrado
- **WHEN** o usuário digita `exit 3` no shell da aba ativa
- **THEN** a aba continua na barra, marcada como encerrada, e a área mostra o código 3 e as teclas de reiniciar e fechar

#### Scenario: Reiniciar
- **WHEN** a aba ativa está encerrada e o usuário digita Ctrl+b r
- **THEN** a sessão volta viva na mesma aba

### Requirement: Fechar aba
Fechar uma aba MUST pedir confirmação e encerrar a sessão como `sandmux kill`. A aba MUST sumir da barra, e a vizinha MUST ficar ativa.

#### Scenario: Fechar com confirmação
- **WHEN** o usuário digita Ctrl+b x e confirma
- **THEN** a sessão é encerrada e a aba some

### Requirement: Sair da TUI
Sair (prefixo e `d`, fechar o terminal ou SIGTERM) MUST desanexar todas as abas, deixando as sessões vivas, e MUST restaurar o terminal do usuário.

#### Scenario: Sair e voltar
- **WHEN** o usuário sai da TUI e roda `sandmux` de novo
- **THEN** as mesmas abas aparecem, com as telas atuais

### Requirement: Mouse
A TUI MUST ligar o mouse no terminal do usuário. Um clique na barra MUST ativar a aba clicada. Na área do terminal, quando o harness da aba ativa ligou o mouse, os eventos MUST ir para a sessão, com as coordenadas relativas à área. Quando o harness não ligou o mouse, a roda MUST rolar o histórico da aba, e arrastar MUST selecionar texto, copiado ao soltar para o clipboard do terminal do usuário.

#### Scenario: Clique na barra
- **WHEN** o usuário clica no nome da aba 2
- **THEN** a aba 2 fica ativa

#### Scenario: Mouse no harness
- **WHEN** o opencode ligou o mouse e o usuário clica na linha 5 da área do terminal
- **THEN** o opencode recebe o clique na linha 5 da tela dele

#### Scenario: Seleção no shell
- **WHEN** o usuário arrasta sobre um texto no shell confinado
- **THEN** ao soltar, o texto vai para o clipboard do terminal do usuário

### Requirement: Colar imagem
O prefixo seguido de `v` MUST ler a imagem do clipboard do sistema, gravá-la como PNG na pasta de entrada e escrever na aba ativa, sem Enter, o caminho absoluto dela no sandbox, dentro de `~/inbox`. Sem imagem no clipboard, ou sem acesso ao clipboard, a TUI MUST avisar na barra, sem escrever nada na aba.

#### Scenario: Screenshot
- **WHEN** o clipboard tem uma imagem e o usuário digita Ctrl+b v
- **THEN** um PNG aparece na pasta de entrada, e a aba ativa recebe o caminho dele em `~/inbox`

#### Scenario: Clipboard sem imagem
- **WHEN** o clipboard só tem texto e o usuário digita Ctrl+b v
- **THEN** a barra avisa que não há imagem, e a aba não recebe nada
