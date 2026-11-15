## Context

- **Proxy decifrado:** o `mitm::serve` aceita o TLS do sandbox com o certificado do domínio e serve HTTP/1.1 com `hyper`. Cada pedido vai pelo TLS já aberto até o destino, depois de `rewrite`, que troca os falsos dos segredos do domínio. Ele registra uma linha por pedido.
- **Portas do host** (change 27): o `hostports::serve` copia bytes entre o sandbox e o `127.0.0.1:porta` do host.
- **Segredos:** chegam por conexão, com `inject::fetch` (pedido ao daemon) e `inject::for_host`.

## Goals / Non-Goals

**Goals:**
- A chave de um servidor local de modelo fica no cofre e é trocada nas portas do host.
- O usuário consegue ligar a chave importada ao `host.sandmux.internal` sem saber o valor falso.

**Non-Goals:**
- Decifrar TLS nas portas do host: lá o tráfego vai ao `127.0.0.1` da máquina, e servidor local com TLS segue sem troca.
- HTTP/2 sem TLS (h2c).

## Decisions

### `serve_http` genérico

O `mitm::serve` passa a ser:
1. o aceite do TLS;
2. `serve_http(guest, upstream, ctx)`, genérico no fluxo do sandbox e no do destino.

O `Ctx` ganha `proto` (`"https"` ou `"http"`), e o `Log::write_http` recebe o protocolo. Assim o proxy, a troca, o WebSocket e o registro são os mesmos nos dois casos.

### Quando o encaminhamento usa o proxy

Para cada conexão aceita numa porta do host:
1. pede os segredos ao daemon, numa thread de bloqueio, e fica com os que casam com `host.sandmux.internal`;
2. sem nenhum, segue a cópia direta;
3. com algum, espia os primeiros bytes da conexão, sem consumir. Se começam com um método HTTP (`GET `, `POST `, `PUT `, `PATCH `, `DELETE `, `HEAD `, `OPTIONS `), conecta no host e chama `serve_http`; senão, cópia direta.

A linha `tcp` de resumo da conexão continua.

O `hostports::run` recebe o acesso ao cofre e a etiqueta do lançamento, como o gateway.

### A pergunta do domínio no `vault import`

Depois de um valor não vazio, se `known_domains(chave)` vem vazio, o comando pergunta:

> Domínio dono (vazio: sem troca no tráfego; um servidor local é `host.sandmux.internal`):

Com a entrada padrão fora de um terminal, a resposta é a linha seguinte, como o valor.

## Risks / Trade-offs

- **[Um pedido HTTP que não é ao servidor esperado]** A troca vale para qualquer porta do host, porque o dono é `host.sandmux.internal`. Um agente que liga para outra porta liberada do host com o falso leva o real até ela. As portas são escolhidas pelo usuário, e cada pedido vai para o registro com o segredo usado.
- **[Espiar os primeiros bytes]** Um cliente que espera o servidor falar primeiro (SMTP, MySQL) não manda nada. A espiada tem um tempo de 300 ms; sem bytes, vale a cópia direta.
