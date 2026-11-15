## Why

Pedido do usuário (2026-11-15): a aba de chat com o modelo existe (Ctrl+b `i`, o menu e `sandmux chat`), mas não aparece no modal de nova aba (Ctrl+b `c`), que só lista os harnesses. Quem procura onde abrir uma aba não a encontra ali.

## What Changes

- **`chat` na lista do modal de nova aba,** depois dos harnesses: `opencode`, `hermes`, `claude`, `omp`, `shell` e `chat`.
- **Com `chat` escolhido, Enter abre o modal de chat** (o de Ctrl+b `i`), com o modelo e a mensagem. A aba nasce dali, como hoje. Os campos de pasta e de perfil não valem para o chat, e a dica do modal diz isso.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `terminal-ui`: o chat como escolha do modal de nova aba.

## Impact

- **TUI:** a lista de escolhas do modal de nova aba, o Enter com `chat` e a dica.
- **Testes:** escolher `chat` no modal de nova aba abre o modal de chat.
- **Documentação:** `docs/tui.md`, `docs/chat.md` e os pares em inglês.
