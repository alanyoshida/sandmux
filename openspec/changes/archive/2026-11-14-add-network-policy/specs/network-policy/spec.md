# Spec Delta

## Purpose
Define o que a rede do sandbox deixa passar: os modos aberta, só inferência e custom, a denylist com o catálogo de destinos de exfiltração, a allowlist com o catálogo de domínios comuns, o catálogo de provedores de inferência, os padrões aceitos, como o gateway decide cada conexão, o efeito imediato das mudanças e as regras da aba.

## ADDED Requirements

### Requirement: Modos de rede
A rede de um sandbox MUST seguir um de quatro modos: `aberta` (tudo passa, menos a denylist), `inferencia` (só os provedores do catálogo de inferência), `custom` (só a allowlist) e `sem-rede` (só o `lo`). Em todo modo, a denylist e os bloqueios fixos MUST valer.

#### Scenario: Modo custom
- **WHEN** a aba está no modo `custom` com `allow = ["host.sandmux.internal"]` e o agente acessa `host.sandmux.internal` e depois outro destino
- **THEN** a primeira conexão passa, e a segunda é bloqueada e registrada como fora da allowlist

#### Scenario: Modo só inferência
- **WHEN** a aba está no modo `inferencia` e o agente consulta `openrouter.ai` e `exemplo.com`
- **THEN** o DNS do gateway responde `openrouter.ai` e devolve NXDOMAIN para `exemplo.com`, com o bloqueio no registro

### Requirement: Denylist
A chave `deny` e a opção `--deny` MUST acrescentar padrões à denylist, somados entre as camadas. Um destino que casa com a denylist MUST ser bloqueado em todo modo, mesmo que também case com a allowlist ou com um catálogo ligado.

#### Scenario: Negar sempre vence
- **WHEN** o modo é `custom`, `allow = ["*.exemplo.com"]` e `deny = ["api.exemplo.com"]`
- **THEN** `www.exemplo.com` passa e `api.exemplo.com` é bloqueado

### Requirement: Catálogo de destinos de exfiltração
O sandmux MUST trazer um catálogo de destinos de exfiltração, versionado com o binário e ligado por padrão na denylist, com os itens do cenário "Itens do catálogo". Cada item MUST poder ser desligado pela chave `catalog-off`. O sandmux MUST NOT atualizar o catálogo pela rede.

#### Scenario: Itens do catálogo
- **WHEN** o usuário roda `sandmux net rules` sem configuração
- **THEN** a denylist tem, ligados, os sites de colagem `pastebin.com`, `paste.ee`, `dpaste.com`, `dpaste.org`, `paste.rs`, `termbin.com`, `ix.io`, `sprunge.us`, `hastebin.com`, `rentry.co`, `controlc.com`; os de envio de arquivos `transfer.sh`, `0x0.st`, `file.io`, `tmpfiles.org`, `catbox.moe`, `litter.catbox.moe`, `gofile.io`, `bashupload.com`, `temp.sh`, `oshi.at`, `wetransfer.com`, `*.wetransfer.com`, `*.sendspace.com`; os coletores de requisição `webhook.site`, `*.webhook.site`, `requestbin.net`, `*.m.pipedream.net`, `*.free.beeceptor.com`, `requestcatcher.com`, `*.requestcatcher.com`; os domínios de OAST e DNS de exfiltração `*.oast.fun`, `*.oast.live`, `*.oast.pro`, `*.oast.site`, `*.oast.online`, `*.oast.me`, `*.oastify.com`, `*.burpcollaborator.net`, `*.dnslog.cn`, `*.ceye.io`, `*.interact.sh`; e os túneis `*.ngrok.io`, `*.ngrok-free.app`, `*.ngrok.app`, `*.loca.lt`, `*.trycloudflare.com`, `serveo.net`, `*.serveo.net`, `*.localhost.run`, `*.pinggy.link`

#### Scenario: Colagem bloqueada por padrão
- **WHEN** a aba está no modo aberta, sem configuração, e o agente faz um pedido HTTP com `Host: pastebin.com`
- **THEN** a conexão é bloqueada, e o registro tem o motivo com o item do catálogo

#### Scenario: Item desligado
- **WHEN** o `[global]` tem `catalog-off = ["pastebin.com"]`
- **THEN** o pedido para `pastebin.com` passa, e os outros itens continuam bloqueados

