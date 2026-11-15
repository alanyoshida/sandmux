## Why

Pedido do usuário (2026-11-15): o modal de comando (Ctrl+b `g`) não segue as cores do tema, e o botão de enviar precisa de outro desenho. Entre quatro opções em imagem, o usuário escolheu a **C · Blocos**, e a mesma aparência também no modal de chat (Ctrl+b `i`).

Hoje esses modais usam o fundo do terminal e texto sem cor. Só a borda e o título seguem o tema, e o botão `[ Inserir ]` é texto entre colchetes.

## What Changes

- **Aparência dos modais de comando e de chat,** com as cores do tema ativo:
  - fundo `page-bg` e texto `page-fg`;
  - título num bloco `accent` (` ≡ Comando `, ` ≡ Chat `), como os blocos da barra;
  - rótulos dos campos em `muted-fg`, e o campo ativo sublinhado na cor `accent`;
  - o modelo entre `‹ ›`, como as escolhas da página de configurações.
- **Botão em dois tons:** um bloco escuro com `⏎` (`tab-index-bg`) seguido de um bloco `accent` com o rótulo (`Enviar` ou `Inserir`), alinhado à direita. O botão:
  - é alcançado pelo Tab, e com o foco o bloco do `⏎` também fica `accent`;
  - funciona com Enter, como hoje;
  - pode ser clicado: o de enviar pergunta (comando) ou envia a mensagem (chat), e o de inserir escreve o comando na aba.
- **Etapa de espera:** "esperando o modelo…" na cor `accent`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: a aparência dos modais de comando e de chat e o botão de enviar.

## Impact

- **TUI:** o desenho dos dois modais em `render.rs`, o campo do botão no Tab e o clique do botão em todas as etapas, em `mod.rs`.
- **Testes:** os testes do modal de comando procuram o botão novo; um teste novo para o clique em Enviar e para as cores do tema.
- **Documentação:** `docs/chat.md` e o par em inglês.
