# Rede

Cada sandbox tem rede própria. Nenhum pacote do agente sai sem passar pelo gateway do sandmux, que refaz cada conexão a partir do host, registra o destino e decide pela política da aba: o modo de rede, a denylist e a allowlist.

## Como funciona

```text
sandbox                                   supervisor (sandmux-run, no host)
+-------------------------------+         +----------------------------------+
| lo                            |         | gateway                          |
| sandmux0  10.0.2.100/24       |  TUN    |   TCP e UDP refeitos pelo host   |
|   rota padrão 10.0.2.2  ------+-------->|   DNS em 10.0.2.3                |
|   DNS 10.0.2.3                |         |   10.0.2.254 -> 127.0.0.1 do host|
+-------------------------------+         |   registro em ~/.local/state/... |
                                          +----------------------------------+
```

- **Namespace de rede próprio, sem privilégio.** O processo 1 do sandbox cria a interface `sandmux0` (um TUN) e passa o descritor ao supervisor, o `sandmux-run` que fica no host. O agente não vê as interfaces do host.
- **Gateway no supervisor.** Uma pilha TCP/IP em espaço de usuário termina cada conexão do sandbox e abre outra a partir do host. O gateway vive exatamente enquanto o sandbox vive, e funciona também no `sandmux run` sem terminal, sem o daemon.
- **Só IPv4.** O sandbox não tem IPv6; os programas caem para IPv4.
- **Sem ICMP.** O `ping` não funciona.

## Endereços

| Endereço | O que é |
|---|---|
| `10.0.2.100` | o sandbox |
| `10.0.2.2` | rota padrão (o gateway) |
| `10.0.2.3` | servidor de nomes |
| `10.0.2.254` | a máquina: `host.sandmux.internal`, levado ao `127.0.0.1` do host |

O `/etc/resolv.conf` do sandbox aponta para `10.0.2.3`. O gateway responde `host.sandmux.internal` e repassa as outras consultas aos servidores de nome do host (os `nameserver` do `/etc/resolv.conf` dele, ou o `127.0.0.53`).

## Serviços da máquina

O `127.0.0.1` do sandbox é dele, não o do host. Para alcançar um serviço que roda na sua máquina (um banco local, um servidor de desenvolvimento, uma porta publicada pelo [Docker privado](docker.md)), use `host.sandmux.internal`:

```bash
curl http://host.sandmux.internal:8080/
```

A variável `SANDMUX_HOST` tem esse nome. A rede local (os outros computadores da sua rede) é alcançada normalmente.

### Portas do host

Um harness configurado no host para um serviço local, como o LM Studio em `http://127.0.0.1:8888/v1`, um Ollama ou um MCP em `127.0.0.1`, recebe `ECONNREFUSED` no sandbox, porque lá o `127.0.0.1` é do próprio sandbox. A chave `host-ports` leva essas portas do host para o `127.0.0.1` de dentro, sem editar a configuração do harness:

```bash
sandmux config add host-ports 8888 --global
sandmux run opencode --host-port 9000
```

- **Como funciona:** o supervisor escuta `127.0.0.1:porta` dentro do namespace de rede do sandbox e repassa cada conexão ao `127.0.0.1:porta` do host.
- **Registro:** cada conexão gera uma linha `tcp` para `host.sandmux.internal` em `127.0.0.1:porta`.
- **Política:** a porta é uma permissão explícita e vale em qualquer modo com rede, inclusive `custom` e `inferencia`. Só uma regra da denylist que casa com `host.sandmux.internal` a recusa. No modo sem rede não há gateway, e a chave não vale.
- **Ao vivo:** uma porta acrescentada com a sessão viva vale em até 2 segundos; uma tirada fecha.
- **Porta ocupada:** se o agente já escuta naquela porta dentro do sandbox, o encaminhamento não abre, e o registro diz o motivo (uma vez por minuto).
- **Chave no cofre:** um servidor local que pede chave (o Unsloth Studio, por exemplo) recebe o valor real sem ele entrar no sandbox. Guarde a chave no cofre com o domínio `host.sandmux.internal` (`sandmux vault add unsloth --domain host.sandmux.internal`, ou pelo `sandmux vault import`, quando a importação trocou a chave por um falso).
  - Quando a conexão começa com um pedido HTTP/1.x, ela passa pelo mesmo proxy das conexões decifradas, que troca o falso pelo real nos cabeçalhos e no caminho.
  - Cada pedido gera uma linha `http`, com o método, o caminho, o status e o nome do segredo.
  - Sem segredo para o host, ou sem HTTP, a conexão é copiada como está. Servidor local com TLS não tem troca.
