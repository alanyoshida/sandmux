## Context

Motivação e decisões em `proposal.md`. O que existe:

- **O supervisor** (`sandmux-launcher/src/files.rs`) recebe pelo seccomp user notify as aberturas do comando. Ele abre o arquivo na raiz do sandbox, decide pela política (`FilePolicy`: segredos por padrão de nome, `file-deny`, `file-allow`), registra em `FileEntry` e entrega o descritor. Os arquivos criados pelo agente ficam numa lista em `state_dir` e continuam legíveis.
- **O confinamento do Docker** (`sandmux-ebpf::prog::Confinement` e `daemon/dockerfiles.rs::Files`):
  - as marcas `DENY`, `PROTECT` e `GITDIR` em armazenamento por inode;
  - os eventos de bloqueio e de criação pela fila do kernel;
  - a vigia da pasta de trabalho (inotify e varredura de recuo);
  - a política ao vivo (`refresh`);
  - as linhas do registro.

  O daemon é quem tem a conexão do `confine`.
- **A aba de uma sessão** roda no escopo próprio do systemd de usuário quando há os tetos, e o daemon acha esse escopo (`session::monitor`). O lançador fala com o daemon pelo socket dele (`VaultAccess`, já usado pelo gateway) e só libera o filho pelo byte de `sync_w`.

## Goals / Non-Goals

**Goals:**

- A leitura em massa numa aba confinada pelo kernel perto de 1,0x.
- A mesma política e o mesmo registro do supervisor, com a janela de até 1 s para os arquivos criados no host.
- O supervisor como recuo, sem mudança, quando falta algo.

**Non-Goals:**

- O `sandmux run` sem sessão, que fica com o supervisor (ele não tem o canal com o daemon).
- Mudar o confinamento do Docker: as marcas `DENY`, `PROTECT` e `GITDIR` e os erros dele ficam iguais.
- Tirar o seccomp user notify das execuções (o recuo do registro de execuções do change 72).

## Decisions

### 1. O daemon confina a aba, a pedido do lançador

As montagens da visão do agente só existem depois que o processo 1 monta a raiz, e isso acontece depois da liberação do filho. Por isso há um segundo encontro, antes de o processo 1 criar o comando, pelo par de sockets que já leva o filtro das aberturas (`SOCK_SEQPACKET`):

1. o processo 1, com a raiz pronta, manda o byte `R` e espera a resposta;
2. o lançador lê o `/proc/<processo 1>/mountinfo`: os ids da montagem que guarda cada pasta do registro e das montadas dentro dela, menos as de `exclude`. Sem montagem nenhuma, ou com mais de 256 (o tamanho do mapa), fica o supervisor;
3. o lançador manda ao daemon `Msg::ConfineTab { tag, mounts, roots, exclude }`, pelo socket do daemon (o mesmo acesso do cofre), com prazo de 10 s;
4. o lançador responde ao processo 1 o byte `2` (kernel) ou `1` (supervisor).

O daemon, ao receber o pedido:

1. acha o processo pelas credenciais do socket (`SO_PEERCRED`) e o escopo dele em `/proc/<pid>/cgroup`. Precisa ser um escopo próprio da aba (`.scope`, do sandmux);
2. pede o `confine` ao auxiliar, com o `dev` da pasta de trabalho e as montagens a registrar;
3. marca, a partir da política da sessão:
   - os segredos (`SECRET`) e o negado (`FDENY`), com o que existe dentro das pastas negadas;
   - o `.git` (`PROTECT` e `GITDIR`);
4. liga o `Files` da aba (a vigia, a política ao vivo e os eventos);
5. responde `ConfineTabReply { kernel: true }`, ou `false` com o motivo, quando falta o auxiliar, o BPF LSM ou o escopo, quando o auxiliar é antigo (o `status` dele não traz `tab`), ou quando as marcas da política não foram aplicadas (acima do teto, ou recusadas). Nesse último caso, o daemon solta a conexão com o auxiliar, e o confinamento sai antes de o comando rodar.

No modo kernel, o comando não instala o filtro de aberturas, só o de execuções, quando ele é pedido. Sem resposta em 10 s, vale o supervisor. O shell de entrada (`Ctrl+b t`) continua com o filtro de aberturas.

O protocolo sobe para 43.

### 2. As marcas da aba

