# Proposal

## Why

Hoje cada sessão confinada ocupa um terminal inteiro (`sandmux attach <id>`), e quem roda vários agentes precisa de vários terminais ou de um tmux por fora. O roadmap decidiu um multiplexador próprio, em ratatui:

- abas, cada uma com um terminal confinado;
- criação de aba escolhendo harness e pasta;
- tecla de prefixo e atalhos configuráveis;
- modal com a lista de abas;
- renomear abas;
- mouse;
- colar imagem para a pasta de entrada.

A etapa 2 deixou pronto o que a TUI precisa: as sessões vivem no daemon e sobrevivem ao cliente, o daemon reenvia o histórico e o estado do terminal ao anexar, e a configuração e os tetos ficam em camadas. Este change é o cliente que junta tudo numa tela só.

## What Changes

- **`sandmux` sem subcomando, ou `sandmux tui`, num terminal, abre a TUI.**
  - Cada sessão do daemon é uma aba; é o conjunto único de abas por usuário que o roadmap decidiu.
  - A barra de abas fica no topo, com o identificador, o nome e o estado de cada aba. A área de baixo mostra o terminal da aba ativa.
  - Sair da TUI (prefixo e `d`) desanexa, e as sessões continuam.
- **Emulação de terminal:**
  - cada aba tem o próprio emulador, alimentado pela saída da sessão;
  - a TUI desenha a tela da aba ativa: cores, inclusive cor verdadeira, atributos, cursor e tela alternativa;
  - os modos que o harness ligou (protocolo de teclado do kitty, colagem entre marcadores, eventos de foco) são repetidos no terminal do usuário quando a aba fica ativa, e a entrada vai crua para a sessão. Assim o opencode e o hermes recebem as teclas como num terminal direto.
- **Tecla de prefixo (Ctrl+b por padrão)** com atalhos no estilo do tmux:
  - `c`: nova aba;
  - `n` e `p`: próxima e anterior;
  - `0` a `9`: ir para a aba;
  - `w`: modal com a lista;
  - `,`: renomear;
  - `r`: reiniciar uma aba encerrada ou parada;
  - `x`: fechar, com confirmação;
  - `v`: colar imagem;
  - `d`: sair;
  - `?`: ajuda;
  - Ctrl+b Ctrl+b envia um Ctrl+b.

  O prefixo e os atalhos se configuram na seção nova `[tui]` do `config.toml`.
- **Nova aba:** um modal pede o harness (opencode, hermes ou shell), a pasta de trabalho (por padrão, a pasta de onde a TUI foi aberta) e o perfil. A aba nova passa pelas mesmas verificações e resolução de camadas do `run`, inclusive a pergunta de importação na primeira execução.
- **Aba de harness encerrado** continua aberta, com a última tela, o código de saída e o aviso de `r` para reiniciar e `x` para fechar. Uma aba parada mostra o motivo.
- **Renomear:** o nome da aba é o nome da sessão. O daemon ganha o pedido `Rename`, que grava o nome na lista de sessões, e a linha de comando ganha `sandmux rename <id> <nome>`.
- **Mouse:**
  - um clique na barra troca de aba;
  - na área do terminal, quando o harness ligou o mouse, os eventos vão para ele com as coordenadas da área;
  - quando o harness não ligou o mouse, a roda rola o histórico da aba, e arrastar seleciona texto, copiado ao soltar para o clipboard do terminal do usuário (OSC 52), o que sempre funciona, conforme a decisão de clipboard do change 2;
  - Shift com arrasto fica para a seleção nativa do terminal.
- **Colar imagem (prefixo e `v`):** a TUI lê a imagem do clipboard do sistema (X11 ou Wayland), grava um PNG na pasta de entrada e escreve na aba ativa o caminho dele no sandbox (`~/inbox/colado-<data>.png`). Sem imagem no clipboard, a TUI avisa na barra.
- **Protocolo 5**, pelo pedido `Rename`.

## Capabilities

### New Capabilities
- `terminal-ui`: a TUI com abas, a emulação de terminal, a tecla de prefixo e os atalhos, os modais de nova aba, lista, renomear, fechar e ajuda, as abas encerradas e paradas, o mouse e a colagem de imagem.

### Modified Capabilities
- `terminal-sessions`: requisito novo "Renomear uma sessão", com o pedido ao daemon e `sandmux rename`.
- `config-profiles`: "Seções do arquivo" passa a aceitar `[tui]`, com o prefixo e os atalhos.

## Impact

- **Dependências novas:**
  - `ratatui` (0.30) e `crossterm` (0.29), para desenhar a tela;
  - `arboard`, para ler imagem do clipboard em X11 e Wayland;
  - `png`, para gravar a imagem.
- **`sandmux` (cliente):**
  - o módulo novo `tui`: abas, emulador por aba com o `vt100`, desenho, entrada crua com a tecla de prefixo, modais, mouse, seleção e colagem;
  - uma conexão anexada por aba, com o protocolo atual;
  - a lista de sessões vem de pedidos `List` periódicos.
- **`sandmux-proto`:**
  - mensagem `Rename`;
  - `PROTOCOL` 5.

  Os modos de mouse, de colagem e de cursor vêm do estado do `vt100` de cada aba.
- **`sandmux-policy`:** a seção `[tui]` na leitura e na validação do `config.toml`.
- **Daemon:** o pedido `Rename`, gravado na lista de sessões.
- **Documentação:** `docs/tui.md`, novo, e o README.
