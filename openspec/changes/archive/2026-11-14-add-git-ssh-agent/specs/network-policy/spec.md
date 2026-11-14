# Spec Delta

## ADDED Requirements

### Requirement: SSH só para os servidores das chaves
Uma conexão TCP MUST ser tratada como SSH quando os primeiros bytes do cliente, ou do servidor, começam com `SSH-`, em qualquer porta. Em qualquer modo, uma conexão SSH MUST seguir só quando um nome ligado a ela casa com um servidor de uma chave SSH escolhida na sessão, com o cofre desbloqueado; senão MUST ser fechada e registrada como bloqueada, com o motivo `ssh fora dos servidores das chaves`. A decisão do modo MUST valer antes desta.

#### Scenario: Servidor de uma chave escolhida
- **WHEN** a sessão escolheu a chave `github` (servidor `github.com`) e o agente roda `ssh -T git@github.com`
- **THEN** a conexão segue

#### Scenario: Outro servidor
- **WHEN** a mesma sessão abre uma conexão SSH com `exemplo.com` na porta 22 ou na porta 443
- **THEN** a conexão é fechada e registrada como bloqueada com o motivo `ssh fora dos servidores das chaves`

#### Scenario: Sem chaves
- **WHEN** a sessão não tem chaves SSH escolhidas e abre uma conexão SSH
- **THEN** a conexão é fechada e registrada como bloqueada
