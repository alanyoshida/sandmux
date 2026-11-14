## Why

Lembrar a sintaxe de um comando (`find` com data, `tar` que exclui uma pasta, `jq` sobre um JSON) é uma das perguntas mais comuns a um modelo, e a resposta só serve no terminal. O change 21 ligou o sandmux aos modelos do cofre; este change usa essa ligação para escrever o comando direto na linha da aba, sem executar, como decidido na etapa 9.

## What Changes

- **Modal de comando na TUI:** prefixo seguido de `g` abre um modal com a pergunta e o modelo. O modelo vem de `[assistant] command-model` (ou, sem ele, de `chat-model`) e pode ser trocado no modal, como no chat.
- **Pergunta ao modelo:** Enter envia. Vão ao modelo só a pergunta, a pasta de trabalho da aba e o shell em uso, com um texto de sistema que pede um único comando, sem explicação. A pergunta passa pelo daemon (`ask`, do change 21), numa execução confinada só com inferência, sem travar a TUI enquanto espera.
- **O comando sugerido:** aparece no modal num campo editável. Enter ou o botão `[ Inserir ]` escreve o comando na linha do terminal da aba que está sob o modal, sem Enter: o usuário lê, edita e executa. Tab volta à pergunta para refazê-la.
- **Comando de várias linhas:** vai como colagem entre marcadores quando a aba ligou o modo de colagem (bash e zsh ligam); sem ele, o modal recusa, porque uma quebra de linha executaria o comando.
- **Limpeza da resposta:** cercas de código e o prompt `$ ` no começo são tirados.
- **Configuração:** `command-model` em `[assistant]` e a ação `command` em `[tui.keys]`.
- **Protocolo 21:** o `ask` ganha o texto de sistema.

## Capabilities

### New Capabilities
- `shell-command-assistant`: o modal de comando, o que vai ao modelo, a limpeza da resposta e a escrita do comando na aba sem executar.

### Modified Capabilities
- `config-profiles`: `[assistant]` passa a aceitar `command-model`.

## Impact

- **`sandmux-policy`:** `config.rs` (`command-model` e a ação `command`); `assistant.rs` (o texto de sistema e a limpeza da resposta).
- **`sandmux`:**
  - `chat.rs`: o texto de sistema na conversa;
  - `tui/`: o modal de comando, a pergunta numa thread e a escrita na aba;
  - `daemon/`: o `ask` com o texto de sistema.
- **`sandmux-proto`:** protocolo 21.
- **Documentação:** `docs/chat.md` (seção do modal de comando), `docs/tui.md`, `docs/config.md`, `docs/sessions.md` e o README.
