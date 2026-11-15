## Context

- **`OPENCODE_CONFIG_CONTENT`:** o preset do opencode já monta essa variável (a chave do provedor pelo `--pass-env` e os MCPs desabilitados), e o opencode a mescla por cima dos arquivos.
- **Portas do host** (change 27): levam o `127.0.0.1:porta` de dentro ao do host.
- **Cofre:** guarda os segredos com o valor falso, e o daemon tem os reais com o cofre desbloqueado.
- **Configuração do opencode:** é JSON ou JSONC. O `mcp::strip_jsonc` já tira os comentários.

## Goals / Non-Goals

**Goals:**
- Os modelos de um servidor local aparecem no opencode sem cadastro à mão e acompanham o servidor a cada lançamento.

**Non-Goals:**
- Provedores remotos, que o opencode já conhece pelo catálogo dele.
- O hermes, que consulta os modelos sozinho, de dentro do sandbox, e passa pela troca do change 30.
- Atualizar a lista com a sessão viva. Vale no próximo lançamento (Ctrl+b r).
- O `run` direto, sem daemon: ele não tem o cofre.

## Decisions

### Quais provedores

`opencode_models::local_providers(home sintético, pasta)` lê `opencode.json` e `opencode.jsonc` das duas origens e, de cada entrada de `provider`, extrai:

- a URL base: `options.baseURL`, ou o padrão de `lmstudio` (`http://127.0.0.1:1234/v1`) e de `ollama` (`http://127.0.0.1:11434/v1`);
- o cabeçalho `Authorization` de `options.headers`;
- o `options.apiKey`.

Ficam só as URLs `http://` com host local. O daemon ainda exige a porta em `host-ports`: sem ela, o modelo apareceria mas não funcionaria no sandbox.

### A consulta

`sandmux::discover::models(url, auth)` faz um `GET` HTTP/1.1 com `Connection: close`, com tempo de 1,5 s para conectar e para ler, e corpo de no máximo 1 MiB. Ele aceita `Content-Length` e `chunked` e lê os ids em `data[].id`. Os provedores são consultados em paralelo, cada um numa thread.

**Autorização:** o daemon troca, no valor do cabeçalho ou do `apiKey` (que vira `Bearer`), cada falso do cofre pelo real. Os segredos são os de domínio `host.sandmux.internal`, os mesmos que o gateway troca nessas portas.

### A mescla

`RunOptions.opencode_models: Vec<(String, Vec<String>)>`. O `compose` soma ao `OPENCODE_CONFIG_CONTENT` o bloco `{"provider": {"<id>": {"models": {"<modelo>": {}}}}}`, mesclando com o que já está lá (o `apiKey` do `--pass-env`). Um modelo sem nome usa o id na lista do opencode. Os modelos da configuração continuam, com os nomes que o usuário deu.

## Risks / Trade-offs

- **[O lançamento fica mais lento]** Até 1,5 s por provedor local, em paralelo. Um servidor fora do ar recusa a conexão na hora.
- **[O servidor lista modelos que não carrega]** É a lista do próprio servidor. O usuário escolhe no opencode, como no servidor.
