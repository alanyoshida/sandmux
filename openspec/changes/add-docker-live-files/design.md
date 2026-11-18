## Context

Motivação em `proposal.md`. O que importa para o desenho, do change 70 (`openspec/changes/archive/2026-11-17-add-ebpf-docker-policy/`):

- **Cada confinamento** tem um mapa `INODE_STORAGE` com bits por inode (`DENY`, `PROTECT`, `GITDIR`) e dez programas `BPF_LSM_CGROUP` presos à slice da pasta. O auxiliar guarda um descritor `O_PATH` de cada arquivo marcado.
- **O verificador só aceita ponteiros confiáveis no mapa:** os argumentos do gancho, o `f_inode` de um `file` e o `d_inode` de uma `dentry` recebida. Ler números (o `i_ino`, o `s_dev` do superbloco) por um ponteiro não confiável é permitido. Passá-lo a um auxiliar que exige confiança, não.
- **As marcas saem da especificação da visão** (`docker_marks`) e do conteúdo das pastas negadas e de `hooks` (`DockerMarks::expand`). São calculadas uma vez, na subida.
- **O daemon já tem um vigia por inotify** (`daemon/secretwatch.rs`), com espera de silêncio e varredura de segurança. Também já tem um vigia da configuração (`net.rs`, `start_policy_watch`), que chama `refresh_engine` quando ela muda.
- **O registro do agente** é gravado pelo supervisor (`sandmux-launcher/src/files.rs`, `FileEntry`), em `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl`. Os segredos que o agente criou ficam em `files/created/<hash>.json` (`read_created`).
- **O auxiliar responde um pedido por vez**, na conexão do confinamento. O cliente lê a resposta logo depois de mandar.

## Goals / Non-Goals

**Goals:**

- Fechar, no kernel, os arquivos que aparecem depois da subida: os segredos novos e o que aparece numa pasta negada.
- A política de arquivos da configuração ao vivo no Docker, como no agente.
- Bloqueios e criações dos contêineres no mesmo registro do agente, com o caminho do host.
- Nenhum custo para os processos fora do Docker da pasta, e nenhum bloqueio dos contêineres por causa do registro.

**Non-Goals:**

- As leituras e escritas comuns dos contêineres no registro (decisão do usuário).
- Tirar ao vivo uma cobertura por montagem: um caminho liberado continua coberto até a subida seguinte.
- O nome do contêiner na linha do registro. A etiqueta é a do Docker da pasta (`docker-<id>`), como no registro de rede.
- Os arquivos criados fora da pasta de trabalho (no `/etc` do host, por exemplo). As coberturas de `/etc` são fixas.

## Decisions

### 1. Vigia da pasta por inotify

Enquanto o Docker roda confinado, uma thread do daemon vigia a pasta de trabalho (`daemon/dockerwatch.rs`).

- **Vigias:** uma vigia por pasta, sem seguir links, com `IN_CREATE`, `IN_MOVED_TO` e `IN_ONLYDIR` nas pastas. Uma pasta nova ganha vigia e é percorrida na hora, porque o que foi criado dentro dela antes da vigia não gera evento.
- **Espera de silêncio:** 200 ms depois do último evento. Depois, relê os segredos criados pelo agente (`read_created`), porque o supervisor grava esse arquivo logo depois de criar. Um arquivo do agente marcado por engano nessa janela é desmarcado quando o `created` muda, e o arquivo `created` da pasta também é vigiado.
- **Decisão de cada caminho:** `docker_mark_for(policy, created, denied_dirs, path)`, em `sandmux-policy`. Nega quando:
  - a política de arquivos diz `Deny` ou `Secret` (a mesma decisão do supervisor);
  - ou o caminho fica dentro de uma pasta negada.
- **Índice de pastas:** a vigia guarda o inode de cada pasta da pasta de trabalho, para traduzir os eventos de criação (decisão 4).
- **Recuo:** se o `inotify_add_watch` falhar com `ENOSPC`, a vigia larga as vigias e refaz a varredura (a mesma de `docker_covers`) a cada 10 segundos. O `DockerStats` ganha `files_poll: bool`, e o `sandmux stats` mostra o aviso.

**Alternativa descartada:** o fanotify com marca de sistema de arquivos, no auxiliar. Pesa no sistema de arquivos inteiro, e o auxiliar passaria a ler caminhos.

### 2. Marcas ao vivo e o `unmark`

O `Engine` guarda as marcas em vigor (caminho → bits). Quando a configuração muda, `refresh_engine` chama `Engines::refresh_marks(workdir)`. Ele faz isto:

1. recalcula as marcas com a política nova: a varredura de `docker_covers`, as coberturas de `/etc` da visão, a proteção do `.git` e o conteúdo das pastas negadas;
2. compara com as em vigor;
3. manda `mark` dos caminhos novos e `unmark` dos que saíram.

Pedido novo no auxiliar: `{"op": "unmark", "kind": ...}`, com descritores `O_PATH`.

- O auxiliar acha, entre os descritores que guarda, o do mesmo arquivo: compara o `st_dev` e o `st_ino` dos dois pelo `fstat`.
- Tira os bits pedidos. Se não sobrar nenhum, apaga a entrada do mapa e fecha o descritor guardado.

O teto de 16384 marcas vale também para as marcas ao vivo. Passou do teto, o `mark` recusa, e o daemon grava o aviso no log e no `DockerStats`. O que já estava marcado continua.

### 3. Eventos: a fila do BPF

Cada confinamento ganha um mapa `BPF_MAP_TYPE_RINGBUF` de 256 KiB.

