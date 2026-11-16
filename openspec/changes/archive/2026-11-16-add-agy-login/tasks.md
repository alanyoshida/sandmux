## 1. Captura com harness dono

- [x] 1.1 `sandmux-spec`: tabela de captura com o harness dono (`claude` e `agy`), papéis, domínios e prefixos dos falsos por harness, e o harness na especificação de rede do lançamento (`NetworkSpec`). Verificação: testes de unidade de `is_capture` (o endpoint do Google só casa com o harness `agy`)
- [x] 1.2 `sandmux-proto`: `VaultCapture` com o harness e protocolo 36; atualizar o número em `docs/sessions.md` e no par em inglês. Verificação: o teste dos docs passa
- [x] 1.3 `sandmux-net`: a captura só para o harness da aba; `body_with_reals` com a forma percentual do falso; descartar no gateway os segredos de login de outro harness. Verificação: testes de unidade em `mitm.rs` (renovação em formulário com `1%2F%2F…`, endpoint do Google numa aba de shell passa sem captura, Claude sem mudança)
- [x] 1.4 Daemon: `VaultState::capture` por harness (`agy:access` em `*.googleapis.com`, `agy:refresh` em `oauth2.googleapis.com`, falsos `ya29.sandmux…` e `1//sandmux…`, estáveis); segredos de login filtrados pelo harness da aba no gateway (pedido dos segredos) e na lista de decifração do daemon. Verificação: testes de unidade em `vault.rs` e um teste de integração em que um shell não decifra `*.googleapis.com` com `agy:access` no cofre
- [x] 1.5 `sandmux-policy`: a aba do `agy` decifra `*.googleapis.com`; o catálogo de inferência com `oauth2.googleapis.com`, `aicode.googleapis.com`, `cloudcode-pa.googleapis.com` e `daily-cloudcode-pa.googleapis.com`; os provedores com segredo casam o domínio dono como padrão. Verificação: testes de unidade da política (modo `inferencia` com `agy:*` no cofre)

## 2. Login do host e login antigo

- [x] 2.1 Importação do `agy`: o login do host levado ao cofre pelo daemon e o arquivo do token gravado com os falsos; sem cofre, segue sem o login e avisa; `--overwrite` troca os reais e mantém os falsos; o relatório diz se o login foi levado. Verificação: testes de integração com um `agy` falso e um arquivo de token de teste (cofre aberto, trancado e `--overwrite`), e nenhum token real no HOME sintético
- [x] 2.2 Vigia de chaves reais e `doctor`: o arquivo do token do `agy` com valores reais vai ao cofre como `agy:*` e é regravado com os falsos; com o cofre trancado, espera. Verificação: teste de integração com um token real de teste gravado no HOME sintético

## 3. Ponta a ponta e documentação

- [x] 3.1 Captura do Google com um servidor de token local no gateway (`mitm.rs`: renovação em formulário com `1%2F%2F…`, o real no corpo, o falso na resposta, o harness no pedido ao daemon; o endpoint numa aba de shell passa sem captura) e testes de integração (`tests/agy_login.rs`: login do host com o cofre aberto, trancado e `--overwrite`; login antigo levado pelo vigia; `*.googleapis.com` decifrado só na aba do `agy`). Um teste pelo DNS real de `oauth2.googleapis.com` precisaria de um gancho só de teste no resolvedor; o caminho completo fica na verificação à mão do usuário
- [x] 3.2 Conferido com o `agy` 1.5.3 real num HOME temporário, sem tokens (2026-11-16): antes do login só `play.googleapis.com` (decifrado sem erro); a tela aceita o código colado (`redirect_uri` em `antigravity.google/oauth-callback`); o código vai a `POST oauth2.googleapis.com/token`; escopos sem Drive. O catálogo ganhou `aicode.googleapis.com`, e os artefatos foram ajustados
- [x] 3.3 `docs/agy.md` (login do host, login colando o código, escopos e destinos do token, cofre trancado), `docs/vault.md`, `docs/network.md` (catálogo) e os pares em inglês; roadmap (decisões, risco aceito dos escopos, verificação à mão do login real). Verificação: o teste dos docs passa
- [x] 3.4 Suíte completa (`scripts/check.sh`) sob o vigia, com `CARGO_BUILD_JOBS=8`

## Workflow follow-up

- Verificação à mão do usuário: `sandmux run agy` já logado com o login do host importado, uma resposta do modelo, e `sandmux vault list` com `agy:access` e `agy:refresh`.
- Arquivar, commit com a varredura da chave, PR e merge; perguntar sobre a release.