Duas marcas novas no `prog.rs`, ao lado das do Docker:
- **`SECRET` (8):** nos ganchos de abertura, devolve `-ENOENT`, como o "segredo escondido" do supervisor;
- **`FDENY` (16):** devolve `-EACCES`, como o `file-deny`.

Nos ganchos de criação, renome e remoção dentro de uma pasta com a marca, as duas devolvem `-EACCES`.

As marcas vêm da mesma decisão do supervisor (`FilePolicy::decide`) sobre a pasta de trabalho, pulando os arquivos criados pelo agente (a lista de `state_dir`). É a mesma área do supervisor: a cara de segredo só vale dentro da pasta de trabalho (`FilePolicy::is_secret`). Um `file-deny` que pode valer fora dela (um nome, ou um caminho de fora) deixa a aba com o supervisor. As pastas negadas são expandidas, como no Docker (`DockerMarks::expand`).

### 3. O evento de abertura

O programa de `file_open` ganha o evento `EV_OPEN`:
- só para os arquivos das montagens registradas: o `mnt_id` da `struct mount` do arquivo (`container_of(f_path.mnt)`, lido por `bpf_probe_read_kernel`) é procurado num mapa preenchido no `confine`;
- o caminho vem por `bpf_d_path(&file->f_path)`, que o gancho `file_open` aceita;
- o evento leva `f_mode`, para saber se a abertura grava.

Um caminho maior que o espaço do evento sai cortado e marcado. A fila do confinamento da aba é maior que a do Docker (aberturas são muitas): 1 MiB. Os eventos descartados vão para a linha de descarte do registro, como no Docker.

### 4. O registro

O daemon converte cada `EV_OPEN` numa `FileEntry`, igual à do supervisor:
- o caminho na visão do agente;
- leitura ou escrita;
- sem bloqueio.

Ele guarda só as aberturas dentro de `roots` e fora de `exclude` (a montagem que guarda uma pasta pode ter outras coisas), e os bloqueios vêm dos eventos de negação, com o motivo pela política (`dockerfiles::reason`). A gravação é a mesma do registro de arquivos (`write_entry`).

### 5. A queda do auxiliar e a política ao vivo

Diferente do Docker, a queda da conexão com o auxiliar não mata a aba de cara: um processo que já roda não passa para o supervisor (o filtro seccomp vem antes do `exec`), e matar levaria a sessão do agente a cada atualização ou reinício do auxiliar. O daemon congela o escopo da aba (`cgroup.freeze`), tenta o confinamento de novo a cada 300 ms (o systemd sobe o auxiliar na conexão seguinte), com a política atual, e descongela quando as marcas voltam. Sem o confinamento em 10 s, ou se o congelamento falha, escreve `1` no `cgroup.kill`, e tudo no escopo morre. Decisão do usuário, 2026-11-20. Soltar o confinamento de uma aba que terminou fecha a conexão na hora, mesmo com a thread dos eventos segurando o `Files`. A política relida (`policy_file`) chama `Files::refresh`, que marca e desmarca em até 1 s.

### 6. O que mostra

O `PolicySummary` ganha `files`: `kernel`, ou `supervisor: <motivo>`, como o `exec` do change 72. O `stats` mostra a linha "aberturas de arquivo: pelo kernel" ou "supervisor: <motivo>", e o painel o item "arquivos pelo kernel", aceso ou apagado. O `doctor` mostra, como informação (a falta não é falha), se o caminho do kernel pode valer: o BPF LSM e um auxiliar que confina abas.

## Risks / Trade-offs

- **[A janela dos segredos criados no host]** → aceita pelo usuário. A vigia por inotify marca em até ~1 s, e a varredura de recuo cobre a falta de vigias.
- **[Muitos eventos de abertura]** → o filtro pelas montagens tira `/usr` e os caches. A fila de 1 MiB e o contador de descartes registram o excesso, sem bloquear o agente.
- **[O `bpf_d_path` e o `mnt_id` no kernel 6.8]** → testados na VM, no 6.8 e no 7.0.
- **[Um erro nos programas globais]** → só na VM, como manda o incidente de 2026-11-20. Os ponteiros que podem ser nulos são lidos por `bpf_probe_read_kernel`.

## Migration Plan

- O auxiliar, o daemon e o lançador mudam juntos. Um auxiliar antigo não conhece as opções novas do `confine`: o daemon detecta e cai no supervisor.
