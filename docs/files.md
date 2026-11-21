# Arquivos

O sandmux vê cada abertura de arquivo do agente, com o agente rodando, e registra os acessos às pastas do agente. Ele também nega e libera arquivos na hora e esconde os arquivos com cara de segredo da pasta de trabalho. Quem decide é o supervisor do sandbox ou, com o auxiliar root, o [kernel](#pelo-kernel).

## Como funciona

Um filtro seccomp com notificação (`SECCOMP_RET_USER_NOTIF`) manda ao supervisor do sandbox cada `open`, `openat`, `openat2` e `creat` do agente. O supervisor:

1. lê o caminho na memória do processo;
2. abre o arquivo ele mesmo, dentro da raiz do sandbox (`RESOLVE_IN_ROOT`: links absolutos e `..` ficam dentro dela);
3. confere as regras de leitura e escrita do Landlock pelo caminho do arquivo que de fato abriu;
4. entrega o descritor ao processo como resultado da chamada (`SECCOMP_IOCTL_NOTIF_ADDFD`).

O processo recebe o arquivo que o supervisor abriu: trocar o caminho na memória depois da leitura, por outra thread, não muda nada. O Landlock segue valendo para todo o resto (executar, renomear, apagar, criar pasta). Se o supervisor morre, as aberturas falham.

Casos especiais:

- **`/proc/self`, `/dev/fd/<n>`, `/dev/stdin`:** são traduzidos para o próprio processo, e `cat <(echo oi)` funciona. `/proc/net`, `/proc/mounts` e `/etc/mtab` também.
- **Links mágicos de outros processos** (`/proc/<pid>/fd`, `cwd`, `root`, `exe`): recusados com `EACCES`. Ferramentas como o `lsof` não veem os arquivos abertos de outros processos dentro do sandbox.
- **`O_PATH`:** segue pelo kernel, sob o Landlock, sem registro. Um descritor `O_PATH` não dá acesso ao conteúdo; ler exige outra abertura, que passa pelo supervisor.

## Pelo kernel

Com o [auxiliar root](helper.md) e o BPF LSM, as aberturas de uma aba saem do supervisor: o kernel decide e registra, sem a ida e volta de cada `open`. O daemon prende à aba os programas BPF LSM do confinamento, os mesmos do [Docker privado](docker.md#confinamento-por-ebpf), e marca os arquivos pelo inode:

- os arquivos com cara de segredo da pasta de trabalho dão `ENOENT`, como no supervisor;
- os do `file-deny`, com o que existe dentro das pastas negadas, dão `EACCES`;
- o `.git` fica protegido como no Docker.

As aberturas das pastas do agente chegam pela fila do kernel, com o caminho na visão do agente, e entram no registro como as do supervisor. Só as montagens dessas pastas mandam eventos: o sistema, as toolchains e os caches não passam pela fila.

Vale sozinho quando tudo isto existe; sem um deles, a aba fica com o supervisor, e o resumo diz por quê:

- uma sessão do daemon (o `run` sem terminal fica com o supervisor);
- o BPF LSM ativo e o auxiliar root, numa versão que confina abas;
- o escopo próprio da aba no systemd de usuário (o mesmo dos [tetos](limits.md));
- um `file-deny` só com caminhos dentro da pasta de trabalho. Um nome (`*.sql`) ou um caminho de fora pode valer em qualquer pasta, e as marcas cobrem só a pasta de trabalho.

O lançador pede ao daemon depois de montar a raiz do sandbox e antes de criar o comando, e espera até 10 s pela resposta. No modo kernel, o comando não tem o filtro de aberturas; o de execuções continua quando o registro das execuções é pelo supervisor. O shell de entrada (`Ctrl+b t`) continua com o supervisor.

Diferenças do supervisor:

- **Arquivos novos do host:** um segredo ou um arquivo de pasta negada criado fora do sandbox durante a sessão é marcado pela vigia da pasta (inotify, ou a varredura a cada 10 s sem vigias), em até ~1 s. Nessa janela, o agente consegue abrir o arquivo. Os segredos que o próprio agente cria continuam dele, como no supervisor.
- **Política ao vivo:** negar e liberar na hora marcam e desmarcam os arquivos em até 1 s.
- **Acesso:** a linha diz `r` ou `w`; uma abertura de leitura e escrita aparece como `w`.
- **Leitura em massa:** o registro sai completo. O kernel não manda de novo a abertura de um arquivo, no mesmo modo, com menos de 1 s da última. Renomear o arquivo ou criar um link para ele, de dentro da aba, faz a abertura seguinte ir para o registro com o caminho novo. Quando a fila enche mesmo assim, o auxiliar congela a aba por alguns milissegundos, até a fila esvaziar ([os eventos](helper.md#arquivos-de-uma-aba)). Um arquivo renomeado no host durante a sessão e aberto pelo nome novo em menos de 1 s depois da última abertura pelo nome antigo fica só com o nome antigo no registro. O mesmo vale para um arquivo visto por duas pastas do agente.
- **Queda do auxiliar:** sem as marcas, nada roda na aba. O daemon congela o escopo dela (`cgroup.freeze`) até o auxiliar voltar (o systemd o sobe na conexão seguinte) e refaz o confinamento, com a política atual; a aba segue de onde parou. Sem o confinamento de volta em 30 s, o daemon mata tudo no escopo (`cgroup.kill`). Numa saída limpa do auxiliar (`systemctl restart`, uma atualização), ele avisa o daemon antes de soltar as regras; morto à força, sobra uma janela curta entre a saída e o congelamento.
- **Casos especiais:** os links mágicos e o `/proc/self` seguem as regras do kernel e do Landlock; um arquivo marcado continua negado por qualquer caminho, porque a marca é do inode.

A proteção do lançamento, no `sandmux stats <id>` e no painel, mostra a linha "aberturas de arquivo": "pelo kernel" ou "supervisor: <motivo>". O `sandmux doctor` mostra se o caminho do kernel pode valer nesta máquina.

## Política de arquivos

O supervisor decide cada abertura pelo arquivo que de fato abriu, nesta ordem:

1. casa com `file-allow`: abre;
2. casa com `file-deny`: falha com `EACCES` ("Permissão negada");
3. é um arquivo com cara de segredo na pasta de trabalho, que o agente não criou: falha com `ENOENT` (como se não existisse); abrir para escrita falha com `EACCES`;
4. o resto segue as regras do Landlock.

Uma mudança vale na abertura seguinte, sem relançar o agente: o daemon regrava a política do lançamento quando a configuração muda ou a aba é editada, e o supervisor a relê.

### Padrões

- **Caminho:** começa com `/` ou `~/`, como `~/proj/privado` ou `/srv/dados/**/*.csv`. `*` e `?` valem dentro de um componente, e `**` vale para qualquer número de componentes.
- **Nome:** sem `/`, como `*.sql` ou `node_modules`; casa com um arquivo ou uma pasta de qualquer lugar.
- **Pasta:** um padrão que casa com uma pasta vale para tudo dentro dela.

| Chave | Opção do `run` | Efeito |
|---|---|---|
| `file-deny` | `--file-deny` | não abre |
| `file-allow` | `--file-allow` | abre, vencendo `file-deny` e os segredos |
| `secret-files` | | soma padrões à lista de segredos |
| `secret-files-off` | | tira padrões da lista de segredos (exceções) |

As listas se somam entre as camadas da [configuração](config.md). Para a aba de uma sessão viva:

```bash
sandmux files deny dados.sql 3      # nega na aba da sessão 3
sandmux files allow ~/proj/ok.sql 3 # libera
sandmux files forget dados.sql 3    # tira o padrão da aba
```

Na TUI, Ctrl+b `f` mostra os acessos da aba, e `b` e `l` negam e liberam o caminho escolhido ([tui.md](tui.md)).

### Arquivos com cara de segredo

Na pasta de trabalho, estes arquivos não abrem para o agente: `.env` e `.env.*` (menos `.env.example`, `.env.sample` e `.env.template`), `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.kdbx`, `id_rsa*`, `id_ecdsa*` e `id_ed25519*` (menos os `*.pub`), `.npmrc`, `.pypirc`, `.netrc`, `credentials*.json`, `*.tfstate` e `.git-credentials`.

- **O nome continua no `ls`:** o `getdents` e o `stat` não passam pelo supervisor; só o conteúdo fica fora.
- **Criados pelo agente:** um arquivo assim que o próprio agente criar (abrindo, renomeando ou ligando) continua aberto para ele, também nos lançamentos seguintes. A lista fica em `$XDG_STATE_HOME/sandmux/files/created/`, fora do alcance do agente. Se você preencher depois um arquivo criado pelo agente com um segredo real, negue-o com `sandmux files deny`.
- **Certificados:** um certificado público em `.pem` na pasta de trabalho também fica escondido; libere com `file-allow` ou `secret-files-off`.
- **Renomear e ligar:** renomear ou ligar um segredo para outro nome é negado, para ele não aparecer com outro nome.
- **No Docker privado:** os contêineres não passam pelo supervisor. Os segredos e os caminhos de `file-deny` que existem quando o Docker da pasta sobe chegam vazios a eles, com limites que o supervisor não tem: veja [docker.md](docker.md#segredos-e-caminhos-negados).

### `.git/commondir` e as chamadas de nome

Um `.git/commondir` criado pelo agente faria o git do host usar outro diretório git, com outros hooks. Criar esse arquivo na pasta de trabalho é negado, por abertura, `mv`, `ln` ou `ln -s`.

Para isso, `rename`, `renameat`, `renameat2`, `link`, `linkat`, `symlink` e `symlinkat` também passam pelo supervisor. Ele lê os caminhos uma vez, resolve as pastas dentro da raiz do sandbox e decide a política. Depois, um auxiliar faz a chamada dentro do sandbox. O auxiliar fica nos namespaces de usuário e de montagem do sandbox, sob o mesmo Landlock, e assim o kernel decide os pontos de montagem e as regras de escrita como para o agente. O `file-deny` também vale para o destino de um rename ou link.

## Pastas novas ao vivo

Uma pasta do host entra na sessão viva, sem relançar o agente, em `/mnt/sandmux/<nome>`. `/mnt/sandmux` existe em todo sandbox, liberado para escrita desde o início, e a variável `SANDMUX_FOLDERS` aponta para ele: ler, gravar, criar, apagar e renomear valem na hora, conforme o modo da pasta.

```bash
sandmux folder add 3 ~/dados --rw   # leitura e escrita em /mnt/sandmux/dados
sandmux folder add 3 ~/docs         # só leitura em /mnt/sandmux/docs
sandmux folder list 3
sandmux folder remove 3 docs
```

- **Nome:** o último componente da pasta, com `-2`, `-3` quando repete.
- **Aviso ao agente:** `/mnt/sandmux/PASTAS.txt`, em somente leitura, lista cada nome com o modo e a pasta do host. Diga ao agente onde procurar.
- **Recusas:** as mesmas do `--rw` (o HOME, a raiz, uma pasta que contém o HOME, o diretório de dados do sandmux e, com escrita, a pasta de configuração); uma pasta com credenciais avisa.
- **Lembradas:** as pastas ficam na aba e voltam nos lançamentos seguintes da sessão, no mesmo lugar. Numa sessão parada, `folder add` vale no próximo lançamento.
- **Registro e política:** `/mnt/sandmux` entra nas pastas do agente do registro de arquivos, e a política de arquivos vale para ela.
- **TUI:** Ctrl+b `o` abre a tela de pastas ([tui.md](tui.md)).

Como funciona: um auxiliar do supervisor entra no user namespace do sandbox e cria um mount namespace próprio, uma cópia da árvore do host. Ali ele copia a pasta (`open_tree`), aplica o modo e liga a cópia em `/mnt/sandmux/<nome>` no mount namespace do sandbox (`move_mount`). O daemon fala com o supervisor por um socket de controle por lançamento, no diretório de execução do usuário.

## Custo

Medido na máquina de referência (mediana de 3 rodadas): `opencode --version` 1,0x; o hermes importando o CLI 1,1x a 1,2x; `cargo check` incremental 1,2x a 1,3x; ler 13 mil arquivos seguidos 3,2x. Quatro threads do supervisor atendem as aberturas.

[Pelo kernel](#pelo-kernel), numa VM do Ubuntu 24.04 (mediana de 3 rodadas, contra o mesmo trabalho fora do sandbox): ler 13 mil arquivos da pasta de trabalho 1,05x no kernel 6.8 e 1,14x no 7.0, contra 2,7x e 3,0x do supervisor na mesma VM (medido em 2026-11-21, com as repetições tiradas no kernel); o import de módulos do Python fica igual ao de fora (o sistema não passa pela fila).

## Registro

Cada acesso distinto (caminho e tipo de acesso) às pastas do agente vira uma linha em `$XDG_STATE_HOME/sandmux/files/AAAA-MM-DD.jsonl` (normalmente `~/.local/state/sandmux/files/`), com pasta 700 e arquivo 600, na primeira vez em cada lançamento. Um bloqueio é sempre registrado.

- **Pastas do agente:** a pasta de trabalho, o HOME sintético (com a pasta de entrada) e as pastas extras (`--ro`, `--rw`).
- **Fora do registro:** o sistema, as toolchains e os caches montados no HOME (as camadas), e `/tmp`.

| Campo | Conteúdo |
|---|---|
| `ts` | data e hora locais |
| `tag` | o lançamento: `s<sessão>-<n>` numa sessão, `run-<pid>` num `run` sem terminal |
| `workdir` | pasta de trabalho |
| `path` | caminho na visão do agente |
| `access` | `r` (leitura), `w` (escrita), `rw` ou `create` (arquivo criado) |
| `blocked` | motivo do bloqueio (`file-deny: <padrão>`, `segredo` ou `landlock`), ou `null` |
| `new` | o caminho apareceu pela primeira vez nesta pasta de trabalho |

## `sandmux files`

```bash
sandmux files            # acessos de hoje, de todas as sessões
sandmux files 3          # só da sessão 3
sandmux files --days 7   # dos últimos 7 dias
```

Mostra as últimas 200 linhas, com a hora, a sessão, o acesso, o caminho e o bloqueio.

### Execuções

Cada programa executado numa aba (pelo agente ou num shell) vira uma linha com o acesso `exec`, o caminho executado e os argumentos (`args`, até 1024 bytes). O campo `via` diz a fonte:

- **`kernel`:** com o [auxiliar root](helper.md) e o BPF LSM ativos, um programa no tracepoint de execução do kernel registra o que o kernel de fato executou. Não dá para falsificar.
- **`supervisor`:** sem eles, o supervisor de aberturas recebe também `execve` e `execveat`, lê o caminho e os argumentos na memória do processo e deixa o kernel seguir. Outra thread pode trocar o caminho ou os argumentos entre a leitura e a execução, então essa fonte pode ser enganada.

O supervisor registra desde o começo da aba. Quando o auxiliar assume (logo depois de o escopo da aba aparecer), o daemon o desliga; se a conexão com o auxiliar cair, ele volta. Na passagem, uma execução pode sair nas duas fontes. Com o auxiliar, as execuções do Docker privado da pasta também entram, com a etiqueta `docker-<id>`.

O caminho é o que foi passado ao `execve`: um programa chamado por `./script` aparece como `./script`. O ambiente da execução não entra no registro. Nenhuma execução é negada por este registro.

A proteção do lançamento, no `sandmux stats <id>` e no painel, mostra a linha "execuções": "pelo kernel" ou "supervisor: <motivo>". `sandmux files` mostra os argumentos depois do caminho.

### Contêineres do Docker privado

Com o [confinamento por eBPF](docker.md#confinamento-por-ebpf), o registro também tem as linhas dos contêineres do Docker privado da pasta, com a etiqueta `docker-<id>` e o caminho do host:

- cada operação negada, com o motivo (`segredo`, `file-deny: <padrão>`, `.git` ou `cobertura`);
- cada arquivo que um contêiner cria na pasta de trabalho, uma vez por subida do Docker.

As leituras e escritas comuns dos contêineres não entram. Uma linha com o caminho da pasta e o acesso `?` diz quantos eventos foram descartados com a fila cheia. `sandmux files <id>` mostra essas linhas junto com as da sessão, na coluna da sessão como `docker`.

## Retenção

O registro de arquivos segue a retenção do registro de rede: `net-log-days` (padrão 30 dias) e `net-log-max` (padrão `"500M"`), em `[global]` na [configuração](config.md). O daemon apaga os arquivos velhos ao subir e de hora em hora.
