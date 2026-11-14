# Proposal

## Why

Na TUI, nada mostra o que protege a sessão da aba nem quanto ela consome. Hoje o usuário precisa rodar `sandmux doctor` e `sandmux stats` noutro terminal. O roadmap decidiu um painel lateral fixo por aba, com o nível de proteção ativo e o consumo de recursos. Nas etapas seguintes, o mesmo painel recebe o registro de acessos, as portas e os MCPs. Este change cria o painel com o que já existe.

## What Changes

- **Painel lateral à direita**, com 34 colunas, sempre visível quando o terminal tem pelo menos 100 colunas. A área do terminal das abas encolhe na mesma medida, e as sessões recebem o tamanho novo.
- **Atalho `s`** (`panel` em `[tui.keys]`) para esconder e mostrar. A chave `panel` de `[tui]` (`true` ou `false`) define como a TUI abre.
- **Seção "Proteção"** da aba ativa:
  - nível de proteção (nível 1: namespaces, Landlock com a ABI e seccomp);
  - camada de escrita (persistente, temporária ou recuo);
  - pasta de trabalho e pastas extras, em somente leitura ou com escrita;
  - GPU e KVM;
  - escrita no clipboard;
  - variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente;
  - tetos de memória e de processos;
  - rede: "aberta, sem registro" até a etapa 4.
- **Seção "Recursos"** da aba ativa: memória (em uso, pico, teto), processos, CPU e mortes por falta de memória, e o Docker privado da pasta (contêineres, memória). Atualiza a cada segundo.
- **Aba encerrada ou parada:** o painel mostra o resumo do último lançamento e o estado.
- **Daemon:**
  - cada lançamento guarda o resumo da política resolvida;
  - o `StatsOf` ganha o resumo;
  - `PROTOCOL` passa a 6.
- **`sandmux stats`** mostra também o resumo da proteção, no mesmo formato.

## Capabilities

### New Capabilities
- Nenhuma.

### Modified Capabilities
- `terminal-ui`: requisito novo "Painel lateral". "Barra de abas" passa a dizer que a área do terminal fica à esquerda do painel.
- `resource-limits`: "Detalhe do consumo" ganha o resumo da proteção.

## Impact

- **`sandmux-proto`:** `PolicySummary` no `StatsOf`, e `PROTOCOL` 6.
- **`sandmux` (lançamento):** `Prepared` devolve o resumo da política (o nível vem do suporte do `sandmux-run --check`).
- **Daemon:** guarda o resumo no lançamento e o envia no `stats`.
- **TUI:** o painel, o atalho e a chave `panel`, o tamanho da área e o pedido `Stats` da aba ativa a cada tique.
- **`sandmux-policy`:** `panel` em `[tui]` e a ação `panel` em `[tui.keys]`.
- **Documentação:** `docs/tui.md` e `docs/limits.md`.
