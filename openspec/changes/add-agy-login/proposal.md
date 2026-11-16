## Why

Sem chave do Gemini no cofre, o `agy` (change 63) usa o login do Google, e esse login hoje acontece dentro do sandbox: o token real (access `ya29.…` e refresh `1//…`) fica no HOME sintético, ao alcance do agente. O login do host não é importado. Pedido do usuário (2026-11-16): o login do `agy` fora do sandbox, no modelo do Claude Code (change 43), em que o agente só vê valores falsos.

O token do `agy` é um OAuth comum do Google, renovado em `POST https://oauth2.googleapis.com/token`, e o arquivo `~/.gemini/antigravity-cli/antigravity-oauth-token` guarda `token.access_token`, `token.refresh_token`, `token.expiry`, `auth_method` e `id_token_email`. Diferente da Anthropic, o Google em geral não troca o refresh na renovação, e por isso o login do host pode ser reaproveitado sem derrubar o `agy` de fora do sandbox.

## What Changes

- **Login do host importado:** a importação do `agy`, com o cofre aberto, guarda o access e o refresh do login do host no cofre (`agy:access` e `agy:refresh`) e escreve no HOME sintético o mesmo arquivo do token com valores falsos. O usuário não faz login de novo. Com o cofre trancado ou ausente, a importação segue sem o login e avisa.
- **Login feito no sandmux:** sem login no host, o login do Google feito dentro do `agy` confinado (colando o código da página) é capturado pelo gateway, como o do Claude Code: os tokens reais vão para o cofre, e o `agy` recebe falsos.
- **Captura generalizada:** a captura de tokens de login passa a ter um dono por harness. O endpoint de token do Google é capturado só nas abas do `agy`, porque outros clientes do Google (o `gcloud` num shell, por exemplo) usam o mesmo endpoint. Nas renovações, o corpo do pedido (formulário, com o falso codificado) leva o real.
- **Destinos do token:** decisão do usuário (2026-11-16): o access real vai a todo `*.googleapis.com`, e o refresh real só a `oauth2.googleapis.com`. Os dois são decifrados nas abas do `agy`.
- **Tokens de login só na aba do harness dono:** os segredos capturados de um login (`claude:*`, `agy:*`) são decifrados e trocados só nas abas do harness deles. Isso evita decifrar todo `*.googleapis.com` em todas as abas.
- **Login antigo no HOME sintético:** um login real feito dentro do sandbox antes deste change é movido para o cofre pela varredura de chaves reais do daemon, e o arquivo passa a ter os falsos.
- **Modo só inferência:** o catálogo de inferência ganha `oauth2.googleapis.com` (renovação) e os hosts do modelo do `agy` (`cloudcode-pa.googleapis.com` e `daily-cloudcode-pa.googleapis.com`, conferidos na implementação pelo registro de rede).
- **BREAKING (interno):** o protocolo entre o gateway e o daemon passa a 36, porque o pedido de captura leva o harness dono. Depois de atualizar, `sandmux daemon restart`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `key-injection`: a captura de tokens de login ganha o `agy` (endpoint de token do Google, corpo em formulário, prefixos `ya29.` e `1//`), passa a valer só nas abas do harness dono, e os segredos de login ficam restritos às abas desse harness.
- `harness-presets`: a importação do Antigravity CLI passa a levar o login do host para o cofre e a escrever o arquivo do token com falsos.
- `harness-state-import`: o login do host do `agy` na reimportação (`--overwrite` troca o login guardado) e no relatório.
- `secret-vault`: a varredura de chaves reais reconhece o arquivo do token do `agy` e o move como login (`agy:access` e `agy:refresh`).
- `network-policy`: o catálogo de inferência ganha os hosts do login e do modelo do `agy`.

## Impact

- **`sandmux-spec`:** a tabela de captura (`CAPTURE_ENDPOINTS` com o harness dono, os campos e os hosts decifrados por harness) e o harness no `VaultAccess` do gateway.
- **`sandmux-net`:** `mitm.rs` (captura só para o harness da aba; troca do falso codificado no corpo em formulário) e `inject.rs` (o harness no pedido de captura).
- **`sandmux-proto`:** `VaultCapture` com o harness, protocolo 36.
- **`sandmux` (daemon):** `vault.rs` (papéis e domínios por harness, falsos com os prefixos do Google, segredos de login filtrados pelo harness da aba) e a varredura de chaves reais.
- **`sandmux-policy`:** `config.rs` (os hosts do `agy` na lista de decifração da aba) e `netcatalog.rs`.
- **`sandmux-import`:** o login do host do `agy` levado ao cofre pelo daemon e o arquivo com falsos.
- **Testes e documentação:** testes de unidade e de integração com um servidor de token falso; `docs/agy.md`, `docs/vault.md` e os pares em inglês.
