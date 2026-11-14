# Tasks

## 1. CA e confiança

- [x] 1.1 Gerar a CA por lançamento (ECDSA P-256) e os certificados por domínio com cache; verificar com testes de unidade de que o certificado do domínio é aceito por um cliente `rustls` que confia só na CA
- [x] 1.2 Levar a CA na especificação (`network.tls`), montar no sandbox o pacote de CAs do sistema mais o da CA e definir as variáveis; verificar com teste de unidade da composição e teste de integração de que o `curl` do sandbox confia no pacote

## 2. Decifração

- [x] 2.1 Acrescentar `decrypt` às camadas e a `--decrypt`, e a lista de decifração à política; o daemon soma os domínios dos segredos do cofre desbloqueado e recompila no desbloqueio e no trancamento; verificar com testes de unidade da leitura e da soma
- [x] 2.2 Fazer o proxy HTTP/1.1 no gateway (fluxo com os bytes já lidos à frente, terminação com o certificado do SNI, cliente até o destino com as raízes do sistema, bloqueio por certificado inválido, ponto de reescrita) e as linhas `https` com método, caminho e status; verificar com testes de integração pelo `run` contra um servidor HTTPS local com CA de teste: pedido decifrado registrado sem a query, domínio fora da lista com o certificado do servidor, e destino com certificado inválido bloqueado
- [x] 2.3 Mostrar o método, o caminho e o status no painel e no `sandmux net`; verificar com teste de CLI

## 3. Documentação e verificação

- [x] 3.1 Atualizar `docs/network.md`, `docs/config.md`, `docs/vault.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas
- [x] 3.3 Verificar à mão o opencode real com `openrouter.ai` decifrado (o runtime aceita a CA e o pedido aparece no registro com o método e o caminho)
