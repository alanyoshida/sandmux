## Why

O change 70 confina os arquivos do Docker privado no kernel, mas com três buracos, todos registrados nele:

- A lista de arquivos negados é fixada quando o Docker da pasta sobe. Um `.env` criado depois, ou um arquivo que o usuário cria no host dentro de uma pasta negada, fica ao alcance dos contêineres até o Docker subir de novo. O segundo caso é o limite do recuo do change 70: o kernel olha só o próprio inode e o diretório da operação.
- Uma mudança no `file-deny` ou nos segredos da configuração da pasta não chega ao Docker que já está rodando. O sandbox do agente aplica a política ao vivo.
- O registro de arquivos só tem o agente. Uma tentativa de um contêiner de ler um segredo ou de gravar um hook não deixa rastro.

Decisões do usuário (2026-11-17), ao propor:

- o registro dos contêineres fica com os bloqueios e as criações de arquivo, sem as leituras e as escritas comuns;
- os arquivos criados com o Docker rodando são vistos por inotify na pasta de trabalho.

## What Changes

- **Arquivos novos marcados na hora:** o daemon vigia a pasta de trabalho com inotify, enquanto o Docker da pasta roda confinado.
  - Um arquivo com cara de segredo criado depois, que o agente não criou, é marcado como negado no kernel.
  - Tudo o que aparece dentro de uma pasta negada também é marcado, seja quem for que o criou.
  - Sem vigias disponíveis no sistema (`fs.inotify.max_user_watches`), o daemon cai numa varredura a cada 10 segundos e avisa no `stats`.
- **Política ao vivo:** quando a configuração da pasta muda (`file-deny`, `file-allow`, `secret-files`, `secret-files-off`), o daemon recalcula as marcas e manda ao auxiliar só a diferença.
  - Um caminho que passa a ser negado vale na operação seguinte.
  - Um caminho liberado deixa de ser negado no kernel. A cobertura por montagem da subida continua até o Docker subir de novo: o contêiner vê o arquivo vazio até lá.
- **Registro dos contêineres:** os programas BPF mandam um evento quando negam uma operação e quando um contêiner cria um arquivo. Os eventos vão por uma fila (`BPF_MAP_TYPE_RINGBUF`) ao auxiliar, e dele ao daemon.
  - O daemon grava cada evento no mesmo registro de arquivos do agente, com a etiqueta `docker-<id>` e o caminho do host.
  - Só entram as criações dentro da pasta de trabalho.
  - `sandmux files` e a tela de arquivos da TUI mostram essas linhas.
- **Auxiliar root:**
  - um pedido novo, `unmark`, que tira marcas;
  - eventos mandados pela mesma conexão do confinamento.

Fora deste change:

- as leituras e as escritas comuns dos contêineres no registro (decisão do usuário);
- liberar ao vivo o conteúdo de um arquivo coberto, porque a cobertura é uma montagem da subida.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `container-engine`: arquivos criados depois e mudanças na política passam a valer no Docker que já está rodando, quando o confinamento por eBPF vale.
- `root-helper`: os pedidos aceitos ganham o `unmark` e o envio de eventos.
- `file-access-log`: o registro ganha as linhas dos contêineres do Docker privado.

## Impact

- `crates/sandmux-ebpf`:
  - o mapa `RINGBUF` por confinamento;
  - o evento nos programas que negam e no `inode_create`;
  - a leitura da fila (`mmap` das páginas do produtor e do consumidor).
- `crates/sandmux/src/helper.rs` e o binário `sandmux-helper`: o `unmark`, a thread que lê a fila e manda os eventos, e o cliente com uma thread de leitura que separa as respostas dos eventos.
- `crates/sandmux/src/daemon/docker.rs` e um vigia novo da pasta, como o `secretwatch.rs`:
  - o inotify, a varredura de recuo e o índice de inodes da pasta;
  - a diferença das marcas quando a configuração muda;
  - a gravação das linhas no registro.
- `crates/sandmux-policy`: a decisão das marcas de um caminho só, para os arquivos novos.
- `crates/sandmux-proto`: o `DockerStats` ganha o aviso da varredura de recuo. A versão do protocolo muda.
- `docs/docker.md`, `docs/files.md`, `docs/helper.md` e os pares em inglês.
- Testes: unitários sem root para a vigia, a diferença das marcas e a tradução dos eventos; com root, pelo `scripts/check-root.sh`.
