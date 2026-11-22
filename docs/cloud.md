# Credenciais de cloud

> English: [en/cloud.md](en/cloud.md)

O agente usa o `gcloud`, as bibliotecas do Google Cloud, o `kubectl`, o `aws` e os SDKs da AWS dentro do sandbox, mas nenhuma credencial real de cloud entra nele. Cada identidade de cloud do cofre vira, na aba, um token falso. O real fica fora do sandbox, com o daemon, e o gateway o põe no lugar do falso só nos pedidos à nuvem dela. Um falso copiado para fora não vale nada.

O sandmux cobre o GCP, o Kubernetes ([abaixo](#kubernetes)) e a AWS ([abaixo](#aws)).

## Guardar uma identidade

Com o cofre desbloqueado:

```sh
# Referência a uma configuração do gcloud do host (o padrão)
sandmux vault add-cloud gcp trabalho --configuration trabalho

# Cópia do ADC do host (o do `gcloud auth application-default login`)
sandmux vault add-cloud gcp pessoal --copy-adc

# Cópia da chave JSON de uma conta de serviço
sandmux vault add-cloud gcp deploy --copy-key ~/Downloads/deploy.json --default
```

O nome no cofre é `gcp:<nome>`. Cada identidade tem uma fonte:

- **Referência ao host (`--configuration`):** o cofre guarda só o nome da configuração do `gcloud`, a conta e o projeto, nunca um token. Para cada pedido, o daemon obtém o token de acesso pelo `gcloud` do host (`gcloud config config-helper`), com o seu HOME e o seu ambiente, fora do sandbox. SSO, reautenticação e contas de serviço funcionam como no seu terminal: quando o `gcloud` pede um login novo, rode `gcloud auth login` no host.
- **Cópia (`--copy-adc`, `--copy-key`):** o refresh token do ADC ou a chave da conta de serviço ficam cifrados no cofre. O daemon cunha o token de acesso no endpoint de token do Google. Serve para tirar a credencial longa do disco do host: depois da cópia, você pode apagar o arquivo.

A primeira identidade de GCP é a padrão, e `--default` torna outra a padrão. `sandmux vault list` mostra cada identidade com o tipo `cloud`, a fonte, a conta, o projeto e o falso, nunca o token nem a chave. Para trocar uma identidade, apague com `sandmux vault remove gcp:<nome>` e guarde de novo.

## Na aba

Com o cofre desbloqueado e ao menos uma identidade de GCP, toda aba recebe, só durante a execução:

- uma configuração do `gcloud` por identidade (`~/.config/gcloud/configurations/config_<nome>`), com a conta, o projeto e o token falso num arquivo (`auth/access_token_file`);
- a identidade padrão como a ativa;
- o ADC (`~/.config/gcloud/application_default_credentials.json`) com um refresh token falso da padrão, para as bibliotecas do Google (`google.auth.default()` e afins).

O agente troca de identidade como no host: `gcloud config configurations activate deploy`, ou `--configuration deploy` num comando. A pasta do `gcloud` da aba é só desta execução: o que a aba grava nela some no fim, e a pasta guardada no HOME sintético não muda. O `gcloud` confia na CA do lançamento (`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`).

## No gateway

Com uma identidade de GCP no cofre, toda aba decifra `*.googleapis.com`, e o gateway:

- troca o token falso de uma identidade pelo real atual dela no cabeçalho `Authorization`;
- responde ele mesmo o `POST /token` de `oauth2.googleapis.com` que leva o refresh falso, com o token de acesso falso, sem ir ao Google;
- registra cada pedido com o nome da identidade (`chave: gcp:trabalho` em `sandmux net`), nunca o token.

O real fica só na memória do daemon, com a validade, e é renovado antes de vencer: o gateway sempre recebe um real com pelo menos 5 minutos de folga, também numa conexão reaproveitada. Com o cofre trancado, nada é trocado.

Quando o real falta (o `gcloud` não está no host, pede um login novo, ou o Google recusa a cópia), o gateway deixa o falso seguir, e o pedido de token da aba recebe `invalid_grant` com o que fazer no host. O daemon avisa uma vez por identidade, pela notificação do desktop e no registro dele, até o real voltar. O `sandmux doctor` avisa de cada referência sem o `gcloud` no host.

## O que o agente pode fazer

O que o IAM da identidade permitir: o gateway não filtra os pedidos de cloud, só os registra. Escolha uma identidade com o mínimo, de preferência uma conta de serviço própria para o agente. Duas permissões deixam o agente obter uma credencial real pela própria API, e devem ficar de fora:

- criar chaves de conta de serviço (`iam.serviceAccountKeys.create`);
- ler segredos do Secret Manager.

Como as chaves de API, a identidade vale em toda aba, em qualquer pasta.

## Limites

- **gRPC:** o gateway decifra só HTTP/1.1. As bibliotecas do Google que usam gRPC (Pub/Sub, Firestore, Spanner) falham com `*.googleapis.com` decifrado. O `gcloud` e as APIs REST (Storage, BigQuery, Compute) funcionam.
- **`gsutil` e `bq` antigos:** têm autenticação própria. Use `gcloud storage` e o `bq` pelo `gcloud`.
- **Tokens de identidade:** `gcloud auth print-identity-token` e o Cloud Run autenticado não funcionam: o falso é um token de acesso.
- **`gcloud auth login` dentro da aba:** grava um token real no HOME sintético, como hoje. Faça o login no host.

## Kubernetes

### Guardar os contextos

Com o cofre desbloqueado, um comando guarda todos os contextos do kubeconfig do host (o de `KUBECONFIG`, ou `~/.kube/config`), cada um como uma identidade:

```sh
# Todos os contextos, por referência ao host (o padrão)
sandmux vault add-cloud kubernetes

# Só alguns contextos
sandmux vault add-cloud kubernetes --context kind-dev --context gke_app_europe-west1_main

# Cópia do servidor, da CA e da credencial de um contexto
sandmux vault add-cloud kubernetes --context kind-dev --copy
```

O nome no cofre é `kube:<nome>`, com o nome do contexto em minúsculas e cada caractere fora de letras, números e `-` trocado por `-`: `arn:aws:eks:us-east-1:123:cluster/prod` vira `kube:arn-aws-eks-us-east-1-123-cluster-prod`. Um nome que já está no cofre é pulado, com o aviso. A identidade do contexto atual do host é a padrão.

- **Referência ao host (o padrão):** o cofre guarda só o contexto, o caminho do kubeconfig, o servidor e o namespace. O daemon lê o contexto pelo `kubectl` do host (`kubectl config view --raw --flatten --minify`) e, quando o usuário dele usa um plugin `exec` (`aws eks get-token`, `gke-gcloud-auth-plugin`, `kubelogin`), roda o plugin no host, sem terminal, com o seu ambiente. Quando o plugin pede um login novo, faça o login no host (`aws sso login`, `gcloud auth login`).
- **Cópia (`--copy`):** o servidor, a CA do cluster e a credencial (o token ou o certificado de cliente com a chave) ficam cifrados no cofre. Um contexto com plugin `exec` não pode ser copiado: o plugin só roda no host.

O sandmux recusa o `auth-provider` antigo e o usuário com senha. Sem `--context`, um contexto recusado é pulado com o motivo, e os outros são guardados. `sandmux vault list` mostra cada identidade com a fonte, o servidor e o falso, nunca o token, o certificado ou a chave. Para tirar um contexto do agente, `sandmux vault remove kube:<nome>`.

### Na aba

Com ao menos uma identidade de Kubernetes, toda aba recebe, só durante a execução, uma pasta `~/.kube` com escrita e um kubeconfig só com falsos (`KUBECONFIG` aponta para ele):

- um contexto por identidade, com o nome original e o namespace do host;
- o servidor `https://<nome>.kube.sandmux.internal`, com a CA do lançamento (`~/.kube/sandmux-ca.crt`);
- um token falso por identidade (`sandmux-fake-kube-…`);
- o contexto atual na identidade padrão.

`kubectl config use-context`, o cache de descoberta e o resto do que o `kubectl` grava ficam na pasta da aba, que some no fim.

### No gateway

O DNS da aba responde todo `*.kube.sandmux.internal` com `10.0.2.253`, e esse nome passa pela política de rede sem regra, em qualquer modo: guardar a identidade é o consentimento. O gateway decifra a conexão e conecta ele mesmo ao servidor guardado, nunca ao destino que o sandbox pediu, inclusive a um cluster no `127.0.0.1` do host (o kind e o minikube). O certificado do servidor é verificado pela CA do cluster, e o gateway apresenta o certificado de cliente quando há.

Em cada pedido, o gateway troca o token falso pelo real (do kubeconfig, do `tokenFile` ou do plugin). Quando a credencial é só o certificado de cliente, o gateway tira o `Authorization`. O `Host` e o prefixo do caminho (o do Rancher, por exemplo) vêm do servidor real. O registro tem o nome da identidade e o servidor real, nunca a credencial. `kubectl exec`, `attach`, `cp`, `logs -f`, `port-forward` e `get -w` funcionam, pelo WebSocket ou pelo SPDY.

O real é obtido de novo antes do fim da validade do plugin. Sem validade (um token ou um certificado do kubeconfig), ele é relido a cada 10 minutos, e uma troca no kubeconfig do host vale nesse prazo. Quando o real falta (o `kubectl` não está no host, o contexto sumiu, o plugin falha), o gateway responde `401` com um `Status` do Kubernetes que diz o motivo, e o daemon avisa uma vez, como no GCP. Um nome sem identidade recebe `404`. O `sandmux doctor` avisa de cada referência sem o `kubectl` no host.

### O que o agente pode fazer

O que o RBAC de cada contexto permitir: o gateway não filtra os pedidos ao API server. Um contexto de administrador dá administrador ao agente. Prefira um contexto com uma ServiceAccount própria e o mínimo de permissões, e tire do cofre os contextos de produção que o agente não deve ver. Ler `secrets` dá ao agente as credenciais reais que o cluster guarda.

### Limites do Kubernetes

- **HTTP/2:** o gateway oferece só HTTP/1.1, e o `client-go` passa a usá-lo sozinho.
- **`proxy-url` do kubeconfig:** o gateway conecta direto ao servidor.
- **`insecure-skip-tls-verify`:** quando o kubeconfig do host pede, o gateway também não verifica o servidor.
- **Um certificado de cliente renovado** vale na conexão seguinte; uma conexão longa (`get -w`) segue com o anterior até o servidor fechá-la.

## AWS

### Guardar os perfis

Com o cofre desbloqueado, um comando guarda todos os perfis do host (os de `~/.aws/config` e `~/.aws/credentials`, ou os de `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE`), cada um como uma identidade:

```sh
# Todos os perfis, por referência ao host (o padrão)
sandmux vault add-cloud aws

# Só alguns perfis
sandmux vault add-cloud aws --profile dev --profile ci

# Cópia da chave de longo prazo de um usuário IAM
sandmux vault add-cloud aws --profile ci --copy
```

O nome no cofre é `aws:<nome>`, com a mesma regra do Kubernetes: `Dev_SSO` vira `aws:dev-sso`. Um nome que já está no cofre é pulado, com o aviso. A identidade do perfil `default` é a padrão; sem ele, a do perfil de `AWS_PROFILE`, ou a primeira guardada.

- **Referência ao host (o padrão):** o cofre guarda só o perfil, a região e as variáveis dos arquivos do host. O daemon tira o real do `aws` do host (`aws configure export-credentials --profile <perfil> --format process`), com o seu ambiente, fora do sandbox. SSO, `assume-role` e `credential_process` funcionam como no seu terminal; quando o SSO vence, rode `aws sso login` no host.
- **Cópia (`--copy`):** as duas partes da chave de longo prazo do perfil ficam cifradas no cofre, e o gateway assina direto com ela. Um perfil sem chave estática (SSO, `role_arn`, `credential_process`) ou com um token de sessão não pode ser copiado.

Sem `--profile`, um perfil recusado é pulado com o motivo. `sandmux vault list` mostra cada identidade com a fonte, a região e o ID falso, nunca a chave.

### Na aba

Com ao menos uma identidade da AWS, toda aba recebe, só durante a execução, uma pasta `~/.aws` com escrita (`AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE` apontam para ela):

- um perfil por identidade, com o nome original e a região;
- um par falso por perfil: o ID da chave (`AKIASANDMUX…`, 20 caracteres) e uma chave secreta falsa;
- o `[default]` com a identidade padrão.

O `aws` confia na CA do lançamento (`AWS_CA_BUNDLE`), e o cache dele (`~/.aws/cli/cache`) some no fim.

### No gateway

Com uma identidade da AWS no cofre, toda aba decifra `*.amazonaws.com`. Na AWS, o segredo não viaja no pedido: o SDK assina cada pedido (SigV4) com a chave secreta. O gateway acha a identidade pelo ID falso do pedido e refaz a assinatura com a chave real, sobre o mesmo pedido canônico:

- no `Authorization` e na URL pré-assinada (`X-Amz-Signature`);
- com o token de sessão real (`X-Amz-Security-Token`), quando o real é temporário;
- nos uploads do S3 com `aws-chunked` assinado (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, também com o trailer), pedaço a pedaço, em fluxo.

Um pedido sem `x-amz-content-sha256` tem o corpo lido para o hash, até 32 MiB; acima disso, `413`. O registro tem o nome da identidade (`chave: aws:dev`), nunca a chave.

**Credenciais emitidas dentro da aba:** a resposta do STS (`AssumeRole`, `GetSessionToken`, `GetFederationToken` e afins), do `GetRoleCredentials` do SSO ou do `GetCredentialsForIdentity` do Cognito traz uma credencial real. O gateway a troca por falsos novos (`ASIASANDMUX…`), e o daemon guarda o real só na memória, até a validade dele. Os pedidos com esses falsos são reassinados com o real guardado (`chave: aws:dev+sts` no registro). A resposta que não pode ser trocada (comprimida, grande demais, ou com o cofre trancado) é recusada com `502`. Um `sandmux daemon restart` esquece essas credenciais.

O real é obtido de novo antes do fim da validade, com 5 minutos de folga; sem validade (uma chave estática), a cada 10 minutos. Quando o real falta (o `aws` não está no host, o SSO venceu, o perfil sumiu), o gateway responde `403` com um erro no formato da AWS que diz o que fazer no host, e o daemon avisa uma vez. O `sandmux doctor` avisa de cada referência sem o `aws` no host.

### O que o agente pode fazer

O que o IAM do perfil permitir: o gateway não filtra os pedidos. Prefira um papel próprio para o agente, com o mínimo, e tire do cofre os perfis de produção que ele não deve ver. Duas permissões entregam uma credencial real que o gateway não troca, e devem ficar de fora:

- criar chaves de acesso (`iam:CreateAccessKey`): a chave nova volta na resposta;
- ler segredos do Secrets Manager.

### Limites da AWS

- **SigV4a** (`AWS4-ECDSA-P256-SHA256`, dos multi-region access points): o pedido passa sem mudar e falha na AWS.
- **Event streams assinados no pedido** (Transcribe streaming, por HTTP/2): o gateway só fala HTTP/1.1. As respostas em stream (Bedrock `InvokeModelWithResponseStream`) funcionam.
- **A China** (`*.amazonaws.com.cn`) e os endpoints `*.api.aws` não são decifrados.
- **`aws configure` ou `aws sso login` dentro da aba** gravam na pasta da aba, que some no fim. Faça o login no host.
