## Context

A captura do login do Claude Code (change 43) é a base:

- `sandmux_spec::net::CAPTURE_ENDPOINTS` lista host e caminho, e `CAPTURE_FIELDS` dá o papel de cada campo.
- `mitm.rs::capture_exchange` lê o pedido e a resposta inteiros, troca os falsos do corpo por reais (`body_with_reals`, substituição de texto) e manda os tokens ao daemon por `Msg::VaultCapture { tag, items }`. Depois troca, na resposta, os reais pelos falsos devolvidos.
- No daemon, `VaultState::capture(role, real)` guarda tudo como `claude:<role>`, com domínios fixos e um falso estável (`capture_fake`, que mantém o prefixo até o último `-` dos 15 primeiros caracteres).
- A captura é escolhida só pelo host e pelo caminho (`is_capture`), sem olhar o harness da aba. Isso basta para a Anthropic, mas não para o Google: `oauth2.googleapis.com/token` serve a qualquer cliente do Google.
- A aba do Claude Code decifra `CLAUDE_HOSTS` (`config.rs`, por `req.harness == "claude"`).
- A lista de decifração de qualquer aba inclui os domínios donos de todos os segredos do cofre.

Do `agy` 1.5.3, observado no binário e no arquivo do host (só os nomes dos campos foram lidos):

- O token fica em `~/.gemini/antigravity-cli/antigravity-oauth-token`, em JSON: `token.{access_token, token_type, refresh_token, expiry, expires_in}`, `auth_method` e `id_token_email`.
- A renovação é a do pacote `oauth2` do Go: `POST oauth2.googleapis.com/token` em formulário, com `refresh_token=1%2F%2F…`.
- O modelo é servido por `cloudcode-pa.googleapis.com` ou `daily-cloudcode-pa.googleapis.com`. O binário também usa `www.googleapis.com` (`userinfo`, `tokeninfo`), `play.googleapis.com` (telemetria) e o Cloud Resource Manager.
- O binário tem o modo de colar o código (`SwitchLoginToAuthCode`, "Then paste the authorization code from the page").

## Goals / Non-Goals

**Goals:**
- Nenhum token real do Google no HOME sintético nem na memória do processo confinado, nos três caminhos: o login importado do host, o login feito no sandbox e um login antigo já gravado no HOME sintético.
- A captura passa a ter um harness dono, sem mudar o comportamento do Claude Code.

**Non-Goals:**
- O login por Application Default Credentials (`AGY_ADC_AUTH`) e o gateway de empresa (`AGY_LLM_GATEWAY_URL`) ficam em "Depois da v1".
- O retorno automático em `localhost` do login não é encaminhado: o código é colado, como no Claude Code.
- O `id_token`, quando vem na resposta, segue como está (veja os riscos).
- Os tokens de login de MCPs e plugins do `agy` não fazem parte deste change.

## Decisions

### Captura com harness dono
`CAPTURE_ENDPOINTS` passa a ter `(harness, host, caminho)`, e `CAPTURE_FIELDS` ganha papéis por harness. Ficam assim:

- `claude`: `platform.claude.com /v1/oauth/token` e `api.anthropic.com /api/oauth/claude_cli/create_api_key`;
- `agy`: `oauth2.googleapis.com /token`.

O gateway sabe o harness da aba por um campo novo em `VaultAccess` (o spec do lançamento já leva o `VaultAccess` ao supervisor) e só captura os endpoints desse harness. `Msg::VaultCapture` ganha `harness`, e o daemon nomeia `<harness>:<papel>`, com os domínios da tabela.

- **Alternativa descartada:** descobrir o harness no daemon pela etiqueta do lançamento. Essa saída não cobre o `run` direto sem daemon de sessão e não resolve o filtro no gateway.
- **Protocolo:** passa a 36.

### Domínios e falsos do Google
- **Domínios:**
  - `agy:access`: `*.googleapis.com`, por decisão do usuário. O padrão casa só com subdomínios, e todos os hosts usados estão neles.
  - `agy:refresh`: `oauth2.googleapis.com`.
- **Falsos:**
  - O prefixo deixa de ser calculado por `-` para os papéis do Google: `ya29.` para o access e `1//` para o refresh, seguidos de `sandmux` e letras e números.
  - O falso não tem caracteres que precisem de codificação, além do `//` do prefixo.
  - `capture_fake` recebe o prefixo pela tabela.

### Corpo em formulário
`body_with_reals` passa a trocar também a forma percentual do falso (cada byte fora de `[A-Za-z0-9._~-]` como `%XX` maiúsculo, como o `url.Values.Encode` do Go) pela forma percentual do real. Corpos JSON continuam com a troca direta. A forma minúscula (`%2f`) também é aceita, porque o custo é baixo.

### Segredos de login só na aba do harness
Um segredo cujo nome tem o prefixo de um harness da tabela de captura (`claude:`, `agy:`) é um segredo de login. O daemon filtra a lista de troca de cada aba (`Msg::VaultInject`, que já leva a etiqueta) e a lista de decifração compilada por aba. Os segredos de login de outro harness ficam fora das duas.

