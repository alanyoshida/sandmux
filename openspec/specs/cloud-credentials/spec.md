# cloud-credentials Specification

## Purpose
Dar ao agente acesso às nuvens do usuário sem que uma credencial real de cloud entre no sandbox. O agente recebe um falso por identidade; o real fica fora, renovado pelo daemon, e o gateway o põe no lugar do falso só nos pedidos à nuvem. Cobre o GCP, o Kubernetes e a AWS.

## Requirements

### Requirement: Identidade de GCP no cofre
Com o cofre desbloqueado, `sandmux vault add-cloud gcp <nome>` MUST guardar uma identidade de cloud `gcp:<nome>` com uma fonte, dada por exatamente uma das opções `--configuration <configuração>` (uma referência a uma configuração do `gcloud` do host), `--copy-adc` (uma cópia do ADC `authorized_user` do host) ou `--copy-key <arquivo>` (uma cópia da chave JSON de uma conta de serviço). O nome MUST ter só letras, números, `-` e `_`, e MUST NOT repetir outra entrada.

#### Scenario: Referência ao host
- **WHEN** o host tem a configuração `trabalho` do `gcloud`, e o usuário roda `sandmux vault add-cloud gcp trabalho --configuration trabalho`
- **THEN** o cofre guarda a identidade `gcp:trabalho` com a fonte "host: configuração trabalho"

#### Scenario: Cópia da chave de uma conta de serviço
- **WHEN** o usuário roda `sandmux vault add-cloud gcp deploy --copy-key sa.json` com a chave de `deploy@app.iam.gserviceaccount.com`
- **THEN** o cofre guarda a identidade `gcp:deploy` com a chave, e o arquivo do host não muda

### Requirement: O que a identidade de GCP guarda
Uma referência MUST guardar só o nome da configuração do host, a conta e o projeto dela, nunca um token. Uma cópia MUST ficar cifrada no cofre, com a conta. Cada identidade MUST ter um token de acesso falso, com o prefixo `ya29.sandmux-fake-`, e um refresh token falso, com o prefixo `1//sandmux-fake-`, e MUST ter `*.googleapis.com` como domínio dono. A primeira identidade de GCP MUST ser a padrão, e `--default` MUST torná-la padrão.

#### Scenario: Referência sem token
- **WHEN** o usuário guarda a referência à configuração `trabalho`, com a conta `ana@exemplo.com` e o projeto `app-dev`
- **THEN** a identidade tem a conta e o projeto, os dois falsos com os prefixos, e nenhum token real

### Requirement: Recusa de uma fonte inválida
`sandmux vault add-cloud gcp` MUST recusar, com o motivo e sem mudar o cofre: uma configuração que não existe no host; um ADC que não é do tipo `authorized_user`; um arquivo de `--copy-key` sem `"type": "service_account"`; mais de uma fonte, ou nenhuma.

#### Scenario: Arquivo errado
- **WHEN** o arquivo de `--copy-key` não tem `"type": "service_account"`
- **THEN** o comando é recusado com o motivo, e nada muda no cofre

### Requirement: Identidades de cloud na lista do cofre
`sandmux vault list` MUST mostrar cada identidade de cloud com o nome, o tipo `cloud`, o provedor, a fonte, a conta e a marca de padrão, nunca um token ou uma chave. `sandmux vault set` e a troca de domínios MUST recusar uma identidade de cloud, com o comando certo, e `sandmux vault remove` MUST apagá-la.

#### Scenario: Lista sem segredo
- **WHEN** o cofre tem a identidade `gcp:deploy`, copiada de uma chave de conta de serviço
- **THEN** `sandmux vault list` mostra o nome, `cloud`, `gcp`, "cópia: conta de serviço" e a conta, e nenhuma parte da chave

### Requirement: Identidades de cloud na TUI
A seção "Cofre" e o grafo MUST mostrar as identidades de cloud no domínio dono delas (`*.googleapis.com`, `<nome>.kube.sandmux.internal`), com uma marca própria. Os detalhes MUST ter os campos da lista e o falso de acesso. `x` MUST apagar a identidade, e `v` e `d` MUST ser recusados com o comando certo. Nenhum token real, certificado ou chave MUST aparecer na tela.

