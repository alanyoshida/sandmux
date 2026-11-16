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
A TUI MUST mostrar, numa linha de barra, uma aba para cada sessão, com a posição (o número usado pelos atalhos de dígito), o nome e uma marca quando a sessão está encerrada ou parada, destacando a aba ativa. O identificador da sessão aparece na lista de abas. Por padrão, a barra MUST ficar na última linha, no estilo em blocos, só com caracteres que toda fonte tem: o `≡` do menu, o número de cada aba num bloco colorido, a aba ativa com outra cor e, à direita, o harness da aba ativa, a pasta dela num bloco de outra cor, o estado do cofre, os alertas de rede (só quando há) e o tempo de vida da aba ativa. `[tui]` MUST aceitar `style = "blocos"`, `"powerline"` (os mesmos blocos com separadores inclinados e ícones de uma Nerd Font) ou `"simples"` (a barra só com texto), e `bar = "bottom"` ou `"top"`. A área fora da barra, à esquerda do painel lateral quando ele está visível, MUST ser o terminal da aba ativa, e o tamanho dessa área MUST ser o tamanho do terminal da sessão.

#### Scenario: Aba ativa
- **WHEN** a aba 2 está ativa
- **THEN** a barra destaca a aba 2, e a área do terminal mostra a tela da sessão 2, com uma linha a menos que o terminal do usuário

#### Scenario: Qualquer fonte
- **WHEN** a TUI abre sem `style` em `[tui]`
- **THEN** a barra não tem nenhum caractere da área de uso privado do Unicode (os símbolos do Powerline e do Nerd Font)

#### Scenario: Blocos da direita
- **WHEN** a aba ativa é o opencode em `~/testesand`, viva há 1 hora e 20 minutos, o cofre está desbloqueado e uma aba teve um bloqueio de rede
- **THEN** a direita da barra mostra `opencode` e `~/testesand` em blocos de cores diferentes, o cofre desbloqueado, `! 1` e `1h 20m`

#### Scenario: Estilo simples no topo
- **WHEN** `[tui]` tem `style = "simples"` e `bar = "top"`
- **THEN** a barra fica na primeira linha, sem separadores do Powerline nem o `≡`, e um clique na linha 1 troca de aba

### Requirement: Emulação de terminal da aba
Cada aba MUST emular o terminal da sua sessão, mostrando o texto, as cores (16, 256 e cor verdadeira), os atributos (negrito, itálico, sublinhado, inverso), a posição e a visibilidade do cursor e a tela alternativa. Com a quebra automática de linha desligada (`CSI ? 7 l`), um caractere escrito depois da última coluna MUST sobrescrevê-la, sem passar para a linha de baixo. O cursor MUST se mover pelas paradas de tabulação para trás (`CSI Z`) e para a frente (`CSI I`). Trocar de aba MUST mostrar a tela atual da outra aba sem esperar o harness redesenhar.

#### Scenario: Cor verdadeira
- **WHEN** o harness da aba ativa escreve um texto com cor de frente `38;2;10;200;30`
- **THEN** a TUI mostra esse texto com a mesma cor

#### Scenario: Troca de aba
- **WHEN** a aba 1 mostra a interface do opencode e o usuário vai para a aba 2 e volta
- **THEN** a interface do opencode aparece de novo na hora, igual a antes

#### Scenario: Quebra automática desligada
- **WHEN** o harness desliga a quebra com `CSI ? 7 l` e escreve mais caracteres que a largura da linha
- **THEN** a linha mostra o começo do texto com o último caractere na última coluna, e a linha de baixo não muda

#### Scenario: Tabulação para trás
- **WHEN** o cursor está na coluna 20 e o harness manda `CSI 2 Z` e escreve um caractere
- **THEN** o caractere aparece na coluna 9, duas paradas de tabulação para trás

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
A seção `[tui]` do `config.toml` MUST aceitar `prefix` (`"C-<letra>"`) e, em `[tui.keys]`, a troca da tecla de cada ação (`new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help` e as acrescentadas depois, como `enter`) por um caractere. Duas ações escolhidas em `[tui.keys]` na mesma tecla, ou uma ação desconhecida, MUST tornar o arquivo inválido. Uma tecla escolhida que é a padrão de outra ação MUST vencer, e a outra ação MUST ficar sem tecla, para que uma ação nova não invalide um arquivo que já existia.

#### Scenario: Prefixo trocado
- **WHEN** `[tui]` tem `prefix = "C-a"` e o usuário digita Ctrl+a n
- **THEN** a próxima aba fica ativa, e Ctrl+b vai cru para a sessão

