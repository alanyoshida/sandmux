## Context

O change 93 deixou as peças comuns das identidades de cloud: o tipo `cloud` no cofre (`CloudInfo` e `CloudAdd`), o cache do real no daemon (`daemon/cloud.rs`, com a margem de 5 minutos, a espera de 20 s depois de uma falha e o aviso único), a injeção pedida por host a cada conexão decifrada (`VaultInject{tag, host}`), a renovação por pedido no gateway (`Ctx::fresh`) e uma pasta de configuração por lançamento, com escrita, ligada no HOME da aba (`runtime_dir/cloud/<tag>`).

O Kubernetes difere do GCP em três pontos:

- **O destino não é um domínio fixo.** Cada cluster tem o servidor dele, muitas vezes um IP, e o kind e o minikube ficam no `127.0.0.1` do host, que o sandbox não alcança.
- **O TLS do destino tem CA própria.** Às vezes ele também pede um certificado de cliente.
- **A credencial vem de vários lugares:** um token estático, um certificado de cliente, ou um plugin `exec` (EKS, GKE, kubelogin) que só roda no host.

O host de desenvolvimento tem o `kubectl` e clusters do kind.

## Goals / Non-Goals

**Goals:**

- O `kubectl` e os clientes do Kubernetes (`client-go`, `kubernetes` do Python) funcionam na aba com todos os contextos guardados, sem credencial real no sandbox.
- Token estático, certificado de cliente e plugin `exec` são aceitos, por referência ao host. Os dois primeiros são aceitos também por cópia.
- `kubectl exec`, `attach`, `cp`, `logs -f`, `port-forward` e `get -w` funcionam pelo gateway.

**Non-Goals:**

- O `auth-provider` antigo (o `gcp` e o `oidc` de antes do `exec`) e usuário e senha: recusados com o motivo.
- O HTTP/2 até o sandbox: o gateway oferece só HTTP/1.1, e o `client-go` cai para ele sozinho.
- Filtrar pedidos ao API server: o limite é o RBAC (decisão do usuário).
- O `proxy-url` do kubeconfig: o gateway conecta direto ao servidor.

## Decisions

### 1. Nome sintético com um IP fixo do gateway

Na aba, o servidor de cada contexto é `https://<nome>.kube.sandmux.internal`, sem porta (443). O DNS do gateway responde todo `*.kube.sandmux.internal` com `10.0.2.253` (`KUBE`), um IP da rede interna que, como o `10.0.2.254` da máquina, nunca sai do gateway.

Numa conexão TCP, o gateway toma o caminho do Kubernetes quando o destino é `KUBE`. Esse caminho vem antes do veredito da política, para que o nome passe em qualquer modo (decisão do usuário), e o gateway decifra sempre que há CA do lançamento. Uma conexão a `KUBE` sem TLS, ou sem um SNI de `*.kube.sandmux.internal`, é fechada, com o motivo no registro.

O destino real nunca vem do sandbox: o gateway lê o servidor da identidade na injeção, então um `tls-server-name` ou um IP trocado pelo agente não levam o real a outro lugar.

*Alternativas:* manter o servidor real com um `tls-server-name` sintético (o kind e o minikube não funcionariam), ou decifrar por IP e porta (sem SNI, frágil). As duas foram descartadas pelo usuário.

### 2. O kubeconfig do host pelo `kubectl` do host

O daemon e o comando leem o contexto com:

```
kubectl config view --raw --flatten --minify --context <c> -o json
```

O `KUBECONFIG` é o valor guardado na identidade. Assim, a junção de vários arquivos, os caminhos relativos e os `*-data` seguem as regras do próprio `kubectl`, e não entra um leitor de YAML no `Cargo.lock`. `SANDMUX_KUBECTL` troca o binário, como o `SANDMUX_GCLOUD` (nos testes, um script). Para listar os contextos, o comando usa a mesma visão sem `--minify`. O `tokenFile` do usuário não é embutido pelo `--flatten`, então o daemon o lê no host a cada renovação.

### 3. As identidades: todos os contextos, com nome de DNS

O nome é o contexto em minúsculas, com cada caractere fora de `[a-z0-9-]` trocado por `-`, sem `-` repetido nem nas pontas, e cortado em 40 caracteres, para caber num rótulo de DNS com folga. Quando dois contextos dão o mesmo nome, o segundo é pulado com o aviso; o mesmo vale para um nome que já está no cofre.

O nome original do contexto vai na identidade. `CloudInfo` ganha os campos com `serde(default)`:

- `server`, para exibir;
- `namespace`;
- `kubeconfig`, o valor de `KUBECONFIG` no comando, ou o caminho padrão.

O `configuration` guarda o nome do contexto. A padrão é a do `current-context` do host. `--default` não existe no Kubernetes: a padrão só vale para a aba.

### 4. O destino do Kubernetes na injeção

`Injection` ganha `kube: Option<KubeTarget>` com estes campos:

- `server` (`https://host:porta`);
- `ca_pem` (vazio usa as CAs do sistema);
- `server_name` (o `tls-server-name`, ou o host do servidor);
- `insecure`;
- `cert_pem` e `key_pem` (o certificado de cliente, real).

O `Debug` esconde a chave, e o `value` é o token real (vazio quando só há certificado). O protocolo passa a 47.

O `Real` do daemon ganha o mesmo `kube`, e o cache guarda o `Real` inteiro, zerando o token e a chave no `Drop`.

### 5. Validade e plugins `exec`

