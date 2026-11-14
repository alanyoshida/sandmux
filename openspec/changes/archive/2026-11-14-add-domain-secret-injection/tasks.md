# Tasks

## 1. Ferramentas

- [x] 1.1 Acrescentar `tools` ao segredo, ao `VaultAdd`, ao `VaultEntry` e ao `vault list` (protocolo 12), com `--tool`, a validação e a derivação pelos domínios conhecidos; verificar com testes de unidade e de CLI
- [x] 1.2 Compor o valor falso de cada ferramenta (`GH_TOKEN`, ajudante do git por `GIT_CONFIG_*`, `~/.npmrc` e `~/.docker/config.json` só da execução, `PIP_EXTRA_INDEX_URL`, `CARGO_REGISTRY_TOKEN`) a partir da lista que o daemon passa; verificar com testes de unidade da composição

## 2. Troca

- [x] 2.1 Trocar o falso dentro do `Authorization: Basic`; verificar com teste de unidade
- [x] 2.2 Dar ao gateway do Docker a CA, a decifração dos domínios dos segredos `docker` e o acesso ao cofre, e montar o pacote de CAs na visão do Docker; verificar com teste de unidade da composição da visão
- [x] 2.3 Teste de PTY: uma sessão com um segredo `git` de `host.sandmux.internal` faz um pedido com o ajudante de credenciais do git (`git credential fill`) e o `curl` com o `Basic` resultante chega ao servidor HTTPS local com o valor real

## 3. Documentação e verificação

- [x] 3.1 Atualizar `docs/vault.md`, `docs/docker.md` e o README; verificar com o teste de documentação
- [x] 3.2 Rodar `./scripts/check.sh` sem falhas
