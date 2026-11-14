# Spec Delta

## MODIFIED Requirements

### Requirement: Lista de sessões guardada sem segredos
O daemon MUST manter no diretório de dados do sandmux um arquivo com a lista de sessões: para cada uma, o identificador, o nome, o harness, a pasta de trabalho, as opções dadas no `run`, o perfil nomeado escolhido, os nomes das variáveis de `pass-env` do último lançamento e se ela estava viva ou encerrada. O arquivo MUST NOT conter valores de variáveis, a política resolvida nem saída das sessões.

#### Scenario: Conteúdo do arquivo
- **WHEN** existe uma sessão criada com `--pass-env OPENROUTER_API_KEY`
- **THEN** o arquivo da lista tem o nome `OPENROUTER_API_KEY` e não tem o valor dele nem nenhuma linha da saída da sessão

#### Scenario: Perfil guardado pelo nome
- **WHEN** existe uma sessão criada com `--profile vm`
- **THEN** o arquivo da lista guarda o nome `vm`, e não as chaves do perfil

#### Scenario: Lista da versão anterior
- **WHEN** o daemon novo sobe com uma lista gravada pela versão anterior do sandmux
- **THEN** as sessões são lidas com as opções que tinham, sem perfil nomeado, e a lista é regravada no formato novo

### Requirement: Restauração depois de reboot
Ao subir, o daemon MUST ler a lista de sessões e relançar cada sessão que estava viva, na mesma pasta, com o mesmo harness, as mesmas opções e o mesmo perfil, resolvendo as camadas com a configuração do momento. MUST ficar parada, com o motivo em `sandmux ls`, até o usuário reiniciá-la, uma sessão cuja política efetiva tem `pass-env`, cujo perfil nomeado não existe mais ou cuja configuração é inválida. Uma sessão encerrada MUST voltar encerrada, com o código de saída.

#### Scenario: Sessão sem variáveis passadas
- **WHEN** a máquina reinicia com uma sessão viva do `opencode` em `~/proj`, criada sem `--pass-env`
- **THEN** depois do login o daemon relança o `opencode` em `~/proj` sem nenhum comando do usuário

#### Scenario: Sessão com variável passada
- **WHEN** a máquina reinicia com uma sessão viva criada com `--pass-env OPENROUTER_API_KEY`
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
