## Why

O próximo item de "Depois da v1", escolhido pelo usuário (2026-11-15), é a tela da aba como contexto do assistente. No modal de comando, um atalho deve incluir as últimas linhas do terminal na pergunta. Assim o modelo consegue, por exemplo, gerar o comando que corrige o erro que acabou de aparecer.

O change 22 decidiu que nada da tela vai ao modelo. Este change mantém isso como padrão e acrescenta só o envio pedido explicitamente, pergunta a pergunta.

## What Changes

- **Campo novo:** o modal de comando (Ctrl+b `g`) ganha o campo "tela", desligado ao abrir. Espaço no campo alterna entre "não enviar" e "enviar as últimas 40 linhas da aba".
- **Com o campo ligado:** vai junto com a pergunta o texto visível da tela da aba (sem o histórico rolado), sem as linhas vazias do fim e com no máximo 40 linhas. O modelo o recebe como contexto.
- **Fica igual:** sem o campo ligado, nada muda: só a pergunta, a pasta e o shell.
- **Aviso:** o modal mostra, com o campo ligado, que a tela vai ao provedor.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `shell-command-assistant`: a tela da aba enviada quando o usuário pede.

## Impact

- **`sandmux`:** `tui/mod.rs` (o campo, as teclas, a montagem do texto) e `tui/render.rs` (o campo no modal).
- **Testes:** com o provedor falso, que confere o que recebeu.
- **Documentação:** `docs/chat.md`, `docs/tui.md` e as versões em inglês.
