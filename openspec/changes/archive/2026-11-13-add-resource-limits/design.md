# Design

## Context

Veja proposal.md, seção Why. O estado atual que importa:

- **Lançamento.** Todo lançamento passa por `launch::prepare`, que resolve as camadas de configuração e devolve o comando do `sandmux-run --spec-fd N`.
  - O `run` sem terminal faz `exec` desse comando.
  - O daemon o executa na thread de lançamento, com um `pre_exec` que faz `setsid`, `TIOCSCTTY` e `PR_SET_PDEATHSIG`, confere o `getppid`, marca todo descritor com CLOEXEC e libera o memfd e a trava.
- **cgroup v2.** A máquina de referência tem os controladores `cpu`, `memory` e `pids` delegados ao `user@1000.service`. Os processos do usuário ficam em escopos sob `app.slice`.
- **Medição de 2026-11-13** com `systemd-run --user --scope`:
  - `MemoryMax` e `TasksMax` funcionam sem root: o fork falhou acima do limite e um processo que passou de 64 MB morreu por OOM dentro do escopo, com `oom_kill 1` em `memory.events`;
  - o `--scope` executa o comando no próprio processo, mantendo o pai e os descritores herdados;
  - leva cerca de 30 ms;
  - sem `--expand-environment=no`, o `systemd-run` expande `$VAR` nos argumentos;
  - o escopo some quando fica vazio.
- **Dentro do sandbox**, `/sys` é montado em somente leitura e o Landlock não dá escrita fora das pastas liberadas.

## Goals / Non-Goals

**Goals:**

- Um único ponto aplica os tetos, o preparo do lançamento, usado pelo `run` direto e pelo daemon.
- Os tetos entram nas camadas do change 3 como mais duas chaves, com a mesma precedência.
- A contagem de mortes por falta de memória sobrevive ao fim do escopo.

**Non-Goals:**

- Teto de CPU, de E/S ou de disco (decisão do usuário: memória e processos).
- Medir os contêineres do Docker privado (change 24).
- Gerenciar o cgroup sem systemd, escrevendo direto no cgroupfs.
- Alterar os tetos de uma sessão viva.

## Decisions

### 1. Escopo transitório do systemd por lançamento

Com suporte, o comando do lançador vira:

```
systemd-run --user --scope --quiet --collect --expand-environment=no \
  --description="sandmux: <harness>:<pasta>" \
  -p OOMPolicy=continue -p MemoryMax=<bytes> -p MemorySwapMax=0 -p TasksMax=<n> \
  -- <sandmux-run> --spec-fd <N>
```

- O `systemd-run` registra o escopo com o próprio PID e faz `exec` do lançador. O pai continua sendo o daemon, ou o `run` direto.
- O `PR_SET_PDEATHSIG` do `pre_exec` continua valendo depois dos dois `exec`, porque nenhum dos binários é setuid. Os descritores liberados (memfd da especificação, trava da camada) também passam.
- **Teto `"max"`:** a propriedade correspondente não é passada. Com `memory = "max"`, `MemorySwapMax` também não é passado, porque quem tira o teto não quer o sandbox morto por falta de swap.
- **`--expand-environment=no`** é obrigatório: um caminho com `$` no nome não pode ser expandido.
- **`OOMPolicy=continue`** é obrigatório: com o padrão (`stop`), o systemd para o escopo inteiro na primeira morte por falta de memória, matando o harness junto com o processo que estourou. Achado ao implementar, em 2026-11-13.

Alternativa descartada: `Delegate=yes` na unidade do daemon, com o daemon criando os cgroups no cgroupfs. Essa saída:

- não cobre o `run` direto nem o daemon iniciado sem systemd;
- obriga a tratar a regra de "nenhum processo interno" do cgroup v2, movendo o próprio daemon para uma folha;
- duplica o que o systemd já faz.

### 2. Detecção de suporte

