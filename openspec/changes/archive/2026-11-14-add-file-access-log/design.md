# Design

## Context

- **Topologia** (`sandmux-launcher/src/launch.rs`):
  - o supervisor (`sandmux-run`) cria todos os namespaces num único `clone` e fica no user namespace pai, como dono do novo;
  - o processo 1 monta a raiz (um tmpfs sem caminho no host, depois do `pivot_root`) e faz o fork do comando;
  - o comando aplica o Landlock (ABI 6), o seccomp de negação (`prctl`) e faz o `execve`.
- **O supervisor hoje:** espera sinais na thread principal; o gateway roda numa thread com tokio. Só conhece o pid do processo 1 no host.
- **Medição** (roadmap, seção 6):
  - abrir no lugar do processo custou 1,0x a 1,3x no uso normal e 3,2x na leitura de 13 mil arquivos seguidos;
  - a prova mostrou que `/proc/self` e `/dev/fd` precisam ser traduzidos para o processo.

## Goals / Non-Goals

**Goals:**

- Ver e decidir cada abertura de arquivo sem a corrida do caminho, com o agente rodando.
- Registrar os acessos às pastas do agente.

**Non-Goals:**

- Negar arquivos e esconder arquivos com cara de segredo (change 16; aqui só o mecanismo e o registro).
- Interceptar `stat`, `getdents`, `rename`, `unlink` e `execve`: o Landlock segue decidindo esses.
- Outras arquiteturas além de x86_64 e aarch64.

## Decisions

### 1. Filtro com notificação

- **Onde:** no processo do comando, depois do Landlock e do seccomp de negação, um segundo filtro instalado por `seccomp(SECCOMP_SET_MODE_FILTER, SECCOMP_FILTER_FLAG_NEW_LISTENER)` devolve `SECCOMP_RET_USER_NOTIF` para `open`, `openat`, `openat2` e `creat` (em aarch64, `openat` e `openat2`).
- **O descritor do filtro:** vai ao supervisor por um socketpair `SOCK_SEQPACKET` criado antes do `clone` (em todo lançamento, não só com rede), e é fechado no comando antes do `execve`. O envio acontece logo depois da instalação: uma abertura antes dele esperaria o supervisor.
- **Supervisor ausente:** se o supervisor morre, o descritor fecha e as aberturas falham (`ENOSYS`): falha fechada.

### 2. Abrir no lugar do processo

Para cada notificação, numa de quatro threads do supervisor:

1. **Ler os argumentos:** o caminho vem de `/proc/<pid>/mem` (o supervisor é o dono do user namespace e ancestral do processo). No `openat2`, a `open_how` também. Depois da leitura, `SECCOMP_IOCTL_NOTIF_ID_VALID` confirma que a notificação continua viva.
2. **Resolver na visão do agente:**
   - **caminho absoluto:** `openat2` a partir de `/proc/<pid>/root`, com `RESOLVE_IN_ROOT` (links absolutos e `..` ficam dentro da raiz do sandbox), somado às flags `RESOLVE_*` do próprio `openat2`;
   - **caminho relativo:** o diretório (o `cwd` ou o `dirfd`) vira o caminho dele na visão do agente (`readlink` de `/proc/<pid>/cwd` ou `/proc/<pid>/fd/<n>`). Esse caminho, somado ao relativo, é resolvido como um absoluto;
   - **links mágicos do próprio processo:** `/proc/self`, `/proc/thread-self` e `/proc/<pid no namespace>` do próprio processo viram `/proc/<pid no host>`, e `/dev/fd/<n>`, `/dev/stdin`, `/dev/stdout` e `/dev/stderr` viram `/proc/<pid no host>/fd/<n>`. Esses são abertos pelo supervisor direto, sem `RESOLVE_IN_ROOT`, porque é o mesmo arquivo que o processo já tem;
   - **links mágicos de outros processos** (`/proc/<outro pid>/fd`, `cwd`, `root`): recusados com `EACCES`.
3. **Conferir o Landlock:** o caminho do arquivo aberto, na visão do agente (o `readlink` do descritor aberto mostra o caminho a partir da raiz do sandbox), é conferido contra `spec.landlock`:
   - leitura (`O_RDONLY` e `O_RDWR`) exige o caminho sob uma regra de leitura ou de escrita;
   - escrita (`O_WRONLY`, `O_RDWR`, `O_TRUNC` e `O_CREAT`) exige o caminho sob uma regra de escrita;
   - `O_PATH` não exige nada, como no Landlock.
   - Fora das regras, o descritor é fechado, e o processo recebe `EACCES`. As montagens em somente leitura valem sozinhas, porque a abertura passa por elas.