#### Scenario: Tecla de outra ação padrão
- **WHEN** `[tui.keys]` tem `new = "t"`, a tecla padrão de `enter`
- **THEN** o arquivo é válido, `t` cria uma aba nova, e `enter` fica sem tecla

#### Scenario: Atalho repetido
- **WHEN** `[tui.keys]` tem `new = "t"` e `close = "t"`
- **THEN** o arquivo é inválido, e a mensagem cita as duas ações

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
Uma aba cuja sessão está encerrada MUST continuar aberta, com a última tela, o código de saída e as teclas de reiniciar (`r`) e fechar (`x`). Uma aba parada MUST mostrar o motivo. Reiniciar MUST relançar a sessão como `sandmux restart`, com as variáveis de `pass-env` do ambiente da TUI. Quando o harness de uma aba termina com a TUI aberta, a TUI MUST abrir um modal com o código de saída, na hora se a aba está ativa ou ao ativá-la, uma vez: Enter ou `r` reinicia, `x` fecha a aba e Esc mantém a aba encerrada. Abas que já estavam encerradas ao abrir a TUI MUST NOT abrir o modal.

#### Scenario: Shell encerrado
- **WHEN** o usuário digita `exit 3` no shell da aba ativa
- **THEN** a aba continua na barra, marcada como encerrada, e a área mostra o código 3 e as teclas de reiniciar e fechar

#### Scenario: Reiniciar
- **WHEN** a aba ativa está encerrada e o usuário digita Ctrl+b r
- **THEN** a sessão volta viva na mesma aba

#### Scenario: Modal ao terminar
- **WHEN** o hermes da aba ativa termina o assistente de configuração e sai com código 0
- **THEN** a TUI abre o modal com o código 0, e Enter relança o hermes na mesma aba

#### Scenario: Aba em segundo plano
- **WHEN** o shell da aba 2 sai enquanto a aba 1 está ativa, e o usuário ativa a aba 2
- **THEN** o modal aparece ao ativar a aba 2, e Esc a deixa encerrada sem perguntar de novo

### Requirement: Fechar aba
Fechar uma aba viva MUST pedir confirmação e encerrar a sessão como `sandmux kill`. Numa aba encerrada, o clique no ✕ da barra MUST fechá-la sem confirmação, porque não há processo a encerrar. A aba MUST sumir da barra, e a vizinha MUST ficar ativa.

#### Scenario: Fechar com confirmação
- **WHEN** o usuário digita Ctrl+b x e confirma
- **THEN** a sessão é encerrada e a aba some

#### Scenario: Clique no ✕ da aba encerrada
- **WHEN** o harness da aba 2 terminou e o usuário clica no ✕ dela na barra
- **THEN** a aba some da barra e a sessão sai de `sandmux ls`

### Requirement: Sair da TUI
Sair (prefixo e `d`, fechar o terminal ou SIGTERM) MUST desanexar todas as abas, deixando as sessões vivas, e MUST restaurar o terminal do usuário.

#### Scenario: Sair e voltar
- **WHEN** o usuário sai da TUI e roda `sandmux` de novo
- **THEN** as mesmas abas aparecem, com as telas atuais

### Requirement: Mouse
A TUI MUST ligar o mouse no terminal do usuário. Um clique na barra MUST ativar a aba clicada. Na área do terminal, quando o harness da aba ativa ligou o mouse, os eventos MUST ir para a sessão, com as coordenadas relativas à área. Quando o harness não ligou o mouse, a roda MUST rolar o histórico da aba, e arrastar MUST selecionar texto, copiado ao soltar para o clipboard do terminal do usuário (OSC 52) e, quando há acesso, também para o clipboard do sistema (Wayland ou X11).

#### Scenario: Clique na barra
- **WHEN** o usuário clica no nome da aba 2
- **THEN** a aba 2 fica ativa

#### Scenario: Mouse no harness
- **WHEN** o opencode ligou o mouse e o usuário clica na linha 5 da área do terminal
- **THEN** o opencode recebe o clique na linha 5 da tela dele

#### Scenario: Seleção no shell
- **WHEN** o usuário arrasta sobre um texto no shell confinado
- **THEN** ao soltar, o texto vai para o clipboard do terminal do usuário e para o clipboard do sistema

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
A seção "Proteção" MUST mostrar, para o último lançamento da aba ativa: o nível de proteção, com a ABI do Landlock; a camada de escrita; a pasta de trabalho e as pastas extras, cada uma em somente leitura ou com escrita; a GPU e o KVM; a janela, restrita ou direta; a escrita no clipboard; as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente; os tetos de memória e de processos; e o estado da rede.

