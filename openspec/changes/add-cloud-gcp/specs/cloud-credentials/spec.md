## Purpose

Dar ao agente acesso às nuvens do usuário sem que uma credencial real de cloud entre no sandbox. O agente recebe um falso por identidade; o real fica fora, renovado pelo daemon, e o gateway o põe no lugar do falso só nos pedidos à nuvem. Começa com o GCP; o Kubernetes e a AWS entram nos changes seguintes.

## ADDED Requirements

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
A seção "Cofre" da TUI e a tela do grafo MUST mostrar as identidades no domínio `*.googleapis.com`, com uma marca própria. Os detalhes MUST ter os campos da lista e o valor falso do token de acesso. Na seção e no grafo, `x` MUST apagar a identidade, e `v` e `d` MUST ser recusados com o comando certo. Nenhum token real ou chave MUST aparecer na tela.

#### Scenario: Grafo com uma identidade
- **WHEN** o cofre tem `gcp:trabalho` e o usuário abre o grafo no domínio `*.googleapis.com`
- **THEN** a identidade aparece com a marca de cloud, e os detalhes mostram a fonte, a conta e o falso de acesso

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
