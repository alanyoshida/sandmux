## Context

Os changes 93 e 94 deixaram prontas as peças das identidades de cloud: o tipo `cloud` no cofre, o cache do real no daemon (a margem de 5 minutos, a espera depois de uma falha, o aviso único, o real sem validade relido a cada 10 minutos), a injeção pedida por host, a renovação por pedido no gateway (`Ctx::fresh`), a pasta de configuração com escrita de cada lançamento e a resposta local do gateway.

A AWS difere num ponto central. O segredo não viaja no pedido: o SDK assina cada pedido com a chave secreta (SigV4), e o pedido leva só o ID da chave e a assinatura. Trocar um texto pelo outro não basta. O gateway precisa refazer a assinatura com a chave real, sobre o mesmo pedido canônico que o SDK assinou.

O host de desenvolvimento não tem o `aws` nem o `boto3`.

## Goals / Non-Goals

**Goals:**

- O `aws` e os SDKs (boto3, Go, JS, Rust, Java) funcionam na aba com os perfis guardados, sem chave real no sandbox.
- A reassinatura cobre o `Authorization`, a URL pré-assinada e o `aws-chunked` assinado, com e sem o trailer.
- As credenciais emitidas dentro da aba (STS, SSO, Cognito) viram falsos.

**Non-Goals:**

- **SigV4a** (`AWS4-ECDSA-P256-SHA256`, dos multi-region access points): o pedido passa sem mudar e falha na AWS.
- **Event streams assinados no pedido** (Transcribe streaming, por HTTP/2): o gateway só fala HTTP/1.1. As respostas em stream (Bedrock `InvokeModelWithResponseStream`) funcionam, porque o pedido é comum.
- **A China** (`*.amazonaws.com.cn`) e os endpoints `*.api.aws`.
- **Filtrar pedidos pelo IAM:** o limite é o IAM do perfil (decisão do usuário).

## Decisions

### 1. O assinador SigV4 em Rust (`sandmux-net/src/sigv4.rs`)

O assinador usa o `hmac` 0.12 e o `sha2` 0.10, que já estão no workspace.

O pedido canônico é refeito a partir do que chegou:

- **Método.**
- **Caminho:** no S3 (o serviço do escopo é `s3`), vai como veio. Nos outros serviços, cada segmento já codificado é codificado de novo, como os SDKs fazem.
- **Query:** cada par é decodificado e codificado de novo pela regra da AWS (só `A-Za-z0-9-_.~` sem escape, espaço como `%20`). Os pares são ordenados pelo nome e pelo valor.
- **Cabeçalhos:** só os de `SignedHeaders`, em minúsculas, com os valores sem os espaços das pontas e com os espaços seguidos reduzidos a um. Vários valores do mesmo cabeçalho são juntados com `,`.
- **Hash do corpo:** o `x-amz-content-sha256` quando o pedido o traz (o hash, `UNSIGNED-PAYLOAD` ou `STREAMING-*`). Sem ele, o gateway calcula o SHA-256 do corpo lido, até 32 MiB. Um corpo maior recebe `413`.

A string a assinar usa o `x-amz-date` e o escopo (data, região, serviço) do `Credential` recebido. O gateway troca só a chave e não muda a data. A chave derivada é a cadeia HMAC `AWS4<segredo>` → data → região → serviço → `aws4_request`.

Os testes de unidade usam os exemplos publicados pela AWS: o `get-vanilla` da suíte do SigV4, o `GET Object` do S3 com `Range` e o upload em pedaços de 64 KiB de `a`, com a assinatura semente e a do primeiro pedaço.

### 2. Qual identidade assina

O ID falso é o `fake` da entrada do cofre: 20 caracteres, `AKIASANDMUX` mais 9 de `[A-Z2-7]`. A chave secreta falsa tem 40 caracteres e fica no `CloudInfo` (`secret_fake`). O gateway nunca precisa dela, porque não confere a assinatura do sandbox: só a refaz.

O gateway acha a identidade pelo `Credential=<ID>/…` do `Authorization` ou pelo `X-Amz-Credential` da query. Um ID que não é falso de ninguém, como uma chave do próprio agente, passa sem mudar.

