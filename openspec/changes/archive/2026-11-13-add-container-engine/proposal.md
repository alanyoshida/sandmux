# Proposal

## Why

Agentes de código precisam rodar `docker` e `docker compose` para subir bancos, serviços e testes de integração. O socket do Docker do host equivale a root na máquina e nunca pode entrar no sandbox (decisão da seção 2.8 do roadmap). O roadmap decidiu um Docker sem root, privado do sandbox, um por pasta de trabalho, cujas montagens só alcançam o que o sandbox já enxerga.

Uma prova feita nesta máquina, em 2026-11-13, mostrou que isso é viável sem instalar nada. Nela, o `dockerd` rodou dentro do `rootlesskit`, com o driver de rede `gvisor-tap-vsock`, embutido, numa raiz nova que só tinha o sistema, a pasta de trabalho e os dados do Docker. O resultado:

- o build e a execução de contêineres funcionaram;
- a rede funcionou;
- `-v ~/.ssh`, e um link simbólico na pasta de trabalho apontando para `~/.ssh`, chegaram vazios ao contêiner.

A mesma prova mostrou dois problemas a corrigir. Rodando com a visão normal do host, o Docker sem root monta qualquer pasta do usuário. E as portas publicadas saem em `0.0.0.0`, abertas para a rede local.

## What Changes

- **Docker privado por pasta de trabalho**, iniciado pelo daemon de sessões:
  - o `dockerd` roda no user namespace do `rootlesskit`, numa raiz montada pela mesma composição do sandbox (sistema em somente leitura com os arquivos sensíveis de `/etc` cobertos, pasta de trabalho com a proteção do `.git`, pasta de entrada), mais os dados e o estado do próprio Docker;
  - um `-v` ou `--mount` só alcança o que essa visão contém;
  - um caminho fora dela, inclusive por link simbólico, chega vazio ou falha.
- **Socket no sandbox:** cada sessão recebe `/run/sandmux/docker.sock` e `DOCKER_HOST` apontando para ele. O daemon repassa as conexões ao Docker da pasta.
  - O Docker sobe na primeira conexão, para não atrasar quem não usa.
  - O Docker para quando a última sessão viva daquela pasta termina.
  - O socket do Docker do host continua fora.
- **Persistência:** imagens, volumes e contêineres parados de cada pasta ficam no diretório de dados (`docker/<identificador da pasta>`) e voltam na próxima sessão. Ficam separados dos do host e dos de outras pastas.
- **Portas publicadas** só no `127.0.0.1` do host, nunca em `0.0.0.0`.
- **Rede dos contêineres:**
  - o driver é o `slirp4netns` ou o `pasta`, quando existem na máquina; senão, o `gvisor-tap-vsock`, embutido no `rootlesskit`;
  - a rede é tão aberta quanto a do sandbox até a etapa 4.
- **Tetos e medição:** o Docker de cada pasta roda num escopo do systemd com os tetos de memória e de processos daquela pasta, resolvidos pela configuração. O `sandmux stats` ganha a seção do Docker privado da pasta, com memória, processos e contêineres rodando. É a medição que o change 4 deixou para cá.
- **Sem Docker sem root na máquina** (sem `rootlesskit`, `dockerd`, `newuidmap` ou faixa em `/etc/subuid`): a sessão sobe normalmente. O socket responde a todo pedido com um erro que o `docker` mostra, dizendo o que falta e indicando `sandmux doctor`. O `doctor` ganha a verificação do Docker privado.
- **`run` sem terminal:** não tem Docker privado, porque não há daemon para mantê-lo. `DOCKER_HOST` não é definido e o `docker` falha por não achar o socket.

Ficam para outros changes, registrados no roadmap:

- **Cache local de downloads de imagem entre projetos:** vai para o change 8, `add-network-gateway`. O gateway de rede no daemon é o lugar natural de um cache de registry. Hoje cada pasta baixa as próprias imagens.
- **Política de rede mais restrita entre abas da mesma pasta:** também vai para o change 8, quando existirem modos de rede.

## Capabilities

### New Capabilities
- `container-engine`: o Docker privado por pasta, com a visão de arquivos restrita, o socket no sandbox, o ciclo de vida, a persistência, as portas no `127.0.0.1`, os tetos e o comportamento sem suporte.

### Modified Capabilities
- `resource-limits`: "Detalhe do consumo" ganha a seção do Docker privado da pasta.
- `sandbox-isolation`: "Diagnóstico de suporte da máquina" ganha a verificação do Docker privado.

## Impact

- **`sandmux-policy`:**
  - composição da visão do Docker (`compose_docker_view`), reaproveitando as partes da composição do sandbox;
  - montagem do socket e `DOCKER_HOST` na especificação do sandbox;
  - identificador da pasta para o diretório de dados.
- **`sandmux-launcher` e `sandmux-run`:** o modo `--view-file <arquivo>` monta a raiz de uma especificação no namespace atual e executa o comando, sem Landlock, sem seccomp e sem criar namespaces. É chamado dentro do `rootlesskit`.
- **Daemon:**
  - gerente do Docker por pasta (subida sob demanda, contagem de sessões, parada, escopo do systemd com `Delegate=yes`);
  - repasse de conexões do socket de cada sessão;
  - resposta de erro sem suporte;
  - seção do Docker no `stats`.
- **CLI:** seção nova no `doctor` e no `stats`.
- **Protocolo 4**, porque o `StatsOf` ganha a seção do Docker.
- **Documentação:** `docs/docker.md`, novo; README e `docs/limits.md` atualizados.
- **Testes:** usam uma imagem construída localmente a partir do `busybox` estático do host, sem baixar nada. Pulam com mensagem sem suporte.
