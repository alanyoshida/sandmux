# Credenciais de cloud

> English: [en/cloud.md](en/cloud.md)

O agente usa o `gcloud` e as bibliotecas do Google Cloud dentro do sandbox, mas nenhuma credencial real de cloud entra nele. Cada identidade de cloud do cofre vira, na aba, um token falso. O real fica fora do sandbox, com o daemon, e o gateway o põe no lugar do falso só nos pedidos às APIs do Google. Um falso copiado para fora não vale nada.

Hoje o sandmux cobre o GCP. O Kubernetes e a AWS vêm depois.

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
