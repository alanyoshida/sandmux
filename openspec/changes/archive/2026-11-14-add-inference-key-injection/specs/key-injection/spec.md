# Spec Delta

## Purpose
Define a troca de chaves no tráfego: o valor falso de cada segredo, as chaves falsas que os presets dos harnesses recebem, a troca do falso pelo real só nos pedidos ao dono do segredo, a origem dos valores reais e o registro de cada uso.

## ADDED Requirements

### Requirement: Valor falso por segredo
Todo segredo do cofre MUST ter um valor falso: o que a importação pôs no lugar, quando o segredo veio dela, ou um gerado na criação, com o prefixo `sandmux-fake-` e 32 caracteres aleatórios. Um segredo sem valor falso, gravado por uma versão anterior, MUST ganhá-lo no desbloqueio.

#### Scenario: Segredo novo
- **WHEN** o usuário guarda o segredo `openrouter` com `sandmux vault add`
- **THEN** o segredo passa a ter um valor falso que começa com `sandmux-fake-`

### Requirement: Chaves falsas pelo preset
Numa sessão do opencode ou do hermes, com o cofre desbloqueado, cada provedor da tabela do cenário "Provedores conhecidos" que tem um segredo no cofre (pelo domínio dono) MUST entrar no ambiente do harness com a variável do provedor e o valor falso do segredo. No opencode, a configuração injetada MUST apontar a chave do provedor para essa variável.

#### Scenario: Provedores conhecidos
- **WHEN** o cofre tem segredos dos provedores
- **THEN** as variáveis são `OPENROUTER_API_KEY` (`openrouter.ai`), `ANTHROPIC_API_KEY` (`api.anthropic.com`), `OPENAI_API_KEY` (`api.openai.com`), `GEMINI_API_KEY` (`generativelanguage.googleapis.com`), `DEEPSEEK_API_KEY` (`api.deepseek.com`), `MISTRAL_API_KEY` (`api.mistral.ai`), `GROQ_API_KEY` (`api.groq.com`) e `XAI_API_KEY` (`api.x.ai`)

#### Scenario: opencode com o OpenRouter no cofre
- **WHEN** o cofre desbloqueado tem um segredo com o domínio `openrouter.ai` e o usuário abre uma sessão do opencode
- **THEN** o ambiente do opencode tem `OPENROUTER_API_KEY` com o valor falso, e o valor real não aparece em nenhum arquivo nem variável do sandbox

### Requirement: Troca no tráfego só para o dono
Num pedido decifrado, o gateway MUST trocar cada valor falso de um segredo pelo valor real, em qualquer cabeçalho, só quando o domínio do pedido casa com um domínio dono do segredo. O valor falso num pedido a outro destino MUST seguir falso. O corpo MUST NOT ser alterado.

#### Scenario: Pedido ao dono
- **WHEN** o agente manda `Authorization: Bearer <falso>` a `openrouter.ai`
- **THEN** o servidor recebe `Authorization: Bearer <real>`

#### Scenario: Pedido a outro destino
- **WHEN** o agente manda o valor falso da chave do OpenRouter num cabeçalho a outro domínio decifrado
- **THEN** o servidor recebe o valor falso

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
