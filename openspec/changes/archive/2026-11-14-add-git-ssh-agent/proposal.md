# Proposal

## Why

O `git push` por SSH precisa da chave privada do usuário, e o sandbox esconde o `~/.ssh` do host. Hoje, para usar o git por SSH, o usuário teria de montar a chave (`--ro ~/.ssh`), e a chave real ficaria ao alcance do agente. O roadmap decidiu um ssh-agent controlado: as chaves escolhidas por perfil, uso livre com registro de cada assinatura, e SSH de saída restrito aos servidores git cadastrados. O usuário decidiu (2026-11-14) que as chaves ficam no cofre, e o daemon assina sem depender do ssh-agent do host.

## What Changes

- **Chaves SSH no cofre:**
  - `sandmux vault add-ssh <nome> --file <chave> --server <domínio>...` importa uma chave privada OpenSSH (Ed25519, ECDSA ou RSA) para o cofre, pedindo a passphrase uma vez quando a chave é cifrada;
  - o cofre guarda a chave sem passphrase, cifrada como os demais segredos, e os servidores dela;
  - `sandmux vault list` mostra o tipo, os servidores e a impressão digital (`SHA256:...`), nunca a chave.
- **Chaves escolhidas por perfil:** a chave de configuração `ssh-keys` (e `--ssh-key <nome>` no `run`) diz quais chaves do cofre a sessão pode usar; `"*"` libera todas. Sem nenhuma, a sessão não tem agente.
- **Agente do sandmux:** com chaves escolhidas, o daemon abre um socket de agente por lançamento, montado no sandbox em `/run/sandmux/ssh-agent.sock`, com `SSH_AUTH_SOCK` apontando para ele. O agente lista só as chaves escolhidas e assina com elas enquanto o cofre está desbloqueado. A chave privada nunca entra no sandbox.
- **Registro de cada uso:** cada assinatura vira uma linha `ssh-sign` no registro de rede, com a etiqueta do lançamento, o nome da chave e a impressão digital; o `sandmux net` e o painel mostram `assinatura ssh: <nome>`.
- **SSH só para os servidores das chaves:** o gateway reconhece o protocolo SSH (a linha `SSH-` do cliente ou do servidor) em qualquer porta e só deixa passar quando o nome da conexão casa com um servidor de uma chave escolhida na sessão. A política do modo vale junto.
- **`known_hosts` da sessão:** um `~/.ssh/known_hosts` só daquela execução, com o guardado no HOME sintético mais as linhas do `known_hosts` do host para os servidores das chaves escolhidas (inclusive as com nome em hash).

## Capabilities

### New Capabilities

- `git-ssh-agent`: agente SSH do sandmux no sandbox, chaves escolhidas por perfil, registro de cada assinatura e `known_hosts` da sessão.

### Modified Capabilities

- `secret-vault`: chaves SSH no cofre (`sandmux vault add-ssh`).
- `network-policy`: SSH de saída só para os servidores das chaves escolhidas.
- `config-profiles`: chave `ssh-keys`.

## Impact

- **`sandmux-vault`:** campos da chave SSH no segredo.
- **`sandmux-policy`:** chave `ssh-keys`, a montagem do socket e o `known_hosts` da sessão.
- **`sandmux-spec`:** servidores SSH na política de rede.
- **`sandmux-net`:** reconhecimento do SSH e a decisão pelos servidores; linha `ssh-sign` no registro.
- **`sandmux`:**
  - o daemon abre o socket do agente por lançamento e assina;
  - `sandmux vault add-ssh`; `--ssh-key` no `run`; a coluna da chave no `vault list`.
- **`sandmux-proto`:** pedido `VaultAddSsh` e o tipo no `VaultEntry`; `PROTOCOL` 13.
- **Dependências:** `ssh-key` (leitura e assinatura das chaves OpenSSH), `hmac` e `sha1` (nomes em hash do `known_hosts`).
- **Documentação:** `docs/vault.md`, `docs/network.md`, `docs/config.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 13).
