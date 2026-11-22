## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Identidades de cloud na TUI
A seção "Cofre" e o grafo MUST mostrar as identidades de cloud no domínio dono delas (`*.googleapis.com`, `<nome>.kube.sandmux.internal`), com uma marca própria. Os detalhes MUST ter os campos da lista e o falso de acesso. `x` MUST apagar a identidade, e `v` e `d` MUST ser recusados com o comando certo. Nenhum token real, certificado ou chave MUST aparecer na tela.

#### Scenario: Grafo com uma identidade
- **WHEN** o cofre tem `gcp:trabalho` e o usuário abre o grafo no domínio `*.googleapis.com`
- **THEN** a identidade aparece com a marca de cloud, e os detalhes mostram a fonte, a conta e o falso de acesso

#### Scenario: Identidade de Kubernetes na seção Cofre
- **WHEN** o cofre tem `kube:kind-dev` e o usuário abre os detalhes dela no grafo
- **THEN** os detalhes mostram o tipo `cloud · kubernetes`, a fonte, o contexto `kind-dev`, o namespace, o servidor e o falso (os valores longos cortados à largura do painel), e nenhuma parte do certificado
