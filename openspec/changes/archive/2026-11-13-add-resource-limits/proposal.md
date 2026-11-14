# Proposal

## Why

Hoje um agente confinado pode consumir toda a memória da máquina ou disparar processos sem fim (uma compilação descontrolada, um teste em loop, uma fork bomb vinda de prompt injection), derrubando o desktop e as outras sessões. O roadmap decidiu um teto generoso de processos e de memória por sessão, ajustável, e a medição do consumo de cada sessão.

Agora há onde ajustar o teto (os perfis do change 3) e onde mostrar o consumo (as sessões do change 2). Uma medição feita nesta máquina, em 2026-11-13, mostrou que um escopo transitório do systemd de usuário aplica os dois limites sem root: o kernel recusou o fork acima do limite e matou por OOM só o processo dentro do escopo.

## What Changes

- **Teto por sandbox**, aplicado a cada lançamento:
  - **memória:** 75% da RAM da máquina, sem uso de swap. Estourar o teto mata processos só dentro daquele sandbox;
  - **processos e threads:** 8192. Acima disso, criar processo falha só dentro do sandbox.
- **O mesmo teto em todos os caminhos:** sessões do daemon e `run` sem terminal. Cada lançamento roda num escopo transitório do systemd de usuário (`systemd-run --user --scope`), com o lançador dentro dele. O sandbox não consegue alterar nem sair do escopo.
- **Ajustável pelas camadas do change 3:**
  - chaves novas `memory` e `processes` no `config.toml`;
  - opções novas `--memory` e `--processes` no `run`;
  - vence a camada mais específica, como nas chaves de liga e desliga;
  - a memória aceita porcentagem da RAM (`"50%"`), tamanho (`"16G"`) ou `"max"`, que tira o teto; os processos aceitam um número ou `"max"`.
- **Máquina sem suporte** (sem systemd de usuário, ou sem os controladores `memory` e `pids` delegados): o lançamento segue sem teto, com aviso a cada lançamento. O `sandmux doctor` mostra a causa e o que fazer. É o mesmo princípio do recuo do overlay: não quebrar o uso por uma proteção de recurso, que não é fronteira de segurança.
- **Medição:**
  - `sandmux ls` ganha as colunas MEMÓRIA e PROC, com o consumo atual e o teto (`1,2G/34G`, `37/8192`);
  - o comando novo `sandmux stats <id>` mostra o detalhe: memória atual e pico, processos atuais e pico, CPU usada, processos mortos por falta de memória e criações de processo recusadas pelo teto;
  - uma sessão encerrada depois de uma morte por falta de memória aparece como "encerrada (137, memória esgotada)".
- **Protocolo 3:** a lista de sessões enviada ao cliente ganha o consumo, e as opções de sessão ganham os tetos.

A medição dos contêineres do Docker privado, prevista no roadmap para este change, fica para o change 24 (`add-container-engine`), que cria esse Docker. Lá, os contêineres de uma pasta entram na conta da sessão.

## Capabilities

### New Capabilities
- `resource-limits`: tetos de memória e de processos por sandbox, os padrões, o comportamento ao estourar, o recuo sem suporte, a proteção dos limites contra o agente, e a medição em `sandmux ls` e `sandmux stats`.

### Modified Capabilities
- `config-profiles`:
  - "Chaves de ajuste" ganha `memory` e `processes`;
  - "Precedência nas chaves de liga e desliga" passa a valer também para os dois tetos.
- `terminal-sessions`: "Listar sessões" ganha as colunas de consumo e a nota de memória esgotada.
- `sandbox-isolation`: "Diagnóstico de suporte da máquina" ganha a verificação dos limites de recurso.

## Impact

- **`sandmux-policy`:**
  - as chaves `memory` e `processes` na leitura, na validação, na resolução das camadas e na edição;
  - um tipo para os tetos;
  - o cálculo da porcentagem pela RAM da máquina (`/proc/meminfo`).
- **`sandmux` (preparo do lançamento):**
  - o comando do lançador passa a ser envolvido por `systemd-run --user --scope --quiet --collect --expand-environment=no` com `MemoryMax`, `MemorySwapMax=0` e `TasksMax`, quando há suporte;
  - a detecção do suporte, com o aviso.
- **Daemon:**
  - lê o consumo do cgroup do escopo de cada sessão;
  - vigia `memory.events` com inotify para contar as mortes por falta de memória antes de o escopo sumir;
  - guarda os tetos efetivos de cada sessão.
- **`sandmux-proto`:** `SessionInfo` ganha o consumo, `LaunchOptions` ganha `memory` e `processes`, e `PROTOCOL` passa a 3.
- **CLI:** `--memory`, `--processes`, as colunas novas do `ls`, `sandmux stats` e a seção nova do `doctor`.
- **Dependências:** a feature `inotify` do `nix`.
- **Documentação:** `docs/limits.md`, novo; `docs/config.md`, `docs/sessions.md` e o README atualizados.
- **Compatibilidade:** sem configuração, todo lançamento passa a ter os tetos padrão. O cliente novo recusa o daemon antigo e indica `sandmux daemon restart`.