A injeção da AWS leva `aws: Option<AwsCreds>`, com o ID real, a chave real, o token de sessão real e, numa credencial capturada, o token de sessão falso. O `value` fica vazio, e a troca comum de texto não toca nada.

### 3. O token de sessão

Os pares falsos das identidades guardadas não têm token de sessão, então o SDK não manda `X-Amz-Security-Token`. Quando o real tem um token (SSO, `assume-role`), o gateway acrescenta o cabeçalho e o põe em `SignedHeaders`, na ordem. Numa URL pré-assinada, ele vai como `X-Amz-Security-Token` na query.

Quando o pedido traz um token falso (de uma credencial capturada), o gateway o troca pelo real. Quando o real não tem token e o pedido traz um, o gateway o tira do pedido e da lista.

### 4. `aws-chunked` assinado

Com `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD` (ou `-PAYLOAD-TRAILER`), o corpo vem em pedaços `<tamanho hex>;chunk-signature=<64 hex>\r\n<dados>\r\n`. O primeiro pedaço encadeia com a assinatura semente (a do cabeçalho), e cada pedaço seguinte com a do anterior.

Um leitor do corpo, em fluxo, refaz cada `chunk-signature` com a chave real e copia os dados sem guardar o corpo inteiro. Ele assina `AWS4-HMAC-SHA256-PAYLOAD`, data, escopo, a assinatura anterior, o hash vazio e o hash do pedaço. No trailer, assina `AWS4-HMAC-SHA256-TRAILER` sobre o hash dos cabeçalhos do trailer e troca o `x-amz-trailer-signature`.

O tamanho não muda, porque toda assinatura tem 64 caracteres. O `STREAMING-UNSIGNED-PAYLOAD-TRAILER` dos SDKs atuais não precisa de nada além da assinatura semente.

### 5. Credenciais emitidas dentro da aba

O gateway reconhece o pedido que emite credenciais pelo próprio pedido, e não pelo host, o que também serve aos testes com um domínio trocado:

- **STS:** um `Action=` do STS que devolve credenciais (`AssumeRole`, `AssumeRoleWithSAML`, `AssumeRoleWithWebIdentity`, `AssumeRoot`, `GetSessionToken`, `GetFederationToken`), na query ou no formulário, ou o `X-Amz-Target` equivalente;
- **SSO:** `GET /federation/credentials`;
- **Cognito:** `X-Amz-Target: AWSCognitoIdentityService.GetCredentialsForIdentity`.

Nesse pedido, o gateway tira o `Accept-Encoding` e lê a resposta, até 1 MiB. Depois procura o ID, a chave, o token e a validade:

- no XML: `AccessKeyId`, `SecretAccessKey`, `SessionToken`, `Expiration`;
- no JSON: `accessKeyId`/`AccessKeyId`, `secretAccessKey`/`SecretKey`, `sessionToken`/`SessionToken`, `expiration`/`Expiration`, em milissegundos, em segundos ou em RFC 3339.

O gateway manda o real ao daemon (`Msg::CloudCapture`). O daemon guarda, só na memória, uma identidade capturada (`<identidade de origem>+sts`) com falsos novos (`ASIASANDMUX…`, chave e token falsos) até a validade, e devolve os falsos. O gateway troca os reais pelos falsos no corpo.

A resposta é recusada com `502` sem o daemon, com o cofre trancado, com o corpo comprimido ou maior que 1 MiB. As identidades capturadas valem para o domínio da AWS de toda aba, somem na validade e quando o cofre é trancado. Elas não aparecem no `vault list`.

### 6. Os perfis do host

O sandmux lê o `config` e o `credentials` do host como INI, sem o `aws`:

- `[default]` e `[profile <nome>]` no `config`;
- `[<nome>]` no `credentials`;
- `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE` quando definidos.

A região vem do `config`. A cópia lê do perfil o `aws_access_key_id` e o `aws_secret_access_key`, e recusa um perfil com `aws_session_token`, `sso_*`, `role_arn` ou `credential_process` sem a chave.

A referência guarda o perfil, a região e as variáveis dos arquivos, e resolve com:

