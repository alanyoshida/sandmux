## Why

O opencode do usuário fala com o Unsloth Studio em `http://127.0.0.1:8888/v1`, com a chave num cabeçalho `Authorization` da configuração. A importação trocou a chave por um valor falso, como deve. Com `host-ports` (change 27), o pedido chega ao Unsloth, mas leva o falso, e o servidor recusa ("Invalid token payload"). A troca do falso pelo real só existia no tráfego HTTPS decifrado, e servidores locais de modelo falam HTTP sem TLS. Para funcionar, o usuário colou a chave real no opencode do sandbox, o que contraria a regra de nenhum segredo real lá dentro. O usuário escolheu (2026-11-14) a troca da chave também nas portas do host.

## What Changes

- **Troca nas portas do host:** numa conexão por uma porta de `host-ports`, quando o cofre desbloqueado tem segredos cujo domínio é `host.sandmux.internal`, e a conexão começa com um pedido HTTP/1.x, o encaminhamento passa pelo mesmo proxy HTTP das conexões decifradas. Ele troca o falso pelo real nos cabeçalhos, no `Authorization: Basic` e no caminho, e registra uma linha `http` por pedido, com o nome do segredo. Sem segredo para o host, ou sem HTTP, continua a cópia direta de hoje.
- **Domínio na importação do cofre:** `sandmux vault import` pergunta o domínio dono quando o nome da chave não indica nenhum (como `Authorization`), para a troca saber para onde vale. Uma resposta vazia guarda o segredo sem domínio, como hoje.
- **Registro:** o proxy das conexões decifradas passa a registrar o protocolo do pedido (`https` ou `http`).

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `key-injection`: a troca vale também nos pedidos HTTP sem TLS pelas portas do host.
- `secret-vault`: a leitura do manifesto pergunta o domínio quando o nome da chave não o indica.

## Impact

- **`sandmux-net`:**
  - `mitm.rs`: `serve_http` genérico, usado pelo TLS e pelo HTTP sem TLS, e o protocolo no contexto;
  - `hostports.rs`: segredos pedidos ao daemon por conexão e o repasse pelo proxy;
  - `log.rs`: o protocolo nas linhas de pedido.
- **`sandmux`:** `vaultcmd.rs` (a pergunta do domínio).
- **Documentação:** `docs/network.md` e `docs/vault.md`.
