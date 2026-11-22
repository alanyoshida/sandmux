## Why

O gateway decifra só HTTP/1.1. Desde o change 93, toda aba com uma identidade de GCP decifra `*.googleapis.com`, e as bibliotecas do Google que usam gRPC sobre HTTP/2 (Pub/Sub, Firestore, Spanner) quebram nessa aba, mesmo quando o agente nem usa a identidade. O mesmo vale para os serviços da AWS que exigem HTTP/2 (o Transcribe streaming) e para qualquer cliente gRPC num domínio decifrado. O usuário escolheu este change em 2026-11-22, depois da etapa 19.

## What Changes

- **HTTP/2 decifrado:** o gateway passa a falar HTTP/2 nas conexões decifradas. A regra é seguir o cliente (decisão do usuário, 2026-11-22):
  - o gateway lê o ALPN do ClientHello do sandbox e oferece o mesmo ao destino;
  - só fala h2 com o sandbox quando o destino também negociou h2;
  - um cliente que oferece só HTTP/1.1 (WebSocket, `kubectl exec`) continua em HTTP/1.1 nas duas pontas;
  - um destino sem h2 recebe HTTP/1.1, e o sandbox também.
- **Em todo domínio decifrado:** provedores de inferência, APIs de cloud (com a troca do token e a reassinatura da AWS) e API servers do Kubernetes. Cada pedido de uma conexão HTTP/2 passa pela mesma troca de falsos, pela mesma resposta local e pelo mesmo registro de um pedido HTTP/1.1. Os pedidos de uma mesma conexão andam em paralelo.
- **gRPC:** os corpos em fluxo nos dois sentidos e os trailers (`grpc-status`) passam sem mudar, e as bibliotecas gRPC funcionam num domínio decifrado.
- **Desligar:** a chave `http2` da configuração (global, perfil ou projeto), ligada por padrão. `http2 = false` volta ao HTTP/1.1 nas conexões seguintes (decisão do usuário).
- **Dependências:** dois crates novos no `Cargo.lock`, o `h2` 0.4 (do hyperium, o mesmo projeto do `hyper`) e o `fnv` (decisão do usuário).

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `tls-interception`:
  - a terminação do TLS deixa de oferecer só HTTP/1.1 e passa a seguir o ALPN do cliente e do destino;
  - requisitos novos para o HTTP/2 decifrado, o gRPC e a chave `http2`.

## Impact

- `sandmux-net`:
  - o ALPN lido do ClientHello (`sniff.rs`);
  - os clientes TLS com `h2`;
  - o servidor TLS do sandbox com o ALPN escolhido;
  - o `serve_http` com HTTP/1.1 ou HTTP/2 nas duas pontas, e o envio em paralelo no HTTP/2;
  - a URI absoluta e o `:authority` ao falar h2 com o destino;
  - o `host` da reassinatura da AWS e do Kubernetes tirado do `:authority` quando o pedido não tem `Host`.
- `sandmux-spec`: o `http2` na `NetPolicy`, com `serde(default)`.
- `sandmux-policy`: a chave `http2` (configuração, camadas, `config get/set`).
- A TUI: o item na página de configurações.
- Docs: `docs/network.md` e `docs/config.md`, nos dois idiomas, e o fim do limite do gRPC em `docs/cloud.md`.
- Workspace: a feature `http2` do `hyper`.
- Testes: um servidor h2 local com TLS na aba (`curl --http2`) e testes de unidade de HTTP/2 nas duas pontas, com trailers e pedidos em paralelo.