#### Scenario: Grafo com uma identidade
- **WHEN** o cofre tem `gcp:trabalho` e o usuário abre o grafo no domínio `*.googleapis.com`
- **THEN** a identidade aparece com a marca de cloud, e os detalhes mostram a fonte, a conta e o falso de acesso

#### Scenario: Identidade de Kubernetes na seção Cofre
- **WHEN** o cofre tem `kube:kind-dev` e o usuário abre os detalhes dela no grafo
- **THEN** os detalhes mostram o tipo `cloud · kubernetes`, a fonte, o contexto `kind-dev`, o namespace, o servidor e o falso (os valores longos cortados à largura do painel), e nenhuma parte do certificado

### Requirement: O gcloud da aba com as identidades
Em toda aba, com o cofre desbloqueado e ao menos uma identidade de GCP, o HOME da aba MUST ter uma configuração do `gcloud` por identidade, com o nome dela, a conta, o projeto (quando há) e o token de acesso falso como arquivo de token, e a configuração da identidade padrão como a ativa. Os arquivos MUST valer só durante a execução, sem alterar os guardados no HOME sintético.

#### Scenario: Troca de identidade dentro da aba
- **WHEN** o cofre tem `gcp:trabalho` (padrão) e `gcp:deploy`, e o agente roda `gcloud config configurations activate deploy`
- **THEN** os comandos seguintes do `gcloud` mandam o token falso de `gcp:deploy`

### Requirement: O ADC da aba
Em toda aba, com o cofre desbloqueado e ao menos uma identidade de GCP, o HOME da aba MUST ter o arquivo de credencial padrão (`application_default_credentials.json`) do tipo `authorized_user`, com o refresh token falso da identidade padrão, só durante a execução. Com o cofre trancado, ou sem identidade de GCP, nem esse arquivo nem as configurações do `gcloud` MUST ser acrescentados.

#### Scenario: Biblioteca do Google pelo ADC
- **WHEN** um programa Python usa `google.auth.default()` dentro da aba
- **THEN** ele obtém as credenciais da identidade padrão, com valores falsos, sem opção extra

### Requirement: Troca do token de GCP no gateway
Com uma identidade de GCP no cofre desbloqueado, `*.googleapis.com` MUST entrar na lista de decifração de toda aba. Num pedido decifrado a `*.googleapis.com`, o gateway MUST trocar o token de acesso falso de uma identidade pelo real atual dela, no cabeçalho `Authorization`. A linha `https` de um pedido com troca MUST ter o nome da identidade, nunca o token. Com o cofre trancado, o gateway MUST NOT trocar.

#### Scenario: gcloud com o real só no gateway
- **WHEN** o agente roda `gcloud storage ls` com a identidade `gcp:trabalho` ativa
- **THEN** o Google recebe o token real de `gcp:trabalho`, o sandbox só vê o falso, e o registro tem uma linha `https` de `storage.googleapis.com` com o nome `gcp:trabalho`

#### Scenario: Cofre trancado
- **WHEN** o cofre é trancado com uma aba rodando, e o agente roda `gcloud storage ls`
- **THEN** o gateway não troca nada, e o Google recebe só o falso

### Requirement: Pedido de token respondido pelo gateway
Num `POST /token` a `oauth2.googleapis.com` com o refresh token falso de uma identidade, o gateway MUST responder ele mesmo, sem ir ao Google, com o token de acesso falso dela, o tipo `Bearer` e um `expires_in` que não passa a validade do real. O token real MUST NOT chegar ao sandbox em nenhuma resposta.

#### Scenario: Refresh respondido pelo gateway
- **WHEN** uma biblioteca do Google pede um token novo a `oauth2.googleapis.com/token` com o refresh falso da identidade padrão
- **THEN** a resposta vem do gateway, com o token de acesso falso da identidade, e nenhum pedido sai para o Google

### Requirement: O real de GCP fora do sandbox
O daemon MUST obter o token de acesso real de cada identidade fora do sandbox, com o HOME e o ambiente do usuário: de uma referência, pelo `gcloud` do host; de uma cópia de ADC, pelo refresh token; de uma cópia de chave, por uma asserção JWT assinada com ela. O real MUST ficar só na memória do daemon, com a validade, e MUST NOT ser gravado em arquivo nem no cofre.