#### Scenario: Pasta extra e variável
- **WHEN** a sessão da aba ativa foi lançada com `--ro ~/docs` e `--pass-env OPENROUTER_API_KEY`
- **THEN** o painel mostra `~/docs` em somente leitura e `OPENROUTER_API_KEY` com o aviso

#### Scenario: Janela
- **WHEN** a sessão da aba ativa foi lançada com `window = true`
- **THEN** o painel mostra a janela ligada, e com `window = "direct"`, a janela direta

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

### Requirement: Cópia do harness pela TUI
Quando a escrita no clipboard está liberada para a sessão, uma sequência OSC 52 de gravação vinda do harness da aba MUST chegar ao usuário pela TUI: a TUI MUST reemiti-la para o terminal do usuário e, quando há acesso, gravar o texto no clipboard do sistema. Com a escrita bloqueada, nada MUST mudar no clipboard.

#### Scenario: opencode com o clipboard liberado
- **WHEN** a sessão do opencode roda com `clipboard = true`, dentro do tmux, e o usuário copia um texto no opencode
- **THEN** o texto vai para o clipboard do sistema e para o do terminal

#### Scenario: Bloqueado por padrão
- **WHEN** a sessão roda sem `clipboard = true` e o harness manda uma sequência OSC 52 de gravação
- **THEN** o clipboard do sistema e o do terminal não mudam

### Requirement: Menu de ações
A tecla de prefixo seguida de `:`, ou um clique no `≡` do começo da barra, MUST abrir um menu com todas as ações da TUI e as teclas delas. As setas MUST mover a seleção, Enter MUST executar a ação, e Esc MUST fechar o menu. A ação MUST se chamar `menu` em `[tui.keys]`.

#### Scenario: Abrir as configurações pelo menu
- **WHEN** o usuário digita Ctrl+b `:`, desce até "configurações" e aperta Enter
- **THEN** a página de configurações abre

### Requirement: Página de configurações
A tecla de prefixo seguida de `e` MUST abrir uma página de tela cheia com as opções do `config.toml`, em seções, cada uma com toggles, escolhas, textos ou listas, e o valor atual de cada opção. Um seletor MUST escolher se a mudança vale para `[global]` ou para o projeto da pasta da aba ativa. Cada mudança MUST ser gravada no `config.toml` na hora, pelo editor de configuração, que valida o arquivo inteiro; uma mudança recusada MUST mostrar o motivo e não gravar. A ação MUST se chamar `settings` em `[tui.keys]`.

#### Scenario: Toggle gravado
- **WHEN** na seção Sandbox o usuário liga `kvm` com o seletor em global
- **THEN** o `config.toml` passa a ter `kvm = true` em `[global]`, e o item mostra `● ligado`

#### Scenario: Valor recusado
- **WHEN** o usuário escreve `abc` em memória
- **THEN** a página mostra o motivo, e o `config.toml` não muda

#### Scenario: Lista
- **WHEN** na seção Rede o usuário acrescenta `8888` às portas do host
- **THEN** o `config.toml` passa a ter `host-ports = [8888]`

### Requirement: Temas prontos
`[tui] theme` MUST aceitar `padrao`, `escuro`, `claro`, `catppuccin`, `rose-pine`, `tokyo-night`, `kanagawa`, `gruvbox` e `dracula`, e o tema MUST pintar a barra, os modais, o painel e a página de configurações. Na página, passar por um tema MUST mostrá-lo na hora, com amostras de cor, e gravá-lo em `[tui]`.

#### Scenario: Trocar de tema
- **WHEN** na seção Aparência o usuário passa o tema para `tokyo-night`
- **THEN** a barra e a página mudam de cor na hora, e o `config.toml` tem `theme = "tokyo-night"`

### Requirement: Seções do painel
Cada seção do painel lateral (proteção, recursos, portas, rede e MCP) MUST começar por uma faixa com o título, um resumo e as setas `↑` e `↓`. Um clique no título MUST recolher ou expandir a seção, e um clique numa seta MUST movê-la uma posição. A ordem e as seções recolhidas MUST ser gravadas em `[tui]` (`panel-order` e `panel-collapsed`) e valer ao abrir a TUI; a ordem padrão e nenhuma seção recolhida MUST apagar as chaves.

#### Scenario: Mudar a ordem
- **WHEN** o usuário clica no `↓` da faixa de Proteção, a primeira
- **THEN** Recursos passa ao topo, e o `config.toml` tem `panel-order = ["recursos", "protecao", "portas", "rede", "mcp"]`

