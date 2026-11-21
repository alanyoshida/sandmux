## Why

Com os arquivos de uma aba pelo kernel (change 78), cada abertura numa pasta do agente vira um evento de 288 bytes na fila de 1 MiB. O caminho vai sempre com 256 bytes, e a mesma leitura repetida manda o evento de novo. O daemon tira as repetições, mas só depois de a fila passar por elas.

O auxiliar lê a fila e manda os eventos ao daemon na mesma thread. Enquanto o envio espera, a fila enche. Uma leitura em massa (um `grep -r`, um indexador, um `cat` de milhares de arquivos) passa da fila: as aberturas a mais saem do registro, e só a contagem de descartes aparece. O registro fica furado justo numa varredura, que é quando ele mais importa. O risco está no roadmap desde o change 78 (2026-11-20).

## What Changes

- **Repetições tiradas no kernel:** o programa da abertura guarda, num mapa LRU da aba, o arquivo (dispositivo, inode e geração) e o modo, com a hora do último evento. Uma abertura do mesmo arquivo no mesmo modo, em menos de 1 s, não manda evento e não chama o `bpf_d_path`.
  - Renomear ou criar um link de um arquivo pela aba tira o arquivo do mapa, e a abertura seguinte, com o caminho novo, vai para o registro.
  - Um evento que não coube na fila não entra no mapa, e a abertura seguinte do arquivo tenta de novo.
  - O daemon continua tirando as repetições do lançamento: o registro não muda (uma linha por caminho e acesso).
- **Evento do tamanho do caminho:** o evento de abertura leva só o caminho que o `bpf_d_path` escreveu, e não os 256 bytes. Um caminho comum cabe em menos da metade.
- **Fila esvaziada à parte:** no auxiliar, uma thread só lê a fila e passa os eventos a uma fila dele em memória. Outra monta as mensagens e as manda ao daemon. Um daemon lento não segura mais a fila do kernel.
- **A aba espera com a fila cheia** (decisão do usuário, 2026-11-21): quando a fila do kernel passa de 3/4, quando a fila do auxiliar passa do teto ou quando há um descarte, o auxiliar congela o cgroup da aba (`cgroup.freeze`). Quando as duas esvaziam, ele descongela, só com a conexão aberta: as regras da aba saem com a conexão, e quem descongela depois disso é o daemon, ao refazer o confinamento. A aba fica parada por alguns milissegundos, e o registro sai completo.
  - Ainda pode faltar um evento na janela entre o aviso e o congelamento. Ele continua contado na linha de descarte.
  - O Docker privado não congela: os eventos dele continuam descartados e contados, mas também passam pela thread que esvazia a fila.
- O auxiliar escreve no registro dele (o journal) cada congelamento, com a duração.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `root-helper`: o requisito "Eventos do confinamento" muda o que acontece com a fila cheia: o Docker segue descartando e contando. Três requisitos novos: "Aberturas repetidas de uma aba" (as de menos de 1 s ficam de fora), "Espera da aba com a fila cheia" e "Descongelamento só com a conexão aberta".
- `file-access-log`: o requisito "Aberturas pelo supervisor" ganha o cenário da leitura em massa numa aba confinada pelo kernel, com o registro completo.

## Impact

- `crates/sandmux-ebpf`:
  - `prog.rs`: o mapa LRU das aberturas e o tamanho variável do evento de abertura;
  - a limpeza do mapa nos ganchos de renomear e de link;
  - os campos novos do `Layout` (`i_generation` e as funções `ktime_get_ns`, `map_update_elem` e `map_delete_elem`);
  - `ring.rs`: o que estava pendente na fila no último esvaziamento.
- `crates/sandmux/src/helper.rs`:
  - o `forward_events` em duas threads, com a fila em memória;
  - o congelamento e o descongelamento do cgroup da aba pelo descritor que o daemon mandou.
- O protocolo entre o lançador e o daemon não muda, nem o do auxiliar. Um auxiliar antigo segue como hoje.
- Testes:
  - unitários na montagem dos programas e no congelamento, com eventos falsos;
  - com root, na VM, no 6.8 e no 7.0: dezenas de milhares de arquivos diferentes lidos na aba, com uma linha de cada um no registro e sem linha de descarte;
  - as aberturas repetidas, com uma linha só;
  - um arquivo renomeado pela aba e lido com o nome novo;
  - o custo da leitura em massa medido de novo.
- Documentação: `docs/files.md` ("Pelo kernel") e `docs/helper.md` (os eventos), com os pares em inglês.
