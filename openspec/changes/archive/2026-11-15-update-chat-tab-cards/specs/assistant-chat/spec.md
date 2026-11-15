## ADDED Requirements

### Requirement: Aba de chat com o tema
A aba de chat MUST usar as cores do tema ativo, recebidas do daemon ao lançar a aba: o fundo e o texto da página, cada mensagem num cartão com o papel no título (a do usuário com a segunda cor de destaque e a do modelo com a cor de destaque), e o campo da mensagem num cartão com o botão `⏎ Enviar` em dois tons. A mensagem do modelo MUST mostrar o tempo de resposta quando ele é conhecido. Uma conversa gravada antes, sem o tempo, MUST abrir normalmente.

#### Scenario: Cores do tema
- **WHEN** o tema ativo é `catppuccin` e o usuário abre uma aba de chat
- **THEN** o fundo da aba tem a cor `page-bg` do tema, e a borda do cartão do modelo tem a cor `accent`

#### Scenario: Tempo de resposta
- **WHEN** o modelo termina uma resposta
- **THEN** o título do cartão da resposta mostra o tempo que ela levou, em segundos
