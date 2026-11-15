## MODIFIED Requirements

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
