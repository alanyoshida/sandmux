# Design

## Context

- **Composição.** `launch::prepare` resolve a política (`Effective`) e compõe a especificação. Ele sabe:
  - o modo da camada de escrita, pelo `LayerMode` escolhido;
  - as pastas extras, a GPU, o KVM, o clipboard e o `pass-env`;
  - os tetos aplicados, ou `None` sem suporte;
  - a ABI do Landlock, pelo relatório do `sandmux-run --check` (`SupportReport::landlock_abi`).
- **Daemon.** Guarda por sessão os tetos, o cgroup e os contadores, e responde `Stats` com `StatsOf`.
- **TUI.** Pede `List` a cada tique pela conexão de controle e desenha a barra e a área do terminal com o tamanho do terminal menos a barra.

## Goals / Non-Goals

**Goals:**

- O painel mostra o que foi aplicado no lançamento da sessão, e não o que a configuração diria agora.
- O painel não esconde a área do terminal em terminais estreitos.

**Non-Goals:**

- Registro de acessos, portas e MCPs (etapas 4 a 7, que acrescentam seções).
- Nível 2 (eBPF, pós-v1).

## Decisions

### 1. `PolicySummary`

Novo tipo em `sandmux-proto`, preenchido por `prepare` e guardado pelo daemon em cada lançamento:

- `level`: `"nível 1: namespaces, Landlock ABI <n>, seccomp"`;
- `layer`: `"persistente"`, `"temporária"` ou `"recuo: caches vazios, toolchains em somente leitura"`;
- `workdir`;
- `ro` e `rw`: as pastas extras;
- `gpu`, `kvm` e `clipboard`;
- `pass_env`: os nomes;
- `memory_max` e `processes_max`: os tetos aplicados, em que `None` é sem teto;
- `limits`: se os tetos estão ativos;
- `network`: `"aberta, sem registro"` até a etapa 4.

O `StatsOf` ganha `policy: Option<PolicySummary>`, com `#[serde(default)]`. `PROTOCOL` passa a 6.

### 2. Painel na TUI

- **Visibilidade:** o painel ocupa as últimas 34 colunas quando está ligado e o terminal tem pelo menos 100 colunas.
- **Tamanho da sessão:** a área do terminal tem `cols - 34` colunas. Ligar ou desligar o painel, ou redimensionar o terminal, refaz o tamanho de todas as abas, como no SIGWINCH.
- **Dados:** a cada tique, a TUI pede `Stats` da aba ativa pela conexão de controle e guarda o último `StatsOf`. O desenho usa esse valor. Ao trocar de aba, a TUI pede de novo, para não mostrar o da aba anterior.
- **Desenho:** um `Paragraph` com bordas à esquerda, em linhas curtas. Caminhos longos ficam com `…` no começo, e o HOME aparece como `~`.

### 3. Configuração

- `TuiConfig` ganha `panel: bool` (padrão `true`), lido de `[tui] panel`.
- A ação `panel` (tecla padrão `s`) entra em `TUI_ACTIONS`.

## Risks / Trade-offs

- **[Um pedido a mais por segundo]** O `Stats` lê alguns arquivos do cgroup e, com o Docker rodando, faz uma chamada à API dele. → Só para a aba ativa.