4. **Entregar:** `SECCOMP_IOCTL_NOTIF_ADDFD` com `SECCOMP_ADDFD_FLAG_SEND` instala o descritor no processo como resultado da chamada, com `O_CLOEXEC` se foi pedido. Um erro da abertura volta como o mesmo `errno`.

- **Criação:** o modo do `O_CREAT` recebe a `umask` do processo (lida de `/proc/<pid>/status`); o supervisor roda com `umask` 0. O dono é o mesmo usuário, porque o mapeamento de ids é a identidade.

### Achados da implementação

- **`O_PATH`:** o `ADDFD` não entrega descritores `O_PATH` (o kernel busca o arquivo com `fget`, que os ignora). Um `O_PATH` não dá acesso ao conteúdo: ler exige outra abertura, que passa pelo supervisor. Então as aberturas `O_PATH` seguem pelo kernel (`SECCOMP_USER_NOTIF_FLAG_CONTINUE`), sob o Landlock, e não vão para o registro.
- **Sinais:** o runtime do Go preempta com SIGURG, e cada sinal recomeçava a abertura. O filtro usa `SECCOMP_FILTER_FLAG_WAIT_KILLABLE_RECV` (kernel 5.19): recebida a notificação, só um sinal fatal interrompe a espera.
- **`EAGAIN`:** com `RESOLVE_IN_ROOT`, uma montagem ou um rename em qualquer lugar durante a resolução dá `EAGAIN`; o supervisor repete. Sem isso, o `ld.so` falhava em carregar bibliotecas com vários sandboxes subindo ao mesmo tempo.
- **Links do sistema para `/proc/self`:** `/proc/net`, `/proc/mounts` e `/etc/mtab` são traduzidos para o pid do processo, porque o `self` do `/proc` do sandbox não existe para o supervisor, que está fora do pid namespace. Um link criado pelo usuário para `/proc/self/...` falha com `ENOENT`.
- **Atravessar um link mágico:** `/proc/self/cwd/x` e `/proc/self/fd/<n>/x` viram o caminho do diretório na visão do agente, resolvido dentro da raiz. Só um link mágico como último componente vai pelo `/proc` do host: atravessá-lo por lá seguiria links absolutos contra a raiz do host.
- **Sem resposta:** se o `ADDFD` falha, o supervisor responde com o erro; antes, o processo esperava para sempre.

### 3. Registro

- **Especificação:** `LaunchSpec.files: Option<FilesSpec { log_dir, tag, workdir, roots }>`, em que `roots` são as pastas do agente na visão do agente (a pasta de trabalho, o HOME sintético, as pastas extras e a pasta de entrada). Sem `files`, as aberturas continuam passando pelo supervisor, mas nada é registrado.
- **Linha:** `ts`, `tag`, `workdir`, `path` (na visão do agente), `access` (`r`, `w`, `rw` ou `create`), `blocked` (o motivo, ou `null`) e `count`.
- **Volume:** cada (caminho, acesso) é registrado uma vez por lançamento, na primeira vez. Um bloqueio é sempre registrado. A contagem das repetições fica na memória e não é gravada.
- **Arquivo:** `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl`, com pasta 700 e arquivo 600.
- **Retenção:** o daemon aplica à pasta `files` os mesmos `net-log-days` e `net-log-max` da rede.

### 4. Consulta

`sandmux files [<id>] [--days N]` lista as linhas recentes (de uma sessão, ou de todas), com hora, sessão, acesso e caminho, como o `sandmux net`.

## Risks / Trade-offs

- **[O supervisor no lugar do Landlock nas aberturas]** Um erro no supervisor abriria o que o Landlock negaria. → A abertura acontece sempre dentro da raiz do sandbox (`RESOLVE_IN_ROOT`), a conferência segue as regras da própria especificação, e o teste de fuga roda com o filtro ligado.
- **[Custo]** 1,0x a 1,3x no uso normal; 3x na leitura em massa. → Quatro threads atendem as notificações; a medição com opencode e hermes reais é repetida na verificação.
- **[Links mágicos de outros processos]** `/proc/<outro pid>/fd/<n>` deixa de abrir dentro do sandbox. → É raro (`lsof`); a documentação cita.
- **[Volume do registro]** Um build na pasta de trabalho abre milhares de arquivos. → Uma linha por (caminho, acesso) por lançamento.

## Migration Plan

Nada a migrar; os lançamentos novos passam a ter o filtro. Não muda o protocolo do daemon.
