## Context

- A seção Cofre da página de configurações (change 38) guarda em `VaultView` o estado do cofre, a lista de `VaultEntry` (nome, domínios, valor falso, ferramentas, tipo e impressão digital, sem o valor), o formulário aberto e a confirmação de apagar. As teclas ficam em `vault_key`.
- A página gasta 27 colunas com a moldura e a coluna de seções. Num terminal de 120 colunas sobram 93, e a tela de três partes pede cerca de 96.
- Toda a TUI é desenhada com `Paragraph`, `Block` e escrita direta no `Buffer`. Não há uso do `Canvas`, nem código de grafo, zoom ou movimento.
- O mouse já chega à TUI no modo `?1002` com SGR, que informa botão, arrastar e roda. A enumeração `Key` não tem Shift com seta.
- A seção Processos (change 83) já usa `+`, `=` e `-` para o zoom, em três níveis de densidade. A tela do grafo usa as mesmas teclas e a mesma ideia de três níveis.
- Os temas não têm cores por tipo de segredo. Os elementos que servem são `accent`, `ok-fg`, `status-fg`, `error-fg` e `muted-fg`.

A exploração de 2026-11-17 comparou sete desenhos em mockup (estrela, constelação, trilhos, híbrido, órbitas, roda e bolhas). O usuário escolheu o híbrido, em tela cheia própria, com edição na própria tela e a estrela simples no meio.

## Goals / Non-Goals

**Goals:**
- Ver, a partir de um domínio, os segredos que podem ir para ele, e a partir de um segredo, todos os domínios donos.
- Zoom e movimento pelo teclado e pelo mouse.
- Editar o cofre sem sair da tela, com os formulários que já existem.
- Modelo e posições em funções puras, testadas sem terminal.

**Non-Goals:**
- Visão do cofre inteiro numa só tela.
- Posicionamento automático por forças ou por empacotamento.
- Criar, trocar ou apagar conexões de banco pela tela.
- Mensagem nova no protocolo.
- Cores novas nos temas.
- Ligar e desligar um segredo de um domínio arrastando no grafo.

## Decisions

- **A tecla `g`:** na página de configurações, `g` já troca o alvo entre o global e o projeto (achado na implementação, 2026-11-21). Na seção Cofre o alvo não tem efeito, então nela `g` abre o grafo, com o foco nas seções ou na tela do cofre, e o rodapé mostra "g grafo" no lugar de "g alvo". Nas outras seções nada muda. Alternativa descartada: outra letra, que deixaria na seção Cofre um `g` sem efeito.

- **Onde fica o estado:** `VaultView` ganha `graph: Option<GraphView>`. Com ele preenchido, `draw_settings` desenha a tela do grafo na área inteira da página, sem a coluna de seções, e `vault_key` trata as teclas do grafo. Assim os formulários, a confirmação de apagar, a linha de aviso e a releitura do cofre são os mesmos da seção, e Esc só limpa o campo. Alternativa descartada: um `Modal` próprio, como a tela de rede, que teria de guardar a página aberta para voltar a ela e duplicar os formulários.

- **Modelo:** uma função pura monta, a partir de `VaultEntry`, a lista de domínios em ordem alfabética, cada um com os seus segredos, e o grupo dos segredos sem domínio. Cada texto de domínio é um nó: `github.com` e `*.github.com` ficam separados, porque são padrões diferentes para a troca no tráfego. As entradas de tipo `db` vêm em `VaultList` sem domínio, e por isso são tiradas dos domínios e do grupo sem domínio, onde cairiam por engano. Alternativa descartada: agrupar padrões que se cobrem, que esconderia a diferença que decide para onde o valor vai.

- **Posições:** o domínio fica na origem e os segredos numa elipse em volta, em ângulos iguais, em ordem alfabética. O primeiro nó fica em cima; meio passo de giro tira os nós da linha do centro, onde está a caixa do domínio, e intercala as linhas de um anel com as do vizinho. O primeiro anel leva 8 segredos, o segundo 12 e o terceiro 16: cada anel leva 4 a mais que o anterior, porque é maior (decisão do usuário, 2026-11-21). Com 20 segredos são dois anéis. A unidade do mundo é uma linha de terminal, e uma unidade na horizontal vale duas colunas, porque a célula tem perto de duas vezes a altura da largura. Alternativa descartada: posições por forças, desnecessárias com um domínio por vez.

- **Grupo "Bancos":** quando o cofre tem conexões de banco, a lista termina com o grupo "Bancos", depois de "Sem domínio" (decisão do usuário, 2026-11-21). A tela pede `DbList` ao abrir e ao reler, e mostra as conexões em volta de uma caixa apagada, sem arestas, com a marca `■`. Os detalhes mostram o tipo, o servidor e a porta, o banco e o usuário; a senha não vem em `DbList`. O grupo é só de leitura: `a`, `s`, `d`, `v` e `x` com uma conexão em foco só mostram o aviso de que as conexões se gerenciam por `sandmux db`. Alternativa descartada: o servidor do banco como um nó entre os domínios, que misturaria dois sentidos de domínio, porque o banco não passa pela troca no tráfego.

- **Janela:** `GraphView` guarda o centro da janela e o zoom, em sete passos: 0.5, 0.64, 0.8, 1.0, 1.25, 1.6 e 2.0. A coluna de um ponto é o meio da área mais `(x - cx) * 2 * zoom`, e a linha é o meio mais `(y - cy) * zoom`. O movimento anda duas unidades por tecla e para quando o centro sai da caixa que contém os nós mais uma margem de 20 unidades na horizontal e 10 na vertical: sem a margem, um domínio com dois segredos, um em cima e um embaixo, não se moveria para os lados. Trocar de domínio volta o centro à origem e mantém o zoom.

