## Why

Os changes 93 e 94 trouxeram as identidades de cloud do GCP e do Kubernetes: o agente recebe um falso, e o gateway põe o real fora do sandbox. Para usar o `aws` e os SDKs da AWS, o agente ainda precisa de uma chave real em `~/.aws` ou no ambiente. Pela ordem decidida em 2026-11-22 (GCP, Kubernetes, AWS), este change é o da AWS, o último da etapa 19.

Na AWS, o falso não basta trocar: cada pedido é assinado (SigV4) com a chave secreta. O gateway precisa refazer a assinatura com a chave real.

## What Changes

- **Identidades da AWS no cofre:** `sandmux vault add-cloud aws` guarda **todos os perfis** de `~/.aws/config` e `~/.aws/credentials` de uma vez, cada um como uma identidade `aws:<nome>` (decisão do usuário, 2026-11-22). `--profile <perfil>` (repetível) limita a alguns perfis.
  - **Referência ao host (o padrão):** o cofre guarda só o perfil e a região. O daemon tira o real do `aws` do host (`aws configure export-credentials`), fora do sandbox. SSO, `assume-role` e `credential_process` funcionam como no terminal do usuário.
  - **Cópia (`--copy`):** a chave de longo prazo do perfil (as duas partes) fica cifrada no cofre. O gateway assina direto com ela, sem o STS (decisão do usuário). Um perfil sem chave estática é recusado na cópia.
- **Em toda aba, como as chaves:** com o cofre aberto, o HOME da aba tem uma pasta `~/.aws` só desta execução. Nela, cada identidade vira um perfil com o nome original, a região e um par falso (o ID da chave e a chave secreta). O perfil `default` é o da identidade padrão. A aba confia na CA do lançamento (`AWS_CA_BUNDLE`).
- **Reassinatura no gateway:** com uma identidade da AWS no cofre, a aba decifra `*.amazonaws.com`. Num pedido assinado com o ID falso de uma identidade, o gateway:
  - refaz a assinatura SigV4 com a chave real, no `Authorization` ou na URL pré-assinada;
  - põe o token de sessão real (`X-Amz-Security-Token`) quando há;
  - reassina pedaço a pedaço os uploads do S3 com `aws-chunked` assinado, com o trailer (decisão do usuário).

  O real nunca chega ao sandbox.
- **Credenciais emitidas dentro da aba (decisão do usuário):** a resposta de um pedido que emite credenciais pode ser do STS (`AssumeRole*`, `GetSessionToken`, `GetFederationToken`), do `GetRoleCredentials` do SSO ou do `GetCredentialsForIdentity` do Cognito. Nela, o gateway troca o real por falsos novos, como no login do `agy`. O daemon guarda esse real só na memória, até a validade dele, e o gateway reassina com ele os pedidos que vierem com esses falsos.
- **Sem o real:** quando o host não dá o real (o `aws` falta, o SSO venceu), o gateway responde `403` com um erro da AWS que diz o que fazer no host, e o daemon avisa uma vez, como nos outros provedores.
- **Registro:** cada pedido reassinado registra o nome da identidade, nunca a chave.
- **Limite:** o que o agente pode fazer é o que o IAM do perfil permitir.
- O protocolo entre o cliente e o daemon passa a 48: a injeção leva a chave real da AWS, e há a mensagem da captura.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `cloud-credentials`: as identidades da AWS (todos os perfis de uma vez, referência ou cópia), a pasta `~/.aws` da aba, a reassinatura SigV4 no gateway (cabeçalho, URL pré-assinada, `aws-chunked`), a troca das credenciais emitidas dentro da aba, o real fora do sandbox e a falta do real; a TUI mostra as identidades no domínio delas.
- `tls-interception`: a variável `AWS_CA_BUNDLE` entra na lista das que apontam para o pacote de CAs com a CA do lançamento.

## Impact

- `sandmux-proto`: a chave da AWS em `Injection`, os campos da AWS em `CloudInfo` e a mensagem da captura; protocolo 48.
- `sandmux-net`: o assinador SigV4 (canônico, chave derivada, URL pré-assinada, pedaços do `aws-chunked`) e a troca das credenciais nas respostas.
- O daemon (`sandmux`):
  - `vault add-cloud aws`;
  - a leitura dos perfis do host;
  - o `aws configure export-credentials`;
  - o cache;
  - as credenciais capturadas, só na memória;
  - a pasta `~/.aws` da aba.
- `sandmux-policy`: a pasta `~/.aws` da aba e a variável da CA.
- A TUI, `vault list` e o `doctor` (o `aws` do host para as referências).
- Dependências: nenhum crate novo; o `hmac` e o `sha2`, que já estão no workspace, passam a ser usados no `sandmux-net`.
- Docs `docs/cloud.md` e `docs/en/cloud.md` (seção da AWS), `docs/vault.md`, o README e `docs/sessions.md` (protocolo), em pt e en.
- Testes: os exemplos de assinatura publicados pela AWS, um `aws` falso no host, um servidor local que confere a assinatura com a chave real, e um cliente SigV4 mínimo em Python na aba (o host não tem o `aws` nem o `boto3`).
