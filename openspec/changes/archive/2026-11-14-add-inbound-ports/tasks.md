# Tasks

## 1. Regras

- [x] 1.1 `PortRule` e `NetPolicy.ports`; a chave `ports`, `--port` e a compilação na política, com o aviso de exposição; verificar com testes de unidade

## 2. Publicação

- [x] 2.1 O vigia de portas segue a allowlist relida e o endereço de cada regra, e registra as conexões de entrada; verificar com testes de unidade e um teste de integração (porta fora da allowlist não publicada; porta com `@0.0.0.0` aceita por outro endereço da máquina; linha `inbound` no registro; mudança ao vivo)
- [x] 2.2 O endereço na lista (`Published`, `PortInfo`, protocolo 18), no `stats` e no painel; verificar com teste de CLI

## 3. Documentação e verificação

- [x] 3.1 Atualizar `docs/network.md`, `docs/config.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas, sob o vigia de memória
