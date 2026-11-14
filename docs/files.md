# Arquivos

O sandmux vê cada abertura de arquivo do agente, com o agente rodando, e registra os acessos às pastas do agente. Ele também nega e libera arquivos na hora e esconde os arquivos com cara de segredo da pasta de trabalho.

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

## Retenção

O registro de arquivos segue a retenção do registro de rede: `net-log-days` (padrão 30 dias) e `net-log-max` (padrão `"500M"`), em `[global]` na [configuração](config.md). O daemon apaga os arquivos velhos ao subir e de hora em hora.
