# Spec Delta

## ADDED Requirements

### Requirement: Chaves SSH no cofre
Com o cofre desbloqueado, `sandmux vault add-ssh <nome> --file <chave> --server <domínio>...` MUST guardar a chave privada OpenSSH (Ed25519, ECDSA ou RSA) sem passphrase, cifrada no cofre, com os servidores dela. A passphrase de uma chave cifrada MUST ser pedida sem eco, no cliente. Um tipo fora da lista, uma passphrase errada ou a falta de `--server` MUST ser recusados. `sandmux vault list` MUST mostrar o tipo, os servidores e a impressão digital, nunca a chave.

#### Scenario: Chave com passphrase
- **WHEN** o usuário roda `sandmux vault add-ssh github --file ~/.ssh/id_ed25519 --server github.com` com uma chave cifrada e digita a passphrase certa
- **THEN** o cofre guarda a chave, e o `vault list` mostra `github`, `ssh`, `github.com` e a impressão digital

#### Scenario: Passphrase errada
- **WHEN** o usuário digita a passphrase errada
- **THEN** o comando falha, e nada é guardado

### Requirement: Chaves SSH fora da troca de tokens
As chaves SSH do cofre MUST NOT ter valor falso nem entrar na troca de tokens, nas ferramentas ou na lista de decifração do TLS.

#### Scenario: Fora da troca
- **WHEN** o cofre tem só a chave SSH `github` com o servidor `github.com`
- **THEN** o `github.com` não entra na lista de decifração do TLS, e nenhuma sessão recebe valor falso dela
