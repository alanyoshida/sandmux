## ADDED Requirements

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