#### Scenario: Referência resolvida pelo host
- **WHEN** chega um pedido com o falso de `gcp:trabalho`, uma referência à configuração `trabalho`
- **THEN** o real vem do `gcloud` do host com essa configuração, e nenhum arquivo novo guarda o real

### Requirement: Renovação do real de GCP
O daemon MUST renovar o real de uma identidade antes do fim da validade: o gateway MUST receber um real com pelo menos 5 minutos de validade, conferida também antes de cada pedido de uma conexão reaproveitada.

#### Scenario: Renovação
- **WHEN** o token real de uma identidade está a menos de 5 minutos do fim da validade e chega um pedido com o falso dela
- **THEN** o gateway usa um token real novo, obtido antes do pedido seguir

### Requirement: Real de GCP que falta
Quando o real de uma identidade não pode ser obtido (o `gcloud` não está no host, pede um login novo, ou o Google recusa a cópia), o gateway MUST NOT trocar o falso, o `POST /token` com o refresh falso MUST receber `invalid_grant` com uma descrição do que fazer no host, e o daemon MUST avisar o usuário uma vez por identidade, até o real voltar. O `sandmux doctor` MUST avisar de cada referência sem o `gcloud` no host.

#### Scenario: Login vencido no host
- **WHEN** o `gcloud` do host pede um login novo para a configuração de `gcp:trabalho`
- **THEN** o pedido de token da aba recebe `invalid_grant` dizendo para rodar `gcloud auth login` no host, e o usuário recebe um aviso

#### Scenario: gcloud fora do host
- **WHEN** `gcp:trabalho` é uma referência e o host não tem o `gcloud`
- **THEN** nada é trocado, e o `sandmux doctor` avisa que a identidade `gcp:trabalho` precisa do `gcloud` no host

### Requirement: Identidades de Kubernetes no cofre
Com o cofre desbloqueado, `sandmux vault add-cloud kubernetes` MUST guardar uma identidade `kube:<nome>` para cada contexto do kubeconfig do host (o de `KUBECONFIG`, ou `~/.kube/config`), ou só para os contextos dados por `--context <contexto>` (repetível). A identidade do contexto atual do host MUST ser a padrão.

#### Scenario: Todos os contextos de uma vez
- **WHEN** o kubeconfig do host tem os contextos `kind-dev` e `arn:aws:eks:us-east-1:123:cluster/prod`, e o usuário roda `sandmux vault add-cloud kubernetes`
- **THEN** o cofre guarda `kube:kind-dev` e `kube:arn-aws-eks-us-east-1-123-cluster-prod`, cada um com o nome original do contexto

#### Scenario: Só um contexto
- **WHEN** o usuário roda `sandmux vault add-cloud kubernetes --context kind-dev`
- **THEN** só `kube:kind-dev` é guardada

### Requirement: Nome da identidade de Kubernetes
O `<nome>` de `kube:<nome>` MUST ser o nome do contexto em minúsculas, com cada caractere fora de letras, números e `-` trocado por `-`, sem `-` nas pontas e com no máximo 40 caracteres. Um nome que já existe no cofre, ou que outro contexto do mesmo comando já deu, MUST ser pulado, com o aviso.

#### Scenario: Nome repetido
- **WHEN** o cofre já tem `kube:kind-dev`, e o usuário roda `sandmux vault add-cloud kubernetes`
- **THEN** o contexto `kind-dev` é pulado com o aviso, e os outros são guardados

### Requirement: O que a identidade de Kubernetes guarda
Uma referência (o padrão) MUST guardar só o nome do contexto, o caminho do kubeconfig do host, o servidor e o namespace, nunca um token, um certificado ou uma chave. Uma cópia (`--copy`) MUST guardar, cifrados no cofre, o servidor, a CA do cluster e a credencial do usuário do contexto (token ou certificado de cliente com a chave). Cada identidade MUST ter um token falso com o prefixo `sandmux-fake-kube-` e MUST ter `<nome>.kube.sandmux.internal` como domínio dono.

