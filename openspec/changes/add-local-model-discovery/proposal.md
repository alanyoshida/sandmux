## Why

O opencode só mostra, num provedor próprio compatível com a OpenAI, os modelos escritos na configuração. Com o Unsloth Studio, o usuário tinha 14 modelos no servidor, mas o opencode mostrava os dois gemma da configuração, e não o Bonsai que ele estava rodando. O usuário pediu (2026-11-14) que o sandmux pergunte ao servidor local quais modelos existem e os cadastre sozinho.

## What Changes

- **Descoberta dos modelos locais:** ao lançar uma sessão do opencode pelo daemon, o sandmux lê os provedores da configuração do opencode (a global do HOME sintético e a do projeto) e escolhe os que apontam para um servidor local:
  - `baseURL` em `127.0.0.1`, `localhost` ou `[::1]`;
  - ou `lmstudio` e `ollama` sem `baseURL`, nas portas padrão 1234 e 11434.

  Para cada um cuja porta está em `host-ports`, o daemon pede `GET <baseURL>/models` ao host, com tempo curto.
- **Chave:** se o cabeçalho `Authorization` ou o `apiKey` do provedor tem um valor falso do cofre, a consulta usa o valor real, fora do sandbox. A chave real continua fora do sandbox.
- **Cadastro em tempo de execução:** os modelos encontrados entram no `OPENCODE_CONFIG_CONTENT` da sessão, em `provider.<id>.models`, somados aos da configuração. Nenhum arquivo é editado, e a lista é refeita a cada lançamento.
- **Servidor fora do ar** ou resposta inválida: o provedor fica como está, sem erro.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `harness-presets`: o preset do opencode cadastra os modelos dos servidores locais.

## Impact

- **`sandmux-policy`:**
  - `opencode_models.rs` novo: os provedores locais e o JSON dos modelos;
  - `compose.rs`: a mescla no `OPENCODE_CONFIG_CONTENT`.
- **`sandmux`:**
  - `daemon/session.rs` e `launch.rs`: a consulta com a chave do cofre;
  - `discover.rs` novo: o `GET` HTTP com tempo.
- **Documentação:** `docs/network.md` (portas do host) e o README.
