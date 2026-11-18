## ADDED Requirements

### Requirement: Registro dos contêineres
Com o confinamento por eBPF, o registro de arquivos MUST ter uma linha para cada operação negada a um processo do Docker privado da pasta e para a primeira criação de cada arquivo da pasta de trabalho por um contêiner, em cada subida do Docker. A linha MUST ter a etiqueta `docker-<id>` e o caminho do host. Leituras e escritas comuns dos contêineres MUST NOT entrar. `sandmux files` MUST mostrar essas linhas com as do agente.

#### Scenario: Segredo tentado por um contêiner
- **WHEN** o confinamento vale e um contêiner tenta ler o `.env` da pasta
- **THEN** o registro tem uma linha com a etiqueta `docker-<id>`, o caminho do `.env` no host, o acesso `r` e o bloqueio `segredo`

#### Scenario: Arquivo criado por um contêiner
- **WHEN** um contêiner com a pasta montada em `/w` cria `/w/saida.txt`
- **THEN** o registro tem uma linha `create` com o caminho de `saida.txt` no host

#### Scenario: Criação fora da pasta
- **WHEN** um contêiner cria `/tmp/x` no próprio sistema de arquivos
- **THEN** nenhuma linha vai para o registro
