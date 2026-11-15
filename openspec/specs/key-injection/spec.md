# key-injection Specification

## Purpose
Define a troca de chaves no tráfego: o valor falso de cada segredo, as chaves falsas que os presets dos harnesses recebem, a troca do falso pelo real só nos pedidos ao dono do segredo, a origem dos valores reais e o registro de cada uso.

## Requirements

### Requirement: Valor falso por segredo
Todo segredo do cofre MUST ter um valor falso: o que a importação pôs no lugar, quando o segredo veio dela, ou um gerado na criação, com o prefixo `sandmux-fake-` e 32 caracteres aleatórios. Um segredo sem valor falso, gravado por uma versão anterior, MUST ganhá-lo no desbloqueio.

#### Scenario: Segredo novo
- **WHEN** o usuário guarda o segredo `openrouter` com `sandmux vault add`
- **THEN** o segredo passa a ter um valor falso que começa com `sandmux-fake-`

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

### Requirement: Troca no tráfego só para o dono
Num pedido decifrado, ou num pedido HTTP sem TLS por uma porta de `host-ports` (com o host `host.sandmux.internal` como domínio), o gateway MUST trocar cada valor falso de um segredo pelo valor real, em qualquer cabeçalho, dentro das credenciais de um `Authorization: Basic`, no caminho da URL e nas mensagens de texto de um WebSocket do sandbox ao destino, só quando o domínio do pedido casa com um domínio dono do segredo. O gateway do Docker privado de cada pasta MUST fazer a mesma troca nos pedidos do `dockerd` aos registries. O valor falso num pedido a outro destino MUST seguir falso. O corpo de um pedido HTTP e as mensagens do destino ao sandbox MUST NOT ser alterados. O registro MUST guardar o caminho com o valor falso.

#### Scenario: Pedido ao dono
- **WHEN** o agente manda `Authorization: Bearer <falso>` a `openrouter.ai`
- **THEN** o servidor recebe `Authorization: Bearer <real>`

#### Scenario: Pedido a outro destino
- **WHEN** o agente manda o valor falso da chave do OpenRouter num cabeçalho a outro domínio decifrado
- **THEN** o servidor recebe o valor falso

#### Scenario: Basic do git
- **WHEN** o git manda `Authorization: Basic` com `usuário:<falso>` em base64 ao dono do segredo
- **THEN** o servidor recebe `Basic` com `usuário:<real>`

#### Scenario: Token na URL do Telegram
- **WHEN** o hermes faz `POST /bot<falso>/getUpdates` a `api.telegram.org`, dono do segredo
- **THEN** o servidor recebe `/bot<real>/getUpdates`, e o registro tem o caminho com o falso

#### Scenario: Token no WebSocket do Discord
- **WHEN** o hermes manda, pelo WebSocket de `gateway.discord.gg`, uma mensagem de texto IDENTIFY com `"token": "<falso>"`
- **THEN** o servidor recebe a mensagem com o valor real

#### Scenario: Servidor local de modelo pela porta do host
- **WHEN** o cofre tem o segredo `unsloth` com o domínio `host.sandmux.internal`, a configuração tem `host-ports = [8888]`, e o opencode manda `Authorization: Bearer <falso>` a `http://127.0.0.1:8888/v1/chat/completions`
- **THEN** o servidor do host recebe `Authorization: Bearer <real>`, e o registro tem uma linha `http` com o segredo `unsloth` e o caminho

### Requirement: Valores reais pedidos ao daemon
O gateway MUST obter os valores reais do daemon pelo socket privado do usuário, a cada conexão decifrada, e guardá-los só na memória durante a conexão. Os valores reais MUST NOT ir para a especificação nem para arquivos. Com o cofre trancado, o gateway MUST NOT trocar nada.

#### Scenario: Cofre trancado
- **WHEN** o cofre é trancado com a sessão viva e o agente faz um pedido ao dono do segredo
- **THEN** o pedido sai com o valor falso

### Requirement: Registro do uso
A linha `https` de um pedido com troca MUST ter o nome do segredo usado, nunca o valor.

#### Scenario: Linha com o segredo
- **WHEN** um pedido a `openrouter.ai` teve a chave trocada
- **THEN** a linha do registro tem o nome do segredo, e nem o valor real nem o falso aparecem no arquivo

### Requirement: Captura de tokens de login
O gateway MUST decifrar `platform.claude.com` e `api.anthropic.com` nas abas do Claude Code. Nas respostas de sucesso de `POST /v1/oauth/token` (em `platform.claude.com`) e de `POST /api/oauth/claude_cli/create_api_key` (em `api.anthropic.com`), cada `access_token`, `refresh_token` e `raw_key` MUST ser guardado no cofre (`claude:access`, `claude:refresh`, `claude:api-key`) e trocado, na resposta ao sandbox, por um falso estável com o mesmo prefixo. No corpo do pedido a esses endpoints, os falsos MUST ser trocados pelos reais. Se o cofre não aceitar (trancado ou sem cofre), a resposta MUST ser recusada ao sandbox, e o token real nunca MUST chegar a ele.

#### Scenario: Renovação
- **WHEN** o agente pede a renovação com o falso do `refresh_token`
- **THEN** o destino recebe o real, o cofre passa a ter os tokens novos, e o agente recebe os mesmos falsos de antes

#### Scenario: Cofre trancado
- **WHEN** a resposta de login chega com o cofre trancado
- **THEN** o sandbox recebe um erro, e nenhum token real
