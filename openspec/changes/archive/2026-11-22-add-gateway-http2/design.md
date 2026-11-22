## Context

No gateway, cada conexão TLS decifrada segue o mesmo caminho:

1. O `tcp()` lê o primeiro pedaço do cliente: o ClientHello, com o SNI.
2. Abre o TLS até o destino (`connect_upstream`), com o `client_config` de ALPN `http/1.1`.
3. Termina o TLS do sandbox com o `server_config` do domínio, também só `http/1.1`.
4. Entrega as duas pontas ao `serve_http`. Nele, um servidor HTTP/1.1 do `hyper` recebe cada pedido do sandbox e o repassa por um único `SendRequest` HTTP/1.1, atrás de um `Mutex`.

Em cada pedido, nessa ordem:
- a troca de falsos (`rewrite`);
- as respostas locais (o token do GCP, o `Status` do Kubernetes, o `403` da AWS);
- a reassinatura da AWS;
- a captura dos logins;
- o registro.

O caminho do Kubernetes (`kube_tcp`) monta o próprio cliente TLS (`kube_client_config`) e usa o mesmo `serve`. As portas do host (`hostports.rs`) usam o `serve_http` com HTTP puro.

O `hyper` 1 já está no workspace, só com a feature `http1`.

## Goals / Non-Goals

**Goals:**

- HTTP/2 nas duas pontas quando o cliente e o destino falam h2, em todo domínio decifrado.
- gRPC funcionando: os corpos em fluxo nos dois sentidos, os trailers, os pedidos em paralelo na mesma conexão.
- Nenhuma diferença, para a política e o cofre, entre um pedido HTTP/1.1 e um HTTP/2.
- Uma chave para desligar.

**Non-Goals:**

- **WebSocket sobre HTTP/2** (o CONNECT estendido, RFC 8441): os clientes de WebSocket oferecem só `http/1.1` e continuam nele.
- **HTTP/3 e QUIC:** o QUIC continua bloqueado, e os clientes caem para o TCP.
- **h2c** (HTTP/2 sem TLS): as portas do host continuam em HTTP/1.1.

## Decisions

### 1. O ALPN segue o cliente

O `sniff.rs` ganha o `tls_alpn(first)`, que lê a extensão 16 do ClientHello. Ela é a lista que o cliente oferece. O gateway decide assim:

- **Oferta ao destino:** `[h2, http/1.1]` quando o cliente oferece `h2` e o `http2` da política está ligado. Nos outros casos, `[http/1.1]`.
- **Oferta ao sandbox:** depois do handshake com o destino, o gateway vê o que ele negociou (`alpn_protocol()`) e oferece `[h2, http/1.1]` só se for `h2`. Nos outros casos, `[http/1.1]`.
- **Protocolo de cada ponta:** depois do handshake com o sandbox, o protocolo dele é o negociado.

Assim, um cliente nunca fala h2 com o gateway enquanto o gateway fala HTTP/1.1 com o destino. O contrário pode acontecer, se o cliente preferir `http/1.1` mesmo oferecendo `h2`, e é tratado na decisão 3.

*Alternativa:* oferecer h2 sempre e traduzir de HTTP/2 para HTTP/1.1. Fica descartada, porque um destino sem h2 receberia gRPC e não saberia o que fazer.

### 2. As configurações do TLS

O `client_config(extra, h2)` passa a ter duas variantes em cache: com e sem o `h2` no ALPN. O `kube_client_config` ganha o mesmo parâmetro. O `Ca::server_config(host, h2)` guarda as duas variantes de cada domínio no cache, com a chave `(domínio, h2)`.

### 3. O `serve_http` com os dois protocolos

O `serve_http(guest, upstream, ctx, proto)` recebe o protocolo de cada ponta.

**Lado do sandbox:**
- em h2, o `hyper::server::conn::http2::Builder` com o executor do tokio, de `hyper-util`;
- em HTTP/1.1, o `http1::Builder` com os upgrades, como hoje.

**Lado do destino:**
- em h2, um `http2::SendRequest`, clonado a cada pedido, o que dá o paralelismo;
- em HTTP/1.1, o `SendRequest` atrás do `Mutex`, como hoje.

Um `enum Upstream { H1(...), H2(...) }` esconde a diferença do resto do código.