Para o `run` direto, o filtro também é feito no gateway: ele conhece o harness pelo `VaultAccess` e descarta os segredos de login de outro harness. Os dois filtros juntos evitam decifrar todo `*.googleapis.com` num shell.

**Hosts decifrados:** a aba do `agy` decifra `*.googleapis.com` sempre, como a do Claude Code decifra `CLAUDE_HOSTS`. Assim o primeiro login, ainda sem segredo no cofre, já passa pela captura.

### Login do host na importação
`sandmux import agy` (e a importação da primeira execução) roda fora do sandbox e já fala com o daemon. Com o arquivo do token no host:

1. Lê `token.access_token` e `token.refresh_token`.
2. Manda os dois ao daemon por `VaultCapture` com `harness = "agy"`, que devolve os falsos estáveis.
3. Grava no HOME sintético o mesmo JSON com os falsos no lugar.

O arquivo gravado mantém o `expiry` do host. Se o access já expirou, o `agy` renova pelo gateway.

- **Ordem:** o cofre é escrito antes do arquivo. Se o cofre recusar, nada é gravado, e o relatório diz para abrir o cofre e rodar `sandmux import agy --overwrite`.
- **Reimportação sem `--overwrite`:** o arquivo existente fica (a regra de sempre). Com `--overwrite`, o cofre recebe os reais novos e os falsos continuam os mesmos.
- **Alternativa descartada:** pedir ao usuário um login novo dentro do sandmux. Ele continua possível (a captura), mas o usuário escolheu os dois caminhos.

### Login antigo no HOME sintético
O vigia de chaves reais (`secretwatch.rs`) ganha uma regra para `.gemini/antigravity-cli/antigravity-oauth-token` do HOME do `agy`. Um valor em `token.access_token` ou `token.refresh_token` sem o prefixo `…sandmux` é movido pelo mesmo `capture` do daemon (`agy:*`, com os domínios da tabela), e o arquivo é regravado com os falsos. A regravação segue a regra existente: só se o arquivo não mudou desde a leitura. O `doctor` mostra o arquivo como chave real, e o `--fix` usa a mesma regra.

### Catálogo de inferência
O catálogo ganha `oauth2.googleapis.com`, `cloudcode-pa.googleapis.com` e `daily-cloudcode-pa.googleapis.com`, e `platform.claude.com` passa a constar no spec (o código já o tem). A regra "só os provedores com segredo" passa a casar o domínio dono como padrão: `*.googleapis.com` libera os hosts do Google do catálogo.

Antes de fechar a lista, o registro de rede da aba mostra os hosts que o `agy` real usa. Se aparecer outro host necessário ao modelo, ele entra no catálogo, e o spec é atualizado.

## Risks / Trade-offs

- **[Escopos amplos]** O token do `agy` pede `cloud-platform` e Drive, e o access real vai a todo `*.googleapis.com`. O agente pode usar o falso para agir no Google Cloud e no Drive do usuário pelo gateway. → Decisão do usuário, registrada no roadmap como risco aceito. O token não sai do gateway, e cada uso fica no registro com o nome `agy:access`.
- **[`id_token` real]** O `id_token`, quando vem na resposta, chega ao sandbox. É um JWT de cerca de uma hora, com o público do cliente do `agy`. → Risco aceito: trocá-lo por um falso quebraria a leitura do e-mail pelo `agy`, e ele não é aceito como `Bearer` pelas APIs `googleapis.com` comuns.
- **[Refresh compartilhado com o host]** Se o Google passar a girar o refresh, ou se o usuário sair da conta no host (revogação), o login do sandbox para de renovar. → A captura guarda um refresh novo quando ele vem na resposta. Para uma revogação, a saída é `sandmux import agy --overwrite` ou o login dentro do sandmux.
- **[Formato do arquivo]** Uma versão nova do `agy` pode mudar o JSON do token. → A importação só troca os dois campos conhecidos. Sem eles, segue sem o login e avisa, e a captura continua cobrindo o login feito no sandbox.
- **[Decifrar `*.googleapis.com` na aba do `agy`]** Um cliente com certificado fixo dentro da aba quebraria. → Só na aba do `agy`. O próprio `agy` confia na CA do lançamento, conferido no change 63 com `generativelanguage.googleapis.com`.

## Migration Plan

- Protocolo 36: depois de atualizar, `sandmux daemon restart` (nas notas da release).
- Logins antigos no HOME sintético são movidos sozinhos pelo vigia quando o cofre abre.
- Os segredos `claude:*` existentes passam a valer só nas abas do Claude Code, sem mudança para o uso normal.
- Rollback: a versão anterior lê o cofre com `agy:*` como segredos comuns, com os domínios guardados.

## Open Questions

- A lista exata de hosts que o `agy` precisa no modo `inferencia`. É conferida na implementação pelo registro de rede, e o resultado ajusta o catálogo e o cenário do spec.
- Como o `agy` 1.5.3 oferece o modo de colar o código no Linux sem navegador. É conferido com o usuário no login real dentro do sandmux.
