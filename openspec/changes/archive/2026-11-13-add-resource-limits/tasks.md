# Tasks

## 1. Tipos e camadas

- [x] 1.1 Implementar no `sandmux-policy` `MemoryLimit` (porcentagem de 1 a 100, tamanho com `K`, `M`, `G` ou `T`, `max`) e `ProcessLimit` (número ou `max`), com `parse`, `Display` e os mínimos de 128 MiB e 32 processos; verificar com testes de unidade de cada formato aceito e recusado
- [x] 1.2 Acrescentar `memory` e `processes` a `Layer`, à leitura do `config.toml` (com a linha nos erros), à resolução (padrões `75%` e `8192` na baseline, vence a camada mais específica) e a `Effective::limits(mem_total)`; verificar com testes de unidade dos cenários "Teto inválido" e "Teto do projeto vence o global" e da conversão da porcentagem
- [x] 1.3 Fazer `sandmux config set` e `unset` aceitarem `memory` e `processes`, validando o valor, e `show` mostrá-los com a origem; verificar com testes de unidade da edição e teste de CLI do `show`

## 2. Escopo do systemd no lançamento

- [x] 2.1 Implementar a detecção de suporte (`systemd-run` no `PATH`, systemd de usuário respondendo, `memory` e `pids` no `cgroup.subtree_control` do `user@<uid>.service`), uma vez por processo, devolvendo o motivo da falta; verificar com teste de unidade da leitura do caminho do `user@` a partir de um `/proc/self/cgroup` de exemplo e do `subtree_control`
- [x] 2.2 Fazer `prepare` envolver o comando do lançador em `systemd-run --user --scope --quiet --collect --expand-environment=no` com `MemoryMax`, `MemorySwapMax=0` e `TasksMax` (omitindo o que é `max`), devolver os tetos efetivos em bytes e em número, e avisar sem suporte; verificar com teste de unidade do comando montado para cada combinação de tetos
- [x] 2.3 Acrescentar `--memory` e `--processes` ao `run` e a `LaunchOptions` (com `#[serde(default)]`), validados antes do lançamento; verificar com testes de CLI de valor inválido (`--memory 64M` recusado com o mínimo) e com a ajuda do `run`
- [x] 2.4 Fazer o `Env` dos testes definir `DBUS_SESSION_BUS_ADDRESS` com o barramento do usuário quando existe, e verificar com testes de integração sem terminal, que pulam com mensagem sem suporte:
  - `--processes 64` faz a criação de processos falhar dentro do sandbox;
  - `memory = "512M"` no perfil do projeto mata um programa que aloca 2 GB e mantém o shell;
  - `--memory max` tira o teto de memória e mantém o de processos;
  - o `memory.max` e o `pids.max` lidos dentro do sandbox batem com os tetos.
- [x] 2.5 Verificar com teste de integração que, sem suporte (barramento inexistente), o shell sobe e o aviso cita `sandmux doctor`

## 3. Proteção dos limites

- [x] 3.1 Acrescentar à suíte de fuga tentativas de gravar em `memory.max`, `pids.max` e `cgroup.procs` do próprio cgroup e do cgroup pai, e de criar um cgroup filho, todas negadas; verificar que os tetos lidos fora do sandbox não mudaram

## 4. Medição no daemon

- [x] 4.1 Ler o caminho do cgroup de uma sessão pelo `/proc/<pid>/cgroup` e os contadores (`memory.current`, `memory.peak`, `pids.current`, `pids.peak`, `cpu.stat`, `memory.events`, `pids.events`); verificar com testes de unidade da leitura sobre uma pasta de exemplo
- [x] 4.2 Vigiar `memory.events` e `pids.events` com inotify numa thread por lançamento, guardando as mortes por falta de memória e as recusas de processo na sessão; acrescentar a feature `inotify` do `nix`. Verificar com teste de integração em que o harness da sessão é morto pelo teto: o `ls` mostra "encerrada (137, memória esgotada)"
- [x] 4.3 Subir `PROTOCOL` para 3, acrescentar `usage` e `oom_kills` a `SessionInfo` e o pedido `Stats`; verificar com os testes de serialização do `sandmux-proto` e com o teste de leitura da lista de sessões do formato 2

## 5. Linha de comando

- [x] 5.1 Acrescentar ao `sandmux ls` as colunas MEMÓRIA e PROC (atual e teto, "sem teto", vazias fora de viva); verificar com teste de PTY de uma sessão viva e uma encerrada
- [x] 5.2 Implementar `sandmux stats <id>`:
  - memória atual, pico e teto;
  - processos atuais, pico e teto;
  - CPU usada;
  - mortes por falta de memória e recusas de processo.

  Uma sessão não viva mostra o estado e as mortes do último lançamento, e um identificador inexistente é erro listando as sessões. Verificar com testes de PTY dos cenários de "Detalhe do consumo"
- [x] 5.3 Acrescentar ao `doctor` a verificação dos limites de recurso (atendida, ou o motivo e o que fazer, com código zero); verificar com teste de CLI com e sem barramento

## 6. Documentação

- [x] 6.1 Escrever `docs/limits.md`, com:
  - os tetos padrão, os formatos e as camadas;
  - o escopo do systemd e o recuo sem suporte;
  - o `ls` e o `stats`;
  - a volta para a versão anterior.

  Atualizar `docs/config.md` (chaves novas), `docs/sessions.md` (protocolo 3, colunas e `stats`) e o README. Verificar com o teste de documentação, estendido para as chaves e os comandos novos

## 7. Verificação final

- [x] 7.1 Rodar `./scripts/check.sh` (testes, clippy e fmt) sem falhas, sem daemon de teste e sem escopo `run-*` de teste sobrando
- [x] 7.2 Verificar à mão uma sessão real do opencode com os tetos padrão: o `ls` mostra o consumo; o `stats` mostra os tetos (75% da RAM, 8192); um `sandmux run shell --memory 256M` com um programa que aloca 1 GB mostra a morte por falta de memória no `stats`; depois do `daemon restart`, a sessão volta com os tetos
