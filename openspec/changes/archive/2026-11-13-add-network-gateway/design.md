# Design

## Context

- **Lançador.** O `sandmux-run` faz `clone` com user, mount, pid, ipc e uts namespaces. O filho (processo 1) espera o supervisor escrever os mapas de UID, monta a raiz com `setup_root` e executa o comando sob Landlock e seccomp. O supervisor fica no host, repassa sinais e espera o filho. Hoje o sandbox usa a rede do host.
- **`/etc/resolv.conf`.** Na máquina de referência é um link para `/run/systemd/resolve/stub-resolv.conf`, com `nameserver 127.0.0.53`. A composição monta o arquivo real no mesmo caminho, dentro do `/run` do sandbox. O lançador recusa montar por cima de um link simbólico.
- **Prova de 2026-11-13** (proposal.md): TUN em namespace de rede sem privilégio, descritor passado por SCM_RIGHTS, `ipstack` sobre tokio no gateway, DNS repassado ao `127.0.0.53`. HTTP e HTTPS funcionaram.

## Goals / Non-Goals

**Goals:**

- Nenhum pacote do sandbox sai sem passar pelo gateway.
- O gateway não depende do daemon. Funciona no `run` sem terminal.
- O registro tem domínio sempre que o sandbox o revelou (DNS, SNI, `Host`).

**Non-Goals:**

- Denylist, allowlist, catálogo, modos só inferência e custom (change 9).
- Decifrar TLS (change 12).
- Docker privado pelo gateway e publicação de portas (change 10).
- IPv6 no sandbox. Só IPv4 nesta fatia; os pacotes IPv6 são descartados.

## Decisions

### 1. Onde roda o gateway: no supervisor do `sandmux-run`

O supervisor já fica fora do sandbox, vive exatamente enquanto o sandbox vive e existe nos dois caminhos (daemon e `run` direto).

- **Passagem do TUN:** antes do `clone`, o lançador cria um `socketpair` (CLOEXEC). O filho cria o TUN no namespace novo e o passa ao supervisor por SCM_RIGHTS.
- **Gateway:** o supervisor sobe o gateway numa thread com um runtime tokio de duas threads de trabalho.
- **Thread única:** o lançador só tem uma thread até o `clone` (exigência do `clone_fork`), e o gateway nasce depois.
- **Saída:** a máscara de sinais bloqueados é herdada pelas threads do gateway, e o laço de sinais do supervisor continua igual. Quando o filho termina, o supervisor sai e o gateway acaba junto.

Alternativa descartada: o gateway no daemon, recebendo o TUN. O `run` sem terminal ficaria sem rede controlada, e o daemon teria de cuidar de um descritor por sandbox. O lançador recebe a política de rede pronta na especificação (modo, caminhos, identificador) e só a aplica, como já faz com as montagens.

### 2. Rede no filho

O filho passa a ser criado com `CLONE_NEWNET`, e a configuração da rede acontece antes de `setup_root`, enquanto o `/dev/net/tun` do host ainda está visível:

- **`lo`** sobe por ioctl `SIOCSIFFLAGS`.
- **Modo aberta:**
  - abre `/dev/net/tun`;
  - `TUNSETIFF` com `IFF_TUN | IFF_NO_PI`, nome `sandmux0`;
  - endereço `10.0.2.100/24` por `SIOCSIFADDR` e `SIOCSIFNETMASK`;
  - sobe a interface com MTU 1500;
  - rota padrão por `10.0.2.2` com `SIOCADDRT`;
  - manda o descritor pelo `socketpair` e o fecha.
- **Modo sem rede:** só o `lo`.

O processo 1 tem `CAP_NET_ADMIN` no user namespace dono do namespace de rede, o que basta, e perde as capabilities antes do comando, como hoje.

### 3. DNS e `resolv.conf`

- **No sandbox:** o `/etc/resolv.conf` tem `nameserver 10.0.2.3`.
  - Quando o arquivo do host é um link para dentro de `/run`, a composição põe um arquivo gerado no destino do link (`Mount::File`, dentro do tmpfs de `/run`).
  - Quando é um arquivo comum, põe o arquivo gerado por cima dele.
