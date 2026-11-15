## MODIFIED Requirements

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

### Requirement: Fechar aba
Fechar uma aba viva MUST pedir confirmação e encerrar a sessão como `sandmux kill`. Numa aba encerrada, o clique no ✕ da barra MUST fechá-la sem confirmação, porque não há processo a encerrar. A aba MUST sumir da barra, e a vizinha MUST ficar ativa.

#### Scenario: Fechar com confirmação
- **WHEN** o usuário digita Ctrl+b x e confirma
- **THEN** a sessão é encerrada e a aba some

#### Scenario: Clique no ✕ da aba encerrada
- **WHEN** o harness da aba 2 terminou e o usuário clica no ✕ dela na barra
- **THEN** a aba some da barra e a sessão sai de `sandmux ls`

## ADDED Requirements

### Requirement: Cópia do harness pela TUI
Quando a escrita no clipboard está liberada para a sessão, uma sequência OSC 52 de gravação vinda do harness da aba MUST chegar ao usuário pela TUI: a TUI MUST reemiti-la para o terminal do usuário e, quando há acesso, gravar o texto no clipboard do sistema. Com a escrita bloqueada, nada MUST mudar no clipboard.

#### Scenario: opencode com o clipboard liberado
- **WHEN** a sessão do opencode roda com `clipboard = true`, dentro do tmux, e o usuário copia um texto no opencode
- **THEN** o texto vai para o clipboard do sistema e para o do terminal

#### Scenario: Bloqueado por padrão
- **WHEN** a sessão roda sem `clipboard = true` e o harness manda uma sequência OSC 52 de gravação
- **THEN** o clipboard do sistema e o do terminal não mudam
