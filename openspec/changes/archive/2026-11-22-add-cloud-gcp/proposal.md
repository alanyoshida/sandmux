## Why

Hoje, para um agente usar o `gcloud`, o `aws` ou o `kubectl`, o usuário monta a credencial de propósito (`--ro ~/.config/gcloud`). Com isso, a credencial real fica ao alcance do agente, e ele pode copiá-la para fora (seção 6 do roadmap). O item "Credencial temporária de cloud" de "Depois da v1" foi escolhido pelo usuário em 2026-11-22. Na exploração do mesmo dia, ele decidiu que o real nunca entra no sandbox: o agente recebe um falso, e o gateway troca o falso pelo real, como já faz com as chaves de API e com o login do `agy`. A ordem é GCP, depois Kubernetes, depois AWS. Este change é o do GCP, com as peças comuns aos três.

## What Changes

- **Identidades de cloud no cofre:** um tipo novo de entrada, a identidade de cloud. Neste change, só o GCP. Ela tem um nome, uma fonte e um falso.
  - **Referência ao host (o padrão):** o cofre guarda só qual configuração do `gcloud` do host usar. O daemon tira o token de acesso real da própria CLI do host, fora do sandbox, e o renova antes de expirar. SSO, reautenticação e contas de serviço funcionam como no terminal do usuário.
  - **Cópia no cofre (opcional):** o arquivo de credencial padrão do host (o ADC de usuário, com o refresh token) ou a chave JSON de uma conta de serviço ficam cifrados no cofre. O daemon cunha o token de acesso a partir deles. Isso serve para tirar a credencial longa do disco do host.
- **Comando:** `sandmux vault add-cloud gcp <nome>`, com `--configuration <configuração>` (referência), `--copy-adc` ou `--copy-key <arquivo>` (cópia). `sandmux vault list` mostra a identidade com o tipo, a fonte e a conta, nunca o token.
- **Em toda aba, como as chaves:** com o cofre aberto, cada identidade vira uma configuração do `gcloud` no HOME da aba, com o token falso, e o ADC das bibliotecas do Google aponta para a identidade padrão com um refresh token falso. O agente troca de identidade pelo `gcloud config configurations activate`, como no host. O `gcloud` confia na CA do lançamento (`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`).
- **Troca no gateway:** com uma identidade de GCP no cofre, a aba decifra `*.googleapis.com`, e o gateway troca o token falso pelo real atual da identidade. No `POST /token` de `oauth2.googleapis.com` com o refresh falso de uma identidade, o gateway responde sozinho com o token falso, sem ir ao Google. O real nunca chega ao sandbox.
- **Sem o real:** quando o host não dá o token (o `gcloud` não está instalado, ou pede um login novo), o gateway não troca, o pedido de token recebe `invalid_grant` com o que fazer no host, e o daemon avisa uma vez.
- **Registro:** cada pedido com troca registra o nome da identidade, como os segredos.
- **TUI:** a seção Cofre e o grafo do cofre mostram as identidades de cloud (só leitura, como as conexões de banco), com uma marca própria; apagar funciona.
- **Limite:** o que o agente pode fazer é o que o IAM da identidade permite. O gateway não filtra os pedidos de cloud, só registra (decisão do usuário).

## Capabilities

### New Capabilities

- `cloud-credentials`: identidades de cloud no cofre, com fonte no host ou cópia; o falso por identidade em toda aba; a troca e a resposta do token pelo gateway; a renovação do real fora do sandbox; o registro e a exibição. Começa com o GCP, e o Kubernetes e a AWS entram nos changes seguintes.

### Modified Capabilities

- `tls-interception`: a variável do `gcloud` (`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`) entra na lista das que apontam para o pacote de CAs com a CA do lançamento.

## Impact

- `sandmux-vault`: o tipo de entrada da identidade de cloud.
- O daemon (`sandmux`): `vault add-cloud`, a resolução do real (CLI do host, refresh do ADC, JWT da conta de serviço), o cache com a expiração e o aviso.
- `sandmux-policy`: os arquivos do `gcloud` e do ADC na aba, e a variável da CA.
- `sandmux-net`: a resposta local do token e a troca pelo real atual.
- A TUI (seção Cofre e grafo), `vault list` e o `doctor` (o `gcloud` do host para as referências).
- Dependências: nenhum crate novo no `Cargo.lock`. O `rsa` e o `sha2`, que já estão lá, podem passar a ser dependência direta, para o JWT da conta de serviço.
- Docs novos `docs/cloud.md` e `docs/en/cloud.md`, mais o README e o `docs/vault.md` (pt e en).
- Testes: um `gcloud` falso no host e um servidor local no lugar do Google.
