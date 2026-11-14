# Tasks

## 1. Valores falsos

- [x] 1.1 Gerar o valor falso no `VaultAdd` (ou usar o da importação), completar os segredos sem falso no desbloqueio e mostrar o falso no `sandmux vault list`; verificar com teste de unidade e de CLI

## 2. Troca

- [x] 2.1 Acrescentar `VaultInject` e `Injections` ao protocolo (11) e atender no daemon só com os segredos com domínio e só com o cofre desbloqueado; verificar com teste de unidade de serialização
- [x] 2.2 No gateway, pedir as trocas ao daemon a cada conexão decifrada (especificação com o socket e as pastas da saudação), filtrar pelo SNI e trocar o falso pelo real nos cabeçalhos, com o nome do segredo na linha `https`; verificar com teste de unidade da troca (dono e outro domínio) e teste de PTY de uma sessão que manda o falso a um servidor HTTPS local e o servidor recebe o real, e de que o cofre trancado deixa o falso

## 3. Presets e modo só inferência

- [x] 3.1 Criar a tabela de provedores e o ambiente falso das sessões do opencode e do hermes, com o `OPENCODE_CONFIG_CONTENT` e a precedência sobre o `pass-env`; verificar com testes de unidade da composição
- [x] 3.2 Restringir o modo só inferência aos provedores com segredo; verificar com teste de unidade

## 4. Documentação e verificação

- [x] 4.1 Atualizar `docs/vault.md`, `docs/network.md` e o README; verificar com o teste de documentação
- [x] 4.2 Rodar `./scripts/check.sh` sem falhas
- [x] 4.3 Verificar à mão uma sessão real do opencode sem `--pass-env`, com a chave do OpenRouter só no cofre: o modelo responde, o sandbox só tem o falso, e o registro mostra o segredo usado