### Requirement: Allowlist
A chave `allow` e a opção `--allow` MUST acrescentar padrões à allowlist do modo `custom`, somados entre as camadas. Um padrão listado em `allow-off`, em qualquer camada, MUST ficar cadastrado e desligado. Fora do modo `custom`, a allowlist MUST NOT ter efeito.

#### Scenario: Item cadastrado e desligado
- **WHEN** o modo é `custom`, o perfil do projeto tem `allow = ["host.sandmux.internal"]` e o `[global]` tem `allow-off = ["host.sandmux.internal"]`
- **THEN** o acesso a `host.sandmux.internal` é bloqueado, e `sandmux net rules` mostra o item como desligado

### Requirement: Catálogo de domínios comuns
O sandmux MUST trazer um catálogo de domínios comuns, desligado por padrão, com os grupos do cenário "Grupos do catálogo", ligados pela chave `catalog`. Um grupo ligado MUST entrar na allowlist do modo `custom`. Um nome de grupo desconhecido MUST tornar o arquivo inválido.

#### Scenario: Grupos do catálogo
- **WHEN** o usuário roda `sandmux net rules` com `catalog` ligando todos os grupos
- **THEN** a allowlist tem `github` (`github.com`, `*.github.com`, `*.githubusercontent.com`, `ghcr.io`), `gitlab` (`gitlab.com`, `*.gitlab.com`), `npm` (`registry.npmjs.org`, `registry.yarnpkg.com`), `pypi` (`pypi.org`, `files.pythonhosted.org`), `crates` (`crates.io`, `index.crates.io`, `static.crates.io`), `go` (`proxy.golang.org`, `sum.golang.org`), `rubygems` (`rubygems.org`, `index.rubygems.org`), `maven` (`repo1.maven.org`, `repo.maven.apache.org`), `docker` (`registry-1.docker.io`, `auth.docker.io`, `production.cloudflare.docker.com`), `node` (`nodejs.org`), `rust` (`static.rust-lang.org`) e `opencode` (`opencode.ai`, `models.opencode.ai`)

#### Scenario: Grupo ligado
- **WHEN** o modo é `custom` e o perfil do projeto tem `catalog = ["npm"]`
- **THEN** `registry.npmjs.org` passa, e `pypi.org` continua bloqueado

### Requirement: Catálogo de provedores de inferência
O modo `inferencia` MUST liberar só os provedores do catálogo de inferência: `openrouter.ai`, `api.anthropic.com`, `api.openai.com`, `generativelanguage.googleapis.com`, `api.deepseek.com`, `api.mistral.ai`, `api.groq.com` e `api.x.ai`. Cada provedor MUST poder ser desligado pela chave `catalog-off`.

#### Scenario: Provedor desligado
- **WHEN** o modo é `inferencia` e o `[global]` tem `catalog-off = ["api.openai.com"]`
- **THEN** `api.openai.com` é bloqueado, e `openrouter.ai` passa

### Requirement: Padrões de destino
Os itens de `deny`, `allow`, `allow-off` e `catalog-off` MUST ser um domínio exato (`exemplo.com`), um curinga de subdomínios (`*.exemplo.com`, que casa com qualquer subdomínio e não com o próprio `exemplo.com`), um endereço IPv4 ou uma faixa IPv4 em CIDR. Os domínios MUST ser comparados sem diferença de maiúsculas. Um item fora desses formatos MUST tornar o arquivo ou a opção inválidos, com o item na mensagem.

#### Scenario: Curinga
- **WHEN** `deny = ["*.exemplo.com"]`
- **THEN** `a.exemplo.com` e `a.b.exemplo.com` são bloqueados, e `exemplo.com` não

#### Scenario: Padrão inválido
- **WHEN** o `[global]` tem `deny = ["http://exemplo.com/x"]`
- **THEN** o arquivo é considerado inválido, com o item na mensagem

#### Scenario: Faixa da rede local
- **WHEN** o modo é `custom` com `allow = ["192.168.0.0/16"]` e o agente conecta em `192.168.1.10:5432`
- **THEN** a conexão passa

### Requirement: DNS pela política
Uma consulta ao DNS do gateway por um nome negado, ou fora do que o modo libera, MUST receber NXDOMAIN, e o bloqueio MUST ser registrado com o motivo.

