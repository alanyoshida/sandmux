## Why

Próximo item de "Depois da v1", escolhido pelo usuário (2026-11-15): mais harnesses. Este change traz o Claude Code; o oh-my-pi vem no change 44.

Decisão do usuário (2026-11-15): o Claude Code usa a assinatura (OAuth) com o token fora do sandbox. O token real fica no cofre e o agente só vê falsos.

O token do host não pode ser compartilhado: a Anthropic renova o token de atualização a cada uso, e o sandbox e o Claude Code de fora do sandbox se derrubariam. Por isso o login é feito uma vez dentro do sandmux.

## What Changes

- **Preset `claude`:** o Claude Code instalado no host (instalação nativa em `~/.local/bin/claude` e `~/.local/share/claude`) roda confinado, em somente leitura e com a atualização automática desligada.
- **Importação:** copia `~/.claude.json` e, de `~/.claude`, as configurações, `CLAUDE.md`, agentes, comandos, skills, hooks e plugins. Ficam de fora:
  - `.credentials.json`, que tem o token do host;
  - as conversas, o histórico e os caches.
- **Login e renovação capturados:** o gateway decifra `platform.claude.com` e `api.anthropic.com` nas abas do Claude Code. Nas respostas do endpoint de token (`/v1/oauth/token`) e da criação de chave (`/api/oauth/claude_cli/create_api_key`), cada token real (`access_token`, `refresh_token`, `raw_key`):
  - vai para o cofre, como os segredos `claude:access`, `claude:refresh` e `claude:api-key`;
  - chega ao agente trocado por um falso estável, com o mesmo prefixo.

  Nos pedidos seguintes, o gateway troca o falso pelo real: nos cabeçalhos, como nos outros segredos, e no corpo do pedido de renovação. Com o cofre trancado, a resposta é recusada, e o token real nunca chega ao sandbox.
- **Protocolo 26:** `VaultCapture { tag, items }` do supervisor ao daemon, com a resposta `Captured { fakes }`.
- **Rede:** `platform.claude.com` entra no catálogo de inferência; o modo só inferência deixa a renovação passar.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: o preset `claude` e a importação dele.
- `key-injection`: a captura de tokens de login e de renovação nas respostas.

## Impact

- **`sandmux-policy`:** `presets.rs`, `netcatalog.rs` e `compose.rs` (a decifração dos hosts da Anthropic na aba do Claude Code).
- **`sandmux-import`:** a tabela do `claude`.
- **`sandmux-proto`:** `VaultCapture` e `Captured`, protocolo 26.
- **`sandmux-net`:** a captura no proxy decifrado.
- **`sandmux`:** o daemon guarda os capturados.
- **Testes e documentação:** testes de unidade da captura, do daemon e do preset; `docs/claude.md` e o par em inglês, README.
