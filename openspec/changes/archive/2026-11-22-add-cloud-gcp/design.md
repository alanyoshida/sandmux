## Context

O modelo do sandmux para segredos é este: o agente recebe um valor falso, e o gateway troca o falso pelo real num pedido decifrado ao domínio dono (`key-injection`). O gateway pede os reais ao daemon a cada conexão decifrada (`sandmux-net/src/inject.rs`). O login do `agy` (change 66) já faz isso com tokens do Google: o `access_token` e o `refresh_token` reais ficam no cofre, e o gateway troca e captura em `*.googleapis.com`, só nas abas do `agy`. As ferramentas recebem o falso por variáveis e por arquivos montados só durante a execução, por cima do HOME sintético (`sandmux-policy/src/toolsecrets.rs`).

As decisões do usuário (2026-11-22) estão no proposal. Elas moldam este design:

- o real nunca entra no sandbox;
- a credencial vale em toda aba, como as chaves;
- a fonte é a referência ao host por padrão, com a cópia no cofre como opção;
- o limite é só o IAM, e o gateway não filtra.

## Goals / Non-Goals

**Goals:**
- O `gcloud` e as bibliotecas do Google (pelo ADC) funcionam dentro da aba sem opção extra, com várias identidades.
- As peças comuns ficam prontas para o Kubernetes e a AWS: o tipo no cofre, a resolução do real fora, o cache com a validade, o aviso, a exibição.

**Non-Goals:**
- Filtrar o que o agente faz na nuvem (decisão do usuário: só o IAM).
- O `gsutil` e o `bq` antigos, que têm autenticação própria (boto); os comandos `gcloud storage` e `gcloud bigquery` (ou `bq` pelo `gcloud`) os substituem.
- Tokens de identidade (`gcloud auth print-identity-token`, Cloud Run autenticado): o falso é um token de acesso.
- Capturar um `gcloud auth login` feito dentro da aba.

## Decisions

**1. O `gcloud` da aba lê o token de um arquivo, por configuração.** Cada identidade vira uma configuração do `gcloud` (`configurations/config_<nome>`) com a propriedade `auth/access_token_file`, que aponta para um arquivo com o falso. A troca de identidade é a do próprio `gcloud` (`configurations activate`).
- *Alternativa descartada:* reescrever o `credentials.db` do `gcloud` com refresh tokens falsos. É SQLite num formato interno, que muda entre versões.
- *Alternativa descartada:* `CLOUDSDK_AUTH_ACCESS_TOKEN_FILE` no ambiente. Prende a aba a uma identidade só.
- *Custo:* com o arquivo de token, o `gcloud` não tenta renovar. É o que se quer, porque o falso não expira; o gateway põe o real atual.

**2. O gateway responde o `POST /token` do refresh falso sozinho.** As bibliotecas do Google (ADC `authorized_user`) pedem um token novo com o refresh. Quando o refresh do pedido é o falso de uma identidade, o gateway monta a resposta (`access_token` falso, `token_type`, `expires_in`) sem abrir conexão com o Google.
- *Por quê:* a mesma resposta serve às três fontes. Na referência, o refresh real nem existe do lado de fora. O falso nunca sai para o Google.
- Esse teste vem antes da captura do `agy`, que usa o mesmo endpoint.
- *Alternativa descartada:* repassar ao Google trocando o refresh falso pelo real. Só funciona na cópia de ADC.

**3. O real da referência vem de `gcloud config config-helper --format=json --configuration <c>`.** Esse comando devolve o token de acesso e a validade (`token_expiry`), e o `print-access-token` não dá a validade.
- Roda com o HOME e o ambiente do usuário, fora do sandbox.
- Usa `CLOUDSDK_CORE_DISABLE_PROMPTS=1`, para nunca esperar uma pergunta.
- Tem prazo de 30 s.
- Uma saída que pede login vira o aviso.

**4. As cópias resolvem o real no próprio daemon.**
- **ADC:** o grant `refresh_token` em `https://oauth2.googleapis.com/token`.
- **Conta de serviço:** o grant `urn:ietf:params:oauth:grant-type:jwt-bearer`, com o JWT assinado em RS256 (PKCS#1 v1.5, SHA-256) pelo crate `rsa`, já no `Cargo.lock`, e o escopo `cloud-platform`.
- O pedido sai do daemon, fora do sandbox e sem passar pelo gateway.

**5. O real fica no daemon, em memória, com a validade.** O daemon devolve ao gateway o real com pelo menos 5 minutos de validade, e renova antes, se preciso. O gateway pede os reais a cada conexão decifrada, como hoje. Para cloud, ele confere a validade também antes de cada pedido da conexão, porque o `gcloud` reaproveita conexões por mais tempo que a validade do token.

**6. Um tipo novo de entrada no cofre, `cloud`.** O valor é um JSON com o provedor, a fonte (`host` ou `copy`) e os dados dela: configuração, conta e projeto; ou o ADC; ou a chave. O tipo `cloud` não é um token comum: tem os próprios falsos (o de acesso e o de refresh), domínios fixos e nenhuma ferramenta. O `vault set` e a edição de domínios recusam, como nas conexões de banco. O nome no cofre é `gcp:<nome>`, e é ele que vai para o registro.

**7. Os domínios donos são fixos por provedor.** No GCP, `*.googleapis.com` entra na lista de decifração de toda aba pela regra que já existe ("os domínios donos dos segredos do cofre desbloqueado"), sem regra nova de rede. O `oauth2.googleapis.com` está dentro desse padrão.

**8. O aviso usa os canais que já existem.** A notificação do desktop do change 48, quando ela está ligada, e uma linha no registro do daemon, uma vez por identidade, até o real voltar. A própria resposta `invalid_grant` diz ao agente o que fazer no host. Um aviso na TUI ficou de fora: o daemon não tem hoje um canal de avisos para a TUI fora das abas.

## Risks / Trade-offs

- [O gateway oferece só HTTP/1.1; as bibliotecas do Google que usam gRPC (Pub/Sub, Firestore, Spanner) falham com `*.googleapis.com` decifrado] → Limite conhecido, escrito no `docs/cloud.md`. O `gcloud` e as APIs REST (Storage, BigQuery, Compute) funcionam. O HTTP/2 no gateway fica para depois, como item próprio.
- [Toda aba passa a decifrar `*.googleapis.com` quando há uma identidade de GCP] → É a escolha do usuário (toda aba). O tráfego decifrado já é o caminho do `agy`, e cada pedido vai para o registro.
- [O IAM da identidade é o único limite: uma identidade que pode criar chaves de conta de serviço deixa o agente criar uma e ler a chave real pela API] → O aviso no comando e no `docs/cloud.md` pede uma identidade sem `iam.serviceAccountKeys.create` e sem acesso ao Secret Manager, ou uma conta de serviço própria para o agente.
- [Um `gcloud auth login` dentro da aba grava um token real no HOME sintético, como hoje] → Fica fora deste change, escrito nos limites do doc.
- [Uma versão antiga do sandmux lendo um cofre com o tipo `cloud`] → O tipo desconhecido não é token: a versão antiga não o troca nem o mostra como token. Nada vaza, mas a identidade some da lista até a atualização.
- [O `gcloud` do host lento ou travado] → Prazo de 30 s na resolução. Enquanto isso, o pedido espera; depois do prazo, segue o caminho do real que falta.

## Migration Plan

Não há migração: o cofre ganha entradas de um tipo novo só quando o usuário roda `vault add-cloud`. Para voltar atrás, basta apagar as identidades (`vault remove`).
