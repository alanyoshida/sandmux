## MODIFIED Requirements

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

## ADDED Requirements

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
