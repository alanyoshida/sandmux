## MODIFIED Requirements

### Requirement: Barra de abas
A TUI MUST mostrar, numa linha de barra, uma aba para cada sessão, com a posição (o número usado pelos atalhos de dígito), o nome e uma marca quando a sessão está encerrada ou parada, destacando a aba ativa. O identificador da sessão aparece na lista de abas. Por padrão, a barra MUST ficar na última linha, no estilo Powerline: o número num bloco colorido, separadores inclinados, a aba ativa com outra cor e, à direita, o harness e a pasta da aba ativa, o estado do cofre, os alertas de rede (só quando há) e o tempo de vida da aba ativa. `[tui]` MUST aceitar `style = "powerline"` ou `"simples"` (a barra sem símbolos especiais) e `bar = "bottom"` ou `"top"`. A área fora da barra, à esquerda do painel lateral quando ele está visível, MUST ser o terminal da aba ativa, e o tamanho dessa área MUST ser o tamanho do terminal da sessão.

#### Scenario: Aba ativa
- **WHEN** a aba 2 está ativa
- **THEN** a barra destaca a aba 2, e a área do terminal mostra a tela da sessão 2, com uma linha a menos que o terminal do usuário

#### Scenario: Blocos da direita
- **WHEN** a aba ativa é o opencode em `~/testesand`, viva há 1 hora e 20 minutos, o cofre está desbloqueado e uma aba teve um bloqueio de rede
- **THEN** a direita da barra mostra `opencode ~/testesand`, o cofre desbloqueado, `! 1` e `1h 20m`

#### Scenario: Estilo simples no topo
- **WHEN** `[tui]` tem `style = "simples"` e `bar = "top"`
- **THEN** a barra fica na primeira linha, sem separadores do Powerline, e um clique na linha 1 troca de aba