#### Scenario: Referência sem credencial
- **WHEN** o usuário guarda a referência ao contexto `kind-dev`, cujo usuário tem um certificado de cliente
- **THEN** a identidade tem o contexto, o servidor e o falso, e nenhuma parte do certificado nem da chave

### Requirement: Recusa de um contexto inválido
`sandmux vault add-cloud kubernetes` MUST recusar, com o motivo e sem mudar o cofre: um `--context` que não existe no kubeconfig do host; um kubeconfig sem contexto; o `kubectl` ausente no host. Com `--copy`, um contexto cujo usuário usa um plugin `exec` ou um `auth-provider` MUST ser recusado, dizendo para guardar a referência. Um contexto cujo usuário usa um `auth-provider` ou usuário e senha MUST ser recusado também na referência.

#### Scenario: Cópia de um usuário com plugin exec
- **WHEN** o usuário roda `sandmux vault add-cloud kubernetes --context eks-prod --copy`, e o usuário do contexto usa `aws eks get-token`
- **THEN** o comando é recusado, dizendo que o plugin só roda no host e que a referência funciona, e nada muda no cofre

### Requirement: Contexto não aceito entre todos
Sem `--context`, um contexto que seria recusado MUST ser pulado com o motivo, e os outros contextos MUST ser guardados.

#### Scenario: Contexto não aceito entre todos
- **WHEN** o kubeconfig do host tem `kind-dev` e `velho`, cujo usuário usa um `auth-provider`, e o usuário roda `sandmux vault add-cloud kubernetes`
- **THEN** `velho` é pulado com o motivo, e `kube:kind-dev` é guardada

### Requirement: O kubeconfig da aba com as identidades
Em toda aba, com o cofre desbloqueado e ao menos uma identidade de Kubernetes, o HOME da aba MUST ter um kubeconfig (`~/.kube/config`, também em `KUBECONFIG`) só com falsos: para cada identidade, um contexto com o nome original e o namespace do host, um cluster com o servidor `https://<nome>.kube.sandmux.internal` e a CA do lançamento, e um usuário com o token falso. O contexto atual MUST ser o da identidade padrão.

#### Scenario: Troca de contexto dentro da aba
- **WHEN** o cofre tem `kube:kind-dev` (padrão) e `kube:kind-prod`, e o agente roda `kubectl config use-context kind-prod`
- **THEN** os comandos seguintes do `kubectl` vão a `kind-prod.kube.sandmux.internal` com o token falso de `kube:kind-prod`

### Requirement: A pasta ~/.kube da aba
Com ao menos uma identidade de Kubernetes, a pasta `~/.kube` da aba MUST aceitar escrita e valer só durante a execução, sem alterar a guardada no HOME sintético. Com o cofre trancado, ou sem identidade de Kubernetes, a aba MUST NOT receber essa pasta.

#### Scenario: Pasta só da execução
- **WHEN** o agente roda `kubectl config use-context kind-prod` e a aba termina
- **THEN** a pasta `~/.kube` do HOME sintético não mudou

### Requirement: Rota do API server pelo gateway
O gateway MUST responder os nomes `<nome>.kube.sandmux.internal` no DNS da aba e MUST deixá-los passar pela política de rede, em qualquer modo, sem regra. Numa conexão TLS a um desses nomes, o gateway MUST decifrar com a CA do lançamento e MUST conectar ao servidor guardado da identidade, nunca ao destino pedido pelo sandbox, inclusive a um servidor no `127.0.0.1` do host. Um nome sem identidade no cofre aberto MUST ser recusado com `404` e um `Status` do Kubernetes.

#### Scenario: Cluster kind no host
- **WHEN** o servidor de `kube:kind-dev` é `https://127.0.0.1:41234` no host, e o agente roda `kubectl get pods`
- **THEN** o gateway conecta ao `127.0.0.1:41234` do host, e o `kubectl` recebe a lista

#### Scenario: Modo inferência
- **WHEN** a aba está no modo inferência, sem regra para `kind-dev.kube.sandmux.internal`
- **THEN** o DNS e a conexão a esse nome passam