#### Scenario: Nome negado
- **WHEN** o agente consulta `pastebin.com` no modo aberta, sem configuração
- **THEN** o DNS do gateway responde NXDOMAIN sem consultar o resolvedor do host, e o registro tem o bloqueio

### Requirement: Nome de uma conexão
O nome de uma conexão MUST vir das consultas que o próprio sandbox fez ao DNS do gateway, em que um IP pode estar ligado a vários nomes, do SNI do TLS e do `Host` do HTTP. Todo bloqueio de conexão MUST ser registrado com o motivo: o item da denylist ou do catálogo, "fora da allowlist", "fora do modo só inferência" ou "sem domínio".

#### Scenario: IP com vários nomes
- **WHEN** o modo é `custom`, `a.test` e `b.test` resolvem para o mesmo IP, só `b.test` é liberado e o agente consultou os dois
- **THEN** uma conexão HTTP ao IP com `Host: b.test` passa, e uma com `Host: a.test` é bloqueada

### Requirement: Decisão no modo aberta
No modo aberta, uma conexão MUST ser bloqueada quando o SNI ou o `Host` casam com a denylist, ou, sem eles, quando o nome mais recente ligado ao IP casa, ou quando o IP casa com uma faixa negada.

#### Scenario: Host negado no modo aberta
- **WHEN** o modo é aberta, `deny = ["proibido.test"]` e o agente manda a `host.sandmux.internal` um pedido HTTP com `Host: proibido.test`
- **THEN** a conexão é bloqueada, e o registro tem o motivo `proibido.test`

### Requirement: Decisão nos modos restritos
Nos modos `custom` e `inferencia`, uma conexão MUST passar só quando um nome ligado ao IP, ou o próprio IP, é liberado e, havendo SNI ou `Host`, ele também é liberado, e nada casa com a denylist. Uma conexão sem nome conhecido MUST ser bloqueada como "sem domínio".

#### Scenario: SNI forjado
- **WHEN** o modo é `custom` com `allow = ["host.sandmux.internal"]` e o agente conecta direto num IP que não consultou, mandando um TLS com SNI `host.sandmux.internal`
- **THEN** a conexão é bloqueada como "sem domínio"

### Requirement: Efeito imediato
Uma mudança na política de rede de uma sessão viva MUST valer na conexão seguinte, sem relançar o harness, quando vem do arquivo de configuração (pela linha de comando ou à mão) ou das regras da aba. A troca entre `aberta`, `inferencia` e `custom` MUST valer na hora; entrar ou sair de `sem-rede` MUST valer só no próximo lançamento, com aviso. O `run` sem terminal MUST usar a política do momento do lançamento.

#### Scenario: Negar com a sessão viva
- **WHEN** uma sessão acessa `host.sandmux.internal` e o usuário roda `sandmux config add deny host.sandmux.internal --global`
- **THEN** em até 2 segundos o próximo acesso da sessão é bloqueado, sem relançar o harness

### Requirement: Regras da aba
As opções `--deny` e `--allow` do `run` MUST formar a camada da aba, a mais específica, guardada na lista de sessões e mantida no reinício e na restauração. `sandmux net deny <padrão> <id>`, `sandmux net allow <padrão> <id>` e `sandmux net forget <padrão> <id>` MUST acrescentar ou tirar padrões da camada da aba de uma sessão, com efeito imediato.

#### Scenario: Liberar na aba
- **WHEN** a sessão 1 está no modo `custom` sem allowlist e o usuário roda `sandmux net allow host.sandmux.internal 1`
- **THEN** o acesso seguinte da sessão 1 a `host.sandmux.internal` passa, e o padrão continua na sessão depois de `sandmux daemon restart`

### Requirement: Regras efetivas
`sandmux net rules [<id>]` MUST mostrar o modo de rede e cada regra efetiva de uma sessão, ou da pasta atual sem `<id>`: os itens da denylist, da allowlist e dos catálogos, cada um com a origem (catálogo, global, perfil, projeto ou aba) e o estado (ligado ou desligado).

#### Scenario: Origem das regras
- **WHEN** o `[global]` tem `deny = ["a.test"]` e a sessão 1 foi lançada com `--deny b.test`
- **THEN** `sandmux net rules 1` mostra `a.test` vindo do global, `b.test` vindo da aba e os itens do catálogo de exfiltração como ligados
