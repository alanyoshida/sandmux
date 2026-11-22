## ADDED Requirements

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
