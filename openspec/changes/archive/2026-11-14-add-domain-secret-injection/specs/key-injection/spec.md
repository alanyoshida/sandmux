# Spec Delta

## MODIFIED Requirements

### Requirement: Troca no tráfego só para o dono
Num pedido decifrado, o gateway MUST trocar cada valor falso de um segredo pelo valor real, em qualquer cabeçalho e dentro das credenciais de um `Authorization: Basic`, só quando o domínio do pedido casa com um domínio dono do segredo. O gateway do Docker privado de cada pasta MUST fazer a mesma troca nos pedidos do `dockerd` aos registries. O valor falso num pedido a outro destino MUST seguir falso. O corpo MUST NOT ser alterado.

#### Scenario: Pedido ao dono
- **WHEN** o agente manda `Authorization: Bearer <falso>` a `openrouter.ai`
- **THEN** o servidor recebe `Authorization: Bearer <real>`

#### Scenario: Pedido a outro destino
- **WHEN** o agente manda o valor falso da chave do OpenRouter num cabeçalho a outro domínio decifrado
- **THEN** o servidor recebe o valor falso

#### Scenario: Basic do git
- **WHEN** o git manda `Authorization: Basic` com `usuário:<falso>` em base64 ao dono do segredo
- **THEN** o servidor recebe `Basic` com `usuário:<real>`
