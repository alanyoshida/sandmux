# Sessões e o daemon `sandmuxd`

O `sandmuxd` é o processo de cada usuário que segura as sessões. Uma sessão é um harness (ou o shell) confinado pelo `sandmux-run` dentro de um pseudoterminal que o daemon mantém aberto. A sessão sobrevive ao terminal de onde veio.

## Quando o `run` usa o daemon

| Chamada | O que acontece |
|---|---|
| `sandmux run <harness>` num terminal (entrada e saída são terminais) | O cliente faz as verificações e perguntas (preset, harness instalado, importação da primeira execução), pede a sessão ao daemon e anexa o terminal a ela |
| `sandmux run <harness>` sem terminal (script, pipe, CI) | Roda direto, em primeiro plano, sem daemon: a saída é a do harness, sem sequências de terminal, e o código de saída é o dele |

## Comandos

| Comando | Efeito |
|---|---|
| `sandmux ls [--group <grupo>]` | Lista as sessões com identificador, nome (`<harness>:<pasta>`), [grupo](#grupos-de-abas), estado, terminais anexados, memória e processos em uso com os tetos, e pasta |
| `sandmux group list\|move <id> <grupo>\|rename <grupo> <novo>` | Os [grupos de abas](#grupos-de-abas): listar, levar uma sessão para outro grupo, renomear um grupo |
| `sandmux stats <id>` | Detalhe do consumo: memória e processos em uso, picos e tetos, CPU usada, mortes por falta de memória e criações de processo recusadas ([docs/limits.md](limits.md)), e as portas publicadas no host ([docs/network.md](network.md)) |
| `sandmux net [<id>]` | As últimas conexões de rede registradas, de uma sessão ou de todas ([docs/network.md](network.md)) |
| `sandmux net deny\|allow\|forget <padrão> <id>` | Muda as regras de rede da aba de uma sessão viva, com efeito na conexão seguinte |
| `sandmux net mode <modo> <id>` | Troca o modo de rede da aba |
| `sandmux files deny\|allow\|forget <padrão> <id>` | Nega, libera ou tira um padrão de arquivo da aba de uma sessão, com efeito na abertura seguinte ([docs/files.md](files.md)) |
| `sandmux db add\|list\|remove\|log` | Conexões de banco no cofre e o registro das queries do MCP de banco ([docs/database.md](database.md)) |
| `sandmux mcp list [<id>]` | Os servidores MCP da sessão, ou do harness na pasta atual, com o estado ([docs/mcp.md](mcp.md)) |
| `sandmux mcp deny\|allow\|forget <nome> <id>` | Desabilita, põe na allowlist ou tira das regras um servidor MCP na aba; vale no próximo lançamento |
| `sandmux folder add <id> <pasta> [--rw]` | Monta uma pasta do host em `/mnt/sandmux/<nome>` na sessão, na hora ([docs/files.md](files.md#pastas-novas-ao-vivo)) |
| `sandmux folder remove <id> <nome>` | Desmonta uma pasta de `/mnt/sandmux` |
| `sandmux folder list <id>` | Lista as pastas da sessão em `/mnt/sandmux` |
| `sandmux files [<id>] [--days N]` | Os acessos às pastas do agente registrados, de uma sessão ou de todas ([docs/files.md](files.md)) |
| `sandmux net rules [<id>]` | O modo e as regras de rede efetivas, com a origem |
| `sandmux attach [id]` | Anexa o terminal. Sem identificador, à única sessão viva |
| `sandmux detach <id>` | Desanexa todos os terminais da sessão |
| `sandmux kill <id>` | Pede ao harness que termine (SIGTERM e SIGHUP); depois de 5 segundos, encerra à força. A sessão sai da lista |
| `sandmux restart <id>` | Relança uma sessão encerrada ou parada, com as mesmas opções e o mesmo perfil, resolvendo a configuração de novo. Os valores de `pass-env` vêm do terminal que pede |
| `sandmux update <harness> [--yes] [-- args]` | Roda o atualizador oficial do harness no host, fora do sandbox ([detalhes](#atualizar-o-harness)) |
| `sandmux daemon status` | Estado do daemon, sem iniciá-lo |
| `sandmux daemon stop` | Encerra os harnesses e para o daemon. As sessões vivas voltam na próxima subida |
| `sandmux daemon restart` | Para e sobe com o binário instalado; use depois de atualizar o sandmux |
| `sandmux daemon disable` | Para o daemon e remove a unidade systemd |

Estados de uma sessão: **viva**, **encerrada** (com o código de saída do harness e, quando houve morte por falta de memória, a nota "memória esgotada"), **parada** (com o motivo: falta de `--pass-env` depois de uma restauração, pasta de trabalho apagada, falha ao relançar) **aguardando cofre** (restaurada com os valores de `--pass-env` guardados no [cofre](vault.md), ou a sessão do gateway do hermes ou uma aba de chat com o cofre criado; sobe quando o cofre é desbloqueado) e **reiniciando em N s** (o [gateway do hermes](hermes.md) terminou e volta depois da espera).

## Grupos de abas

Cada sessão (aba) pertence a um grupo, e cada TUI mostra um grupo por vez, como as sessões do tmux:

- **Criar:** `sandmux run <harness> --group estudos` cria a sessão no grupo `estudos`. Sem `--group`, ela vai para o `principal`. Uma aba criada na TUI vai para o grupo que a TUI está mostrando.
- **Existência:** um grupo existe enquanto tem abas; o `principal` existe sempre. Não há comando para criar ou apagar grupo: ele nasce com a primeira aba e some com a última.
- **Nomes:** de 1 a 32 caracteres, entre letras, dígitos, `-`, `_` e `.`.
- **Comandos:**
  - `sandmux group list` lista os grupos, com o número de abas;
  - `sandmux group move <id> <grupo>` leva uma sessão para outro grupo;
  - `sandmux group rename <grupo> <novo>` renomeia um grupo; para um nome que já existe, junta os dois.
- **TUI:** `sandmux tui --group <grupo>` abre a TUI no grupo. Sem ele, abre no `principal`, ou, com o `principal` vazio, no primeiro grupo com abas. Dentro da TUI, Ctrl+b `(` e `)` trocam de grupo, e Ctrl+b `G` abre a lista de grupos ([tui.md](tui.md#grupos-de-abas)).
- **Fora dos grupos:** o `attach <id>` e os outros comandos por identificador ignoram os grupos. O grupo volta com a sessão na restauração.

## Modo anexado

O terminal do usuário passa a ser o terminal da sessão. Ao anexar, o daemon envia o histórico (para a rolagem do próprio terminal), depois a tela atual e os modos de terminal que o harness ativou.

| Teclas | Efeito |
|---|---|
| Ctrl+b d | Desanexa |
| Ctrl+b Ctrl+b | Envia um Ctrl+b ao harness |
| Ctrl+b seguido de outra tecla | A tecla é descartada |

O Ctrl+b é reconhecido como o byte 0x02 e também na forma do protocolo de teclado do kitty (`CSI 98 ; 5 u`), que o opencode liga.

Fechar o terminal, ou mandar SIGTERM ao cliente, só desanexa. Ao desanexar, o cliente desfaz no terminal do usuário a tela alternativa, o mouse, os eventos de foco, a colagem entre marcadores e o protocolo do kitty.

Vários terminais podem se anexar à mesma sessão; todos recebem a saída e todos digitam. A tela da sessão tem o menor número de colunas e de linhas entre os terminais anexados.

## Shell dentro do sandbox de uma aba

`sandmux enter [id]` (sem identificador, a única sessão viva), ou Ctrl+b `t` na TUI, abre uma sessão nova, `dentro:<pasta>`, com um shell dentro do sandbox da aba alvo. O shell vê exatamente o que o agente vê:

- **arquivos:** a mesma visão, a mesma pasta inicial, o mesmo Landlock;
- **regras de arquivos:** as aberturas passam pelas mesmas regras e entram no mesmo registro (um `.env` continua negado);
- **rede e restrições:** a mesma rede e o mesmo seccomp;
- **processos:** os mesmos, porque o shell está no mesmo pid namespace (`ps` mostra o agente).

O shell conta nos tetos da aba alvo, e o que ele faz na rede e nos arquivos aparece nos registros dela.

A aba de dentro termina quando o shell sai, ou quando o sandbox alvo termina. Depois de um reinício do daemon, ela volta parada, porque o sandbox alvo é outro; Ctrl+b `r` entra de novo. Uma aba de dentro não abre outra. Uma aba lançada por uma versão anterior do sandmux precisa ser reiniciada antes.

## Atualizar o harness

O código do harness entra no sandbox em somente leitura, e a atualização automática fica desligada lá dentro. `sandmux update <harness>` roda o atualizador oficial no host, fora do sandbox e fora do daemon, em primeiro plano, no seu terminal:

| Harness | Atualizador |
|---|---|
| `opencode` | `~/.opencode/bin/opencode upgrade` |
| `hermes` | `~/.local/bin/hermes update` |
| `claude` | `~/.local/bin/claude update` |
| `omp` | `~/.local/bin/omp update` |

- O comando termina com o código de saída do atualizador.
- Os argumentos depois de `--` vão para o atualizador: `sandmux update omp -- --check`.
- O `shell` não tem atualizador. Um harness não instalado é recusado com a instrução de instalação, como no `run`: o sandmux não instala harness.
- **Abas vivas:** se há abas vivas do harness, o comando as lista e pergunta antes, porque o atualizador troca arquivos que elas podem estar usando. `--yes` pula a pergunta; sem terminal e sem `--yes`, o comando recusa. Depois, ele diz quais abas reiniciar (Ctrl+b `r`, ou `sandmux restart <id>`) para usar a versão nova.

O sandmux em si é atualizado pelo pacote ou pelo `install.sh` ([install.md](install.md)).

## Histórico e clipboard

- Cada sessão guarda as últimas 10.000 linhas que saíram da tela principal, só em memória (`sandmuxd --history-lines N` muda o limite). Nada da saída das sessões é gravado em disco, nem no log do daemon.
- A escrita no clipboard pelo harness (OSC 52) é retirada da saída, a menos que a sessão tenha sido criada com `--allow-clipboard`. A consulta do clipboard (`OSC 52 ; … ; ?`) é sempre retirada. O filtro reconhece OSC iniciado por `ESC ]` ou pelo C1 em UTF-8 (`C2 9D`) e terminado por BEL, `ESC \` ou `C2 9C`, mesmo dividido entre leituras; um OSC acima de 1 MiB é descartado.

## Como o daemon sobe

Quando um comando precisa do daemon e ele não responde:

1. Com systemd de usuário, o cliente grava `~/.config/systemd/user/sandmuxd.service` (com `ExecStart` apontando para o `sandmuxd` instalado e `XDG_DATA_HOME`, quando definido), habilita no login e inicia, avisando na primeira vez. Se o binário mudar de lugar, a unidade é regravada.
2. Sem systemd, ou com `SANDMUX_DAEMON_START=direct`, o cliente sobe o `sandmuxd` como processo próprio, desligado do terminal, com o log em `$XDG_STATE_HOME/sandmux/sandmuxd.log` (ou `~/.local/state/sandmux/sandmuxd.log`).

## Socket e acesso

- Socket: `$XDG_RUNTIME_DIR/sandmux/sandmuxd.sock`, ou `/tmp/sandmux-<uid>/sandmuxd.sock` sem `XDG_RUNTIME_DIR`. A pasta tem modo 700; uma pasta que já existe com outro dono, outro modo ou como link simbólico é recusada.
- Cada conexão tem o UID conferido por `SO_PEERCRED`; outro UID é recusado.
- Uma trava (`sandmuxd.lock`, com o PID) garante um daemon por usuário.
- O sandbox não alcança o socket: `/run` e `/tmp` são novos lá dentro, e o Landlock nega sockets abstratos do host. A suíte de fuga tenta os dois caminhos.

## Protocolo

Socket Unix de fluxo. Cada quadro tem 4 bytes de tamanho (big-endian, só o conteúdo, até 16 MiB), 1 byte de tipo e o conteúdo:

| Tipo | Conteúdo |
|---|---|
| 0 | Mensagem de controle em JSON (`{"t": "list"}`, `{"t": "created", "id": 1, "name": "shell:app"}`, …) |
| 1 | Bytes de terminal: entrada do cliente ou saída da sessão |

A primeira mensagem é a saudação (`hello`), com o número do protocolo, a versão, o diretório de dados, a pasta de configuração e o idioma do cliente (o daemon responde nesse idioma). Esta versão usa o protocolo 30. Números diferentes encerram a conexão, e o cliente mostra as duas versões e indica `sandmux daemon restart`. Um diretório de dados ou uma pasta de configuração diferente também é recusado, com as duas pastas na mensagem.

O cliente envia ao daemon, junto com o pedido de sessão, as opções dadas no `run`, o perfil nomeado, a pasta atual, as variáveis da lista segura, o `SHELL` e os valores de `pass-env` da política efetiva. O daemon resolve as camadas de configuração ([docs/config.md](config.md)) e compõe a especificação de lançamento com a mesma função do `run` direto.

## Lista de sessões e restauração

O daemon grava `sessions.json` no diretório de dados (modo 600, de forma atômica) a cada mudança. Para cada sessão: identificador, nome, harness, pasta, opções dadas no `run`, perfil nomeado, nomes das variáveis de `pass-env` do último lançamento e estado. Nunca os valores das variáveis, nunca a política resolvida, nunca a saída. O arquivo tem `"version": 2`; a lista sem versão, gravada pelo change anterior, é lida e convertida.

Ao subir, o daemon lê a lista:

| Sessão guardada | Ao subir |
|---|---|
| Viva, sem `pass-env` na política resolvida agora | Relançada sozinha, sem terminal anexado, com `TERM=xterm-256color` |
| Viva, com `pass-env` e os valores guardados no [cofre](vault.md) | Aguardando cofre; sobe sozinha quando o cofre é desbloqueado |
| Viva, com `pass-env` sem os valores no cofre | Parada até `sandmux restart <id>` num terminal com as variáveis |
| Gateway do hermes, vivo ou reiniciando, com o cofre criado | Aguardando cofre; sobe sozinha quando o cofre é desbloqueado |
| Aba do opencode viva, com provedor local (porta em `host-ports`) que usa uma chave do cofre, e o cofre criado | Aguardando cofre; sobe com a lista de modelos do servidor local quando o cofre é desbloqueado ([network.md](network.md#portas-do-host)) |
| Aba de chat viva, com o cofre criado | Aguardando cofre; sobe sozinha, com a conversa, quando o cofre é desbloqueado ([chat.md](chat.md)) |
| Viva, com o perfil nomeado apagado ou a configuração inválida | Parada, com o motivo |
| Viva, com a pasta apagada | Parada, com o motivo |
| Encerrada ou parada | Volta como estava |

Quando o daemon termina, por parada, reinício ou falha, nenhum processo de sandbox continua: o `sandmux-run` de cada sessão nasce com `PR_SET_PDEATHSIG` ligado ao daemon, e o processo 1 do sandbox, ligado ao `sandmux-run`.

## Limites

- **Repasse cru.** A saída passa crua aos clientes. Um terminal maior que o menor anexado mostra a sessão no canto superior esquerdo.
- **Consultas ao terminal.** Com dois clientes anexados, uma consulta do harness ao terminal (atributos, cor de fundo, posição do cursor) recebe duas respostas. Numa sessão restaurada sem cliente, as consultas feitas na partida ficam sem resposta.
- **`tty` dentro do sandbox.** O `tty` responde "not a tty", embora a entrada seja um terminal: o `/dev/pts` do sandbox é uma instância nova, sem o pseudoterminal do host. Isso vale também para o `run` direto.
- **Histórico em tela cheia.** Numa sessão em tela alternativa (opencode, hermes), o histórico reenviado é o da tela principal, que costuma estar vazio.

## Variáveis para testes e diagnóstico

| Variável | Efeito |
|---|---|
| `SANDMUX_DAEMON_START=direct` | O cliente nunca usa o systemd |
| `SANDMUX_START_MODE` | Como o daemon diz ter sido iniciado (`systemd`, `direct`); preenchida pelo cliente e pela unidade |
| `SANDMUX_PROTOCOL_OVERRIDE` | O daemon anuncia outro número de protocolo; só serve para testar a recusa |
| `SANDMUX_SECRET_SCAN_SECS` | Intervalo, em segundos, da varredura periódica de chaves reais nos HOMEs sintéticos (60 por padrão) |
| `SANDMUX_NOTIFY_BUS` | Endereço do barramento para as notificações do desktop, no lugar do barramento de sessão; os testes apontam para um barramento próprio |
| `SANDMUX_SECRET_INOTIFY=0` | O vigia de chaves reais não usa inotify, só a varredura periódica; só serve para testá-la |
