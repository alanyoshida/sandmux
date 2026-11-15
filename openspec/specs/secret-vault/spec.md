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
`sandmux vault import` MUST percorrer os manifestos gerados pela importação de cada harness e, para cada valor falso ainda sem segredo no cofre, mostrar o harness, o arquivo, a chave e a linha do arquivo importado (com o falso como `<falso>`), e pedir o valor real, sem mostrá-lo: cada caractere digitado aparece como `•`, que MUST ficar no cofre ligado ao valor falso. Uma resposta vazia MUST pular o item.

#### Scenario: Chave do opencode
- **WHEN** a importação do opencode trocou a chave do OpenRouter em `auth.json` por um valor falso e o usuário roda `sandmux vault import`
- **THEN** o comando pede o valor real dessa chave, e o cofre passa a ter um segredo ligado ao valor falso

#### Scenario: Digitação visível sem o valor
- **WHEN** o usuário cola uma chave de 43 caracteres na pergunta do `sandmux vault import`
- **THEN** o terminal mostra 43 bolinhas, e o valor não aparece

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

### Requirement: Chaves reais nos HOMEs sintéticos
`sandmux doctor` MUST procurar, nos arquivos da tabela de importação de cada harness dentro do HOME sintético, valores de credencial que não são falsos do sandmux, com as mesmas regras da importação, e listar cada um com o harness, o arquivo, o provedor (quando há), a chave e a linha, sem o valor. `sandmux doctor --fix` MUST, arquivo a arquivo, guardar cada chave no cofre com um valor falso novo e o domínio do provedor (ou perguntado), e só depois regravar o arquivo com os falsos. Se o cofre recusar uma chave, o arquivo MUST ficar como estava.

#### Scenario: Chave colada no opencode do sandbox
- **WHEN** o usuário conectou o OpenRouter pelo opencode dentro do sandbox, e o `auth.json` do HOME sintético tem a chave real
- **THEN** o `sandmux doctor` lista `opencode: .local/share/opencode/auth.json`, provedor `openrouter`, chave `key`, sem mostrar o valor, e indica `sandmux doctor --fix`

#### Scenario: Mover para o cofre
- **WHEN** o usuário roda `sandmux doctor --fix` e confirma
- **THEN** o cofre ganha o segredo com o domínio `openrouter.ai` e um valor falso, o `auth.json` passa a ter o falso, e o real não fica em nenhum arquivo do HOME sintético

### Requirement: Cofre na página de configurações
A página de configurações da TUI MUST ter a seção "Cofre", com o estado do cofre e a lista de segredos (nome, tipo e domínios, nunca o valor). Pela seção, o usuário MUST poder:
- criar, desbloquear e trancar o cofre;
- trocar a senha;
- acrescentar um segredo (nome, valor e domínios) ou uma chave SSH (arquivo e servidores);
- editar os domínios de um segredo;
- apagar um segredo, depois de confirmar.

Senhas e valores MUST aparecer só como `•` e MUST ser apagados da memória da TUI depois do envio.

#### Scenario: Acrescentar pela TUI
- **WHEN** com o cofre aberto, o usuário acrescenta `lmstudio` com o valor `abc` e o domínio `host.sandmux.internal`
- **THEN** o segredo aparece na lista com o domínio, o valor nunca aparece na tela, e `sandmux vault list` mostra o segredo

#### Scenario: Apagar com confirmação
- **WHEN** o usuário escolhe um segredo, aperta `x` e confirma
- **THEN** o segredo some da lista e do cofre

### Requirement: Troca dos domínios de um segredo
O daemon MUST aceitar a troca dos domínios de um segredo existente sem receber o valor de novo, com o cofre desbloqueado, e MUST validar cada domínio como na criação.

#### Scenario: Editar domínios
- **WHEN** o usuário troca os domínios de `lmstudio` para `host.sandmux.internal` e `localhost`
- **THEN** o segredo fica com os dois domínios, e o valor continua o mesmo

### Requirement: Varredura das chaves reais pelo daemon
O daemon MUST procurar chaves reais nos arquivos que o `sandmux doctor` lê nos HOMEs sintéticos, com as mesmas regras, nestes momentos:

- quando um desses arquivos é gravado;
- ao subir;
- a cada 60 segundos;
- logo depois de o cofre ser aberto.

