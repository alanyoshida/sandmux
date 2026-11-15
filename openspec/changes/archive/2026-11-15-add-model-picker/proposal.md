## Why

O usuário pediu (2026-11-15) que, nos lugares onde se escolhe o modelo do assistente, dê para listar os modelos que existem, pela API `/models` dos provedores, e escolher numa lista. Hoje o modelo se escreve à mão, e os modais só percorrem a lista `models` da configuração.

## What Changes

- **Lista de modelos:** `sandmux-chat --models` pede `GET <url>/models` a cada provedor configurado, dentro do sandbox da aba de chat, e imprime `provedor/modelo`. A chave é trocada pelo gateway, como no chat.
  - **Lista parcial:** provedor sem chave, ou que recusa, fica de fora, com o motivo.
  - **Erro:** a lista só falha quando nenhum provedor responde.
- **Daemon:** a mensagem `ListModels` (protocolo 25) roda esse modo, como o `Ask`, e devolve a lista e os avisos.
- **Seletor na TUI:** uma lista com filtro por digitação (todas as palavras, sem caixa), ↑ ↓ e Enter. Sem resultado, Enter usa o texto digitado. A lista fica guardada até fechar a TUI; `Ctrl+r` relê. O seletor abre:
  - na página de configurações, no Enter de "Modelo do chat" e de "Modelo de comandos" (grava a escolha), e no `a` de "Modelos sugeridos" (acrescenta);
  - nos modais de chat (Ctrl+b `i`) e de comando (Ctrl+b `g`), com → no campo do modelo.
- **Aba de chat:** `/modelo` sem argumento abre o mesmo seletor dentro da aba, com a lista pedida pela própria aba. `/modelo <id>` continua trocando direto.
- **Idioma:** os textos nos dois idiomas.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `assistant-chat`: a lista de modelos dos provedores e o seletor.

## Impact

- **`sandmux`:**
  - `chat.rs`: `list_models`;
  - `bin/sandmux-chat.rs`: `--models` e o seletor de `/modelo`;
  - `daemon`: `ListModels`;
  - `tui`: o seletor e os pontos de uso.
- **`sandmux-proto`:** `ListModels` e `Models`, protocolo 25.
- **Testes e documentação:** testes com um servidor `/models` falso; `docs/chat.md`, `docs/tui.md` e as versões em inglês.
