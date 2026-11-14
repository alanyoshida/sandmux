# Design

## Context

- **Gateway do change 8.** Roda no supervisor do `sandmux-run`, fora do sandbox, e recebe na `LaunchSpec` a seção `network` (modo, pasta do registro, etiqueta, pasta de trabalho, nameservers). Termina cada conexão TCP, repassa UDP, responde o DNS em `10.0.2.3` e liga IP a domínio pelas respostas A. Lê o primeiro pedaço de cada conexão TCP (até 300 ms) para achar o SNI ou o `Host` antes de conectar no destino, então já sabe o nome antes de abrir a conexão de saída.
- **Política.** O `sandmux-policy` resolve as camadas (baseline, preset, `[global]`, perfil nomeado, perfil do projeto, opções do `run`) e compõe a especificação. O lançador e o gateway não conhecem política: recebem tudo pronto.
- **Sessões.** O daemon guarda as `LaunchOptions` de cada sessão (a camada da aba) e as etiquetas `s<id>-<n>` dos lançamentos.

## Goals / Non-Goals

**Goals:**

- Bloquear antes de abrir a conexão de saída, e antes de responder o DNS.
- Mudança de regra vale na conexão seguinte de uma sessão viva.
- O gateway continua sem conhecer camadas: recebe uma política compilada.

**Non-Goals:**

- Decifrar TLS e decidir por caminho (change 12).
- Docker privado pelo gateway, com o cache de imagens e a política mais restrita entre abas da mesma pasta (change 10, decisão do usuário em 2026-11-14).
- Provedores de inferência vindos do cofre (change 13).
- IPv6 no sandbox.

## Decisions

### 1. Política compilada na especificação

O `sandmux-spec` ganha `NetPolicy`:

- `mode`: `open`, `inference`, `custom` ou `none`;
- `rules`: lista de `{pattern, kind, origin, enabled}`, com `kind` = `deny` ou `allow`.

O `sandmux-policy` compila a política a cada lançamento, e o daemon a cada mudança:

- **Denylist:** os itens do catálogo de exfiltração (origem `catálogo`, desligados quando aparecem em `catalog-off`) e os `deny` de todas as camadas.
- **Allowlist:**
  - no modo `custom`, os `allow` de todas as camadas (desligados quando aparecem em `allow-off`) e os grupos ligados por `catalog`;
  - no modo `inferencia`, os provedores do catálogo de inferência (desligados por `catalog-off`).

Os itens desligados vão junto, marcados como `enabled = false`, para `sandmux net rules` e a tela de rede mostrarem o estado sem recalcular.

Os catálogos ficam num módulo do `sandmux-policy` (`netcatalog`), como as listas embutidas do change 1.

### 2. Padrões

`Pattern` com três formas:

- `Exact("exemplo.com")`;
- `Suffix("exemplo.com")`, escrito `*.exemplo.com`, que casa com `x.exemplo.com` e não com `exemplo.com`;
- `Net(Ipv4Addr, prefixo)`, escrito `10.0.0.5` ou `10.0.0.0/8`.

- **Validação:** rótulos de 1 a 63 caracteres com letras, dígitos e `-`, sem `-` nas pontas, pelo menos dois rótulos (ou o nome `host.sandmux.internal`), até 253 caracteres; tudo em minúsculas.
- **Erros:** um padrão inválido torna o arquivo inválido, com o item; nas opções do `run` e no `sandmux net`, recusa o comando.

### 3. Decisão no gateway

O gateway guarda, por IP, os nomes que o sandbox consultou (até 8 por IP, por 10 minutos): um IP de CDN pode servir vários nomes.

- **DNS:**
  - nome negado → NXDOMAIN e linha `dns` com o motivo;
  - nos modos `custom` e `inferencia`, nome não liberado → NXDOMAIN;
  - o resto é repassado como no change 8.
- **TCP e UDP:** sejam `sni` o SNI ou o `Host` (quando há) e `nomes` os nomes ligados ao IP (mais `host.sandmux.internal` para `10.0.2.254`).

  | Modo | Bloqueia quando |
  |---|---|
  | `aberta` | o `sni` casa com a denylist, ou, sem `sni`, o nome mais recente do IP casa com ela, ou o IP casa com uma faixa negada |
  | `custom` e `inferencia` | o IP não casa com a allowlist e nenhum dos `nomes` é liberado (motivo "sem domínio" quando não há nome nenhum); ou o `sni` existe e não é liberado; ou qualquer coisa casa com a denylist |

  A regra do `sni` nos modos restritos impede que um SNI forjado leve a um IP qualquer: o IP precisa ter vindo de uma consulta liberada.
- **UDP:** usa os `nomes` do IP, sem `sni`.
- **Motivos registrados:**
  - `denylist: <padrão>` (ou `catálogo de exfiltração: <padrão>`);
  - `fora da allowlist`;
  - `fora do modo só inferência`;
  - `sem domínio`.