**Para um destino h2:** o pedido vai com a URI absoluta `https://<autoridade><caminho e query>`, que vira o `:authority`. A autoridade é o `Host` do pedido, quando há, e a do `:authority` do sandbox, nos outros casos. O `Host` vence porque o caminho do Kubernetes o troca pelo servidor real. O cabeçalho `Host` sai, e o `hyper` tira os cabeçalhos de conexão (`connection`, `transfer-encoding`, `upgrade`) que o HTTP/2 não aceita.

**Para um destino HTTP/1.1:** o pedido vai com a URI em forma de origem, como hoje.

### 4. O `host` de quem assina

Um pedido HTTP/2 do sandbox chega sem o cabeçalho `Host`: a autoridade vem no `:authority`.

- **AWS:** o SDK assinou `host` com o valor do `:authority`. A lista de cabeçalhos da reassinatura acrescenta `host` a partir da URI quando o pedido não o tem.
- **Kubernetes:** o `kube_prepare` também troca a autoridade da URI, quando ela é absoluta. O `:authority` do destino, então, é o servidor real.

### 5. Corpos em fluxo e trailers

O `Counted` já repassa cada `Frame` do corpo, inclusive o de trailers, nos dois sentidos, sem juntar o corpo. Com o HTTP/2 nas duas pontas, o `hyper` manda os trailers da resposta (`grpc-status`) ao sandbox. O pedido gRPC chega com `te: trailers`, o único valor que o HTTP/2 aceita, e segue assim.

Só os caminhos que já juntavam o corpo continuam juntando:
- a captura de login;
- o pedido de token do GCP;
- a reassinatura sem hash declarado;
- as credenciais da AWS.

Esses corpos são pequenos e não são gRPC.

### 6. A chave `http2`

A chave fica em `BOOL_KEYS`, com padrão `true`, e entra na camada, no `Effective` e no `config get/set`. A `NetPolicy` ganha `http2: bool` com `serde(default = true)`. O gateway lê a política a cada conexão nova (`st.policy.get()`), então uma mudança vale nas conexões seguintes, também nas abas abertas, pela recompilação da política.

A TUI mostra o item na página de configurações, como o `portal`.

### 7. Testes

**Unidade, no `mitm.rs`, com `duplex`:**
- h2 nas duas pontas: a troca de falsos, os trailers e dois pedidos em paralelo, o primeiro preso até o segundo terminar;
- HTTP/1.1 no sandbox e h2 no destino: a URI absoluta e a autoridade do `Host`.

**Unidade, no `sniff.rs`:** o ALPN de um ClientHello do `rustls`.

**Integração, em `tests/http2.rs`:** um servidor h2 local com TLS, no `host.sandmux.internal`, ecoa o protocolo, o `Authorization` e um trailer. Na aba:
- `curl --http2` mostra `HTTP/2 200` e o real da chave;
- `curl --http1.1` mostra HTTP/1.1;
- com `http2 = false` no projeto, o `--http2` cai para HTTP/1.1.

## Risks / Trade-offs

- **[Todo o tráfego decifrado muda de protocolo]** → Um defeito no caminho do HTTP/2 afeta os provedores de inferência. Mitigação: a chave `http2 = false` e o protocolo que segue o cliente e o destino. Os testes cobrem a troca de falsos e o registro em h2.
- **[Um cliente que prefere `http/1.1` com um destino h2]** → O gateway traduz de HTTP/1.1 para HTTP/2. Um upgrade nesse caso falha com `502`, e o registro mostra. Os clientes de WebSocket não oferecem `h2`.
- **[Dois crates novos]** → O `h2` e o `fnv`, aceitos pelo usuário. O `h2` é do mesmo projeto do `hyper`.
- **[As respostas locais em h2]** → O `Status` do Kubernetes, o `403` da AWS e o token do GCP saem como respostas HTTP/2 comuns. Os clientes gRPC esperam o `content-type: application/grpc` e o `grpc-status`. Uma resposta local a um pedido gRPC chega como erro HTTP, e a biblioteca a mostra como `UNAVAILABLE` ou `UNKNOWN`, com o status HTTP. Fica assim neste change.

## Migration Plan

Nada a migrar. A `NetPolicy` do protocolo anterior não tem o `http2`, e o `serde(default)` o liga. O gateway roda no `sandmux-run`, então as abas novas pegam o HTTP/2 com o binário novo, sem `daemon restart`. O protocolo do daemon não muda.

## Open Questions

Nenhuma. As três decisões (os crates, o alcance e a chave) foram tomadas pelo usuário em 2026-11-22.