- **Modelos no opencode:** o opencode só mostra, num provedor próprio, os modelos escritos na configuração. Ao lançar uma sessão do opencode, o daemon pergunta aos servidores locais quais existem e os cadastra só naquela execução, sem editar arquivos. A lista é refeita a cada lançamento: Ctrl+b r na aba pega um modelo novo do servidor.
  - **Quais provedores:** os que apontam para `127.0.0.1`, `localhost` ou `[::1]` (o `baseURL`, ou o padrão do `lmstudio`, porta 1234, e do `ollama`, porta 11434), com a porta em `host-ports`.
  - **Consulta:** é `GET <baseURL>/models`, com 1,5 s para conectar e 6 s para a resposta (o Unsloth Studio leva perto de 1,6 s). Um cabeçalho `Authorization` ou `apiKey` com valor falso vai com o real do cofre (domínio `host.sandmux.internal`), fora do sandbox.
  - **Resultado:** os modelos entram no `OPENCODE_CONFIG_CONTENT`, somados aos da configuração. Um servidor fora do ar deixa o provedor como está.
  - **Cofre trancado:** sem o cofre, a consulta não tem a chave, e a lista volta vazia. Por isso, depois de um reinício do daemon ou de um reboot, uma aba do opencode cujos provedores locais usam uma chave do cofre fica "aguardando cofre" e sobe, já com a lista, no desbloqueio.
