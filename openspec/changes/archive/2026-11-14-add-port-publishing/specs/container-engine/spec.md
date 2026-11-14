# Spec Delta

## ADDED Requirements

### Requirement: Rede do Docker pelo gateway
O Docker privado de uma pasta MUST rodar num namespace de rede sem rede própria, com um TUN ligado a um gateway do sandmux, de modo que todo tráfego dos contêineres e do `dockerd` passe pelo gateway, com o DNS do gateway, os bloqueios fixos e o registro, com a etiqueta `docker-<id>`.

#### Scenario: Contêiner registrado
- **WHEN** um contêiner do Docker privado acessa `host.sandmux.internal`
- **THEN** o registro de rede tem a linha da conexão com a etiqueta do Docker da pasta

### Requirement: Política do Docker
A política de rede do Docker privado MUST ser a mais restrita entre as sessões vivas da pasta, na ordem `sem-rede`, `inferencia`, `custom` e `aberta`. Entre sessões com o mesmo modo, a denylist MUST ser a soma e a allowlist MUST ser a interseção. A política MUST ser recompilada quando uma sessão da pasta sobe ou termina e quando o arquivo de configuração muda.

#### Scenario: Aba restrita na pasta
- **WHEN** a pasta tem uma sessão no modo aberta e outra no modo `inferencia`, e um contêiner consulta `host.sandmux.internal`
- **THEN** a consulta recebe NXDOMAIN, e o registro tem o bloqueio "fora do modo só inferência"

### Requirement: Cache de imagens entre pastas
Quando a máquina tem o binário `registry` (ou `docker-registry`), o daemon MUST subir um espelho pull-through do Docker Hub no `127.0.0.1` do host, com as camadas numa pasta comum do diretório de dados, e cada Docker privado MUST usá-lo como espelho. O gateway do Docker MUST decidir a conexão ao espelho como acesso a `registry-1.docker.io`. Sem o binário, cada pasta MUST baixar as próprias imagens, e o `sandmux doctor` MUST mostrar como instalar o espelho.

#### Scenario: Mesma imagem em outra pasta
- **WHEN** a pasta `~/a` baixou `alpine` e o agente da pasta `~/b` roda `docker pull alpine`
- **THEN** as camadas vêm do espelho, sem nova consulta ao Docker Hub pelas camadas

#### Scenario: Sem o binário
- **WHEN** a máquina não tem `registry` nem `docker-registry` e o usuário roda `sandmux doctor`
- **THEN** a saída diz que o cache de imagens está desligado e como instalar o pacote
