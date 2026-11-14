# Proposal

## Why

O hermes roda servidores dentro do sandbox: a API compatível com a da OpenAI e os webhooks dos canais. O change 10 publica toda porta em escuta só no `127.0.0.1` do host, sem controle de quais e sem registro. O roadmap pede portas de entrada com allowlist e registro.

O usuário decidiu (2026-11-14):

- uma porta da allowlist pode escutar num endereço escolhido (como `0.0.0.0` ou o IP da rede local), além do `127.0.0.1`, para receber webhooks direto;
- com a allowlist preenchida, só as portas dela são publicadas;
- com ela vazia, vale o comportamento de hoje.

## What Changes

- **Allowlist de portas:** a chave `ports` (por camada) e `--port` no `run`, com itens `8642` (só no `127.0.0.1`) ou `8443@0.0.0.0` (também no endereço dado).
  - **Allowlist vazia:** toda porta segue publicada no `127.0.0.1`, como no change 10.
  - **Preenchida:** só as portas dela são publicadas; as outras ficam dentro do sandbox.
- **Endereço por porta:** uma porta com endereço escuta também nele, no mesmo número. Se o número estiver ocupado nesse endereço, ela não é publicada ali, e o registro mostra o motivo. O lançamento avisa quando um endereço diferente do `127.0.0.1` expõe uma porta à rede.
- **Ao vivo:** a allowlist vai na política de rede, que o gateway relê. Uma mudança na configuração ou na aba vale em até 2 segundos, sem relançar o agente.
- **Registro das conexões de entrada:** com a allowlist preenchida, cada conexão aceita numa porta publicada vira uma linha `inbound` no registro de rede, com o endereço de quem conectou, a porta, os bytes e a duração.
- **Lista:** o `sandmux stats`, o painel e o arquivo de portas mostram o endereço de cada publicação.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `port-publishing`: a allowlist, o endereço por porta e o registro das conexões de entrada.
- `config-profiles`: chave `ports`.

## Impact

- **`sandmux-spec`:** `PortRule` e as regras de porta na política de rede.
- **`sandmux-policy`:** a chave `ports`, compilada na política de rede.
- **`sandmux-net`:** a publicação segue a allowlist da política relida e o endereço de cada regra, e registra as conexões de entrada.
- **`sandmux-proto` e `sandmux`:** o endereço na lista de portas (`PortInfo`), `--port`, o `stats` e o painel.
- **Documentação:** `docs/network.md`, `docs/config.md` e o README.