```
aws configure export-credentials --profile <p> --format process
```

O comando roda com essas variáveis, `AWS_PAGER=` e 60 s de limite, e devolve `AccessKeyId`, `SecretAccessKey`, `SessionToken` e `Expiration`. `SANDMUX_AWS` troca o binário (testes). A validade em RFC 3339 com fuso (`+00:00`) passa a ser lida, e sem ela o real vale 15 minutos.

O nome da identidade segue a regra do Kubernetes. A padrão é o perfil `default`, que o `[default]` da aba representa; sem ele, o de `AWS_PROFILE` no comando, ou o primeiro.

### 7. A pasta `~/.aws` da aba

O daemon grava em `runtime_dir/aws/<tag>`:

- `config`: `[default]` e `[profile <nome original>]`, com a região;
- `credentials`: os pares falsos, e o `[default]` com o da padrão.

O `compose` liga a pasta com escrita em `~/.aws`, porque o `aws` grava o cache em `~/.aws/cli/cache`. Ele também define `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE`, e `AWS_CA_BUNDLE` entra em `CA_VARS`. A pasta é apagada no fim da aba.

### 8. Sem o real

O gateway responde `403` sem ir à AWS. Quando o `Content-Type` do pedido é `application/x-amz-json-*` ou o pedido traz `X-Amz-Target`, a resposta vai em JSON, com `x-amzn-ErrorType: ExpiredTokenException` e `{"__type":"ExpiredTokenException","message":…}`. Nos outros casos, vai em XML:

```
<ErrorResponse><Error><Code>ExpiredToken</Code><Message>sandmux: …</Message></Error></ErrorResponse>
```

Assim o `aws` mostra o motivo, por exemplo "rode `aws sso login` no host".

### 9. Domínio e testes

O domínio dono é `*.amazonaws.com`. `SANDMUX_TEST_AWS_DOMAIN` o troca por `host.sandmux.internal` nos testes, como no GCP.

Nos testes de integração, um cliente SigV4 mínimo em Python, com a biblioteca padrão, assina na aba com o par falso. Um servidor HTTPS local confere a assinatura com a chave real, pelo assinador do `sandmux-net`, e responde se valeu e o que recebeu.

### 10. Protocolo 48

`Msg::CloudCapture` e a resposta `Msg::CloudCaptured`. `Injection.aws` e `CloudInfo.region`, `secret_fake` e `env` têm `serde(default)`.

## Risks / Trade-offs

- **[O IAM é o limite]** → Um perfil de administrador dá administrador ao agente, e `iam:CreateAccessKey` cria uma chave real que volta na resposta: ela não é trocada, porque não é uma credencial temporária. O aviso do comando e o `docs/cloud.md` recomendam um perfil sem `iam:CreateAccessKey` nem leitura do Secrets Manager.
- **[Todos os perfis de uma vez]** → Os perfis de produção entram junto. O aviso lista o que foi guardado, e `vault remove aws:<nome>` tira um.
- **[O pedido canônico refeito]** → Uma diferença de canonização entre o SDK e o gateway gera `SignatureDoesNotMatch`. Os exemplos da AWS, o cliente Python independente e as regras de cada serviço (o S3 sem a codificação dupla) cobrem os casos comuns. Um serviço com regra própria aparece no registro com o status `403`.
- **[O corpo lido para o hash]** → Pedidos sem `x-amz-content-sha256` têm o corpo lido, até 32 MiB. Fora do S3, os corpos são pequenos, e o S3 sempre manda o cabeçalho.
- **[As credenciais capturadas só na memória]** → Um `sandmux daemon restart` as perde. O agente refaz o `assume-role`.

## Migration Plan

O protocolo 48 pede `sandmux daemon restart` depois de atualizar, e o auxiliar root não muda. Os campos novos têm `serde(default)`, então um cofre do 94 abre sem mudança.

## Open Questions

Nenhuma. As quatro decisões (todos os perfis, a troca das credenciais emitidas, a reassinatura dos pedaços e a assinatura direta da cópia) foram tomadas pelo usuário em 2026-11-22.
