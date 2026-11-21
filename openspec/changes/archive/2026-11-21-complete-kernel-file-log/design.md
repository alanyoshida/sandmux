## Context

O motivo está em proposal.md, em "Why". O que importa para o desenho:

- O programa do gancho `file_open` de uma aba (`crates/sandmux-ebpf/src/prog.rs`, `Gen::open_event`) olha se a montagem do arquivo está no mapa das registradas. Se está, monta na pilha um evento de `EV_HEADER + EV_NAME` (32 + 256) bytes, chama o `bpf_d_path` e manda o evento inteiro por `bpf_ringbuf_output`. Na falha, soma 1 ao contador de descartes.
- A fila de uma aba tem 1 MiB (`RING_SIZE_TAB`). O `Reader::poll` (`ring.rs`) copia tudo o que está pendente e avança a posição do consumidor de uma vez.
- No auxiliar, `forward_events` (`crates/sandmux/src/helper.rs`) faz numa thread só o `poll`, a montagem do JSON e o envio ao daemon, que pode esperar.
- O daemon (`daemon/dockerfiles.rs`, `entries`) tira as repetições por caminho e acesso (`opened`) e grava uma linha por acesso distinto no lançamento.
- O cgroup que o daemon manda no `confine` é o escopo próprio da aba (o `own_scope` do change 78), que é delegado ao usuário. O auxiliar é root e escreve nele.
- O daemon também congela esse escopo, na recuperação de uma queda do auxiliar (change 79). Ele congela primeiro e descongela depois de refazer tudo. O `cgroup.freeze` é um valor só, sem contagem de quem congelou.

## Goals / Non-Goals

**Goals:**

- Uma varredura de dezenas de milhares de arquivos numa aba sai inteira no registro, nos kernels 6.8 e 7.0.
- O custo da leitura em massa no kernel não piora; com as repetições tiradas, tende a cair.
- O mapa das marcas de segurança (o armazenamento por inode) não muda.

**Non-Goals:**

- O Docker privado não congela. Os eventos dele são de bloqueio, de criação e de execução, e o congelamento de uma slice inteira de contêineres fica para outro change, se fizer falta.
- O shell de entrada (`Ctrl+b t`) continua com o supervisor.
- A janela entre o aviso de fila cheia e o congelamento não é fechada; ela só fica pequena.

## Decisions

### 1. Repetições num mapa LRU por aba, com janela de 1 s

O mapa é um `BPF_MAP_TYPE_LRU_HASH` com 16384 entradas, criado com o confinamento. Ele só existe quando há montagens a registrar.

- A chave tem 24 bytes: o inode (u64), o `s_dev` do superbloco (u32), o `i_generation` (u32), o modo (o `FMODE_WRITE` do arquivo, u32) e 4 bytes de enchimento zerados.
- O valor é a hora do último evento (`bpf_ktime_get_ns`).

O programa olha a montagem, depois o mapa. Se o arquivo está no mapa há menos de 1 s, a abertura segue sem evento e sem `bpf_d_path`. Senão, monta e manda o evento. Só com o `bpf_ringbuf_output` bem-sucedido ele grava a hora no mapa.

- **A geração entra na chave:** o ext4 reaproveita números de inode. Um arquivo apagado e criado de novo no host, com o mesmo número, tem outra geração, e não é tomado pelo anterior.
- **A janela de 1 s:** limita o estrago de qualquer caso que o mapa não vê, como um arquivo renomeado no host durante a sessão. Uma leitura em massa repete os mesmos arquivos em rajadas curtas, e a janela as cobre.
- **Renomear e criar link pela aba:** os programas de `inode_rename` e `inode_link` da aba apagam do mapa as duas chaves do inode de origem (leitura e escrita), com `bpf_map_delete_elem`. A abertura seguinte, com o caminho novo, vira evento dentro da janela.
- **O LRU dá um teto de memória fixo**, perto de 1,5 MB por aba. Uma entrada expulsa só faz a próxima abertura do arquivo mandar evento, e o daemon tira a repetição.

Alternativas descartadas:

- **Bits de "já aberto" no armazenamento por inode das marcas.** Não teria expulsão por tamanho, e alocaria memória do kernel para cada inode aberto. Também misturaria o registro com o mapa que decide a segurança. Com as marcas regravadas pelo daemon e o `OR` do programa, a ordem das escritas teria de ser provada segura.
- **A montagem (`mnt_id`) na chave, no lugar do dispositivo:** os ganchos de renomear não sabem a montagem, e a limpeza ficaria impossível.

Consequência aceita: o mesmo inode visto por duas montagens registradas (uma pasta extra dentro da pasta de trabalho, por exemplo) só vira evento pelo primeiro caminho, dentro da janela.

### 2. Evento do tamanho do caminho

