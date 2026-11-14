# terminal-ui Specification

## Purpose
Define a interface de terminal do sandmux: as abas, uma por sessão do daemon; a emulação de terminal de cada aba; a tecla de prefixo e os atalhos; os modais de nova aba, lista, renomear, fechar e ajuda; as abas encerradas e paradas; o mouse; e a colagem de imagem na pasta de entrada.

## Requirements

### Requirement: Abrir a TUI
`sandmux` sem subcomando, ou `sandmux tui`, com terminal interativo na entrada e na saída, MUST abrir a TUI, com uma aba para cada sessão do daemon, na ordem do identificador. Sem terminal interativo, o comando MUST terminar com erro dizendo que a TUI precisa de um terminal. Sem nenhuma sessão, a TUI MUST abrir o modal de nova aba.

#### Scenario: Sessões existentes
- **WHEN** o daemon tem as sessões 1 e 2 e o usuário roda `sandmux` num terminal
- **THEN** a barra de abas mostra as duas, e a área do terminal mostra a tela da aba ativa

#### Scenario: Sem terminal
- **WHEN** um script roda `sandmux tui` com a saída num pipe
- **THEN** o comando termina com erro e nada é desenhado

### Requirement: Barra de abas
A TUI MUST mostrar na primeira linha uma aba para cada sessão, com a posição (o número usado pelos atalhos de dígito), o nome e uma marca quando a sessão está encerrada ou parada, destacando a aba ativa. O identificador da sessão aparece na lista de abas. A área abaixo da barra, à esquerda do painel lateral quando ele está visível, MUST ser o terminal da aba ativa, e o tamanho dessa área MUST ser o tamanho do terminal da sessão.

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

### Requirement: Tema de cores
A chave `theme` da seção `[tui]` MUST escolher um tema embutido: `padrao`, que usa as cores do terminal e vídeo inverso e é o padrão; `escuro`; ou `claro`. A TUI MUST desenhar a barra, os avisos, as faixas de aba encerrada e os modais com as cores do tema. Um tema desconhecido MUST tornar o arquivo inválido.

#### Scenario: Tema escuro
- **WHEN** `[tui]` tem `theme = "escuro"` e o usuário abre a TUI
- **THEN** a aba ativa aparece com as cores de destaque do tema escuro, e não em vídeo inverso

#### Scenario: Tema desconhecido
- **WHEN** `[tui]` tem `theme = "neon"`
- **THEN** o arquivo é considerado inválido, com a linha e os temas aceitos na mensagem

### Requirement: Cor por elemento
A seção `[tui.colors]` MUST trocar, por cima do tema, a cor de cada elemento: `tab-active-fg`, `tab-active-bg`, `tab-fg`, `tab-bg`, `bar-bg`, `status-fg`, `note-fg`, `note-bg`, `modal-border`, `modal-title`, `selected-fg`, `selected-bg`, `error-fg`, `selection-fg` e `selection-bg`. Cada valor MUST ser `#rrggbb`, um índice de 0 a 255, um dos 16 nomes ANSI ou `default`. Um elemento ou valor desconhecido MUST tornar o arquivo inválido.

#### Scenario: Aba ativa vermelha
- **WHEN** `[tui.colors]` tem `tab-active-bg = "#aa0000"`
- **THEN** a aba ativa aparece com fundo `#aa0000`, e o resto segue o tema

#### Scenario: Cor inválida
- **WHEN** `[tui.colors]` tem `tab-bg = "roxo-claro"`
- **THEN** o arquivo é considerado inválido, com a linha e os formatos aceitos

### Requirement: Painel lateral
A TUI MUST mostrar à direita um painel de 34 colunas com as seções "Proteção", "Recursos" e "Rede" da aba ativa, quando o terminal tem pelo menos 100 colunas e o painel não foi escondido. O atalho `s` (`panel`) MUST esconder e mostrar o painel, e a chave `panel` de `[tui]` MUST definir se ele abre visível.

#### Scenario: Painel visível
- **WHEN** o terminal tem 120 colunas e o usuário abre a TUI
- **THEN** o painel aparece à direita, e a sessão da aba ativa tem 86 colunas

#### Scenario: Esconder
- **WHEN** o painel está visível e o usuário digita Ctrl+b s
- **THEN** o painel some, e a sessão volta a ter a largura toda

#### Scenario: Terminal estreito
- **WHEN** o terminal tem 80 colunas
- **THEN** o painel não aparece