- **Bloqueio:** antes de devolver "nega", o programa monta o evento na pilha e chama `bpf_ringbuf_output` (sem reserva). O evento traz:
  - o tipo e o gancho;
  - os bits que negaram;
  - o `i_ino` do inode marcado que decidiu;
  - se a abertura era para escrita.
- **Criação:** o `inode_create` manda um evento quando não nega. O evento traz o `i_ino` do diretório, o nome da `dentry` nova (até 255 bytes, lido com `bpf_probe_read_kernel_str`) e o `s_dev` do superbloco do diretório.
  - Para não inundar a fila num `docker build`, o programa compara o `s_dev` com o da pasta de trabalho, gravado como constante na carga, e só manda os do mesmo sistema de arquivos.
  - Os arquivos que os contêineres criam no overlay ficam em outro superbloco e não geram evento.
- **Fila cheia:** quando o `bpf_ringbuf_output` falha, o programa soma 1 num contador (um mapa `ARRAY` de uma posição, com uma soma atômica). O auxiliar lê o contador e o zera a cada lote que manda.
- **O `s_dev` do kernel:** o daemon manda no `confine` o `major:minor` do superbloco da pasta de trabalho, lido em `/proc/self/mountinfo`. Esse campo é o `s_dev` do superbloco, enquanto o `stat` devolve, no btrfs, o dispositivo do subvolume. Sem o campo (zero), não há filtro.

**Alternativa descartada:** o caminho pelo `bpf_d_path`. Ele dá o caminho de dentro do contêiner (`/w/a.txt`), que o daemon teria de traduzir pelos volumes de cada contêiner na API do Docker. E só vale em alguns ganchos.

### 4. Do evento à linha do registro

O auxiliar lê a fila e manda cada lote ao daemon pela conexão do confinamento, como uma mensagem `{"ev": [...], "dropped": n}`.

- **No auxiliar:** a leitura da fila faz `mmap` da página do consumidor (com escrita), da página do produtor e dos dados, e espera com `epoll` no descritor do mapa. O envio pelo socket fica atrás de um `Mutex`, porque a thread da fila e as respostas dos pedidos usam o mesmo socket.
- **No cliente:** passa a ter uma thread de leitura. Ela manda as respostas para quem fez o pedido e os eventos para um canal. A thread que vigiava o fim da conexão (change 70) vira essa thread. O `Drop` do cliente faz `shutdown` no socket, porque a thread de leitura segura uma cópia dele (achado na implementação: sem isso, soltar o cliente não fechava a conexão, e as regras ficavam).

O daemon traduz cada evento num caminho do host:

- **Bloqueio:** o `i_ino` do inode marcado, procurado nas marcas em vigor (o daemon faz o `stat` de cada caminho quando marca). O motivo do bloqueio vem do tipo da marca:
  - `segredo` ou `file-deny: <padrão>`, pela decisão da política;
  - `.git`, para `PROTECT` e `GITDIR`;
  - `cobertura`, para as de `/etc`.
- **Criação:** o `i_ino` do diretório é procurado no índice de pastas da vigia (decisão 1), e o caminho é a pasta mais o nome. Um diretório fora do índice é descartado: é uma criação fora da pasta de trabalho. Cada caminho criado entra uma vez por subida do Docker.

As linhas usam o `FileEntry` do supervisor:

- a etiqueta `docker-<id>`;
- o `workdir` da pasta;
- o acesso: `r`, `w` ou `create`;
- o `blocked`, quando houver.

O `sandmux files` já lê todas as linhas e mostra a etiqueta. Para filtrar o Docker de uma sessão, `sandmux files <id>` passa a incluir as linhas com a etiqueta `docker-<id>` da pasta da sessão.

### 5. Protocolo

- **Auxiliar:** o `unmark`, o campo `dev` no `confine` e as mensagens de evento. Um cliente antigo, que não espera eventos, não existe: o daemon e o auxiliar vêm do mesmo pacote. Quando as versões diferem, o `status` mostra as duas, e o `doctor` pede o `sandmux setup` (um `try-restart` do serviço).
- **Cliente e daemon:** o `DockerStats` ganha `files_poll`, e a versão do protocolo sobe para 40.

## Risks / Trade-offs

- **[Janela entre a criação e a marca]** → até 200 ms de espera, mais o caminho até o kernel. Um contêiner que lê o arquivo nessa janela consegue lê-lo. O spec promete 1 segundo, e o risco é aceito.
- **[Arquivo do agente marcado por engano na janela do `created`]** → desmarcado quando o `created` muda. Durante a janela, um contêiner não lê um segredo que o agente acabou de criar.
- **[Limite de vigias do inotify]** → recuo para a varredura a cada 10 s, com o aviso. Uma pasta com centenas de milhares de subpastas cai nesse caso.
- **[Inodes repetidos entre sistemas de arquivos]** → os bloqueios são procurados pelo `i_ino` só nas marcas em vigor, que são poucas, e as criações só no índice da pasta, depois do filtro do `s_dev`. Uma colisão daria um caminho errado numa linha do registro, nunca uma decisão errada.
- **[Fila cheia num pico de criações]** → os eventos a mais são descartados e contados. O registro ganha uma linha com a contagem, e os contêineres não esperam.
- **[O `bpf_probe_read_kernel_str` e o `bpf_ringbuf_output` no `BPF_LSM_CGROUP`]** → verificados numa tarefa com root antes do resto. Se faltarem, os bloqueios entram no registro sem o nome do arquivo criado.

## Migration Plan

- O auxiliar e o daemon mudam juntos. Depois de atualizar: `sandmux setup` (que reinicia o serviço) e `sandmux daemon restart`.
- Sem o confinamento por eBPF, nada muda: os arquivos novos e a política ao vivo dependem dele.
