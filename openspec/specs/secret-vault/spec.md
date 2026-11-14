# secret-vault Specification

## Purpose
Define o cofre de segredos do sandmux: o arquivo cifrado com senha, o desbloqueio e o trancamento, o cadastro e a listagem de segredos, a troca de senha, a leitura do manifesto da importação e a guarda dos valores de `--pass-env` para a restauração das sessões.

## Requirements

### Requirement: Arquivo cifrado com senha
O cofre MUST ficar em `$XDG_DATA_HOME/sandmux/vault.json`, com modo 600, cifrado com uma chave derivada da senha pelo Argon2id e com XChaCha20-Poly1305. O arquivo MUST NOT conter nenhum segredo nem nome de segredo em claro. `sandmux vault init` MUST criar o cofre, pedindo a senha duas vezes no terminal.

#### Scenario: Nada em claro
- **WHEN** o cofre tem um segredo `openrouter` com o valor `sk-teste-123`
- **THEN** nem `sk-teste-123` nem `openrouter` aparecem no arquivo

#### Scenario: Senha errada
- **WHEN** o usuário roda `sandmux vault unlock` com uma senha errada
- **THEN** o comando falha dizendo que a senha não confere, e o cofre continua trancado

### Requirement: Cofre trancado quando o daemon sobe
O daemon MUST subir com o cofre trancado e guardar os segredos só na memória, e só enquanto o cofre está desbloqueado. `sandmux vault lock` MUST trancar o cofre e apagar os segredos da memória do daemon. `sandmux vault status` MUST dizer se o cofre existe e se está desbloqueado.

#### Scenario: Depois de um reinício do daemon
- **WHEN** o cofre estava desbloqueado e o usuário roda `sandmux daemon restart`
- **THEN** `sandmux vault status` diz que o cofre está trancado

### Requirement: Desbloqueio pela linha de comando
`sandmux vault unlock` MUST pedir a senha no terminal, sem eco, e mandá-la ao daemon pelo socket privado. Sem terminal, MUST ler a senha da primeira linha da entrada padrão.

#### Scenario: Desbloqueio
- **WHEN** o cofre está trancado e o usuário roda `sandmux vault unlock` com a senha certa
- **THEN** `sandmux vault status` diz que o cofre está desbloqueado

### Requirement: Cadastro de segredos
Com o cofre desbloqueado, `sandmux vault add <nome> [--domain <padrão>]... [--tool <ferramenta>]...` MUST guardar um segredo com o valor lido do terminal sem eco, ou da entrada padrão, nunca de um argumento, e com um valor falso gerado. `sandmux vault list` MUST mostrar só os nomes, os domínios, as ferramentas e o valor falso. `sandmux vault remove <nome>` MUST apagar o segredo. Cada mudança MUST ser gravada no arquivo na hora.

#### Scenario: Listar sem valores
- **WHEN** o cofre tem o segredo `openrouter` com o domínio `openrouter.ai` e o usuário roda `sandmux vault list`
- **THEN** a saída tem `openrouter` e `openrouter.ai`, e não tem o valor

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o usuário roda `sandmux vault add x`
- **THEN** o comando falha pedindo `sandmux vault unlock`

### Requirement: Troca de senha
`sandmux vault passwd` MUST pedir a senha atual e a nova (duas vezes) e regravar o cofre com a chave nova.

#### Scenario: Senha nova
- **WHEN** o usuário troca a senha e depois tranca o cofre
- **THEN** só a senha nova desbloqueia o cofre

### Requirement: Leitura do manifesto da importação
`sandmux vault import` MUST percorrer os manifestos gerados pela importação de cada harness e, para cada valor falso ainda sem segredo no cofre, mostrar o harness, o arquivo e a chave, e pedir o valor real (sem eco), que MUST ficar no cofre ligado ao valor falso. Uma resposta vazia MUST pular o item.

#### Scenario: Chave do opencode
- **WHEN** a importação do opencode trocou a chave do OpenRouter em `auth.json` por um valor falso e o usuário roda `sandmux vault import`
- **THEN** o comando pede o valor real dessa chave, e o cofre passa a ter um segredo ligado ao valor falso

### Requirement: Valores de pass-env guardados no cofre
Quando uma sessão é criada ou reiniciada com `--pass-env` e o cofre está desbloqueado, os valores MUST ser guardados no cofre, ligados à sessão. Quando o cofre não existe ou está trancado, nada MUST ser guardado, e o cliente MUST avisar que, num reboot, a sessão fica parada. Os valores de uma sessão MUST sair do cofre quando a sessão é encerrada com `sandmux kill`.

#### Scenario: Valor guardado
- **WHEN** o cofre está desbloqueado e o usuário roda `sandmux run shell --pass-env CHAVE_TESTE` com `CHAVE_TESTE=valor-x`
- **THEN** o cofre passa a ter o valor da sessão, e `valor-x` não aparece em nenhum arquivo em claro

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