### Requirement: Troca da credencial do Kubernetes no gateway
Num pedido decifrado a `<nome>.kube.sandmux.internal`, o gateway MUST trocar o token falso da identidade no `Authorization` pelo token real dela, ou, quando a credencial real é um certificado de cliente sem token, MUST tirar o `Authorization` e apresentar o certificado ao servidor. O `Host` MUST ir com o endereço do servidor real. A linha `https` do pedido MUST ter o nome da identidade e o servidor real, nunca a credencial.

#### Scenario: Token real só no gateway
- **WHEN** `kube:eks-prod` tem um token de `aws eks get-token`, e o agente roda `kubectl get ns`
- **THEN** o API server recebe o token real, o sandbox só vê o falso, e o registro tem uma linha `https` com `kube:eks-prod`

#### Scenario: Certificado de cliente
- **WHEN** o usuário de `kube:kind-dev` tem um certificado de cliente, e o agente roda `kubectl get ns`
- **THEN** o gateway apresenta o certificado ao API server, e o pedido chega sem o `Authorization` falso

### Requirement: O real do Kubernetes fora do sandbox
O daemon MUST obter a credencial real de cada identidade fora do sandbox, com o HOME e o ambiente do usuário: de uma referência, lendo o contexto pelo `kubectl` do host; de uma cópia, do cofre. Quando o usuário do contexto usa um plugin `exec`, o daemon MUST rodá-lo no host, sem terminal, e usar o token ou o certificado que ele devolve, com a validade dele. O real MUST ficar só na memória do daemon e MUST NOT ser gravado em arquivo.

#### Scenario: Plugin exec no host
- **WHEN** o usuário de `kube:gke-dev` usa `gke-gcloud-auth-plugin`, e chega um pedido com o falso dela
- **THEN** o daemon roda o plugin no host, e o gateway usa o token devolvido até perto da validade dele

### Requirement: Renovação do real do Kubernetes
O daemon MUST obter de novo o real de uma identidade de Kubernetes antes do fim da validade dele, com pelo menos 5 minutos de folga. Um real sem validade (um token estático, um certificado, ou um plugin que não a dá) MUST ser obtido de novo a cada 10 minutos, no máximo.

#### Scenario: Token trocado no host
- **WHEN** o usuário troca o token de `kube:kind-dev` no kubeconfig do host
- **THEN** em até 10 minutos o gateway usa o token novo

### Requirement: Real do Kubernetes que falta
Quando o real de uma identidade de Kubernetes não pode ser obtido (o `kubectl` não está no host, o contexto sumiu, o plugin `exec` falha ou pede login), o gateway MUST responder o pedido com `401` e um `Status` do Kubernetes que diz o motivo e o que fazer no host, sem ir ao servidor, e o daemon MUST avisar o usuário uma vez por identidade, até o real voltar. O `sandmux doctor` MUST avisar de cada referência sem o `kubectl` no host.

#### Scenario: Login vencido no host
- **WHEN** o plugin `exec` de `kube:eks-prod` falha porque a sessão SSO do host venceu
- **THEN** o `kubectl` da aba recebe `401` com o motivo, e o usuário recebe um aviso

### Requirement: Identidades da AWS no cofre
Com o cofre desbloqueado, `sandmux vault add-cloud aws` MUST guardar uma identidade `aws:<nome>` para cada perfil do host (os de `~/.aws/config` e `~/.aws/credentials`, ou os arquivos de `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE`), ou só para os perfis dados por `--profile <perfil>` (repetível). O `<nome>` MUST seguir a mesma regra do Kubernetes (minúsculas, `[a-z0-9-]`, até 40). A identidade do perfil `default` MUST ser a padrão; sem ele, a do perfil de `AWS_PROFILE`, ou a primeira.

#### Scenario: Todos os perfis de uma vez
- **WHEN** o host tem os perfis `default` e `Dev_SSO`, e o usuário roda `sandmux vault add-cloud aws`
- **THEN** o cofre guarda `aws:default` (padrão) e `aws:dev-sso`, cada uma com o nome original do perfil

