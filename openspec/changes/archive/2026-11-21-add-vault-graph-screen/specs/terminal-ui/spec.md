## ADDED Requirements

### Requirement: Foco e filtro na tela do grafo do cofre
Na tela do grafo do cofre, `↑` e `↓` MUST escolher o domínio na lista. `Tab` ou `→` MUST levar o foco aos segredos do domínio escolhido, onde `↑`, `↓` e `Tab` trocam o segredo em foco; `←` ou Esc MUST devolver o foco à lista. `o` MUST levar ao dono seguinte de um segredo com vários donos. `/` MUST filtrar a lista pelo nome do domínio ou de um segredo dele. O rodapé MUST mostrar as teclas que valem e o zoom atual.

#### Scenario: Foco num segredo
- **WHEN** com `github.com` escolhido, o usuário aperta `Tab`
- **THEN** o primeiro segredo de `github.com` fica em destaque, e os detalhes mostram esse segredo

#### Scenario: Outro dono
- **WHEN** com `gh-pessoal` em foco no grafo de `github.com`, o usuário aperta `o`
- **THEN** a lista passa para o dono seguinte de `gh-pessoal`, com `gh-pessoal` ainda em foco

#### Scenario: Filtro pelo nome do segredo
- **WHEN** o usuário aperta `/` e digita `npm`
- **THEN** a lista mostra só `registry.npmjs.org`, o dono de `npm-token`

### Requirement: Zoom e movimento do grafo do cofre
`+` e `-` MUST aumentar e diminuir o zoom do grafo, de 0.5x a 2.0x. `h`, `j`, `k` e `l` MUST mover o grafo para a esquerda, para baixo, para cima e para a direita. `0` MUST voltar ao centro e ao zoom 1.0x. A roda do mouse sobre o grafo MUST mudar o zoom, e arrastar com o botão esquerdo MUST mover o grafo. Um clique MUST escolher o domínio clicado na lista ou pôr o foco no segredo clicado no grafo.

#### Scenario: Zoom pelo teclado
- **WHEN** o zoom está em 1.0x e o usuário aperta `+`
- **THEN** o rodapé mostra um zoom maior que 1.0x, e os segredos ficam mais longe do domínio do centro

#### Scenario: Limite do zoom
- **WHEN** o zoom está em 2.0x e o usuário aperta `+`
- **THEN** o zoom continua em 2.0x

#### Scenario: Arrastar
- **WHEN** o usuário arrasta o grafo 10 colunas para a direita
- **THEN** o domínio do centro aparece 10 colunas à direita de onde estava

#### Scenario: Voltar ao centro
- **WHEN** depois de mover e mudar o zoom o usuário aperta `0`
- **THEN** o domínio volta ao meio da área do grafo, e o rodapé mostra `1.0x`

### Requirement: Detalhe dos nós pelo zoom
O texto dos nós MUST NOT mudar de tamanho com o zoom. Abaixo de 0.7x, cada segredo MUST aparecer só como a marca do tipo; de 0.7x até abaixo de 1.3x, como a marca e o nome; de 1.3x em diante, como um cartão com o nome, o tipo e as ferramentas. A marca MUST dizer o tipo sem depender da cor: `●` para um token, `◆` para uma chave SSH e `■` para uma conexão de banco.

#### Scenario: Zoom padrão
- **WHEN** o zoom está em 1.0x
- **THEN** cada segredo aparece como a marca e o nome, sem o tipo por extenso

#### Scenario: Zoom de perto
- **WHEN** o usuário aumenta o zoom até 1.6x com um token de ferramentas `gh git` no grafo
- **THEN** o segredo aparece num cartão com o nome e `token · gh git`

#### Scenario: Zoom de longe
- **WHEN** o usuário diminui o zoom até 0.5x
- **THEN** os segredos aparecem só como marcas, e o domínio do centro continua com o nome

### Requirement: Grafo do cofre em terminal estreito
Com 100 colunas ou mais, a tela do grafo MUST mostrar a lista, o grafo e os detalhes lado a lado. De 70 a 99 colunas, MUST mostrar a lista e o grafo, e Enter MUST abrir e fechar os detalhes do segredo em foco sobre o grafo. Com menos de 70 colunas, MUST mostrar só a lista e um aviso de que o grafo pede 70 colunas.

#### Scenario: Terminal de 120 colunas
- **WHEN** o terminal tem 120 colunas e um segredo está em foco
- **THEN** os detalhes aparecem à direita do grafo, sem apertar Enter

#### Scenario: Terminal de 80 colunas
- **WHEN** o terminal tem 80 colunas e o usuário põe o foco num segredo e aperta Enter
- **THEN** os detalhes aparecem numa caixa sobre o grafo, e Enter de novo fecha a caixa

#### Scenario: Terminal de 60 colunas
- **WHEN** o terminal tem 60 colunas e o usuário abre a tela do grafo
- **THEN** a tela mostra a lista de domínios e o aviso das 70 colunas

### Requirement: Arestas do grafo sem Braille
`[tui]` MUST aceitar `graph-edges = "braille"` ou `"blocos"`; sem a chave, vale `"braille"`. Com `"braille"`, as arestas do grafo do cofre MUST usar os pontos do Braille; com `"blocos"`, MUST usar meio-bloco, sem nenhum caractere de Braille na tela. A seção Aparência da página de configurações MUST ter essa escolha. Outro valor MUST tornar o arquivo inválido.

#### Scenario: Blocos
- **WHEN** `[tui]` tem `graph-edges = "blocos"` e o usuário abre o grafo de um domínio com segredos
- **THEN** as arestas aparecem em meio-bloco, e a tela não tem nenhum caractere de U+2800 a U+28FF

#### Scenario: Valor inválido
- **WHEN** `[tui]` tem `graph-edges = "linhas"`
- **THEN** o arquivo é considerado inválido, com a linha e os valores aceitos

#### Scenario: Escolha na página
- **WHEN** na seção Aparência o usuário troca as arestas do grafo para blocos
- **THEN** o `config.toml` passa a ter `graph-edges = "blocos"` em `[tui]`