- **Sem eco:** a porta do host não é publicada de volta como [porta de entrada](#portas-publicadas).
- **Importação:** quando a configuração importada de um harness cita `127.0.0.1`, `localhost` ou `[::1]` com porta, o `sandmux import` lista as portas e o comando de cada uma.
- **Aba de chat:** não recebe portas do host; um servidor local de modelo entra como provedor próprio ([chat.md](chat.md#provedor-próprio)).

## Modos

| Modo | O que passa |
|---|---|
| `aberta` (padrão) | Tudo, menos a denylist |
| `inferencia` | Só os provedores do [catálogo de inferência](#catálogo-de-provedores-de-inferência) |
| `custom` | Só a allowlist |
| `sem-rede` | Nada: só o `lo`, e toda conexão para fora falha na hora com rede inalcançável |

Em todo modo valem a denylist e os [bloqueios fixos](#bloqueios-fixos). O modo vem da chave `network` da [configuração](config.md), com a precedência das camadas, ou de `--network` no `run`:

```bash
sandmux run shell --network sem-rede
sandmux config set network custom --project ~/projetos/api
```

## Política

### Padrões

Os itens de `deny`, `allow`, `allow-off` e `catalog-off` são padrões de destino:

| Padrão | Casa com |
|---|---|
| `exemplo.com` | só `exemplo.com` (sem diferença de maiúsculas) |
| `*.exemplo.com` | qualquer subdomínio, como `a.exemplo.com` e `a.b.exemplo.com`, mas não `exemplo.com` |
| `192.168.1.10` | esse IPv4 |
| `192.168.0.0/16` | a faixa, útil para a rede local no modo `custom` |

Um padrão fora desses formatos (com esquema, caminho ou espaço) torna o arquivo ou a opção inválidos.

### Denylist

A chave `deny` e a opção `--deny` acrescentam padrões à denylist, somados entre as camadas. Negar sempre vence: um destino da denylist é bloqueado em todo modo, mesmo que também esteja na allowlist.

A denylist já vem com o **catálogo de destinos de exfiltração**, ligado e versionado com o binário. O sandmux nunca o atualiza pela rede. Cada item pode ser desligado com `catalog-off`:

- **colagem de texto:** `pastebin.com`, `paste.ee`, `dpaste.com`, `dpaste.org`, `paste.rs`, `termbin.com`, `ix.io`, `sprunge.us`, `hastebin.com`, `rentry.co`, `controlc.com`
- **envio de arquivos:** `transfer.sh`, `0x0.st`, `file.io`, `tmpfiles.org`, `catbox.moe`, `litter.catbox.moe`, `gofile.io`, `bashupload.com`, `temp.sh`, `oshi.at`, `wetransfer.com`, `*.wetransfer.com`, `*.sendspace.com`
- **coletores de requisição:** `webhook.site`, `*.webhook.site`, `requestbin.net`, `*.m.pipedream.net`, `*.free.beeceptor.com`, `requestcatcher.com`, `*.requestcatcher.com`
- **OAST e DNS de exfiltração:** `*.oast.fun`, `*.oast.live`, `*.oast.pro`, `*.oast.site`, `*.oast.online`, `*.oast.me`, `*.oastify.com`, `*.burpcollaborator.net`, `*.dnslog.cn`, `*.ceye.io`, `*.interact.sh`
- **túneis:** `*.ngrok.io`, `*.ngrok-free.app`, `*.ngrok.app`, `*.loca.lt`, `*.trycloudflare.com`, `serveo.net`, `*.serveo.net`, `*.localhost.run`, `*.pinggy.link`

```bash
sandmux config add catalog-off pastebin.com --global
```

### Allowlist

No modo `custom` só passa o que está na allowlist: os padrões de `allow` e `--allow`, somados entre as camadas, e os grupos ligados do catálogo comum. Um padrão em `allow-off`, em qualquer camada, fica cadastrado e desligado. Fora do modo `custom` a allowlist não tem efeito.

O **catálogo de domínios comuns** vem desligado. Cada grupo é ligado com `catalog`:

| Grupo | Domínios |
|---|---|
| `github` | `github.com`, `*.github.com`, `*.githubusercontent.com`, `ghcr.io` |
| `gitlab` | `gitlab.com`, `*.gitlab.com` |
| `npm` | `registry.npmjs.org`, `registry.yarnpkg.com` |
| `pypi` | `pypi.org`, `files.pythonhosted.org` |
| `crates` | `crates.io`, `index.crates.io`, `static.crates.io` |
| `go` | `proxy.golang.org`, `sum.golang.org` |
| `rubygems` | `rubygems.org`, `index.rubygems.org` |
| `maven` | `repo1.maven.org`, `repo.maven.apache.org` |
| `docker` | `registry-1.docker.io`, `auth.docker.io`, `production.cloudflare.docker.com` |
| `node` | `nodejs.org` |
| `rust` | `static.rust-lang.org` |
| `opencode` | `opencode.ai`, `models.opencode.ai` |

```bash
sandmux config set network custom --project ~/projetos/api
sandmux config add catalog npm --project ~/projetos/api
sandmux config add allow api.minha-empresa.com --project ~/projetos/api
```

### Catálogo de provedores de inferência

O modo `inferencia` libera só: `openrouter.ai`, `api.anthropic.com`, `platform.claude.com` (a renovação do login do Claude Code, [claude.md](claude.md)), `api.openai.com`, `generativelanguage.googleapis.com`, `api.deepseek.com`, `api.mistral.ai`, `api.groq.com`, `api.x.ai`. Cada provedor pode ser desligado com `catalog-off`. Quando o cofre de segredos existir, a lista passa a vir dos provedores cadastrados nele.

### Como o gateway decide

- **DNS:** um nome negado, ou fora do que o modo libera, recebe NXDOMAIN sem consultar o resolvedor do host.
- **Nome de uma conexão:** vem das consultas que o próprio sandbox fez ao DNS do gateway (um IP pode estar ligado a vários nomes, como numa CDN), do SNI do TLS e do cabeçalho `Host` do HTTP.
- **Modo aberta:** bloqueia quando o SNI ou o `Host` casam com a denylist, ou, sem eles, quando o nome mais recente do IP casa, ou quando o IP casa com uma faixa negada.
- **Modos `custom` e `inferencia`:** passa só quando um nome ligado ao IP (ou o próprio IP) é liberado e, havendo SNI ou `Host`, ele também é. Assim, um SNI forjado não leva a um IP que o sandbox não consultou. Uma conexão sem nome conhecido é bloqueada como "sem domínio".

- **SSH:** uma conexão TCP é SSH quando os primeiros bytes do cliente, ou do servidor (num cliente que espera o servidor falar), começam com `SSH-`, em qualquer porta. Depois da decisão do modo, ela só segue quando um nome ligado a ela casa com um servidor das [chaves SSH](vault.md#chaves-ssh) escolhidas na sessão; senão é fechada com o motivo `ssh fora dos servidores das chaves`. Sem chaves escolhidas (ou com o cofre trancado), nenhuma conexão SSH sai.

O motivo vai para o registro: o item da denylist (`denylist: x` ou `catálogo de exfiltração: x`), `fora da allowlist`, `fora do modo só inferência` ou `sem domínio`.

### Efeito imediato

Numa sessão viva, uma mudança vale na conexão seguinte, sem relançar o harness:

- no arquivo de configuração, por `sandmux config` ou à mão (o daemon confere o arquivo a cada segundo);
- nas regras da aba, por `sandmux net` ou pela tela de rede da TUI.

O daemon grava a política de cada lançamento em `$XDG_RUNTIME_DIR/sandmux/net/<etiqueta>.json`, e o gateway a relê quando o arquivo muda. A troca entre `aberta`, `inferencia` e `custom` vale na hora; entrar ou sair de `sem-rede` vale no próximo lançamento (`sandmux kill` e `sandmux restart`). O `run` sem terminal, que não tem daemon, usa a política do momento do lançamento.

### Regras da aba

As opções `--deny` e `--allow` do `run` formam a camada da aba, a mais específica. Ela fica guardada na lista de sessões e volta no reinício e na restauração. Com a sessão viva:

```bash
sandmux net deny api.exemplo.com 1     # nega na aba da sessão 1
sandmux net allow registry.npmjs.org 1 # libera na aba (modo custom)
sandmux net forget api.exemplo.com 1   # tira das regras da aba
sandmux net mode custom 1              # troca o modo da aba
sandmux net rules 1                    # modo e regras efetivas, com a origem
```

`sandmux net rules` sem a sessão mostra as regras da pasta atual. Cada linha tem o estado (ligado ou desligado), o padrão e a origem: um catálogo, `global`, `perfil <nome>`, `projeto <pasta>` ou `aba`.

## Decifração do TLS

O gateway decifra o TLS só dos domínios da lista de decifração:

- os domínios donos dos segredos do [cofre](vault.md) desbloqueado (é ali que, nas próximas etapas, a chave falsa vira a real);
- os padrões da chave `decrypt` da [configuração](config.md) e da opção `--decrypt` do `run`, somados entre as camadas.

O resto passa cifrado de ponta a ponta. A lista muda na hora, como as outras regras: desbloquear o cofre ou mudar a configuração vale na conexão seguinte.

- **CA do lançamento:** cada lançamento gera uma CA própria. A chave fica só na memória e no memfd da especificação, nunca em disco, e um sandbox não confia na CA de outro.
- **Confiança no sandbox:** o pacote de CAs do sistema (`/etc/ssl/certs/ca-certificates.crt` no Debian e no Ubuntu) é coberto por um com as CAs do sistema mais a do lançamento. `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE` e `GIT_SSL_CAINFO` apontam para ele.
- **No gateway:** a conexão do sandbox recebe um certificado do domínio assinado pela CA do lançamento, só com HTTP/1.1, e o gateway abre TLS até o destino real, verificando o certificado dele pelas CAs do sistema. Um certificado inválido do destino bloqueia a conexão, com o motivo `certificado inválido` no registro.
- **Registro:** cada pedido decifrado vira uma linha `https` com o método, o caminho sem a query e o status. Os cabeçalhos e o corpo nunca são gravados. O `sandmux net` e o painel da TUI mostram `GET /v1/models 200`.
- **Certificado:** o certificado do domínio leva o Authority Key Identifier e o uso `serverAuth`, que o Python 3.13+ exige na verificação estrita (`VERIFY_X509_STRICT`).
- **WebSocket:** um upgrade para WebSocket passa sem o cabeçalho `Sec-WebSocket-Extensions`, para que as mensagens não venham comprimidas, e as mensagens seguem nos dois sentidos até uma ponta fechar. Nas mensagens de texto do sandbox ao dono de um segredo, o falso vira o real ([hermes.md](hermes.md)). A conexão vira uma linha `https` com o status `101`, gravada no fim, com os bytes dos dois sentidos.
- **Servidor local com HTTPS:** para decifrar um destino com uma CA própria (um servidor de desenvolvimento com o `mkcert`), aponte `SANDMUX_UPSTREAM_CA` para o PEM dessa CA ao lançar.

```bash
sandmux run shell --decrypt api.exemplo.com
sandmux config add decrypt api.exemplo.com --project ~/projetos/api
```

## Portas publicadas

Uma porta TCP que o agente passa a escutar dentro do sandbox, em `127.0.0.1`, `0.0.0.0` ou `::`, é publicada no `127.0.0.1` do host em até 2 segundos: um servidor de desenvolvimento na porta 3000 abre no navegador em `http://127.0.0.1:3000`. Quando o número está ocupado no host, vale o próximo livre acima dele. A porta do host fecha quando o agente deixa de escutar.

- **Lista:** o `sandmux stats <id>` e a seção Portas do painel da TUI mostram cada porta, como `3000 → 127.0.0.1:3001 (sandbox)`.
- **Como funciona:** o supervisor lê as tabelas de sockets do namespace do sandbox. Um processo auxiliar, que entrou nos namespaces de usuário e de rede do sandbox, abre cada conexão lá dentro. O auxiliar fica fora do pid namespace do sandbox, longe do alcance do agente.
- **Só TCP e, sem allowlist, só no localhost do host.** Nada é publicado em outro endereço da máquina.
- **Docker:** as portas dos contêineres saem pelo Docker privado da pasta ([docker.md](docker.md)).

### Allowlist de portas

A chave `ports` da [configuração](config.md) (ou `--port` no `sandmux run`) troca a publicação de toda porta em escuta por uma allowlist: só as portas listadas saem do sandbox.

```toml
[global]
ports = ["8642", "8443@0.0.0.0"]
```

- **`8642`:** publicada no `127.0.0.1` do host, como antes. Ela não aceita conexões por outro endereço da máquina.
- **`8443@0.0.0.0`:** escuta também no endereço dado, no mesmo número, para receber webhooks direto (o gateway do hermes, por exemplo). Com `0.0.0.0`, um ouvinte só, em `0.0.0.0:8443`, cobre também o `127.0.0.1`. Com um endereço específico (`8443@192.168.1.10`), são dois ouvintes, o do `127.0.0.1` e o do endereço. O `sandmux run` avisa quando uma porta fica exposta à rede.
- **Número ocupado:** se o número estiver ocupado no endereço, a porta não é publicada nele, e o registro ganha uma linha `inbound` com o motivo em `blocked`. No `127.0.0.1` vale o próximo livre, como sem allowlist.
- **Ao vivo:** a allowlist vai na política de rede, que o gateway relê. Uma mudança na configuração vale em até 2 segundos, sem relançar o agente: as portas que saíram fecham, e as que entraram abrem.
- **Registro:** com a allowlist preenchida, cada conexão aceita vira uma linha `inbound` no registro, com o endereço de quem conectou, a porta do sandbox, os bytes nos dois sentidos e a duração.
- **Lista:** o `stats` e o painel mostram o endereço, como `8443 → 0.0.0.0:8443 (sandbox)`.

Uma porta em `0.0.0.0` aceita conexões de qualquer máquina que alcance a sua. Use um endereço específico quando bastar a rede local, e prefira um túnel com autenticação para a internet.

## Bloqueios fixos

Recusados sempre, e registrados com o motivo:

- **Metadados de cloud** (`169.254.169.254` e `fd00:ec2::254`): a conexão fecha na hora.
- **QUIC** (UDP na porta 443): os navegadores e clientes HTTP/3 caem para TCP. Assim o domínio aparece no SNI do TLS e entra no registro.
- **ICMP.**

## Registro

Cada conexão TCP, cada fluxo UDP e cada consulta de DNS vira uma linha JSON em `$XDG_STATE_HOME/sandmux/net/AAAA-MM-DD.jsonl` (normalmente `~/.local/state/sandmux/net/`), com pasta 700 e arquivo 600. O conteúdo das conexões nunca é gravado.

| Campo | O que é |
|---|---|
| `ts` | data e hora locais, quando a conexão terminou |
| `tag` | o lançamento: `s<sessão>-<n>` numa sessão, `run-<pid>` num `run` sem terminal |
| `workdir` | pasta de trabalho |
| `proto` | `tcp`, `udp`, `dns`, `icmp`, `https` (um pedido decifrado), `ssh` (uma conexão SSH) `ssh-sign` (uma assinatura do agente SSH; o `domain` é a impressão digital da chave e o `secret`, o nome dela) ou `inbound` (uma conexão de entrada numa [porta da allowlist](#allowlist-de-portas); o `domain` e o `ip` são o endereço de quem conectou, e o `port`, a porta do sandbox) |
| `domain` | domínio, ou "sem domínio" |
| `ip`, `port` | destino |
| `up`, `down` | bytes enviados e recebidos |
| `ms` | duração |
| `blocked` | motivo do bloqueio, ou `null` |
| `new` | o domínio apareceu pela primeira vez nesta pasta de trabalho |
| `method`, `path`, `status` | só nas linhas `https` (pedidos decifrados): o método, o caminho sem a query e o status da resposta |
| `secret` | só quando o gateway trocou a chave falsa pela real no pedido: o nome do segredo do [cofre](vault.md), nunca o valor; nas linhas `ssh-sign`, o nome da chave SSH (`assinatura ssh: <nome>` no `sandmux net` e no painel) |

O domínio vem da consulta de DNS que o próprio sandbox fez (o IP fica ligado ao nome por 10 minutos), do SNI do TLS ou do cabeçalho `Host` do HTTP. Uma conexão direta por IP fica como "sem domínio". Os domínios conhecidos de cada pasta ficam em `net/known/`.

### `sandmux net`

```bash
sandmux net      # as últimas 50 linhas de todas as sessões e dos `run`
sandmux net 1    # só da sessão 1
```

Mostra a hora, a sessão (`run` para um `run` sem terminal), o protocolo, o domínio, o destino, os bytes de cada lado, e "novo" ou o bloqueio. Lê as linhas de hoje e de ontem.

Na [TUI](tui.md), a seção Rede do painel mostra as últimas conexões da aba ativa, e uma aba inativa ganha `•` na barra quando acessa um domínio novo e `!` quando tem um bloqueio. A tela de rede (Ctrl+b `a`) mostra os acessos e as regras da aba e permite negar, liberar, ligar e desligar itens e trocar o modo.

### Retenção

O daemon apaga os arquivos com mais de 30 dias e, se a soma passar de 500 MB, os mais velhos até caber. Roda quando o daemon sobe e de hora em hora. Os dois limites ficam em `[global]`:

```toml
[global]
net-log-days = 7
net-log-max = "1G"
```

## Desempenho

Uma pilha TCP/IP em espaço de usuário é mais lenta que a do kernel, como no `slirp4netns`. Na máquina de referência, um download de 148 MB pelo gateway saiu a 11,6 MB/s, a mesma velocidade do host: o limite foi o link, não o gateway.

## Limites

- **CDN compartilhada no modo aberta.** Um destino da denylist atrás da mesma CDN de um site liberado só é pego pelo SNI ou pelo `Host`. Quem precisa de garantia usa o modo `custom`.
- **Conexões já abertas** não são cortadas por uma regra nova; a regra vale nas conexões seguintes.
- **Decifração só com HTTP/1.1:** nos domínios decifrados, o gateway oferece só HTTP/1.1; programas que fixam o certificado do servidor falham nesses domínios.
- **Conexões longas** só aparecem no registro quando terminam.
- **Sem IPv6 e sem ICMP** dentro do sandbox.