A função `limits::support()` devolve suporte ou o motivo da falta. Ela confere três coisas, uma vez por processo:

1. o `systemd-run` está no `PATH`;
2. o systemd de usuário cria um escopo de teste (`systemd-run --user --scope -- true`), que é o mesmo caminho do lançamento. O `systemctl --user` não serve, porque fala com o systemd pelo socket privado em `$XDG_RUNTIME_DIR`, e não pelo barramento;
3. o `cgroup.subtree_control` do `user@<uid>.service` contém `memory` e `pids`. O caminho é achado pelo `/proc/self/cgroup`, subindo até o componente `user@<uid>.service`.

O `systemd-run --user` fala com o systemd pelo socket privado `$XDG_RUNTIME_DIR/systemd/private`, e não pelo barramento. Quando o `XDG_RUNTIME_DIR` do ambiente não tem esse socket, o sandmux chama o `systemd-run` com o `XDG_RUNTIME_DIR` do systemd, `/run/user/<uid>`. Isso acontece, por exemplo, nos testes, que usam outra pasta de runtime. Achado ao implementar, em 2026-11-13.

Sem suporte, `prepare` não envolve o comando e devolve um aviso: "esta sessão roda sem teto de memória e de processos: <motivo>; veja `sandmux doctor`". O mesmo motivo aparece no `doctor`.

### 3. Tipos e camadas

- **No `sandmux-policy`:**
  - `MemoryLimit { Percent(u8), Bytes(u64), Max }` e `ProcessLimit { Count(u32), Max }`, com `parse` e `Display`;
  - mínimos de 128 MiB e 32 processos;
  - porcentagem de 1 a 100;
  - sufixos `K`, `M`, `G` e `T` em potências de 1024, sem diferença entre maiúscula e minúscula.
- **Camadas:** `Layer` ganha `memory: Option<MemoryLimit>` e `processes: Option<ProcessLimit>`, e `Effective` ganha os dois como `Setting` com origem. Os padrões vêm da camada baseline: `Percent(75)` e `Count(8192)`.
- **Bytes:** `Effective::limits(mem_total)` converte a porcentagem pela `MemTotal` de `/proc/meminfo`, lida no lançamento.
- **No arquivo:** `memory` é um texto. `processes` é um inteiro, ou o texto `"max"`.
- **Na edição:** `sandmux config set` passa a aceitar `memory` e `processes`, validando o valor com o mesmo `parse`. `unset` apaga as duas.
- **Opções do `run`:** `--memory <valor>` e `--processes <valor>`, guardadas em `LaunchOptions` como texto e validadas pelo mesmo `parse`. Assim o `sandmux-proto` não depende da política.

### 4. Medição no daemon

- **Caminho do cgroup.** O daemon descobre o caminho do cgroup de uma sessão pelo `/proc/<pid>/cgroup` do processo que lançou, que depois do `exec` é o lançador dentro do escopo. A leitura acontece quando precisa e é guardada quando termina em `.scope`.
- **Arquivos lidos:**
  - `memory.current`, `memory.peak` e `memory.max`;
  - `pids.current`, `pids.peak` e `pids.max`;
  - `cpu.stat` (`usage_usec`);
  - `memory.events` (`oom_kill`) e `pids.events` (`max`).
- **Onde aparece:** o `ls` usa os valores atuais e os tetos; o `stats` usa tudo.
- **Mortes por falta de memória.** Uma thread por lançamento vigia `memory.events` com inotify (`IN_MODIFY`). O kernel avisa ali quando os contadores mudam. A cada aviso, a thread lê `oom_kill` para o `Session`.
  - É assim que a contagem sobrevive à morte do último processo, que apaga o escopo antes de o leitor do PTY ver o fim.
  - A thread termina com `IN_IGNORED` ou quando a sessão relança.
  - A mesma thread grava `pids.events` (`max`), para as recusas de criação de processo.
