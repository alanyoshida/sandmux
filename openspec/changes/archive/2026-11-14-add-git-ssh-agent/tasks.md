# Tasks

## 1. Cofre e configuração

- [x] 1.1 Acrescentar o tipo `ssh` ao segredo, `sandmux vault add-ssh` (leitura da chave e da passphrase no cliente, `VaultAddSsh`, protocolo 13) e o tipo e a impressão digital no `vault list`, deixando os segredos `ssh` fora da troca, das ferramentas e da decifração; verificar com testes de unidade e de CLI
- [x] 1.2 Acrescentar a chave `ssh-keys` e o `--ssh-key` do `run`, com o aviso de nome desconhecido; verificar com testes de unidade da configuração

## 2. Agente

- [x] 2.1 Implementar o protocolo do agente (listar e assinar com Ed25519, ECDSA e RSA com `rsa-sha2-256`/`512`; `FAILURE` no resto); verificar com testes de unidade que conferem as assinaturas
- [x] 2.2 Abrir o socket do agente por lançamento no daemon, montá-lo em `/run/sandmux/ssh-agent.sock` com `SSH_AUTH_SOCK`, e gravar a linha `ssh-sign` no registro; verificar com teste de PTY (`ssh-add -L` lista só as chaves escolhidas; `ssh-keygen -Y sign` pelo agente produz uma assinatura válida; a linha aparece no registro)
- [x] 2.3 Compor o `~/.ssh/known_hosts` da sessão com as linhas do host para os servidores (nomes simples, com porta, listas e em hash); verificar com testes de unidade

## 3. Rede

- [x] 3.1 Acrescentar `ssh` à política, somado pelo daemon, e reconhecer o SSH no gateway (bytes do cliente e do servidor), bloqueando fora dos servidores; verificar com testes de unidade e um teste de PTY com um servidor local que fala `SSH-2.0`

## 4. Documentação e verificação

- [x] 4.1 Atualizar `docs/vault.md`, `docs/network.md`, `docs/config.md` e o README; verificar com o teste de documentação
- [x] 4.2 Rodar `./scripts/check.sh` sem falhas e verificar à mão um `ssh -T git@github.com` dentro do sandbox com uma chave de teste (a recusa da chave pelo GitHub prova que o agente ofereceu a chave e que a conexão passou)
