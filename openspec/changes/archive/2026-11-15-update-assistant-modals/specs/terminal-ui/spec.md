## ADDED Requirements

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
