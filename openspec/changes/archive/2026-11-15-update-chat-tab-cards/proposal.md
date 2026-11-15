## Why

Pedido do usuário (2026-11-15): melhorar o visual da aba de chat com o modelo e seguir as cores do tema. Entre quatro opções em imagem, o usuário escolheu a **C · Cartões**.

Hoje a aba de chat (o `sandmux-chat`, que roda no sandbox) usa cores fixas: "você" em ciano, "modelo" em verde, a barra do topo em vídeo inverso e o botão `[ Enviar ]` como texto. Ela não lê o tema, porque a configuração não entra no sandbox.

## What Changes

- **Cores do tema na aba de chat:** o daemon passa ao `sandmux-chat`, por variável de ambiente, as cores resolvidas do tema ativo (`SANDMUX_THEME`), como já faz com os provedores. Sem a variável, valem as cores padrão da TUI.
- **Desenho em cartões:**
  - o fundo e o texto da página;
  - o topo com ` chat › <modelo>` (o "chat" no destaque);
  - cada mensagem num cartão de borda arredondada, com o papel no título: as do usuário com a borda da segunda cor de destaque (`chip-harness-bg`), e as do modelo com a borda de destaque (`accent`) e o tempo de resposta no canto direito (`1,2 s`);
  - a resposta em andamento num cartão do modelo com `▌`;
  - os avisos na cor de erro.
- **Campo da mensagem num cartão** com a borda secundária, o título ` mensagem ` e as dicas curtas no rodapé (`Ctrl+j quebra · /modelo troca · Ctrl+b d sai`). O botão `⏎ Enviar` fica em dois tons, como nos modais, dentro do cartão e à direita; durante a resposta, ele fica apagado.
- **Seletor de `/modelo`** com as cores do tema.
- **Tempo de resposta guardado** na mensagem do modelo (`ms`, opcional na `conversa.json`). As conversas antigas não têm o tempo e mostram o cartão sem ele.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `assistant-chat`: a aparência da aba de chat com o tema e o tempo de resposta.

## Impact

- **`sandmux-policy`:** `ThemeColor::to_text`.
- **`sandmux`:**
  - `launch::chat_env` com `SANDMUX_THEME`;
  - `chat::Mensagem` com `ms`;
  - o desenho do `sandmux-chat` (cartões, campo, botão e seletor).
- **Testes:** cores do tema na aba de chat (fundo, borda do cartão do modelo e botão); o tempo de resposta no título do cartão; a conversa antiga sem `ms` continua abrindo.
- **Documentação:** `docs/chat.md` e o par em inglês.
