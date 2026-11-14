# Proposal

## Why

Desde o change 8 o sandbox tem rede própria. Um servidor de desenvolvimento que o agente sobe na porta 3000 fica preso no `127.0.0.1` do sandbox: o usuário não abre a página no navegador. E o Docker privado ainda sai direto pelo `rootlesskit`, sem passar pelo gateway: os contêineres escapam do registro e da política de rede da aba. O roadmap juntou no change 10 as portas publicadas, o Docker pelo gateway, o cache de imagens entre pastas e a política mais restrita entre as abas que dividem o Docker de uma pasta.

Uma prova feita nesta máquina, em 2026-11-14, mostrou que o `rootlesskit --net=none --port-driver=builtin` cria um namespace só com o `lo` e ainda publica no `127.0.0.1` do host uma porta aberta lá dentro. Assim o sandmux pode pôr o TUN e o gateway na rede do Docker, como faz no sandbox.

## What Changes

- **Portas do sandbox publicadas no host.**
  - **Detecção:** o supervisor do `sandmux-run` lê a cada segundo as tabelas de sockets do namespace de rede do sandbox.
  - **Publicação:** cada porta TCP que o agente escuta em `127.0.0.1`, em `0.0.0.0` ou em `::` ganha uma porta no `127.0.0.1` do host, de mesmo número quando está livre, ou a próxima livre.
  - **Encaminhamento:** cada conexão do host é levada para dentro do namespace.
  - **Fim:** a porta do host fecha quando o agente deixa de escutar.
  - **Lista:** as portas ficam num arquivo do lançamento, que o daemon lê.
- **Docker privado pelo gateway.**
  - **Rede:** o `rootlesskit` passa a usar `--net=none`. A visão do Docker cria o TUN no namespace dele e passa o descritor ao daemon, que roda um gateway para o Docker da pasta, com a mesma pilha, o mesmo DNS e o mesmo registro do change 8 (etiqueta `docker-<id>`).
  - **Política:** a do Docker é a mais restrita entre as sessões vivas da pasta. A ordem é `sem-rede`, `inferencia`, `custom` e `aberta`. Com o mesmo modo, a denylist é a soma e a allowlist é a interseção. É recompilada quando uma sessão da pasta sobe ou termina e quando a configuração muda.
  - **Dependência a menos:** o Docker privado deixa de precisar do `slirp4netns`, do `pasta` ou do `gvisor-tap-vsock`.
- **Cache de imagens entre pastas** (decisão do usuário, 2026-11-14: usar o registry oficial):
  - **Espelho:** quando a máquina tem o binário `registry` do projeto distribution (ou `docker-registry`, nome do pacote no Debian e no Ubuntu), o daemon sobe um espelho pull-through do Docker Hub no `127.0.0.1` do host, com as camadas numa pasta comum do diretório de dados.
  - **Uso:** cada `dockerd` privado usa o espelho por `--registry-mirror`.
  - **Política:** o gateway do Docker trata a conexão ao espelho como acesso a `registry-1.docker.io`, para a política valer.
  - **Sem o binário:** cada pasta baixa as próprias imagens, como hoje, e o `sandmux doctor` mostra como instalar.
- **Portas no painel e no `stats`:** a seção nova "Portas" do painel e o `sandmux stats` mostram as portas do sandbox e as publicadas pelos contêineres do Docker da pasta.
- **Protocolo 9:** `StatsOf` ganha as portas.

## Capabilities

### New Capabilities

- `port-publishing`: detecção das portas que o agente escuta, publicação no `127.0.0.1` do host com o mesmo número ou o próximo livre, encaminhamento, fechamento e lista das portas.

### Modified Capabilities

- `container-engine`:
  - entra o requisito "Rede do Docker pelo gateway" (namespace sem rede, TUN, gateway no daemon, registro e política mais restrita);
  - entra o requisito "Cache de imagens entre pastas".
- `terminal-ui`: entra o requisito "Portas no painel".

## Impact

- **`sandmux-launcher`:**
  - o supervisor publica as portas;
  - a visão do Docker cria o TUN e o manda ao daemon.
- **`sandmux-net`:**
  - leitura de `/proc/<pid>/net/tcp` e `tcp6`;
  - encaminhamento do host para dentro do namespace;
  - nomes fixos por destino (o espelho como `registry-1.docker.io`).
- **`sandmux-spec`:** a seção `network` ganha o arquivo de portas; a visão do Docker ganha o socket do TUN.
- **`sandmux`:**
  - no daemon, o gateway do Docker, a política mais restrita e o espelho de registry;
  - as portas no `StatsOf`, no painel e no `stats`;
  - o `doctor`.
- **`sandmux-proto`:** `PortInfo` e o protocolo 9.
- **Documentação:** `docs/network.md`, `docs/docker.md`, `docs/tui.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 9). O Docker de uma pasta é recriado na primeira sessão depois da troca, com a rede nova; imagens e volumes continuam.
