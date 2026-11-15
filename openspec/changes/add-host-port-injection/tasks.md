## 1. Proxy e encaminhamento

- [x] 1.1 `mitm::serve_http` genérico e o protocolo no `Ctx` e no `Log::write_http`
- [x] 1.2 `hostports`: segredos por conexão, espiada do HTTP com tempo, repasse pelo proxy ou cópia direta

## 2. Cofre

- [x] 2.1 `sandmux vault import` pergunta o domínio quando a chave não indica um

## 3. Verificação e documentação

- [x] 3.1 Testes: falso trocado pelo real num servidor HTTP do host pela porta do host, linha `http` com o segredo, sem segredo a cópia direta; `vault import` com a pergunta do domínio
- [x] 3.2 `docs/network.md`, `docs/vault.md`; a chave real fora do `auth.json` do sandbox
- [ ] 3.3 O usuário guarda a chave do Unsloth no cofre (`sandmux vault import`, domínio `host.sandmux.internal`) e confirma o Bonsai pelo Unsloth no opencode