- **Sessão encerrada.** O `SessionInfo` leva a contagem de mortes do último lançamento. O cliente mostra "encerrada (137, memória esgotada)" quando a contagem é maior que zero.

Alternativas descartadas:

- Amostragem periódica: perde a morte que encerra a sessão.
- Não usar `--collect` e pedir ao systemd o `Result=oom-kill` do escopo: deixa escopos falhos acumulados e exige `reset-failed`.

### 5. Protocolo 3

- `LaunchOptions` ganha `memory: Option<String>` e `processes: Option<String>`, com `#[serde(default)]`. Assim a lista de sessões no formato 2 continua sendo lida sem conversão.
- `SessionInfo` ganha `usage: Option<Usage>` (memória, teto de memória, processos, teto de processos, cada teto opcional) e `oom_kills: u64`.
- Pedido novo `Stats { id }`, com a resposta `StatsOf { id, state, stats: Option<Stats>, oom_kills }`. `Stats` traz os valores atuais, os picos, os tetos, o CPU e as recusas.
- `PROTOCOL` passa a 3.

### 6. Formatação

- **Memória:** em unidades binárias, com uma casa decimal e vírgula: `1,2G`, `512M`. O teto padrão de 34,5 GB aparece como `34,5G`.
- **Colunas do `ls`:** MEMÓRIA (`1,2G/34,5G`, ou `1,2G/sem teto`) e PROC (`37/8192`). Ficam vazias fora do estado viva, ou quando o cgroup não pôde ser lido.
- **`stats`:** linhas "chave: valor", em português.

### 7. Testes

- **Systemd nos testes.** O ambiente dos testes de integração aponta `XDG_RUNTIME_DIR` para uma pasta temporária. O preparo do lançamento recorre ao `/run/user/<uid>` (decisão 2), e o `Env` dos testes ainda define `DBUS_SESSION_BUS_ADDRESS` com o barramento real. Os testes de teto pulam com mensagem quando não há suporte.
- **Escopos de teste.** Cada teste que lança cria escopos transitórios no systemd de usuário de quem roda os testes. Eles somem quando o sandbox termina, por causa do `--collect`.

## Risks / Trade-offs

- **[systemd-run falha depois do fork]** O barramento pode cair entre a detecção e o lançamento. → O `systemd-run` termina com erro na saída do PTY ou do `run`. A sessão aparece encerrada com o código dele e a mensagem fica no histórico. O `doctor` mostra a causa.
- **[Teto em porcentagem e RAM que muda]** Em máquinas virtuais, a RAM pode mudar entre lançamentos. → A porcentagem é calculada a cada lançamento, e o `stats` mostra o teto em bytes.
- **[Sem swap no sandbox]** Com `MemorySwapMax=0`, o sandbox não usa swap. Num pico, o OOM vem antes do que viria com swap. → O teto padrão é generoso (75%), e o `stats` mostra o pico para o usuário ajustar.
- **[Recuo silencioso demais]** Sem suporte, a sessão roda sem teto. → Há aviso a cada lançamento e a seção do `doctor`. O recuo segue o princípio do roadmap: o teto protege a máquina e não é fronteira de segurança.
- **[Testes dependem do systemd do desenvolvedor]** → Pulam com mensagem sem suporte. A suíte de fuga só exige que a gravação nos arquivos de cgroup falhe, o que vale com ou sem suporte.

## Migration Plan

1. Atualizar o binário e rodar `sandmux daemon restart`. O cliente novo recusa o daemon antigo pelo protocolo.
2. O daemon novo lê a lista de sessões do formato 2 (os tetos ausentes viram "não dado") e relança as vivas, já com os tetos padrão.

Rollback: instalar o binário anterior e rodar `sandmux daemon restart`. O binário anterior recusa as chaves `memory` e `processes` do `config.toml`, por serem desconhecidas, então quem voltar de versão precisa retirá-las do arquivo. A lista de sessões continua legível, porque os campos novos são ignorados na leitura. Isso fica documentado em `docs/limits.md`.