- **No gateway:** o DNS atende UDP 53 em `10.0.2.3`.
  - Lê o nome consultado (parser próprio, sem dependência).
  - Responde `host.sandmux.internal` com `10.0.2.254`.
  - Repassa o resto aos `nameserver` do `/etc/resolv.conf` do host, lido pelo supervisor, ou ao `127.0.0.53`.
  - Das respostas, lê os registros A, para ligar IP a domínio por 10 minutos.

### 4. Encaminhamento

- **TCP:**
  - destino `10.0.2.254` vai para `127.0.0.1` do host, na mesma porta;
  - os metadados de cloud são recusados;
  - os outros destinos são conectados a partir do host.
  - O gateway lê o primeiro pedaço que o cliente manda, esperando até 300 ms, para achar o SNI (TLS ClientHello) ou o `Host` (HTTP). Depois conecta, escreve esse pedaço e copia nos dois sentidos, contando os bytes.
- **UDP:**
  - porta 53 em `10.0.2.3`: DNS;
  - porta 443: QUIC, bloqueado;
  - o resto é repassado por um socket do host, com as respostas por 30 s sem tráfego.
- **Outros protocolos (ICMP):** descartados e registrados, no máximo uma linha por destino por minuto.

### 5. Registro

- **Arquivo:** linhas JSON em `$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl` (pasta 700, arquivo 600), com `O_APPEND`, uma escrita por linha.
- **Campos:** `ts`, `tag`, `workdir`, `proto` (`tcp`, `udp`, `dns`, `icmp`), `domain`, `ip`, `port`, `up`, `down`, `ms`, `blocked` e `new`.
- **`new`:** verdadeiro quando o domínio aparece pela primeira vez para a pasta. A lista de domínios conhecidos de cada pasta fica em `net/known/<FNV da pasta>.txt`, acrescentada pelo gateway.
- **Etiqueta (`tag`) de cada lançamento:**
  - `s<id>-<n>` nas sessões, com o identificador da sessão reservado antes do lançamento;
  - `run-<pid>` no `run` direto.

  A sessão guarda as etiquetas dos últimos lançamentos na lista de sessões, para o histórico.
- **Retenção:** feita pelo daemon, ao subir e de hora em hora, com `net-log-days` (padrão 30) e `net-log-max` (padrão `500M`), lidos de `[global]`.

### 6. Daemon, TUI e `sandmux net`

- **Pedido novo `Net { id: Option<u32>, since: Option<String>, limit }`:** o daemon lê os arquivos de hoje e de ontem, filtra pelas etiquetas das sessões e devolve as linhas com o identificador da sessão. `PROTOCOL` passa a 7.
- **Painel:** a seção Rede pede as últimas 6 linhas da aba ativa a cada tique.
- **Barra:** um pedido geral com `since` marca com `•` as abas inativas que tiveram domínio novo.
- **`sandmux net [<id>]`** mostra as últimas 50 linhas.

### 7. Configuração

- **`network`** (`aberta` ou `sem-rede`) entra em `Layer`, com a precedência das chaves de liga e desliga, e `--network` entra no `run`.
- **`net-log-days`** (inteiro) e **`net-log-max`** (tamanho, como `memory`) só são aceitas em `[global]`.
- **O resumo da política** passa a mostrar "aberta, registrada" ou "sem rede".

### 8. Testes sem internet

Os testes de integração sobem servidores TCP e UDP no `127.0.0.1` do host e os alcançam por `host.sandmux.internal`. O DNS é testado com o nome especial e com `localhost` mandado direto ao `10.0.2.3`. Os bloqueios são testados com destinos que nunca saem do gateway.

## Risks / Trade-offs

- **[Desempenho]** Uma pilha TCP/IP em espaço de usuário é mais lenta que a do kernel. → É o que o `slirp4netns` e o `pasta` fazem; o gargalo é o mesmo que um agente aceita para instalar pacotes. Medido à mão no fim.
- **[Programas que dependem de IPv6]** Sem IPv6 no sandbox. → Os resolvedores e clientes caem para IPv4. Registrado como limite.
- **[Ping]** Sem ICMP: `ping` falha. → Decisão do roadmap.
- **[Gateway no lançador]** O lançador passa a ter código de rede. → A política vem pronta da especificação, e o gateway não decide nada além dela. O código fica num crate próprio (`sandmux-net`).

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 7). As sessões relançadas passam a ter rede pelo gateway, no modo aberta, sem configuração.