O plugin roda com o comando, os argumentos e o `env` do kubeconfig sobre o ambiente do daemon. Ele recebe `KUBERNETES_EXEC_INFO` (`ExecCredential`, `interactive: false`, mais o cluster quando `provideClusterInfo`), o stdin vazio e 60 s de limite. A saída é o `ExecCredential`:

- `status.token`;
- `status.clientCertificateData` e `status.clientKeyData`;
- `status.expirationTimestamp` (RFC 3339, com o `parse_rfc3339` do GCP).

Sem validade, seja de um token estático, de um certificado ou de um plugin que não dá a data, o real vale por 15 minutos. Com a margem de 5 minutos, ele é relido a cada 10 minutos, o que pega uma troca no kubeconfig do host.

Uma falha do plugin traz a última linha do stderr no motivo, com o comando: "rode `aws sso login` no host" vem do próprio plugin.

### 6. O caminho do Kubernetes no gateway

1. O DNS responde `KUBE`, e o TCP a `KUBE` lê o SNI.
2. O gateway pede as injeções do host (o SNI) ao daemon **antes** de conectar ao destino, porque o destino está nelas.
3. Sem injeção do Kubernetes, ou com `error`, o gateway termina o TLS com a CA do lançamento e responde a todo pedido com um `Status` do Kubernetes: `404` sem a identidade e `401` sem o real, com o motivo. Nada sai para fora.
4. Com o real, o gateway monta um `rustls::ClientConfig`:
   - as raízes são a CA do cluster ou as do sistema, e a `SANDMUX_UPSTREAM_CA`, como hoje;
   - o certificado de cliente entra quando há;
   - com `insecure`, o verificador aceita qualquer certificado;
   - o ALPN é só HTTP/1.1.

   Ele conecta ao `server` com `connect_upstream`, usando `127.0.0.1` do host como no `target()`, porque o gateway roda na rede do host. O `ServerName` é o `server_name`, que pode ser um IP.
5. Em cada pedido:
   - quando o `Authorization` leva o falso da identidade, vira `Bearer <real>`, ou sai quando não há token;
   - o `Host` vira a autoridade do servidor real;
   - a linha `https` do registro usa o nome da identidade e o servidor real.

   A renovação por pedido (`Ctx::fresh`) já cobre a validade, e o `kube` vem junto da injeção renovada. Uma renovação que muda o certificado de cliente só vale na próxima conexão, porque o TLS já está aberto.

### 7. Upgrades que não são WebSocket

O `serve_http` passa a reconhecer qualquer pedido com `Connection: upgrade` e `Upgrade`:

- o WebSocket segue pelo `ws.rs`, com a troca dos falsos nos quadros;
- os outros (`SPDY/3.1`) passam como túnel de bytes, depois do `101`, com a contagem para o registro.

O `kubectl` 1.30+ usa WebSocket no `exec` e no `port-forward` e cai para o SPDY quando o servidor não aceita. Os dois caminhos funcionam.

### 8. A pasta `~/.kube` da aba

O daemon grava `runtime_dir/kube/<tag>/config` em JSON, que o `kubectl` lê como YAML, com:

- `clusters`, com o servidor sintético e `certificate-authority: <HOME>/.kube/sandmux-ca.crt`;
- `users`, com o token falso;
- `contexts`, com o nome original e o namespace;
- `current-context`, o da padrão.

O `compose` liga a pasta com escrita em `~/.kube`, põe a CA do lançamento em `~/.kube/sandmux-ca.crt` (um `Mount::File`, porque a CA nasce no `prepare_with`, depois da pasta) e define `KUBECONFIG=<HOME>/.kube/config`. O `kubectl config use-context` grava na pasta, e o `kubectl` guarda nela o cache de descoberta (`~/.kube/cache`). A pasta é apagada no fim da aba, com a do `gcloud`.

### 9. Cópia

`--copy` lê o contexto pelo `kubectl` e guarda um JSON no `secret` do `CloudAdd`, cifrado no cofre, com `server`, `ca_pem`, `server_name`, `insecure`, `token`, `cert_pem` e `key_pem`. Para resolver, o daemon só decodifica esse JSON.

Um usuário com `exec` ou `auth-provider` é recusado, porque o plugin só roda no host. Um `tokenFile` é lido na hora.

## Risks / Trade-offs

- **[O RBAC é o limite]** → Um contexto de admin dá admin ao agente. A mensagem do comando e o `docs/cloud.md` mandam usar um contexto com uma ServiceAccount de permissão mínima, e lembram que o `secrets get` dá credenciais reais do cluster.
- **[Todos os contextos de uma vez]** → Um contexto de produção entra junto. O aviso do comando lista o que foi guardado, e `sandmux vault remove kube:<nome>` tira um.
- **[O `insecure-skip-tls-verify` do host vale no gateway]** → É o mesmo risco do host. O registro e o `vault list` mostram "sem verificação".
- **[O ambiente do daemon]** → O plugin `exec` precisa do `PATH` e das variáveis da nuvem (`AWS_PROFILE`, por exemplo). O daemon roda com o ambiente de quem o subiu, como no `gcloud`. Um plugin ausente gera o aviso com o motivo.
- **[Um certificado de cliente renovado só vale na conexão seguinte]** → O `client-go` reabre as conexões. Uma conexão longa (`get -w`) segue até o servidor fechá-la.

## Migration Plan

O protocolo 47 pede `sandmux daemon restart` depois de atualizar, e o auxiliar root não muda. Os campos novos de `CloudInfo` e `Injection` têm `serde(default)`, então um cofre do 93 abre sem mudança.

## Open Questions

Nenhuma. As três decisões (a rota, todos os contextos e a política) foram tomadas pelo usuário em 2026-11-22.
