# Design

## Context

- **Rede do sandbox (change 8).** O processo 1 cria o TUN `sandmux0` no namespace de rede novo e passa o descritor ao supervisor do `sandmux-run`, que roda o gateway numa thread com um runtime tokio. O gateway só aceita conexões que o sandbox inicia: a pilha (`ipstack`) não abre conexões em direção ao sandbox.
- **Docker privado (change 24).** O daemon sobe, por pasta, `rootlesskit` com `--net=slirp4netns|pasta|gvisor-tap-vsock`, `--port-driver=builtin` e `--pidns`, e dentro dele `sandmux-run --view-file` (a visão de arquivos) e o `dockerd`. As portas dos contêineres saem no `127.0.0.1` do host pelo driver `builtin`.
- **Provas de 2026-11-14:**
  - `rootlesskit --net=none --port-driver=builtin` cria um namespace só com o `lo` e publica no host uma porta aberta lá dentro.
  - Um processo sem privilégio não entra direto no namespace de rede de um sandbox (`setns` pede `CAP_SYS_ADMIN` também no próprio namespace de usuário). Entrando antes no namespace de usuário do sandbox, de que é dono, ele entra no de rede e conecta no `127.0.0.1` de lá.
- **Máquina:** sem o binário `registry` instalado.

## Goals / Non-Goals

**Goals:**

- O usuário abre no navegador o servidor que o agente subiu, sem configurar nada.
- Nenhum pacote dos contêineres sai sem o gateway.
- A mesma imagem do Docker Hub não é baixada de novo em outra pasta, quando o espelho existe.

**Non-Goals:**

- Publicar portas UDP.
- Publicar em outro endereço que não o `127.0.0.1` do host.
- Cache de outros registries além do Docker Hub (o `--registry-mirror` do `dockerd` só vale para o Docker Hub).
- Escolher à mão o número da porta do host.

## Decisions

### 1. Portas do sandbox: detecção pelo `/proc`, conexão por um auxiliar

- **Detecção:** o supervisor lê `/proc/<processo 1>/net/tcp` e `tcp6`, que mostram o namespace de rede do sandbox, a cada segundo. Entram os sockets em escuta (estado `0A`) em `127.0.0.1`, `0.0.0.0` ou `::`. O parser fica no `sandmux-net`, com testes de unidade.
- **Auxiliar de conexão:**
  - **Quando nasce:** o supervisor cria o auxiliar com `fork` logo depois do `clone` e antes do gateway, quando ainda tem uma só thread, exigência do `setns` no namespace de usuário.
  - **Como entra:** o auxiliar entra no namespace de usuário do processo 1 e depois no de rede.
  - **O que faz:** atende por um `socketpair`. Para cada pedido (uma porta), conecta em `127.0.0.1:<porta>` lá dentro e devolve o descritor conectado por SCM_RIGHTS.
  - **Proteções:** `PR_SET_PDEATHSIG` e `PR_SET_DUMPABLE` desligado; termina quando o supervisor fecha o par.
- **Publicação:**
  - para cada porta nova, o supervisor tenta `127.0.0.1:<porta>` no host e, se estiver ocupada, os próximos números, até 100 acima;
  - cada conexão aceita pede um descritor ao auxiliar e copia nos dois sentidos, no runtime do gateway;
  - quando a porta some da tabela, o ouvinte do host fecha.
- **Lista:** o supervisor grava `[{guest, host}]` em `ports_file` (novo na seção `network` da especificação), ao lado do arquivo de política (`$XDG_RUNTIME_DIR/sandmux/net/<etiqueta>.ports.json`), de forma atômica. No `run` sem terminal, o arquivo também é gravado, para `sandmux stats` não depender do daemon no futuro; o daemon só lê os das sessões.

Alternativa descartada: um processo dentro do sandbox fazendo o encaminhamento. Ficaria ao alcance do agente (mesmo usuário, mesmo pid namespace).

### 2. Docker pelo gateway

- **`rootlesskit --net=none`**, com o mesmo `--port-driver=builtin`. A detecção de `slirp4netns`, `pasta` e `gvisor-tap-vsock` sai do `support()` e do `doctor`.
- **A visão do Docker** (`sandmux-run --view-file`):
  - ganha `tun_socket`, um socket Unix do daemon;
  - antes de montar a raiz nova, sobe o `lo`, cria `sandmux0` com o mesmo endereço e a mesma rota do sandbox e manda o descritor pelo socket;
  - o `resolv.conf` da visão aponta para `10.0.2.3`.
- **O daemon:**
  - escuta o socket antes de subir o `rootlesskit`, recebe o descritor e chama `sandmux_net::spawn`;
  - usa a etiqueta `docker-<id>`, a pasta de trabalho e um arquivo de política `docker-<id>.json`;
  - o gateway vive com o Docker da pasta e para junto com ele.

### 3. Política mais restrita

- **Ordem dos modos:** `sem-rede` > `inferencia` > `custom` > `aberta`.
- **Escolha:** entre as sessões vivas da pasta, vale o modo mais restrito. Com o mesmo modo:
  - a denylist é a soma das ligadas;
  - a allowlist fica com os itens ligados em todas.
- **`sem-rede`:** no Docker, vira `custom` sem allowlist: nada passa, mas o namespace continua com o TUN.
- **Quando recompila:** quando uma sessão da pasta sobe ou termina, e na verificação da configuração a cada segundo do change 9.

### 4. Espelho do Docker Hub

- **Binário:** `registry` ou `docker-registry` no `PATH`.
- **Configuração:** o daemon grava `$XDG_STATE_HOME/sandmux/registry/config.yml` com:
  - armazenamento em `<dados>/registry-cache`;
  - `http.addr` em `127.0.0.1:<porta livre>`;
  - `proxy.remoteurl` `https://registry-1.docker.io`.
- **Vida:** o espelho sobe com o primeiro Docker privado e para com o daemon.
- **No `dockerd`:** `--registry-mirror=http://10.0.2.254:<porta>`, alcançado pelo gateway como serviço da máquina.
- **Política:** o `sandmux-net` ganha nomes fixos por destino (`aliases`). A conexão a `10.0.2.254:<porta do espelho>` é decidida e registrada como `registry-1.docker.io`. Assim, num Docker em modo `custom` sem o grupo `docker`, o espelho é bloqueado como o Docker Hub.

### 5. Portas no protocolo, no painel e no `stats`

- **`PortInfo {guest, host, source}`:** `source` é `sandbox` ou o nome do contêiner. O `StatsOf` ganha `ports`, e o `PROTOCOL` passa a 9.
- **Portas dos contêineres:** o daemon consulta a API do Docker da pasta (`GET /containers/json`), como já faz para contar os contêineres.
- **Painel:** a seção "Portas" mostra uma linha por porta.

## Risks / Trade-offs

- **[Porta diferente da pedida]** Com a porta ocupada no host, o número muda, e um link fixo no código do projeto não acerta. → A lista mostra a porta real, no painel e no `stats`.
- **[Espelho fora do gateway]** O processo `registry` busca as camadas pela rede do host, sem o gateway. → A decisão de política é tomada no gateway do Docker, pelo nome `registry-1.docker.io`, antes de o pedido chegar ao espelho; o espelho só fala com o Docker Hub.
- **[Docker recriado]** O Docker de uma pasta que estava rodando continua com a rede antiga até parar. → O daemon para o Docker de cada pasta com a última sessão; depois do `daemon restart`, sobe com a rede nova.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 9). As imagens e os volumes de cada pasta continuam. Para ter o cache entre pastas, instalar o pacote `docker-registry` (Debian e Ubuntu) ou o binário `registry`.
