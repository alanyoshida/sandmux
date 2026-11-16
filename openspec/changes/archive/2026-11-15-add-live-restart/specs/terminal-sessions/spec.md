## MODIFIED Requirements

### Requirement: Reiniciar uma sessão
`sandmux restart <id>` MUST relançar uma sessão encerrada ou parada com o mesmo harness, a mesma pasta, as mesmas opções do `run` e o mesmo perfil nomeado, resolvendo as camadas com a configuração do momento, e anexar o terminal quando houver um. Os valores das variáveis de `pass-env` da política efetiva MUST vir do ambiente do terminal que pediu o reinício, e uma variável ausente MUST impedir o reinício com erro. Sem `--force`, uma sessão viva MUST NOT ser reiniciada.

#### Scenario: Reinício de sessão encerrada
- **WHEN** o harness da sessão 2 terminou e o usuário roda `sandmux restart 2`
- **THEN** o harness sobe de novo na mesma pasta, o terminal se anexa e o estado passa a viva

#### Scenario: Variável ausente
- **WHEN** a sessão 3 foi criada com `--pass-env OPENROUTER_API_KEY` e o usuário roda `sandmux restart 3` num terminal sem a variável
- **THEN** o comando termina com erro citando a variável e a sessão continua parada

#### Scenario: Sessão viva
- **WHEN** o usuário roda `sandmux restart 1` e a sessão 1 está viva
- **THEN** o comando termina com erro dizendo que a sessão está viva e que `--force` a reinicia, sem afetá-la

#### Scenario: Configuração mudou
- **WHEN** a sessão 2 foi criada com `--profile vm`, o usuário tira `kvm = true` do perfil `vm` e roda `sandmux restart 2`
- **THEN** o harness relançado não tem `/dev/kvm`

#### Scenario: Perfil apagado
- **WHEN** a sessão 2 foi criada com `--profile vm`, o perfil `vm` foi apagado e o usuário roda `sandmux restart 2`
- **THEN** o comando termina com erro dizendo que o perfil não existe, e a sessão continua como estava

## ADDED Requirements

### Requirement: Reinício forçado de sessão viva
`sandmux restart <id> --force` MUST encerrar uma sessão viva (TERM e, depois de 5 s, KILL) e relançá-la como num reinício comum, sem a notificação de aba encerrada e sem o relançamento automático do gateway.

#### Scenario: Sessão viva com --force
- **WHEN** o usuário roda `sandmux restart 1 --force` e a sessão 1 está viva
- **THEN** o harness é encerrado e sobe de novo na mesma pasta, com a configuração de agora
