Os testes rodam no host, sob o vigia de memória. Nenhum teste chama o Google: o `gcloud` do host é um executável falso, e o destino é um servidor local com a CA de teste (`SANDMUX_UPSTREAM_CA`). Para os testes, uma variável só de teste troca o domínio do GCP por `host.sandmux.internal`.

## 1. Cofre e comando

- [x] 1.1 Em `sandmux-vault`, o tipo `cloud` (provedor, fonte e dados da fonte, o falso de acesso `ya29.sandmux-fake-…` e o de refresh `1//sandmux-fake-…`, a marca de padrão), fora da troca de tokens comuns, das ferramentas e do `vault set`. Teste de unidade: os falsos têm os prefixos, e um cofre com o tipo novo abre e fecha sem perder a entrada
- [x] 1.2 `sandmux vault add-cloud gcp <nome>` com `--configuration`, `--copy-adc` ou `--copy-key`, e `--default`: valida o nome, a configuração do host (pelo `gcloud` do host), o tipo do ADC e o `"type": "service_account"` da chave, e recusa com o motivo. A mensagem de sucesso avisa que o limite é o IAM da identidade. Testes em `tests/cloud.rs`, com um `gcloud` falso: os três casos de sucesso e as recusas da spec
- [x] 1.3 `sandmux vault list` mostra o nome, `cloud`, `gcp`, a fonte, a conta e a marca de padrão, sem token nem chave; `vault set` e a troca de domínios recusam uma identidade; `vault remove` a apaga. Teste em `tests/cloud.rs`, que confere que nenhuma parte da chave copiada aparece na saída

## 2. O real fora do sandbox

- [x] 2.1 No daemon, a resolução do real: referência por `gcloud config config-helper --format=json --configuration <c>` (com o ambiente do usuário, `CLOUDSDK_CORE_DISABLE_PROMPTS=1` e um prazo de 30 s); cópia de ADC pelo grant `refresh_token`; cópia de chave pelo JWT RS256 (`jwt-bearer`, escopo `cloud-platform`). Testes de unidade: a leitura da saída do `config-helper` (com e sem login vencido), o corpo do grant de refresh, e o JWT verificado com a chave pública do teste
- [x] 2.2 O cache do real em memória, com a validade: devolve um real com pelo menos 5 minutos sobrando e renova antes; nunca grava o real. Teste de unidade com um relógio controlado e um resolvedor falso que conta as chamadas
- [x] 2.3 Sem o real: o aviso uma vez por identidade (a notificação do desktop, quando ligada, e o registro do daemon), até o real voltar; o `sandmux doctor` avisa da referência sem o `gcloud` no host. Teste em `tests/cloud.rs` com o `gcloud` falso saindo com o erro de login, e teste do `doctor` sem `gcloud` no `PATH`

## 3. A aba

- [x] 3.1 Em `sandmux-policy`, os arquivos só da execução: uma configuração do `gcloud` por identidade (conta, projeto e `auth/access_token_file` com o falso), o `active_config` na padrão, e o `application_default_credentials.json` `authorized_user` com o refresh falso da padrão; nada com o cofre trancado ou sem identidade. Testes de unidade dos arquivos gerados, e teste em `tests/cloud.rs` que lê os arquivos dentro da aba e confere que os guardados no HOME sintético não mudam
- [x] 3.2 `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` na lista de variáveis da CA (`CA_VARS`). Atualizar o teste da composição que confere as variáveis

## 4. O gateway

- [x] 4.1 `*.googleapis.com` na lista de decifração de toda aba com uma identidade de GCP (o domínio dono da identidade). Teste em `tests/cloud.rs`: a política da aba de shell tem o domínio com a identidade, e não tem sem ela
- [x] 4.2 A troca do falso de acesso pelo real atual no `Authorization`, com a validade conferida antes de cada pedido da conexão, e o nome `gcp:<nome>` na linha `https`. Teste em `tests/cloud.rs` com o servidor local de eco: o destino recebe o real do `gcloud` falso, o sandbox só vê o falso, o registro tem o nome e não tem o real; com o cofre trancado, sai o falso
- [x] 4.3 A resposta local do `POST /token` com o refresh falso de uma identidade (antes da captura do `agy`): o falso de acesso, `Bearer` e o `expires_in` limitado pela validade do real; `invalid_grant` com a descrição quando falta o real; nenhum pedido ao destino. Teste em `tests/cloud.rs`: o servidor local não recebe o pedido de token, e a resposta tem o falso; e teste de unidade da resposta de `invalid_grant`

## 5. TUI

- [x] 5.1 A seção Cofre e o grafo mostram as identidades de cloud no domínio `*.googleapis.com`, com a marca própria e os detalhes (provedor, fonte, conta, padrão, falso de acesso); `x` apaga, e `v` e `d` recusam com o comando certo. Testes em `tests/tui.rs`, que conferem que nenhum valor real aparece na tela

## 6. Documentação e fechamento

- [x] 6.1 `docs/cloud.md` e `docs/en/cloud.md`: o modelo (o falso no sandbox, o real no gateway), o comando, as fontes, o que funciona, os limites (gRPC, `gsutil` e `bq` antigos, tokens de identidade, `gcloud auth login` dentro da aba) e o aviso do IAM (sem criação de chaves, sem Secret Manager). Também o README e o `docs/vault.md`, nos dois idiomas, com o teste de docs atualizado
- [x] 6.2 Rodar o `./scripts/check.sh` inteiro, sob o vigia, e conferir que termina verde e sem processo de teste vivo
- [x] 6.3 Atualizar o roadmap: a situação do change 93, as decisões na seção 2 e os limites na seção 6, e a verificação à mão com um `gcloud` real

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova (a ordem do usuário para cada change).
- Verificação à mão pelo usuário, numa máquina com o `gcloud` logado: `gcloud storage ls` e um script Python com `google.auth.default()` dentro de uma aba, com a identidade por referência e por cópia de chave.
