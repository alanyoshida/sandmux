## MODIFIED Requirements

### Requirement: Restauração depois de reboot
Ao subir, o daemon MUST relançar cada sessão que estava viva, com as mesmas opções, resolvendo as camadas com a configuração do momento. Uma sessão com `pass-env` cujos valores estão no cofre MUST ficar "aguardando cofre" e subir sozinha, com eles, no desbloqueio. A sessão do gateway do hermes e as sessões de chat MUST ficar "aguardando cofre" enquanto o cofre existe e está trancado, e subir sozinhas no desbloqueio. MUST ficar parada, com o motivo em `sandmux ls`, uma sessão com `pass-env` sem os valores no cofre, com perfil que não existe mais ou com configuração inválida. Uma sessão encerrada MUST voltar encerrada.

#### Scenario: Sessão sem variáveis passadas
- **WHEN** a máquina reinicia com uma sessão viva do `opencode` em `~/proj`, criada sem `--pass-env`
- **THEN** depois do login o daemon relança o `opencode` em `~/proj` sem nenhum comando do usuário

#### Scenario: Sessão com variável no cofre
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY` enquanto o cofre estava desbloqueado
- **THEN** a sessão aparece "aguardando cofre" em `sandmux ls` e sobe sozinha, com o valor, quando o usuário desbloqueia o cofre

#### Scenario: Sessão com variável passada
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY` sem cofre
- **THEN** a sessão aparece parada em `sandmux ls`, com o motivo, e só sobe quando o usuário roda `sandmux restart` de um terminal que tem a variável

#### Scenario: Pasta de trabalho apagada
- **WHEN** a pasta de trabalho de uma sessão viva não existe mais quando o daemon sobe
- **THEN** a sessão fica parada com o motivo, e as demais são restauradas

#### Scenario: Variável acrescentada pelo perfil
- **WHEN** uma sessão viva foi criada sem `--pass-env` e, antes do reboot, o perfil do projeto dela ganhou `pass-env = ["OPENROUTER_API_KEY"]`
- **THEN** depois do login a sessão aparece parada, com o motivo citando a variável

#### Scenario: Perfil apagado
- **WHEN** uma sessão viva foi criada com `--profile vm` e o perfil `vm` foi apagado antes do reboot
- **THEN** depois do login a sessão aparece parada, com o motivo dizendo que o perfil não existe

#### Scenario: Configuração inválida
- **WHEN** o arquivo de configuração está inválido quando o daemon sobe
- **THEN** todas as sessões que estavam vivas aparecem paradas, com o motivo citando o arquivo e a linha, e o daemon continua rodando

#### Scenario: Gateway do hermes com o cofre trancado
- **WHEN** a máquina reinicia com a sessão do gateway do hermes viva e o cofre existe
- **THEN** a sessão aparece "aguardando cofre" em `sandmux ls` e sobe sozinha quando o usuário desbloqueia o cofre

#### Scenario: Aba de chat depois do reboot
- **WHEN** a máquina reinicia com uma aba de chat viva e o cofre existe
- **THEN** a aba de chat aparece "aguardando cofre" e volta, com a conversa, quando o usuário desbloqueia o cofre
