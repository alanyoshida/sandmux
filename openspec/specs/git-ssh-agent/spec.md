# git-ssh-agent Specification

## Purpose
Define o agente SSH do sandmux dentro do sandbox: as chaves do cofre escolhidas por perfil, a assinatura sem a chave privada no sandbox, o registro de cada assinatura e o `known_hosts` da sessão.

## Requirements

### Requirement: Agente SSH no sandbox
Uma sessão com chaves SSH escolhidas (chave `ssh-keys` ou `--ssh-key`) MUST ter um agente SSH em `/run/sandmux/ssh-agent.sock`, com `SSH_AUTH_SOCK` apontando para ele. O agente MUST listar só as chaves escolhidas que estão no cofre desbloqueado e assinar só com elas. A chave privada MUST NOT entrar no sandbox. Sem chaves escolhidas, a sessão MUST NOT ter agente nem `SSH_AUTH_SOCK`.

#### Scenario: Só as chaves escolhidas
- **WHEN** o cofre desbloqueado tem as chaves SSH `github` e `trabalho`, e o usuário roda `sandmux run shell --ssh-key github`
- **THEN** `ssh-add -L` dentro do sandbox lista só a chave pública de `github`

#### Scenario: Assinatura pelo agente
- **WHEN** o agente pede uma assinatura com a chave `github` pelo agente do sandbox
- **THEN** a assinatura confere com a chave pública de `github`, e nenhum arquivo nem variável do sandbox tem a chave privada

#### Scenario: Cofre trancado
- **WHEN** o cofre é trancado com a sessão viva
- **THEN** o agente lista nenhuma chave e recusa as assinaturas, e um desbloqueio posterior volta a liberá-las sem relançar a sessão

#### Scenario: Pedidos de alteração
- **WHEN** um processo do sandbox pede ao agente para adicionar, remover ou trancar chaves
- **THEN** o agente responde com falha, e as chaves não mudam

#### Scenario: RSA com SHA-1
- **WHEN** um processo pede uma assinatura RSA sem as flags `rsa-sha2-256` ou `rsa-sha2-512`
- **THEN** o agente responde com falha

### Requirement: Registro de cada assinatura
Cada pedido de assinatura ao agente MUST virar uma linha `ssh-sign` no registro de rede, com a etiqueta do lançamento, o nome da chave, a impressão digital e o resultado, sem a chave nem os dados assinados. O `sandmux net` e o painel MUST mostrar `assinatura ssh: <nome>`.

#### Scenario: Assinatura registrada
- **WHEN** o agente assina com a chave `github` na sessão 1
- **THEN** o registro do dia tem uma linha `ssh-sign` com `github` e a impressão digital, e o `sandmux net 1` mostra `assinatura ssh: github`

### Requirement: known_hosts da sessão
Uma sessão com chaves SSH escolhidas MUST receber um `~/.ssh/known_hosts` só daquela execução, com o arquivo guardado no HOME sintético mais as linhas do `~/.ssh/known_hosts` do host cujos nomes casam com os servidores das chaves escolhidas, inclusive nomes com porta, listas separadas por vírgula e nomes em hash. O arquivo guardado MUST NOT mudar.

#### Scenario: Nome em hash
- **WHEN** o `known_hosts` do host tem a linha de `github.com` com o nome em hash, e a chave escolhida tem o servidor `github.com`
- **THEN** o `known_hosts` da sessão tem essa linha, e não tem as linhas de servidores que não são das chaves escolhidas