Alternativa descartada: decidir só pelo SNI. Seria simples, mas o agente escolhe o SNI que quiser.

### 4. Efeito imediato por arquivo de política

A especificação ganha `policy_file: Option<PathBuf>`.

- **Gateway:**
  - parte da política inline da especificação;
  - a cada decisão, compara o tamanho e a data de modificação do arquivo e relê a política quando mudam (a primeira versão do design previa uma verificação a cada 250 ms; na implementação, um `stat` por decisão saiu barato e garante que a regra vale na conexão seguinte);
  - um arquivo ilegível ou inválido mantém a política anterior.
- **Daemon:**
  - grava `$XDG_RUNTIME_DIR/sandmux/net/<etiqueta>.json` (pasta 700, gravação atômica por arquivo temporário e `rename`) antes do lançamento;
  - grava de novo quando a política de uma sessão viva muda;
  - apaga o arquivo quando o lançamento termina.
- **Quando o daemon recompila:**
  - quando a data de modificação do arquivo de configuração muda (verificada a cada segundo por uma thread);
  - quando a camada da aba de uma sessão muda.
- **`run` sem terminal:** fica com `policy_file` vazio e a política do lançamento, porque não há daemon para atualizar.
- **`sem-rede`:** o namespace sem TUN não tem como ganhar rede depois, e o namespace com TUN não deve perder o gateway no meio de uma conexão. Por isso entrar ou sair de `sem-rede` com a sessão viva só vale no próximo lançamento: o daemon mantém a política anterior e a resposta ao pedido traz um aviso.

Alternativa descartada: um canal do daemon para cada supervisor. Exigiria um socket por sandbox, e o `run` sem terminal continuaria sem atualização. O arquivo é simples e o custo é um `stat` por decisão.

### 5. Camada da aba

- **`LaunchOptions`:** ganha `deny` e `allow` (com `serde(default)`, para ler a lista de sessões do protocolo 7).
- **Pedido `NetEdit {id, action, value}`**, com `action`:
  - `deny` ou `allow`: acrescenta o padrão;
  - `forget`: tira o padrão das duas listas;
  - `mode`: troca `opts.network`.

  O daemon valida, guarda na lista de sessões, recompila e grava o arquivo da sessão. Responde `Ok` ou `Warning` (no caso de `sem-rede`).
- **`PROTOCOL` passa a 8.**
- **`sandmux net rules`** é calculado no cliente, como o `sandmux restart` já faz: com `<id>`, pelas opções guardadas da sessão (vindas do `List`); sem `<id>`, pela pasta atual. Não precisa de pedido novo.

### 6. TUI

- **Marca na barra:** `•` para domínio novo e `!` para bloqueio. As duas vêm do mesmo pedido `Net` com `since`, já feito a cada tique.
- **Tela de rede** (`network`, tecla `a`): um modal em duas listas, alternadas com Tab.
  - **Acessos:** as últimas 50 linhas da aba, mais novas primeiro, com domínio, destino, volume e "novo" ou o bloqueio. Comandos: `n` nega o domínio (ou o IP, sem domínio), `l` libera.
  - **Regras:** as regras efetivas com a origem e o estado. Comandos: espaço liga ou desliga um item (de catálogo ou da allowlist), `x` tira um padrão da camada de onde veio, `m` troca o modo.
  - **Camada:** negar, liberar, ligar e desligar perguntam onde gravar, com `a` (aba), `p` (perfil do projeto) ou `g` (global). Na aba, a TUI manda `NetEdit`; no projeto e no global, edita o arquivo de configuração pelo mesmo código do `sandmux config` (o daemon percebe a mudança).
  - **Modo:** a troca de modo vale só na aba.

### 7. Configuração

- **Chaves novas nas camadas:** `deny`, `allow`, `allow-off`, `catalog-off` (padrões) e `catalog` (nomes de grupo). São listas, somadas entre as camadas.
- **`sandmux config add|remove`** passam a aceitar essas chaves, e o `config show` mostra o modo e a contagem de regras.
- **`network`** aceita `aberta`, `inferencia`, `custom` e `sem-rede`.

## Risks / Trade-offs

- **[CDN compartilhada]** No modo aberta, um destino da denylist atrás da mesma CDN de um site liberado só é pego pelo SNI ou pelo `Host`. → Um cliente que manda SNI vazio para um IP de CDN passa. Aceito: o modo aberta é "tudo menos a denylist"; quem quer garantia usa `custom`.
- **[Conexões abertas]** Uma regra nova vale nas conexões seguintes; as já abertas não são cortadas. → Registrado; cortar conexões abertas fica para depois.
- **[NXDOMAIN em modo restrito]** Programas podem tentar outros resolvedores por IP. → Saem pelo gateway e são bloqueados como "sem domínio".

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 8). A lista de sessões do protocolo 7 é lida sem mudança: `deny` e `allow` começam vazios. Quem usa o modo aberta passa a ter o catálogo de exfiltração negado; para desfazer um item, `catalog-off`.