### Requirement: O que a identidade da AWS guarda
Uma referência (o padrão) MUST guardar só o nome do perfil, a região e as variáveis dos arquivos do host, nunca uma chave. Uma cópia (`--copy`) MUST guardar, cifradas no cofre, as duas partes da chave de longo prazo do perfil. Cada identidade MUST ter um ID de chave falso de 20 caracteres com o prefixo `AKIASANDMUX` e uma chave secreta falsa, e MUST ter `*.amazonaws.com` como domínio dono.

#### Scenario: Referência sem chave
- **WHEN** o usuário guarda a referência ao perfil `dev`, um perfil de SSO com a região `sa-east-1`
- **THEN** a identidade tem o perfil, a região e os falsos, e nenhuma chave real

### Requirement: Recusas na identidade da AWS
`sandmux vault add-cloud aws` MUST recusar, com o motivo e sem mudar o cofre: um `--profile` que não existe; o host sem perfil nenhum; com `--copy`, um perfil sem chave estática (SSO, `role_arn`, `credential_process`) ou com um token de sessão. Sem `--profile`, um perfil recusado MUST ser pulado com o motivo, e os outros MUST ser guardados. Um nome que já existe no cofre MUST ser pulado, com o aviso.

#### Scenario: Cópia de um perfil de SSO
- **WHEN** o usuário roda `sandmux vault add-cloud aws --profile dev --copy`, e `dev` é um perfil de SSO
- **THEN** o comando é recusado, dizendo que só uma chave estática pode ser copiada e que a referência funciona, e nada muda no cofre

### Requirement: A pasta ~/.aws da aba
Em toda aba, com o cofre desbloqueado e ao menos uma identidade da AWS, o HOME da aba MUST ter uma pasta `~/.aws` com escrita, só da execução, com um `config` (um perfil por identidade, com o nome original e a região) e um `credentials` (o par falso de cada perfil), e o perfil `default` com os dados da identidade padrão.

#### Scenario: Perfil na aba
- **WHEN** o cofre tem `aws:default` e `aws:dev-sso`, e o agente roda `aws s3 ls --profile Dev_SSO`
- **THEN** o `aws` da aba assina o pedido com o par falso de `aws:dev-sso`

### Requirement: Variáveis da AWS na aba
Com a pasta `~/.aws` da aba, a aba MUST ter `AWS_CONFIG_FILE` e `AWS_SHARED_CREDENTIALS_FILE` apontando para os arquivos dela. Com o cofre trancado, ou sem identidade da AWS, nem a pasta nem as variáveis MUST ser acrescentadas.

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e uma aba começa
- **THEN** a aba não tem a pasta `~/.aws` da execução nem as variáveis dela

### Requirement: Reassinatura SigV4 no gateway
Com uma identidade da AWS no cofre desbloqueado, `*.amazonaws.com` MUST entrar na lista de decifração de toda aba. Num pedido decifrado assinado com SigV4 (`AWS4-HMAC-SHA256`, no `Authorization` ou na URL pré-assinada) com o ID falso de uma identidade, o gateway MUST refazer a assinatura com a chave real dela, sobre o mesmo pedido canônico, e MUST pôr o ID real no lugar do falso. A linha `https` do pedido MUST ter o nome da identidade, nunca a chave.

#### Scenario: Pedido reassinado
- **WHEN** o agente roda `aws s3 ls` com a identidade `aws:dev`
- **THEN** a AWS recebe uma assinatura válida para a chave real de `aws:dev`, o sandbox só vê o par falso, e o registro tem uma linha `https` com `aws:dev`

#### Scenario: URL pré-assinada
- **WHEN** o agente cria uma URL pré-assinada do S3 com o par falso e a usa dentro da aba
- **THEN** o gateway reassina a URL, e o S3 responde o objeto

### Requirement: Token de sessão na reassinatura
Quando o real de uma identidade tem um token de sessão, o pedido reassinado MUST levar o `X-Amz-Security-Token` real, entre os cabeçalhos assinados (ou na query, numa URL pré-assinada). Quando o real não tem, um token falso no pedido MUST sair, junto da lista dos assinados.

#### Scenario: Credencial temporária do SSO
- **WHEN** o real de `aws:dev` vem do SSO, com um token de sessão, e o agente roda `aws s3 ls`
- **THEN** o S3 recebe o token de sessão real, assinado

