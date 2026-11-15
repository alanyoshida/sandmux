## ADDED Requirements

### Requirement: Início no boot
`sandmux daemon boot on` MUST ligar o linger do usuário pelo `loginctl`, sem sudo, e deixar a unidade systemd de usuário do daemon habilitada. `sandmux daemon boot off` MUST desligar o linger e avisar que outros serviços de usuário também deixam de subir no boot. `sandmux daemon boot`, sem argumento, `sandmux daemon status` e `sandmux doctor` MUST dizer se o daemon sobe no boot.

#### Scenario: Ligar
- **WHEN** o usuário roda `sandmux daemon boot on`
- **THEN** o `loginctl enable-linger` roda para o usuário, e `sandmux daemon boot` diz que o daemon sobe no boot

#### Scenario: loginctl recusa
- **WHEN** o `loginctl` recusa ligar o linger
- **THEN** o comando termina com erro e a mensagem do `loginctl`

### Requirement: Espera do login
Quando o daemon sobe sem nenhuma sessão de login do usuário, as sessões que seriam relançadas MUST NOT ser relançadas: elas MUST ficar no estado "aguardando o login", guardado na lista de sessões. Nesse estado, o daemon MUST NOT consultar o keyring. Quando o usuário faz login, o daemon MUST relançar essas sessões, tentar abrir o cofre pelo keyring e avisar as sessões que esperam o cofre. Com o usuário logado ao subir, a restauração MUST seguir sem espera.

#### Scenario: Boot sem login
- **WHEN** o daemon sobe no boot, sem login, e havia duas abas vivas
- **THEN** `sandmux ls` mostra as duas como aguardando o login, e nenhum harness roda

#### Scenario: Primeiro login
- **WHEN** o usuário faz login
- **THEN** as duas abas voltam vivas, sem comando do usuário

#### Scenario: Reinício antes do login
- **WHEN** o daemon reinicia de novo antes do login
- **THEN** as abas continuam aguardando o login, e voltam no primeiro login
