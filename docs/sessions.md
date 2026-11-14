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
| `sandmux ls` | Lista as sessões com identificador, nome (`<harness>:<pasta>`), estado, terminais anexados e pasta |
| `sandmux attach [id]` | Anexa o terminal. Sem identificador, à única sessão viva |
| `sandmux detach <id>` | Desanexa todos os terminais da sessão |
| `sandmux kill <id>` | Pede ao harness que termine (SIGTERM e SIGHUP); depois de 5 segundos, encerra à força. A sessão sai da lista |
| `sandmux restart <id>` | Relança uma sessão encerrada ou parada, com as mesmas opções e o mesmo perfil, resolvendo a configuração de novo. Os valores de `pass-env` vêm do terminal que pede |
| `sandmux daemon status` | Estado do daemon, sem iniciá-lo |
| `sandmux daemon stop` | Encerra os harnesses e para o daemon. As sessões vivas voltam na próxima subida |
| `sandmux daemon restart` | Para e sobe com o binário instalado; use depois de atualizar o sandmux |
| `sandmux daemon disable` | Para o daemon e remove a unidade systemd |

Estados de uma sessão: **viva**, **encerrada** (com o código de saída do harness) e **parada** (com o motivo: falta de `--pass-env` depois de uma restauração, pasta de trabalho apagada, falha ao relançar).

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

A primeira mensagem é a saudação (`hello`), com o número do protocolo, a versão, o diretório de dados e a pasta de configuração do cliente. Esta versão usa o protocolo 2. Números diferentes encerram a conexão, e o cliente mostra as duas versões e indica `sandmux daemon restart`. Um diretório de dados ou uma pasta de configuração diferente também é recusado, com as duas pastas na mensagem.

O cliente envia ao daemon, junto com o pedido de sessão, as opções dadas no `run`, o perfil nomeado, a pasta atual, as variáveis da lista segura, o `SHELL` e os valores de `pass-env` da política efetiva. O daemon resolve as camadas de configuração ([docs/config.md](config.md)) e compõe a especificação de lançamento com a mesma função do `run` direto.

## Lista de sessões e restauração

O daemon grava `sessions.json` no diretório de dados (modo 600, de forma atômica) a cada mudança. Para cada sessão: identificador, nome, harness, pasta, opções dadas no `run`, perfil nomeado, nomes das variáveis de `pass-env` do último lançamento e estado. Nunca os valores das variáveis, nunca a política resolvida, nunca a saída. O arquivo tem `"version": 2`; a lista sem versão, gravada pelo change anterior, é lida e convertida.

Ao subir, o daemon lê a lista:

| Sessão guardada | Ao subir |
|---|---|
| Viva, sem `pass-env` na política resolvida agora | Relançada sozinha, sem terminal anexado, com `TERM=xterm-256color` |
| Viva, com `pass-env` (pela opção ou pela configuração) | Parada até `sandmux restart <id>` num terminal com as variáveis |
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
