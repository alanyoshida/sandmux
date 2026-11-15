## ADDED Requirements

### Requirement: Captura de tokens de login
O gateway MUST decifrar `platform.claude.com` e `api.anthropic.com` nas abas do Claude Code. Nas respostas de sucesso de `POST /v1/oauth/token` (em `platform.claude.com`) e de `POST /api/oauth/claude_cli/create_api_key` (em `api.anthropic.com`), cada `access_token`, `refresh_token` e `raw_key` MUST ser guardado no cofre (`claude:access`, `claude:refresh`, `claude:api-key`) e trocado, na resposta ao sandbox, por um falso estável com o mesmo prefixo. No corpo do pedido a esses endpoints, os falsos MUST ser trocados pelos reais. Se o cofre não aceitar (trancado ou sem cofre), a resposta MUST ser recusada ao sandbox, e o token real nunca MUST chegar a ele.

#### Scenario: Renovação
- **WHEN** o agente pede a renovação com o falso do `refresh_token`
- **THEN** o destino recebe o real, o cofre passa a ter os tokens novos, e o agente recebe os mesmos falsos de antes

#### Scenario: Cofre trancado
- **WHEN** a resposta de login chega com o cofre trancado
- **THEN** o sandbox recebe um erro, e nenhum token real
