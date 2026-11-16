## MODIFIED Requirements

### Requirement: Catálogo de provedores de inferência
O modo `inferencia` MUST liberar os provedores do catálogo de inferência: `openrouter.ai`, `api.anthropic.com`, `platform.claude.com`, `api.openai.com`, `generativelanguage.googleapis.com`, `oauth2.googleapis.com`, `cloudcode-pa.googleapis.com`, `daily-cloudcode-pa.googleapis.com`, `api.deepseek.com`, `api.mistral.ai`, `api.groq.com` e `api.x.ai`. Com o cofre desbloqueado e algum segredo cujo domínio dono casa com um desses provedores, MUST liberar só os provedores com segredo. Cada provedor MUST poder ser desligado pela chave `catalog-off`.

#### Scenario: Provedor desligado
- **WHEN** o modo é `inferencia` e o `[global]` tem `catalog-off = ["api.openai.com"]`
- **THEN** `api.openai.com` é bloqueado, e `openrouter.ai` passa

#### Scenario: Provedores do cofre
- **WHEN** o modo é `inferencia` e o cofre desbloqueado tem só um segredo com o domínio `openrouter.ai`
- **THEN** `openrouter.ai` passa, e `api.openai.com` é bloqueado

#### Scenario: Login do agy no cofre
- **WHEN** o modo é `inferencia` e o cofre tem `agy:access` (`*.googleapis.com`) e `agy:refresh` (`oauth2.googleapis.com`)
- **THEN** `cloudcode-pa.googleapis.com` e `oauth2.googleapis.com` passam