O `bpf_d_path` devolve o tamanho escrito, com o NUL. Com um resultado fora de 1 a 256, o evento não vai, e o arquivo não entra no mapa, como hoje, em que o daemon ignora um caminho vazio. Com um resultado válido, o tamanho mandado é `EV_HEADER + R0`, e o verificador vê o limite pelos testes de faixa. O `Event::decode` já corta o nome pelo tamanho gravado no evento e não depende do tamanho do registro.

### 3. Duas threads no auxiliar, com uma fila em memória

O `forward_events` vira duas threads:

- **A leitora:** chama o `poll` e passa os eventos já em JSON para uma `VecDeque` com `Mutex` e `Condvar`. Ela não espera o envio.
- **A que envia:** tira lotes da fila e os manda como hoje, juntando o número de descartes ao lote seguinte.

O `Reader::poll` passa a guardar quantos bytes estavam pendentes na fila do kernel no último esvaziamento (`backlog`). O `Events` ganha `fn backlog(&self) -> (u64, u64)` (pendentes e tamanho), que por padrão devolve `(0, 0)`.

### 4. A aba espera com a fila cheia

O `confine` com montagens guarda, na conexão, uma cópia do descritor do cgroup. O `forward_events` recebe o descritor e o congela pelo `cgroup.freeze` (`openat` no descritor, escrita de `1` ou `0`).

- **Congela** quando os bytes pendentes no último esvaziamento passam de 3/4 da fila, quando a fila do auxiliar passa de 32768 eventos ou quando o `poll` traz descartes.
- **Descongela** quando a fila do auxiliar cai abaixo de 4096 e os pendentes do último esvaziamento ficam abaixo de 1/4. Congelada, a leitora espera no `poll` por 50 ms, e não 300, para descongelar logo que a fila esvazia.
- **Registra cada congelamento** no stderr do auxiliar, que vai para o journal, com a duração: "aba congelada por N ms (fila de eventos cheia)".

**Só com a conexão aberta:** o auxiliar não descongela depois do fim da conexão, nem com o servidor fechando (`closing`, do change 79). As regras da aba saem com a conexão, e o daemon pode ter congelado a aba para a recuperação; o `cgroup.freeze` não diz quem congelou. Descongelar ali deixaria a aba rodar sem as regras. A conferência do fim e a escrita do descongelamento ficam sob uma trava que o `serve` também toma ao marcar o fim, antes de soltar o confinamento, e assim não há corrida entre as duas.

- **Na recuperação**, o daemon descongela depois de refazer o confinamento.
- **Com o daemon morto**, a aba fica congelada. É o mesmo caso do risco aceito no change 78, de o daemon morrer com a aba congelada, e é o lado seguro: sem a conexão, a aba não tem mais as regras.

Alternativas descartadas:

- **Esperar dentro do kernel:** um programa BPF LSM preso a cgroup não dorme, e o `bpf_ringbuf_reserve` não espera espaço.
- **Negar a abertura com a fila cheia:** recusado pelo usuário (2026-11-21), porque os programas veriam erros de abertura no meio de uma varredura.
- **Só aumentar a fila:** adia o problema, porque uma varredura longa enche qualquer tamanho.
- **Congelar pelo daemon:** o daemon não vê a fila do kernel. Quem esvazia a fila é quem sabe a hora de congelar, sem uma ida e volta a mais.

## Risks / Trade-offs

- **[A janela entre passar de 3/4 e o congelamento valer]** A leitora acorda pelo `epoll`, e o congelamento para cada processo no retorno ao modo usuário. O 1/4 que sobra comporta uns 2,5 mil eventos de tamanho comum. → O que passar disso continua contado na linha de descarte, e o teste da leitura em massa confere que ela não aparece.
- **[Uma aba parada sem aviso visível ao usuário]** Os congelamentos duram milissegundos, e cada um vai para o journal do auxiliar com a duração. → Se aparecer uma aba lenta, o `journalctl -u sandmux-helper` mostra se foi a fila.
- **[O congelamento do daemon e o do auxiliar no mesmo arquivo]** → O auxiliar só descongela com a conexão aberta e o servidor sem fechar (decisão 4). Morto à força, ele não descongela nada, e o daemon refaz e descongela.
- **[O daemon morre com a aba congelada pela fila]** → A aba fica congelada até o usuário a encerrar, como no change 78.
- **[Programas maiores no `file_open`]** São uma busca, uma leitura da hora e uma gravação a mais. → O teste de custo mede de novo no 6.8 e no 7.0, na VM.
- **[Renomear no host]** A abertura pelo nome novo, dentro de 1 s da última pelo nome antigo, não vira evento. → É a janela da decisão 1. Depois de 1 s, vira.

## Migration Plan

Nada a migrar. Um auxiliar antigo continua com uma thread só e com os descartes; um daemon novo com um auxiliar antigo funciona como hoje. Os testes com root rodam só na VM (`scripts/vm-ubuntu-2404.sh check-root`), no 6.8 e no 7.0. Nenhum programa novo ou alterado é carregado no host.