- **Zoom semântico:** o texto do terminal não escala. Abaixo de 0.7x o nó é só a marca; de 0.7x até 1.3x, a marca e o nome; de 1.3x em diante, um cartão de duas linhas com o nome, o tipo e as ferramentas. O domínio do centro mostra o nome numa caixa e, de 1.3x em diante, também o número de segredos. Abaixo de 0.7x os nós ficam junto do centro e cairiam sobre os cantos da caixa, então o centro mostra só `◉` e o nome.

- **Desenho:** as arestas vão num `Canvas` do ratatui com os limites iguais à área em células, de modo que as coordenadas do `Canvas` sejam as da janela. Os nós são escritos no `Buffer` depois do `Canvas`, por cima das arestas. O texto de um nó sai para fora do centro: à direita do nó quando ele está à direita, à esquerda quando está à esquerda, e centrado quando está em cima ou embaixo. Assim dois nós da mesma linha não se cobrem. Enquanto o nó está na área, o texto é empurrado para dentro dela em vez de ser cortado na borda. O marcador é `Marker::Braille` ou `Marker::HalfBlock`, pela chave `graph-edges`. Alternativa descartada: um rasterizador próprio de linhas, que repetiria o que o `Canvas` já faz e não trocaria de marcador de graça.

- **Tipo e estado sem depender da cor:** a marca diz o tipo (`●` token, `◆` chave SSH, `■` conexão de banco) e a cor diz o estado: `ok-fg` para um segredo de um só domínio, `status-fg` para um segredo com outros donos, que também leva `+N` depois do nome, e `error-fg` para o grupo sem domínio. O domínio usa `accent`. Nenhum elemento novo de tema.

- **Teclas:**

  | Tecla | Ação |
  |---|---|
  | `↑` `↓` | Com o foco na lista, escolhem o domínio; com o foco no grafo, o segredo anterior e o seguinte |
  | `Tab` `→` | Foco no grafo, no primeiro segredo; `Tab` de novo passa ao seguinte |
  | `←` | Foco de volta na lista |
  | `Enter` | Com um nó em foco, abre e fecha os detalhes, quando eles não cabem ao lado; sem foco, leva o foco ao primeiro nó |
  | `o` | Com um segredo de vários donos em foco, vai ao dono seguinte |
  | `+` `=` `-` | Zoom |
  | `h` `j` `k` `l` | Movem o grafo |
  | `0` | Centro na origem e zoom 1.0x |
  | `/` | Filtro da lista; Enter mantém, Esc limpa |
  | `a` `s` `d` `v` `x` | Como na seção Cofre |
  | `u` | Desbloqueia, com o cofre trancado |
  | `r` | Relê o cofre |
  | `Esc` | Sai do filtro, depois volta o foco à lista, depois fecha a tela |

  `l` e `k` da seção (trancar e keyring) não valem na tela do grafo: trancar esvaziaria a tela, e os dois continuam a um Esc de distância. Alternativa descartada para o movimento: Shift com seta, que a `Key` não distingue.

- **Mouse:** a roda muda o zoom em torno do meio da área do grafo. Arrastar com o botão esquerdo dentro da área do grafo move a janela pelo deslocamento em células. Um clique numa linha da lista escolhe o domínio, e um clique num nó põe o foco nele. As áreas de clique são guardadas no desenho, em `settings_hits`, e a área do grafo num campo próprio do `App`, para a roda e o arrastar.

- **Edição:** `a` e `s` abrem o formulário com o campo de domínios já preenchido com o domínio escolhido, pelo método novo `VaultForm::prefill`. `d`, `v` e `x` agem no segredo em foco. Depois do envio a tela relê o cofre, remonta o modelo e mantém o domínio e o segredo pelo nome; se o domínio sumiu, escolhe o vizinho na lista.

- **Cofre trancado ou ausente:** sem cofre, `g` na seção só mostra o aviso de que não há cofre. Com o cofre trancado, a tela abre com o aviso e a tecla `u`; ao desbloquear, o grafo aparece.

- **Largura:** com 100 colunas ou mais, a lista tem 26 colunas, os detalhes 28 e o grafo o resto. De 70 a 99, só lista e grafo, e os detalhes abrem com Enter numa caixa sobre o grafo. Com menos de 70, só a lista, com uma linha dizendo que o grafo pede 70 colunas.

- **`graph-edges`:** chave de `[tui]`, `"braille"` (padrão) ou `"blocos"`, lida e validada como `style` e `bar`, com um item de escolha na seção Aparência. O padrão é Braille porque as arestas ficam quatro vezes mais finas que em meio-bloco.

## Risks / Trade-offs

- **[Fonte sem Braille]** O padrão da barra evita símbolos que nem toda fonte tem, e o Braille pode faltar numa fonte. → A chave `graph-edges = "blocos"`, e uma conferência à mão no Konsole do usuário antes da release.
- **[Rótulos que se sobrepõem]** Com nomes longos e muitos segredos num domínio, dois rótulos podem cair na mesma linha. → Ângulos iguais, anéis que levam mais nós quanto maiores, o nó em foco desenhado por último, e zoom e movimento para o resto. Sem algoritmo de colisão nesta versão.
- **[Tamanho do change]** Desenho novo, mouse, edição e uma chave de configuração num só change. → Modelo, posições e janela em funções puras com testes de unidade, e os formulários reaproveitados sem mudança de comportamento.
- **[Arrastar em terminais sem o modo `?1002`]** O movimento pelo mouse não chega. → As teclas `h` `j` `k` `l` fazem o mesmo.

## Open Questions

Nenhuma. O `Canvas` compila com os recursos atuais: o recurso `crossterm` do ratatui já liga o `std`, que é o que o `ratatui-widgets` pede para o `Canvas` (confirmado em 2026-11-21).