### Requirement: Proteção no painel
A seção "Proteção" MUST mostrar, para o último lançamento da aba ativa: o nível de proteção, com a ABI do Landlock; a camada de escrita; a pasta de trabalho e as pastas extras, cada uma em somente leitura ou com escrita; a GPU e o KVM; a escrita no clipboard; as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente; os tetos de memória e de processos; e o estado da rede.

#### Scenario: Pasta extra e variável
- **WHEN** a sessão da aba ativa foi lançada com `--ro ~/docs` e `--pass-env OPENROUTER_API_KEY`
- **THEN** o painel mostra `~/docs` em somente leitura e `OPENROUTER_API_KEY` com o aviso

### Requirement: Recursos no painel
A seção "Recursos" MUST mostrar a memória em uso, o pico e o teto, os processos em uso e o teto, o CPU usado, as mortes por falta de memória e, quando o Docker privado da pasta está rodando, os contêineres e a memória dele, atualizados a cada segundo.

#### Scenario: Consumo atualizado
- **WHEN** um programa da aba ativa passa a usar 200 MB
- **THEN** em até dois segundos o painel mostra a memória em uso maior que 200 MB

### Requirement: Rede no painel e na barra
A seção "Rede" do painel MUST mostrar o modo de rede e as últimas conexões da aba ativa, com domínio (ou "sem domínio"), volume e bloqueio, destacando como "novo" o domínio visto pela primeira vez naquela pasta de trabalho. Uma aba inativa MUST ganhar na barra `•` quando a sessão acessa um domínio novo e `!` quando a sessão tem um bloqueio, até ficar ativa.

#### Scenario: Domínio novo na aba ativa
- **WHEN** o agente da aba ativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a seção Rede mostra o domínio marcado como novo

#### Scenario: Domínio novo em outra aba
- **WHEN** o agente de uma aba inativa acessa pela primeira vez um domínio naquela pasta de trabalho
- **THEN** a aba ganha uma marca na barra, que some quando ela fica ativa

#### Scenario: Bloqueio em outra aba
- **WHEN** o agente de uma aba inativa tem uma conexão bloqueada
- **THEN** a aba ganha `!` na barra, que some quando ela fica ativa

### Requirement: Tela de rede
O atalho `a` (`network`) MUST abrir a tela de rede da aba ativa, com os acessos recentes (mais novos primeiro) e as regras efetivas. Na tela, o usuário MUST poder:

- negar o destino de um acesso, ou liberar o de um bloqueio, escolhendo a camada: a aba, o perfil do projeto ou o global;
- ligar e desligar um item dos catálogos ou da allowlist;
- trocar o modo de rede da aba.

Cada mudança MUST valer na conexão seguinte da sessão.

#### Scenario: Negar um acesso novo
- **WHEN** o usuário abre a tela de rede, escolhe um acesso a `host.sandmux.internal` e o nega na aba
- **THEN** o acesso seguinte da sessão a `host.sandmux.internal` é bloqueado

#### Scenario: Liberar no projeto
- **WHEN** a aba está no modo `custom`, o usuário escolhe um bloqueio de `host.sandmux.internal` e o libera no projeto
- **THEN** o perfil do projeto ganha `allow = ["host.sandmux.internal"]`, e o acesso seguinte passa

#### Scenario: Trocar o modo
- **WHEN** o usuário troca o modo da aba de `aberta` para `custom` na tela de rede
- **THEN** a conexão seguinte da sessão já segue o modo `custom`

### Requirement: Portas no painel
O painel lateral MUST ter uma seção "Portas" com as portas publicadas da aba ativa (porta do sandbox e porta do host) e as publicadas pelos contêineres do Docker privado da pasta, atualizada a cada segundo.

#### Scenario: Porta nova
- **WHEN** o agente da aba ativa passa a escutar na porta 3000
- **THEN** a seção Portas mostra `3000 → 127.0.0.1:3000` em até 2 segundos

### Requirement: Desbloqueio do cofre na TUI
Quando o cofre existe e está trancado, a TUI MUST mostrar, ao abrir, um modal pedindo a senha, sem mostrar o que é digitado. Enter MUST desbloquear; Esc MUST fechar o modal e deixar o cofre trancado. Uma senha errada MUST mostrar o erro no modal e pedir de novo. As abas "aguardando cofre" MUST mostrar o estado na barra.

#### Scenario: Senha na abertura
- **WHEN** o cofre está trancado e o usuário abre a TUI, digita a senha certa e Enter
- **THEN** o cofre fica desbloqueado, e as sessões aguardando o cofre sobem
