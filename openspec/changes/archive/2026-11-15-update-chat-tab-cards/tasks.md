## 1. Tema e dados

- [x] 1.1 `ThemeColor::to_text`; `SANDMUX_THEME` em `launch::chat_env` com as cores resolvidas do tema ativo; leitura no `sandmux-chat` com as cores padrão da TUI como recuo
- [x] 1.2 `ms` opcional em `chat::Mensagem` (conversas antigas abrem sem ele), medido no `sandmux-chat` da hora do envio até o fim da resposta

## 2. Desenho

- [x] 2.1 Cartões das mensagens (papel no título, tempo à direita, resposta em andamento), topo, avisos, fundo e texto da página
- [x] 2.2 Cartão do campo com dicas no rodapé e o botão `⏎ Enviar` em dois tons (apagado durante a resposta, clicável); seletor de `/modelo` com o tema

## 3. Testes e documentação

- [x] 3.1 Testes: cores do tema na aba (fundo, borda do modelo, botão), tempo no título, conversa antiga sem `ms`
- [x] 3.2 Documentação: `docs/chat.md` e o par em inglês