#### Scenario: Recolher
- **WHEN** o usuário clica no título de Recursos e abre a TUI de novo
- **THEN** a seção aparece recolhida, só com a faixa, e o `config.toml` tem `panel-collapsed = ["recursos"]`

### Requirement: Foco do terminal para as notificações
A TUI MUST ligar o aviso de foco do terminal enquanto está aberta e MUST informar ao daemon quando ganha ou perde o foco e qual aba está mostrando, também ao trocar de aba. Os eventos de foco MUST NOT chegar à aba como texto. Eles MUST ser repassados à aba só quando o harness dela pediu o aviso de foco. A página de configurações MUST ter, na seção Geral, o item que liga e desliga as notificações (`[global] notify`).

#### Scenario: Harness sem aviso de foco
- **WHEN** a TUI perde o foco e o harness da aba ativa não pediu o aviso de foco
- **THEN** a aba não recebe nada, e o daemon fica sabendo que a TUI está sem foco

#### Scenario: Harness com aviso de foco
- **WHEN** o harness da aba ativa pediu o aviso de foco e a TUI ganha o foco
- **THEN** a aba recebe o aviso de foco, como receberia num terminal comum

### Requirement: TUI por grupo
A TUI MUST mostrar só as abas do seu grupo atual e o nome do grupo na barra. Uma aba criada na TUI MUST ir para o grupo atual. A TUI MUST abrir no grupo de `--group`; sem ele, no `principal`, ou, com o `principal` vazio, no primeiro grupo com abas. Cada TUI MUST ter o seu grupo, sem mudar o de outra TUI aberta.

#### Scenario: Duas TUIs
- **WHEN** uma TUI mostra o grupo `principal` e outra, aberta com `--group estudos`, mostra o grupo `estudos`
- **THEN** cada uma mostra só as abas do seu grupo, e uma aba criada em uma não aparece na outra

### Requirement: Troca de grupo
Ctrl+b `(` e Ctrl+b `)` MUST ir ao grupo anterior e ao próximo, em ordem de nome. Ctrl+b `G` MUST abrir a lista de grupos, com o número de abas de cada um:

- Enter troca para o grupo escolhido;
- `n` pede um nome e troca para um grupo novo, vazio até a primeira aba;
- `r` renomeia o grupo escolhido;
- `m` leva a aba ativa para o grupo escolhido.

Ao trocar de grupo, a TUI MUST desanexar as abas do grupo que sai da tela, sem encerrá-las.

#### Scenario: Próximo grupo
- **WHEN** existem os grupos `estudos` e `principal`, a TUI mostra `estudos`, e o usuário aperta Ctrl+b `)`
- **THEN** a TUI passa a mostrar as abas do `principal`, e as abas de `estudos` continuam vivas

#### Scenario: Mover a aba ativa
- **WHEN** o usuário abre a lista de grupos, escolhe `estudos` e aperta `m`
- **THEN** a aba ativa sai da tela e passa a ser do grupo `estudos`

### Requirement: Modais do assistente com o tema
Os modais de comando (Ctrl+b `g`) e de chat (Ctrl+b `i`) MUST usar as cores do tema ativo: o fundo e o texto da página, o título num bloco da cor de destaque e os rótulos na cor secundária. O campo ativo MUST aparecer sublinhado na cor de destaque. Cada modal MUST ter um botão em dois tons, com `⏎` e o rótulo da ação (`Enviar` ao perguntar, `Inserir` com o comando pronto), alcançável pelo Tab e clicável, com o mesmo efeito do Enter.

#### Scenario: Cores do tema
- **WHEN** o tema ativo é `catppuccin` e o usuário abre o modal de comando
- **THEN** o fundo do modal tem a cor `page-bg` do tema, e o título aparece num bloco da cor `accent`

#### Scenario: Clique em Enviar
- **WHEN** o usuário escreve a pergunta no modal de comando e clica em `Enviar`
- **THEN** a pergunta vai ao modelo, como com Enter

#### Scenario: Tab até o botão
- **WHEN** o usuário aperta Tab até o botão e Enter
- **THEN** a ação do botão acontece

### Requirement: Chat no modal de nova aba
O modal de nova aba MUST oferecer `chat` depois dos harnesses. Com `chat` escolhido, Enter MUST abrir o modal de chat, com o modelo e a mensagem, e a dica do modal MUST dizer que a pasta e o perfil não valem para o chat.

#### Scenario: Escolher chat
- **WHEN** o usuário abre o modal de nova aba, passa com → até `chat` e aperta Enter
- **THEN** o modal de chat abre, com o modelo padrão de `[assistant]`

