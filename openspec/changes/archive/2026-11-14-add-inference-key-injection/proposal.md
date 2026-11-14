# Proposal

## Why

A regra do projeto é que nenhuma credencial real entra no sandbox, mas o harness ainda precisa chamar o modelo. Hoje isso só funciona com `--pass-env`, que põe a chave real lá dentro. O change 11 guardou os segredos no cofre, e o change 12 deu ao gateway um ponto onde os pedidos HTTP aos domínios donos dos segredos passam decifrados. Falta a troca: o agente recebe uma chave falsa, e o gateway a troca pela real só no pedido ao dono dela. O roadmap começa pelo OpenRouter, com API key, e deriva o modo só inferência dos provedores cadastrados.

## What Changes

- **Valor falso por segredo:** todo segredo do cofre passa a ter um valor falso (`sandmux-fake-` e 32 caracteres aleatórios). É o que a importação pôs no lugar, quando o segredo veio dela, ou um gerado no `sandmux vault add`.
- **Chaves falsas automáticas pelo preset** (decisão do usuário, 2026-11-14):
  - nas sessões do opencode e do hermes, com o cofre desbloqueado, cada provedor de inferência conhecido que tem segredo no cofre (pelo domínio dono) entra no ambiente com a variável do provedor e o valor falso: `OPENROUTER_API_KEY` para `openrouter.ai`, `ANTHROPIC_API_KEY` para `api.anthropic.com`, e assim por diante;
  - no opencode, o `OPENCODE_CONFIG_CONTENT` passa a apontar a chave do provedor para essa variável;
  - o `--pass-env` de uma variável que o preset já preencheu com o valor falso é recusado com um aviso.
- **Troca no tráfego:**
  - num pedido decifrado, o gateway troca cada valor falso de um segredo pelo real, em qualquer cabeçalho, só quando o domínio do pedido casa com um domínio dono do segredo;
  - o valor falso de um segredo num pedido a outro destino segue falso;
  - o corpo não é alterado.
- **De onde o gateway tira os valores reais:** ele pede ao daemon pelo socket privado do usuário, a cada conexão decifrada (pedido `VaultInject` com a etiqueta do lançamento). Os valores nunca vão para a especificação nem para arquivos. Com o cofre trancado, o gateway não troca nada, e o pedido sai com o valor falso.
- **Registro:** a linha `https` de um pedido com troca ganha o nome do segredo usado (`secret`), nunca o valor. O `sandmux net` e o painel mostram `chave: <nome>`.
- **Modo só inferência pelo cofre** (decisão do usuário, 2026-11-14): com o cofre desbloqueado e algum segredo de um provedor do catálogo de inferência, o modo `inferencia` libera só os provedores com segredo; sem isso, o catálogo inteiro, como no change 9.
- **Protocolo 11:** pedido `VaultInject`.

## Capabilities

### New Capabilities

- `key-injection`: valor falso por segredo, chaves falsas pelo preset, troca no tráfego só para o dono, valores reais pedidos ao daemon, registro do segredo usado.

### Modified Capabilities

- `secret-vault`: "Cadastro de segredos" passa a gerar o valor falso de cada segredo.
- `network-policy`: "Catálogo de provedores de inferência" passa a seguir os provedores com segredo no cofre.

## Impact

- **`sandmux-vault`:** valor falso gerado.
- **`sandmux-policy`:** tabela de provedores (variável e domínio); os presets do opencode e do hermes recebem as variáveis falsas; o `OPENCODE_CONFIG_CONTENT`.
- **`sandmux-net`:** troca dos cabeçalhos no `mitm::rewrite`; cliente do socket do daemon para o `VaultInject`; o campo `secret` no registro.
- **`sandmux-proto`:** `VaultInject`, `Injections` e o protocolo 11.
- **`sandmux`:** no daemon, o atendimento do `VaultInject` e o modo só inferência pelo cofre; o ambiente falso das sessões; o `sandmux net` e o painel.
- **Documentação:** `docs/vault.md`, `docs/network.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 11). Segredos criados antes ganham o valor falso no primeiro desbloqueio.
