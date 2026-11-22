Os testes rodam no host, sob o vigia de memória. O destino é um servidor h2 local com a CA de teste (`SANDMUX_UPSTREAM_CA`), no `host.sandmux.internal`.

## 1. Dependência e política

- [x] 1.1 A feature `http2` do `hyper` no workspace (entram o `h2` e o `fnv`); conferir o `Cargo.lock` com só esses dois crates novos
- [x] 1.2 A chave `http2` da configuração (`BOOL_KEYS`, a camada, o `Effective` com padrão `true`, o `config get/set`) e o `http2` da `NetPolicy` com `serde(default = true)`. Teste de unidade: a política sem o campo lê com `http2 = true`, e o `http2 = false` do projeto vence o global

## 2. O gateway

- [x] 2.1 O `tls_alpn` do ClientHello no `sniff.rs`. Teste de unidade com um ClientHello do `rustls` com `h2` e `http/1.1`, e com um sem ALPN
- [x] 2.2 As variantes com e sem `h2` do `client_config`, do `kube_client_config` e do `Ca::server_config` (no cache por domínio e protocolo)
- [x] 2.3 A escolha do protocolo no `tcp()` e no `kube_tcp()`:
  - a oferta ao destino pelo ALPN do cliente e pelo `http2` da política;
  - a oferta ao sandbox pelo ALPN que o destino negociou;
  - o protocolo de cada ponta passado ao `serve`
- [x] 2.4 O `serve_http` com HTTP/1.1 ou h2 em cada ponta:
  - o servidor h2 com o executor do tokio;
  - o `SendRequest` h2 clonado a cada pedido;
  - a URI absoluta e a autoridade (o `Host` ou o `:authority`) para um destino h2;
  - o `host` da reassinatura da AWS a partir da URI;
  - a autoridade da URI trocada pelo `kube_prepare`.

  Testes de unidade com `duplex`:
  - h2 nas duas pontas, com a troca do falso, os trailers e dois pedidos em paralelo;
  - HTTP/1.1 no sandbox e h2 no destino;
  - o `host` da AWS sem cabeçalho `Host`

## 3. TUI

- [x] 3.1 O item `http2` na página de configurações (liga e desliga, padrão `true`), com o texto nos dois idiomas. Teste em `tests/tui.rs` ou na lista de itens

## 4. Testes de integração

- [x] 4.1 `tests/http2.rs`: um servidor h2 local com TLS, que ecoa o protocolo, o `Authorization` e um trailer. Na aba:
  - `curl --http2` mostra `HTTP/2 200`, o real da chave do cofre e o nome do segredo no registro;
  - `curl --http1.1` mostra HTTP/1.1;
  - com `http2 = false` no projeto, o `--http2` mostra HTTP/1.1

## 5. Documentação e fechamento

- [x] 5.1 `docs/network.md` e `docs/en/network.md`: o protocolo das conexões decifradas, o gRPC e a chave. Também:
  - `docs/config.md` e `docs/en/config.md`: a linha da chave `http2`;
  - `docs/cloud.md` e `docs/en/cloud.md`: o fim do limite do gRPC;
  - o teste de docs atualizado
- [x] 5.2 Rodar o `./scripts/check.sh` inteiro, sob o vigia, e conferir que termina verde e sem processo de teste vivo
- [x] 5.3 Atualizar o roadmap: a situação do change, as decisões na seção 2, o limite do gRPC tirado da seção 6, e o rastreio na seção 7

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova.
- Verificação à mão: uma biblioteca gRPC de verdade (Pub/Sub ou Firestore) numa aba com uma identidade de GCP, numa máquina com o `gcloud`.
