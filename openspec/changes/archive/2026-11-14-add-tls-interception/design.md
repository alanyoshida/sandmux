# Design

## Context

- **Gateway (changes 8 a 10):**
  - lê o primeiro pedaço de cada conexão TCP (até 300 ms) para achar o SNI;
  - decide pela política compilada, relida do arquivo da sessão a cada decisão;
  - conecta no destino e copia os bytes.
- **Especificação:** chega ao lançador por um memfd. O lançador não grava nada dela em disco.
- **Cofre (change 11):** segredos com domínios donos, na memória do daemon enquanto desbloqueado.
- **Máquina de referência:** Ubuntu, com o pacote de CAs em `/etc/ssl/certs/ca-certificates.crt`. O opencode roda sobre o Bun, que respeita `NODE_EXTRA_CA_CERTS` (a verificar à mão, pendência do roadmap).

## Goals / Non-Goals

**Goals:**

- Decifrar só o necessário.
- A chave da CA nunca no disco.
- Um ponto único no gateway onde os changes 13 e 25 trocam a chave falsa pela real.

**Non-Goals:**

- Trocar chaves (changes 13 e 25).
- HTTP/2 e WebSocket decifrados: o gateway oferece só HTTP/1.1 ao sandbox. Um `Upgrade` passa sem registro de método.
- Fixação de certificado: um programa que fixa o certificado do servidor falha no domínio decifrado. Fica registrado como limite.

## Decisions

### 1. CA efêmera por lançamento

- **Geração:** no `prepare` (cliente no `run` direto, daemon nas sessões), o sandmux gera com o `rcgen` uma CA ECDSA P-256 válida por 30 dias.
- **Na especificação:** a seção `network` ganha `tls: {ca_cert_pem, ca_key_pem}`. O memfd é o único lugar da chave fora da memória do supervisor.
- **Certificados por domínio:** o gateway os gera na hora, assinados pela CA, e guarda num cache na memória.
- **Alternativa descartada: uma CA persistente no cofre.** Um vazamento valeria para todo sandbox futuro, e o `run` sem cofre ficaria sem decifração.

### 2. Pacote de CAs no sandbox

- **Conteúdo:** a composição lê o pacote do sistema (o primeiro que existir entre `/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt` e `/etc/ssl/cert.pem`) e acrescenta o certificado da CA.
- **Montagem:** um `Mount::File` cobre o mesmo caminho no sandbox, e as variáveis apontam para ele.
- **Quando:** a confiança entra em todo lançamento com o gateway, mesmo sem decifração no momento, porque a lista de decifração muda com a sessão viva.

### 3. Lista de decifração na política

- **Na política:** `NetPolicy` ganha `decrypt: Vec<Pattern>`.
- **Na configuração:** a chave `decrypt` (e `--decrypt`) entra nas camadas como lista.
- **No daemon:** ao compilar a política de uma sessão, o daemon soma os domínios donos dos segredos do cofre desbloqueado. Desbloquear e trancar o cofre recompila as sessões vivas, como uma mudança de configuração.
- **No `run` sem terminal:** usa só a configuração, porque não há daemon nem cofre aberto.

### 4. Proxy no gateway

Numa conexão TCP cujo SNI casa com `decrypt`, e que a política deixou passar:

1. o primeiro pedaço lido volta à frente do fluxo (um adaptador que devolve os bytes já lidos antes de ler o resto);
2. `tokio-rustls` termina o TLS com o certificado do SNI e ALPN `http/1.1`;
3. o servidor HTTP/1.1 do `hyper` lê cada pedido;
4. para cada pedido, o gateway abre (ou reaproveita, por conexão) o cliente `hyper` sobre `tokio-rustls` até `destino:443`, com o SNI do pedido e as raízes do sistema (`rustls-native-certs`);
5. a resposta volta e uma linha `https` é gravada, com o método, o caminho sem a query, o status e os bytes.

- **Erro de certificado do destino:** a conexão fecha, com uma linha bloqueada (motivo `certificado inválido: <erro>`).
- **Ponto de troca:** um `fn rewrite(&mut Request)` vazio neste change, que os changes 13 e 25 preenchem.

Alternativa descartada: só reescrever bytes sem um servidor HTTP. Não acharia os limites dos pedidos com corpo `chunked` nem com `keep-alive`.

### 5. Registro

- **Campos novos:** `Entry` ganha `method`, `path` e `status`, opcionais (`#[serde(default)]`).
- **Exibição:** `NetLine` leva os três. O painel e o `sandmux net` mostram `GET /v1/models 200` no lugar do destino nas linhas `https`.

## Risks / Trade-offs

- **[Programas com fixação de certificado]** Falham nos domínios decifrados. → Só os domínios com segredo e os pedidos pelo perfil são decifrados; o registro mostra a falha.
- **[Runtimes que ignoram as variáveis]** Um programa com pacote de CAs próprio embutido não confia na CA. → O pacote do sistema é coberto, e as variáveis comuns são definidas. O opencode é verificado à mão.
- **[Desempenho]** Cada domínio decifrado custa um certificado gerado (ECDSA, rápido) e uma conexão TLS a mais. → Cache de certificados por domínio.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart`. Nada é decifrado sem segredos com domínio e sem `decrypt`.