### Requirement: Corpo do pedido na reassinatura
O gateway MUST usar o hash do corpo do `x-amz-content-sha256` quando o pedido o traz, sem ler o corpo, e MUST calcular o hash quando ele falta, lendo um corpo de até 32 MiB; um corpo maior sem o hash MUST receber `413`. Num upload com `aws-chunked` assinado (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, também com o trailer), o gateway MUST refazer a assinatura de cada pedaço e a do trailer, em fluxo, sem mudar o tamanho do corpo.

#### Scenario: Upload com pedaços assinados
- **WHEN** um SDK antigo envia um objeto ao S3 com `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
- **THEN** cada pedaço chega ao S3 com a assinatura refeita pela chave real, e o upload termina

### Requirement: Credenciais emitidas dentro da aba
Na resposta de um pedido decifrado que emite credenciais (as ações do STS que devolvem credenciais, o `GetRoleCredentials` do SSO e o `GetCredentialsForIdentity` do Cognito), o gateway MUST trocar o ID, a chave secreta e o token de sessão reais por falsos novos, e o daemon MUST guardar o real só na memória, até a validade dele. Um pedido seguinte com esses falsos MUST ser reassinado com o real guardado.

#### Scenario: GetSessionToken dentro da aba
- **WHEN** o agente roda `aws sts get-session-token` com uma identidade copiada
- **THEN** a resposta traz um ID, uma chave e um token falsos, e nenhuma parte da credencial real

#### Scenario: AssumeRole dentro da aba
- **WHEN** o agente roda `aws sts assume-role` e usa a credencial devolvida em `aws s3 ls`
- **THEN** o agente só vê falsos, e o S3 recebe a assinatura da credencial temporária real

### Requirement: Resposta de credenciais que não pode ser trocada
Numa resposta que emite credenciais, sem o daemon, ou com um corpo que o gateway não lê (comprimido, ou maior que 1 MiB), o gateway MUST recusar a resposta com `502`: o real MUST NOT chegar ao sandbox.

#### Scenario: Resposta comprimida
- **WHEN** a resposta de um `AssumeRole` vem comprimida, mesmo sem o pedido aceitar compressão
- **THEN** o gateway responde `502`, e nenhuma credencial chega à aba

### Requirement: O real da AWS fora do sandbox
O daemon MUST obter o real de cada identidade fora do sandbox, com o HOME e o ambiente do usuário: de uma referência, pelo `aws configure export-credentials --profile <perfil> --format process` do host; de uma cópia, do cofre. O real MUST ficar só na memória do daemon, com a validade, e MUST ser obtido de novo antes do fim dela, com 5 minutos de folga. Um real sem validade MUST ser obtido de novo a cada 10 minutos, no máximo.

#### Scenario: SSO pelo host
- **WHEN** chega um pedido com o ID falso de `aws:dev`, uma referência a um perfil de SSO
- **THEN** o real vem do `aws` do host com esse perfil, e nenhum arquivo novo guarda o real

### Requirement: Real da AWS que falta
Quando o real de uma identidade da AWS não pode ser obtido (o `aws` não está no host, o SSO venceu, o perfil sumiu), o gateway MUST responder ao pedido com `403` e um erro no formato da AWS com o motivo e o que fazer no host, sem ir à AWS, e o daemon MUST avisar o usuário uma vez por identidade, até o real voltar. O `sandmux doctor` MUST avisar de cada referência sem o `aws` no host.

#### Scenario: SSO vencido no host
- **WHEN** o `aws` do host diz que a sessão de SSO de `aws:dev` venceu
- **THEN** o pedido da aba recebe `403` dizendo para rodar `aws sso login` no host, e o usuário recebe um aviso

### Requirement: Identidades da AWS na TUI
A seção "Cofre" e o grafo MUST mostrar as identidades da AWS no domínio `*.amazonaws.com`, com a marca de cloud. Os detalhes MUST ter a fonte, o perfil, a região e o ID falso. Nenhuma chave real MUST aparecer na tela.

#### Scenario: Identidade da AWS no grafo
- **WHEN** o cofre tem `aws:dev` e o usuário abre os detalhes dela no grafo
- **THEN** os detalhes mostram `cloud · aws`, a fonte, o perfil, a região e o ID falso