### Requirement: Painéis na aba
Ctrl+b `%` MUST dividir o painel ativo lado a lado, e Ctrl+b `"` em cima e embaixo. As duas abrem o modal de nova aba com a pasta do painel ativo, e a sessão criada MUST ocupar a parte nova. Cada painel MUST mostrar a sua sessão, com o tamanho do painel. A entrada do teclado MUST ir só ao painel ativo.

#### Scenario: Dividir lado a lado
- **WHEN** a aba tem um painel e o usuário aperta Ctrl+b `%` e cria uma sessão do shell no modal
- **THEN** a aba mostra as duas sessões lado a lado, e o que é digitado vai à sessão nova

### Requirement: Foco, ampliar, redimensionar e fechar painéis
Ctrl+b com uma seta MUST mover o foco ao painel vizinho naquela direção, e um clique num painel MUST pôr o foco nele. Ctrl+b `z` MUST alternar entre o painel ativo na aba inteira e o layout. Ctrl+b `<` e `>` MUST diminuir e aumentar o painel ativo. Fechar o painel ativo MUST encerrar a sessão dele e dar o espaço e o foco ao vizinho, na mesma aba; com um painel só, a aba volta a ser comum.

#### Scenario: Foco pela seta
- **WHEN** a aba tem dois painéis lado a lado, o da direita está ativo e o usuário aperta Ctrl+b ←
- **THEN** o painel da esquerda passa a receber o que é digitado

#### Scenario: Fechar um painel
- **WHEN** o usuário fecha o painel ativo com Ctrl+b `x`, e há outra aba
- **THEN** a sessão dele é encerrada, e o outro painel ocupa a aba inteira e recebe o foco, sem trocar de aba

### Requirement: Barra por aba com painéis
A barra MUST mostrar uma entrada por aba, com o nome do primeiro painel e `+N` para os outros painéis. Os números de Ctrl+b `1`..`9` e a troca de aba MUST contar as abas, não os painéis.

#### Scenario: Aba com dois painéis
- **WHEN** a aba 1 tem dois painéis
- **THEN** a barra mostra uma só entrada para ela, com `+1`

### Requirement: Reiniciar aba viva
A ação `restart` (Ctrl+b `r`) numa aba viva MUST pedir confirmação. Confirmada, a TUI MUST pedir ao daemon o reinício forçado e anexar a aba relançada. Recusada, nada muda.

#### Scenario: Confirmação
- **WHEN** a aba ativa está viva e o usuário aperta Ctrl+b `r` e `s`
- **THEN** o harness é encerrado e relançado na mesma aba, com a configuração de agora

#### Scenario: Recusa
- **WHEN** o usuário aperta Ctrl+b `r` e `n` numa aba viva
- **THEN** a aba continua como estava

### Requirement: Aviso das abas vivas na configuração
Uma mudança gravada pela tela de configurações num item que vale no lançamento MUST dizer quantas abas vivas continuam com o valor antigo e que Ctrl+b `r` as reinicia. O `sandmux config set`, `unset`, `add` e `remove` MUST mostrar o mesmo aviso quando o daemon tem sessões vivas.

#### Scenario: Janela ligada com abas vivas
- **WHEN** há duas abas vivas e o usuário liga `window` na tela de configurações
- **THEN** a página diz que 2 abas vivas continuam sem a mudança e que Ctrl+b `r` reinicia a aba

### Requirement: Diagnóstico na página de configurações
A página de configurações MUST ter a seção "Diagnóstico", depois do Cofre, com as verificações do `sandmux doctor`: uma linha por verificação, com `✓`, `✗` ou `·`, agrupadas, e a causa e a correção da linha escolhida embaixo. A seção MUST rodar as verificações ao entrar, e `r` MUST rodá-las de novo. O nome da seção MUST mostrar quantas falharam. A ação `doctor` (padrão `D`) MUST abrir a página na seção.

#### Scenario: Ver uma falha
- **WHEN** a máquina não tem o binário `registry` e o usuário digita Ctrl+b `D`
- **THEN** a página abre no Diagnóstico, a lateral mostra "Diagnóstico ✗1", a linha do cache de imagens tem `✗`, e, escolhida, mostra embaixo como instalar o `registry`

#### Scenario: Rodar de novo
- **WHEN** o arquivo com uma chave real é apagado fora da TUI e o usuário aperta `r` na seção
- **THEN** a linha da chave some

#### Scenario: Saída do terminal igual
- **WHEN** o usuário roda `sandmux doctor` no terminal
- **THEN** a saída é a mesma de antes deste change
