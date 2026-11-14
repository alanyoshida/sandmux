# Proposal

## Why

O change 8 pôs todo o tráfego do sandbox num gateway que registra cada conexão, mas o gateway ainda não bloqueia nada além dos bloqueios fixos. Um agente manipulado pode mandar a pasta de trabalho para um site de colagem ou de envio de arquivos, e o usuário só vê depois, no registro. O roadmap decidiu a segunda metade da etapa 4: denylist com efeito imediato, catálogo de destinos de exfiltração ligado por padrão, os modos **só inferência** e **custom**, allowlist e a tela para gerenciar tudo isso pela TUI.

## What Changes

- **Modos de rede:** a chave `network` e a opção `--network` passam a aceitar quatro valores:

  | Modo | O que passa |
  |---|---|
  | `aberta` (padrão) | tudo, menos a denylist |
  | `inferencia` | só as APIs de inferência do catálogo de provedores |
  | `custom` | só a allowlist |
  | `sem-rede` | nada (só o `lo`, como no change 8) |

- **Denylist** (chave `deny`, opção `--deny`), em todo modo: "negar sempre vence". Já vem com o **catálogo de destinos de exfiltração**, versionado com o binário e ligado: sites de colagem, de envio de arquivos, coletores de requisição, domínios de OAST e túneis. Cada item pode ser desligado com `catalog-off`.
- **Allowlist** do modo custom (chave `allow`, opção `--allow`). Cada item pode ficar cadastrado e desligado, com `allow-off`. O **catálogo de domínios comuns** (github, gitlab, npm, pypi, crates, go, rubygems, maven, docker, node, rust e opencode) vem desligado, e cada grupo é ligado com `catalog`.
- **Catálogo de provedores de inferência** do modo só inferência: openrouter.ai, api.anthropic.com, api.openai.com, generativelanguage.googleapis.com, api.deepseek.com, api.mistral.ai, api.groq.com e api.x.ai, cada um desligável com `catalog-off`. O change 13 troca esse catálogo pelos provedores cadastrados no cofre.
- **Padrões:**
  - um domínio exato (`exemplo.com`);
  - um curinga de subdomínios (`*.exemplo.com`, que não pega o próprio `exemplo.com`);
  - um IPv4 ou uma faixa CIDR, para a rede local.
- **Como o gateway decide:**
  - pelo nome que o próprio sandbox consultou no DNS do gateway, e também pelo SNI do TLS e pelo `Host` do HTTP;
  - nos modos restritos, o IP de destino precisa ter vindo de uma consulta a um nome liberado, e o SNI ou o `Host`, quando existem, também precisam estar liberados. Assim, um SNI forjado não abre caminho para um IP qualquer;
  - um nome negado ou fora da allowlist recebe NXDOMAIN do DNS do gateway;
  - todo bloqueio é registrado com o motivo.
- **Efeito imediato nas sessões.** O daemon recompila a política de rede de cada sessão viva quando o arquivo de configuração muda, ou quando a regra é da aba, e grava o resultado num arquivo por lançamento. O gateway relê o arquivo na conexão seguinte. A troca entre `aberta`, `inferencia` e `custom` vale na hora; entrar ou sair de `sem-rede` vale no próximo lançamento. O `run` sem terminal, que não tem daemon, usa a política do lançamento.
- **Regras da aba.** `--deny` e `--allow` no `run` formam a camada da aba, guardada na lista de sessões. A TUI e o `sandmux net deny|allow <padrão> <id>` acrescentam a ela com a sessão viva.
- **Linha de comando:**
  - `sandmux config add deny|allow|allow-off|catalog|catalog-off <item>` nas camadas do arquivo;
  - `sandmux net deny|allow <padrão> <id>` e `sandmux net forget <padrão> <id>` na camada da aba;
  - `sandmux net rules [<id>]` mostra o modo e cada regra efetiva, com a origem e o estado.
- **TUI:**
  - uma aba inativa ganha `!` na barra quando a sessão tem um bloqueio, além do `•` de domínio novo;
  - a **tela de rede** (Ctrl+b `a`) mostra os acessos recentes da aba e as regras. Dali o usuário nega um acesso novo, libera um bloqueio (na aba, no projeto ou no global), liga e desliga itens dos catálogos e da allowlist, e troca o modo da aba.
- **Protocolo 8:** pedidos `NetRule` e `NetRules`.
- **Movidos para o change 10** (decisão do usuário, 2026-11-14): o cache de imagens do Docker privado entre pastas e a política mais restrita entre abas que dividem o Docker de uma pasta. Os dois dependem de o tráfego dos contêineres passar pelo gateway.

## Capabilities

### New Capabilities

- `network-policy`: modos aberta, só inferência e custom; denylist com o catálogo de exfiltração; allowlist com o catálogo de domínios comuns; catálogo de provedores de inferência; padrões; decisão do gateway; efeito imediato; regras da aba; `sandmux net rules`.

### Modified Capabilities

- `network-gateway`: o requisito "Modo de rede nas camadas" passa a aceitar os quatro modos.
- `config-profiles`: o requisito "Chaves de ajuste" ganha `network`, `net-log-days`, `net-log-max`, `deny`, `allow`, `allow-off`, `catalog` e `catalog-off`.
- `terminal-ui`: o requisito "Rede no painel e na barra" ganha a marca de bloqueio, e entra o requisito novo "Tela de rede".

## Impact

- **`sandmux-policy`:**
  - catálogos embutidos;
  - chaves novas nas camadas;
  - validação dos padrões;
  - compilação da política de rede.
- **`sandmux-spec`:** a seção `network` ganha a política compilada e o caminho do arquivo de política.
- **`sandmux-net`:** decisão por regra, NXDOMAIN, ligação de IP a vários nomes, releitura do arquivo de política.
- **`sandmux-proto`:**
  - `LaunchOptions` ganha `deny` e `allow`;
  - pedidos `NetRule` e `NetRules`;
  - `PROTOCOL` 8.
- **`sandmux`:**
  - o daemon grava os arquivos de política e vigia o arquivo de configuração;
  - `sandmux net deny|allow|forget|rules`;
  - a TUI ganha a tela de rede e a marca de bloqueio;
  - `--deny` e `--allow` no `run`.
- **Documentação:** `docs/network.md`, `docs/config.md`, `docs/tui.md`, `docs/sessions.md` e o README.
- **Migração:** `sandmux daemon restart` (protocolo 8). Quem não configura nada passa a ter o catálogo de exfiltração negado no modo aberta.
