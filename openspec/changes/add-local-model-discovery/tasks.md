## 1. Descoberta

- [x] 1.1 `opencode_models::local_providers` (JSON e JSONC, global e projeto, padrões do LM Studio e do Ollama, só URLs locais) e o JSON dos modelos, com testes
- [x] 1.2 `discover::models`: `GET` com tempo, `Content-Length` e `chunked`, ids de `data[].id`, com teste contra um servidor local
- [x] 1.3 No daemon: provedores com a porta em `host-ports`, autorização com o real do cofre, consultas em paralelo, `RunOptions.opencode_models` e a mescla no `OPENCODE_CONFIG_CONTENT`

## 2. Verificação e documentação

- [x] 2.1 Teste de integração: servidor local com `/v1/models` e chave, opencode falso que mostra o `OPENCODE_CONFIG_CONTENT`; o modelo aparece, e a chave real não entra no sandbox
- [x] 2.2 `docs/network.md`, README
- [x] 2.4 Tempo de resposta de 6 s (o Unsloth Studio leva 1,6 s para listar; com 1,5 s a lista vinha vazia) e a aba do opencode com provedor local de chave do cofre aguardando o cofre na restauração, com teste
- [ ] 2.3 Verificação do usuário com o Unsloth Studio
