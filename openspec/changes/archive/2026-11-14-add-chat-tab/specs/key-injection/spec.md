## MODIFIED Requirements

### Requirement: Chaves falsas pelo preset
Numa sessão do opencode, do hermes ou de chat, com o cofre desbloqueado, cada provedor da tabela do cenário "Provedores conhecidos" que tem um segredo no cofre (pelo domínio dono) MUST entrar no ambiente do harness com a variável do provedor e o valor falso do segredo. No opencode, a configuração injetada MUST apontar a chave do provedor para essa variável.

#### Scenario: Provedores conhecidos
- **WHEN** o cofre tem segredos dos provedores
- **THEN** as variáveis são `OPENROUTER_API_KEY` (`openrouter.ai`), `ANTHROPIC_API_KEY` (`api.anthropic.com`), `OPENAI_API_KEY` (`api.openai.com`), `GEMINI_API_KEY` (`generativelanguage.googleapis.com`), `DEEPSEEK_API_KEY` (`api.deepseek.com`), `MISTRAL_API_KEY` (`api.mistral.ai`), `GROQ_API_KEY` (`api.groq.com`) e `XAI_API_KEY` (`api.x.ai`)

#### Scenario: opencode com o OpenRouter no cofre
- **WHEN** o cofre desbloqueado tem um segredo com o domínio `openrouter.ai` e o usuário abre uma sessão do opencode
- **THEN** o ambiente do opencode tem `OPENROUTER_API_KEY` com o valor falso, e o valor real não aparece em nenhum arquivo nem variável do sandbox

#### Scenario: Aba de chat com o OpenRouter no cofre
- **WHEN** o cofre desbloqueado tem um segredo com o domínio `openrouter.ai` e o usuário abre uma aba de chat
- **THEN** o ambiente do `sandmux-chat` tem `OPENROUTER_API_KEY` com o valor falso