#### Scenario: Gravação perdida
- **WHEN** uma chave real está num arquivo do HOME sintético e nenhum aviso de gravação chegou ao daemon
- **THEN** a varredura periódica a acha em até 60 segundos

### Requirement: Movimento automático das chaves reais
Com o cofre aberto e o domínio de cada chave conhecido pelas mesmas regras do `doctor --fix`, o daemon MUST guardar cada chave do arquivo no cofre, com um falso novo e o domínio, e só depois regravar o arquivo com os falsos. Se o cofre recusar uma chave, se uma chave do arquivo não tiver domínio conhecido, ou se o arquivo mudou desde a leitura, o arquivo MUST ficar como estava. Os valores reais MUST NOT aparecer no registro do daemon nem nas mensagens.

#### Scenario: Chave conectada pelo opencode
- **WHEN** o cofre está aberto e o opencode do sandbox grava a chave real do OpenRouter no `auth.json`
- **THEN** em poucos segundos o cofre tem o segredo com o domínio `openrouter.ai`, o `auth.json` tem o falso, e a aba diz que foi movida uma chave e que precisa ser reiniciada

### Requirement: Chaves esperando o cofre ou um domínio
Quando uma chave real não pode ser movida sozinha, o daemon MUST deixar o arquivo como está e informar, nas abas do harness, quantas chaves esperam o cofre (trancado ou ausente) e quantas esperam um domínio. As que esperam o cofre MUST ser movidas logo depois de o cofre ser aberto. As que esperam um domínio continuam para o `sandmux doctor --fix`.

#### Scenario: Cofre trancado
- **WHEN** o cofre está trancado e o hermes grava uma chave real no `.env` do HOME sintético
- **THEN** o arquivo fica como está, a aba do hermes mostra uma chave esperando o cofre, e, quando o cofre é aberto, a chave vai para o cofre e o arquivo passa a ter o falso

#### Scenario: Domínio desconhecido
- **WHEN** uma chave real tem um nome e um provedor que não indicam o domínio
- **THEN** o arquivo fica como está, a aba mostra uma chave esperando um domínio, e o `sandmux doctor --fix` pergunta o domínio

### Requirement: Senha do cofre no keyring do sistema
`sandmux vault keyring on` MUST pedir a senha do cofre, conferi-la com o arquivo do cofre e só então guardá-la no keyring do sistema, pelo Secret Service. Uma senha que não abre o cofre MUST ser recusada sem gravar nada. `sandmux vault keyring off` MUST apagar a senha do keyring. `sandmux vault keyring` MUST dizer se o keyring está ligado e, quando ligado, se ele responde.


#### Scenario: Ligar
- **WHEN** o usuário roda `sandmux vault keyring on` e digita a senha certa
- **THEN** a senha fica no keyring, e `sandmux vault keyring` diz que está ligado

#### Scenario: Senha errada
- **WHEN** o usuário roda `sandmux vault keyring on` e digita uma senha que não abre o cofre
- **THEN** o comando termina com erro e nada é gravado no keyring

### Requirement: Keyring opcional
O keyring MUST ficar desligado até o usuário ligá-lo, e, desligado, o sandmux MUST NOT consultar o keyring. `sandmux vault passwd` MUST atualizar a senha guardada quando o keyring está ligado.

#### Scenario: Troca de senha
- **WHEN** o keyring está ligado e o usuário troca a senha com `sandmux vault passwd`
- **THEN** o keyring passa a ter a senha nova

### Requirement: Desbloqueio pelo keyring
Com o keyring ligado, o daemon MUST tentar abrir o cofre com a senha do keyring ao subir, e quando um cliente pergunta o estado do cofre trancado, no máximo a cada 5 segundos. Um desbloqueio pelo keyring MUST ter o mesmo efeito do `sandmux vault unlock`. Se o keyring não responde, não tem a senha ou tem uma senha que não abre o cofre, o cofre MUST ficar trancado, e o registro do daemon MUST dizer o motivo, sem a senha.

#### Scenario: Depois de reiniciar o daemon
- **WHEN** o keyring está ligado e o daemon sobe de novo
- **THEN** o cofre fica aberto sem a senha ser digitada, e as abas que esperavam o cofre sobem

#### Scenario: Keyring desligado
- **WHEN** o keyring está desligado e o daemon sobe
- **THEN** o cofre fica trancado e o daemon não consulta o keyring
