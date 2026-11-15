## Context

Os segredos do cofre têm um falso no sandbox, e o gateway troca o falso pelo real nos cabeçalhos e no caminho dos pedidos ao domínio dono, nas conexões decifradas (change 12). O OAuth da assinatura do Claude Code tem duas partes:
- um token de acesso, curto, usado em `api.anthropic.com`;
- um token de renovação, que gira a cada uso em `https://platform.claude.com/v1/oauth/token` (o `TOKEN_URL` no binário 2.1.325).

O login (`/login`) troca um código pelo par de tokens no mesmo endpoint. O login pela "Console" cria uma chave de API em `api.anthropic.com/api/oauth/claude_cli/create_api_key`.

## Decisions

- **Login dentro do sandmux, sem importar o token:** o token de renovação gira, então compartilhá-lo com o Claude Code do host derrubaria um dos dois. O login é feito uma vez na aba; como o callback em `localhost` não alcança o sandbox, o Claude Code mostra o endereço e aceita o código colado.
- **Captura no proxy decifrado (`mitm::serve_http`):** para os dois endpoints (host e caminho exatos, `POST`):
  - **Pedido:** é lido inteiro (até 64 KiB). Os falsos dos segredos do host são trocados pelos reais no texto do corpo, e o `content-length` é refeito. O `accept-encoding` sai, para a resposta vir sem compressão.
  - **Resposta:** é lida inteira (até 1 MiB). Num 2xx com JSON, os campos `access_token`, `refresh_token` e `raw_key` (no primeiro nível) vão ao daemon por `VaultCapture`, que devolve os falsos. A resposta sai com os falsos e o `content-length` refeito.
  - **Daemon recusa:** com o cofre trancado ou sem cofre, o sandbox recebe um `502` com o motivo, e nada do corpo original.
- **Daemon:** `VaultState::capture(role, real)`:
  - grava `claude:<role>` com o domínio de cada papel: `access` vale para `api.anthropic.com`, `claude.ai` e `platform.claude.com`; `refresh`, para `platform.claude.com`; `api-key`, para `api.anthropic.com`;
  - o falso é estável: o segredo que já existe só troca o valor, e um novo ganha um falso com o prefixo do real até o último `-` dos 15 primeiros caracteres (como `sk-ant-oat01-`), mais `sandmux` e letras aleatórias.
- **Decifração e rede:** o `compose` acrescenta `platform.claude.com` e `api.anthropic.com` à decifração das abas `claude`. `platform.claude.com` entra no catálogo de inferência.
- **Preset:**
  - **Comando:** o `claude` é o alvo real do link `~/.local/bin/claude`;
  - **Montagens:** `~/.local/share/claude` em somente leitura;
  - **Ambiente:** `DISABLE_AUTOUPDATER=1`. As chaves falsas dos provedores não entram (`FAKE_KEY_HARNESSES`): com uma `ANTHROPIC_API_KEY`, o Claude Code trocaria a assinatura pela cobrança da API.

## Risks / Trade-offs

- **[O formato da resposta muda]** Os campos são os do OAuth (RFC 6749) e o `raw_key` da criação de chave; um campo novo com token passaria. Os nomes ficam numa lista, fácil de acrescentar.
- **[Versões novas trocam o endpoint]** A URL vem do binário; o teste de unidade fixa o comportamento, e a documentação diz onde olhar.
