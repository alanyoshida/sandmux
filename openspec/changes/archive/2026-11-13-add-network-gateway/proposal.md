# Proposal

## Why

Hoje o sandbox usa a rede do host sem filtro e sem registro. O agente fala com qualquer destino e nada fica anotado. A ameaça alvo do projeto é um agente manipulado tentando exfiltrar dados, e sem ver a rede não há como detectar nem bloquear isso. O roadmap decidiu a etapa 4: todo o tráfego do agente passa por um gateway, é registrado e pode ser bloqueado.

Este change faz a base:

- network namespace por sandbox;
- interface sem privilégio;
- DNS e conexões pelo gateway;
- modos aberta e sem rede;
- bloqueios fixos;
- registro, com destaque para domínio novo no painel.

A denylist, a allowlist e os modos só inferência e custom vêm no change 9.

Uma prova feita nesta máquina, em 2026-11-13, mostrou que isso funciona sem root. Um processo num network namespace novo criou um TUN e passou o descritor por SCM_RIGHTS a um gateway fora do namespace. O gateway usou uma pilha TCP/IP em espaço de usuário (o crate `ipstack`, sobre tokio) e refez as conexões a partir do host. Um `curl` lá dentro fez HTTP e HTTPS, e o gateway respondeu o DNS e viu cada conexão, com o destino e a porta.

## What Changes

- **Network namespace por sandbox.** O `sandmux-run` cria um namespace de rede junto com os outros e, dentro dele, uma interface TUN `sandmux0` com o endereço `10.0.2.100/24` e a rota padrão por `10.0.2.2`. O `lo` também sobe. O `/etc/resolv.conf` do sandbox aponta para `10.0.2.3`.
- **Gateway no supervisor do sandbox.** O processo supervisor do `sandmux-run` (fora do sandbox e fora do Landlock) recebe o descritor do TUN e roda o gateway, numa pilha TCP/IP em espaço de usuário:
  - cada conexão TCP é terminada e refeita a partir do host;
  - cada datagrama UDP é repassado;
  - o DNS em `10.0.2.3` é respondido com o resolvedor do host.

  O gateway morre junto com o sandbox. Assim o `run` sem terminal também passa pelo gateway, sem depender do daemon. Isso substitui o "gateway no daemon" do roadmap; o daemon e a TUI leem o registro que o gateway grava.
- **Modos:**
  - **aberta** (padrão): tudo passa e é registrado;
  - **sem rede**: o namespace só tem o `lo`, e qualquer conexão para fora falha na hora.

  O modo vem da chave `network` do `config.toml` e da opção `--network` do `run`, pelas mesmas camadas.
- **Bloqueios fixos em todo modo:**
  - metadados de cloud (`169.254.169.254` e `fd00:ec2::254`);
  - QUIC (UDP para a porta 443), para os programas caírem para TLS comum, em que o domínio aparece;
  - ICMP, ou seja, sem `ping`.
- **Rede local e serviços da máquina:**
  - no modo aberta, a rede local é alcançável;
  - o nome especial `host.sandmux.internal` (respondido pelo DNS do gateway com `10.0.2.254`) leva ao `127.0.0.1` do host, e o agente recebe o nome na variável `SANDMUX_HOST`.
- **Registro de rede**, em `$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl`, uma linha por conexão ou consulta. Cada linha tem:
  - a data e a hora;
  - o identificador do sandbox (o da sessão, ou `run`);
  - a pasta de trabalho;
  - o protocolo;
  - o domínio, vindo do DNS que o próprio sandbox consultou, ou do SNI do TLS, ou do `Host` do HTTP;
  - o IP e a porta;
  - os bytes de cada lado;
  - a duração;
  - o bloqueio, quando houve.

  Uma conexão direta por IP, sem consulta de DNS antes, fica marcada como "sem domínio". Nada do conteúdo é gravado.
- **Retenção:** 30 dias e 500 MB, ajustáveis por `[global]` com `net-log-days` e `net-log-max`. Arquivos mais velhos ou acima do teto são apagados quando o daemon sobe e uma vez por hora.
- **Painel lateral:**
  - a seção nova "Rede" mostra o modo e as últimas conexões da aba ativa, com domínio e volume;
  - um domínio visto pela primeira vez naquela pasta de trabalho aparece destacado como "novo";
  - a aba ganha uma marca na barra quando aparece domínio novo.
- **`sandmux net [<id>]`** mostra as últimas conexões de uma sessão, ou de todas.
- **Fora deste change, registrado no roadmap:**
  - o tráfego do Docker privado pelo mesmo gateway vai para o change 10, junto com a publicação de portas, porque muda a rede do Docker inteira;
  - baixar o requisito de kernel da etapa 1 (o escopo de sockets abstratos do Landlock deixa de ser necessário com namespace de rede próprio) fica para o change 23, de instalação e níveis.

## Capabilities

### New Capabilities
- `network-gateway`: o namespace de rede, a interface, o gateway, o DNS, os modos aberta e sem rede, os bloqueios fixos, a rede local e o nome da máquina, o registro e a retenção, e `sandmux net`.

### Modified Capabilities
- `sandbox-isolation`: "Rede do host sem filtro nesta fatia" sai (REMOVED, com a migração para a capacidade nova).
- `terminal-ui`: "Painel lateral" ganha a seção Rede e a marca de domínio novo na barra.

## Impact

- **Crate novo `sandmux-net`:**
  - o gateway: pilha `ipstack` sobre tokio, encaminhamento TCP e UDP, DNS, bloqueios, extração de domínio por SNI e `Host`, e eventos de registro;
  - a gravação do registro.
- **`sandmux-spec`:** seção `network` na especificação: modo, endereços, caminho do registro, identificador e pasta.
- **`sandmux-launcher`:** `CLONE_NEWNET`, criação do TUN no namespace novo antes do Landlock, configuração dos endereços por netlink ou ioctl, `resolv.conf` gerado, e o gateway no supervisor.
- **`sandmux-policy`:** chave `network`, opção `--network`, chaves de retenção e a seção `network` na composição.
- **Daemon e TUI:** retenção, leitura do registro para o painel e para `sandmux net`, e a marca de domínio novo.
- **Dependências:** `tokio`, `ipstack` e `etherparse` (usado pelo `ipstack`).
- **Documentação:** `docs/network.md`, novo; README, `docs/tui.md` e `docs/config.md`.
- **Testes:** usam servidores locais pelo `host.sandmux.internal` e nomes resolvidos sem internet. Nenhum teste depende de acesso externo.
