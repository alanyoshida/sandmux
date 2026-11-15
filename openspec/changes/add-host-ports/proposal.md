## Why

O primeiro uso real da release `v0.1.0` deu `ECONNREFUSED` no opencode. A configuração dele, importada do host, aponta o modelo para o LM Studio em `http://127.0.0.1:8888/v1` e um MCP para `http://127.0.0.1:9000/mcp`. Dentro do sandbox, o `127.0.0.1` é o do próprio sandbox, onde nada escuta. Hoje o caminho até um serviço do host é `host.sandmux.internal`, o que obriga a editar a configuração importada, e ela deixa de ser igual à do host. Servidores locais de modelo (LM Studio, Ollama, llama.cpp) e MCPs locais são comuns, e isso precisa funcionar sem editar nada.

## What Changes

- **Portas do host (decisão do usuário):** a chave nova `host-ports`, uma lista de portas (global, perfil, projeto) e `--host-port` no `run`. Para cada porta, o `127.0.0.1:porta` de dentro do sandbox leva ao `127.0.0.1:porta` do host. A configuração do harness fica igual à do host.
- **Pelo gateway:** o supervisor escuta a porta dentro do namespace de rede do sandbox, pelo auxiliar que já abre conexões lá, e repassa cada conexão ao host. Cada conexão vai para o registro de rede (`tcp`, `host.sandmux.internal`). Uma regra de bloqueio para `host.sandmux.internal` vence.
- **Ao vivo:** a lista é relida como a das portas de entrada. Uma porta acrescentada com a sessão viva passa a valer em até 2 segundos, e uma tirada fecha.
- **Sem eco:** a porta do host não é publicada de volta como porta de entrada do sandbox.
- **Importação:** quando a configuração importada cita `127.0.0.1`, `localhost` ou `[::1]` com porta, a importação avisa e mostra o comando que acrescenta as portas.
- **Aba de chat:** não recebe portas do host. Ela tem os provedores próprios.

## Capabilities

### New Capabilities
- `host-ports`: o encaminhamento de portas do host para o `127.0.0.1` do sandbox, a configuração, o registro, a mudança ao vivo e o aviso da importação.

### Modified Capabilities
- `config-profiles`: a chave `host-ports` e `--host-port`.

## Impact

- **`sandmux-spec`:** `NetPolicy.host_ports`.
- **`sandmux-launcher`:** o auxiliar de conexão também abre ouvintes no namespace do sandbox.
- **`sandmux-net`:** `hostports.rs` novo e a exclusão dessas portas na publicação de entrada.
- **`sandmux-policy`:** a chave na configuração e na edição, a política efetiva e o confinamento do chat.
- **`sandmux-import`:** o aviso das URLs locais.
- **`sandmux`:** `--host-port` e `LaunchOptions.host_ports`.
- **`sandmux-proto`:** protocolo 22.
- **Documentação:** `docs/network.md`, `docs/config.md` e o README.
