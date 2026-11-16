## MODIFIED Requirements

### Requirement: Captura de tokens de login
Cada endpoint de login tem um harness dono, e o gateway MUST capturá-lo só nas abas desse harness. Na resposta de sucesso, cada token MUST ir para o cofre como `<harness>:<papel>` e ser trocado, na resposta ao sandbox, por um falso estável com o prefixo do real. No corpo do pedido, os falsos MUST virar os reais. Sem cofre que aceite, a resposta MUST ser recusada ao sandbox, e nenhum token real MUST chegar a ele.

#### Scenario: Renovação
- **WHEN** o agente pede a renovação com o falso do `refresh_token`
- **THEN** o destino recebe o real, o cofre passa a ter os tokens novos, e o agente recebe os mesmos falsos de antes

#### Scenario: Cofre trancado
- **WHEN** a resposta de login chega com o cofre trancado
- **THEN** o sandbox recebe um erro, e nenhum token real

#### Scenario: Endpoint de outro harness
- **WHEN** um shell confinado com `oauth2.googleapis.com` na lista de decifração renova um token do `gcloud`
- **THEN** a resposta chega como veio, e o cofre não muda

## ADDED Requirements

### Requirement: Login do Claude Code capturado
Nas abas do Claude Code, o gateway MUST decifrar `platform.claude.com` e `api.anthropic.com` e capturar `POST /v1/oauth/token` (em `platform.claude.com`) e `POST /api/oauth/claude_cli/create_api_key` (em `api.anthropic.com`): `access_token`, `refresh_token` e `raw_key` viram `claude:access`, `claude:refresh` e `claude:api-key`.

#### Scenario: Login pela assinatura
- **WHEN** o Claude Code troca o código do login em `platform.claude.com`
- **THEN** o cofre tem `claude:access` e `claude:refresh`, e o Claude Code recebe falsos com o prefixo `sk-ant-`

### Requirement: Login do Antigravity CLI capturado
Nas abas do `agy`, o gateway MUST decifrar `*.googleapis.com` e capturar `POST /token` em `oauth2.googleapis.com`: `access_token` vira `agy:access`, com o domínio `*.googleapis.com`, e `refresh_token` vira `agy:refresh`, com o domínio `oauth2.googleapis.com`. Os falsos MUST manter os prefixos `ya29.` e `1//`. No corpo em formulário, o falso codificado (`1%2F%2F…`) MUST virar o real codificado.

#### Scenario: Login colando o código
- **WHEN** o `agy` confinado troca o código do login do Google em `oauth2.googleapis.com/token`
- **THEN** o cofre tem `agy:access` e `agy:refresh`, e o `agy` recebe um access `ya29.sandmux…` e um refresh `1//sandmux…`

#### Scenario: Renovação em formulário
- **WHEN** o `agy` manda `grant_type=refresh_token&refresh_token=1%2F%2Fsandmux…`
- **THEN** o Google recebe o refresh real, codificado, e o `agy` recebe de volta o mesmo access falso

#### Scenario: Pedido ao modelo
- **WHEN** o `agy` manda `Authorization: Bearer ya29.sandmux…` a `cloudcode-pa.googleapis.com`
- **THEN** o servidor recebe o access real

### Requirement: Segredos de login só nas abas do harness dono
Os segredos capturados de um login (`claude:*`, `agy:*`) MUST ser decifrados e trocados só nas abas do harness dono. Nas outras abas, os domínios deles MUST NOT entrar na lista de decifração por causa desses segredos, e o falso MUST seguir falso.

#### Scenario: Shell com o login do agy no cofre
- **WHEN** o cofre tem `agy:access` e um shell confinado acessa `storage.googleapis.com`
- **THEN** a conexão não é decifrada pelo sandmux, e o certificado é o do Google
