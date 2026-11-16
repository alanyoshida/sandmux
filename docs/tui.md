# Interface com abas

`sandmux` sem subcomando, ou `sandmux tui`, num terminal, abre a interface do sandmux. Cada sessão do daemon é uma aba: as mesmas abas aparecem em todo terminal que abre a TUI, e as sessões continuam quando ela fecha.

```
[sandmux] ana@host:~/projetos/app$ █

 ≡   1  shell:app   2  opencode:api ✕   3  hermes:notas   imagem colada…   shell   ~/projetos/app   cofre aberto   ! 1   1h 20m
```

- **Última linha, a barra:** no estilo em blocos (o padrão), o `≡` do começo abre o [menu](#menu). Cada aba é um bloco com a posição (o número dos atalhos), seguido do nome e de uma marca: ✕ para encerrada (um clique nele fecha a aba), ‖ para parada, (cofre) para aguardando cofre e ↻ para o gateway reiniciando, ✱ quando há [chaves reais](vault.md#movimento-automático) movidas ou esperando, e ▣ quando a aba tem [janela](window.md). A aba ativa tem o bloco em destaque. No meio aparecem os avisos. À direita, em blocos coloridos:
  - o harness da aba ativa;
  - a pasta da aba ativa, num bloco de outra cor;
  - o estado do cofre (aberto, trancado ou sem cofre);
  - os alertas, quantas abas têm bloqueio (`!`), domínio novo (`•`) e chaves reais (`✱`), só quando há;
  - há quanto tempo a aba ativa está viva.

  Sem espaço, os blocos da direita saem da esquerda para a direita, e o tempo fica por último.
- **Resto da tela:** o terminal da aba ativa, com cores (inclusive cor verdadeira), atributos, cursor e tela alternativa. Trocar de aba mostra a tela da outra na hora.

### Fonte e estilo da barra

O estilo padrão, `"blocos"`, usa só blocos coloridos e caracteres que toda fonte tem. O estilo `"powerline"` acrescenta separadores inclinados e ícones, que só existem nas fontes com os símbolos do Powerline e do Nerd Font (por exemplo, as do projeto Nerd Fonts); sem uma delas, eles aparecem como quadrados. `[tui]` tem duas chaves:

| Chave | Valores | Padrão |
|---|---|---|
| `style` | `"blocos"`; `"powerline"`, com separadores e ícones de Nerd Font; ou `"simples"`, a barra só com texto e a aba ativa em vídeo inverso | `"blocos"` |
| `bar` | `"bottom"` (última linha) ou `"top"` (primeira linha) | `"bottom"` |

```toml
# Sem uma fonte com os símbolos do Powerline:
[tui]
style = "simples"
bar = "top"
```

```toml
# Com uma Nerd Font no terminal, os separadores e os ícones:
[tui]
style = "powerline"
```

Sem nenhuma sessão, a TUI abre o modal de nova aba. Sem terminal interativo (pipe, script), `sandmux tui` termina com erro.

## Atalhos

Todos começam pela tecla de prefixo, Ctrl+b por padrão:

| Tecla | Ação |
|---|---|
| `c` | Nova aba |
| `n` / `p` | Próxima / anterior |
| `1` a `9` | Ir para a aba nessa posição (`0` é a décima) |
| `w` | Lista de abas (setas, Enter, Esc) |
| `(` / `)` | Grupo de abas anterior / próximo ([grupos](#grupos-de-abas)) |
| `G` | Lista de grupos: trocar, criar, renomear, levar a aba |
| `%` / `"` | Divide o painel ativo lado a lado / em cima e embaixo ([painéis](#painéis)) |
| ← → ↑ ↓ | Foco no painel vizinho |
| `z` | Amplia o painel ativo para a aba inteira, ou volta ao layout |
| `<` / `>` | Diminui / aumenta o painel ativo |
| `,` | Renomear a aba |
| `r` | Reiniciar a aba encerrada ou parada |
| `x` | Fechar a aba (encerra a sessão, com confirmação) |
| `v` | Colar imagem do clipboard |
| `d` | Sair da TUI (as sessões continuam) |
| `?` | Ajuda |
| `s` | Mostrar ou esconder o painel lateral |
| `a` | Tela de rede da aba: acessos e regras |
| `f` | Tela de arquivos da aba: acessos, com `b` para bloquear e `l` para liberar |
| `o` | Tela de pastas da aba: monta e desmonta pastas do host em `/mnt/sandmux` |
| `m` | Tela de MCPs da aba: habilita e desabilita servidores |
| `i` | Modal de chat: uma pergunta a um modelo, que abre uma aba de chat ([chat.md](chat.md)) |
| `g` | Modal de comando: o modelo gera um comando, escrito na aba sem executar ([chat.md](chat.md#modal-de-comando)) |
| `e` | [Configurações](#configurações) |
| `t` | Shell dentro do sandbox da aba ([sessions.md](sessions.md#shell-dentro-do-sandbox-de-uma-aba)) |
| `:` | [Menu](#menu) com todas as ações |
| o prefixo de novo | Envia o prefixo ao harness |

Outra tecla depois do prefixo é descartada. O prefixo funciona como byte de controle e também na forma do protocolo de teclado do kitty.

### Trocar os atalhos

Na seção `[tui]` do [arquivo de configuração](config.md):

```toml
[tui]
prefix = "C-a"

[tui.keys]
new = "t"
close = "q"
```

- **Ações:** `new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help`, `panel`, `network`, `files`, `folders`, `mcp`, `chat`, `command`, `menu`, `settings`, `enter`, `group-prev`, `group-next`, `groups`, `split-side`, `split-stack`, `zoom`, `pane-grow` e `pane-shrink`. As setas depois do prefixo, que movem o foco entre os painéis, são fixas.
- **Teclas:** um caractere visível, que não seja dígito, porque os dígitos levam às abas.
- **Duas ações na mesma tecla** escolhidas em `[tui.keys]` tornam o arquivo inválido. Uma tecla escolhida que é a padrão de outra ação vence, e a outra ação fica sem tecla (aparece com `—` na ajuda e continua no menu). Assim uma ação nova numa versão nova nunca invalida o arquivo.
- **`[tui]` não é camada de política:** não muda nada nos sandboxes.

## Painel lateral

Com o terminal de pelo menos 100 colunas, um painel de 34 colunas fica à direita. A área do terminal das abas encolhe na mesma medida, e as sessões recebem o tamanho novo. O painel tem cinco seções sobre a aba ativa. Cada uma começa por uma faixa colorida com o nome e, à direita, um resumo: o nível de proteção, o estado da sessão, quantas portas estão publicadas, o modo de rede, quantos MCPs estão habilitados. Os valores ficam em colunas: o rótulo à esquerda, num tom mais apagado, e o valor alinhado à direita.

**Proteção** mostra o que foi aplicado no último lançamento da sessão. O que é ligado ou desligado aparece em pontos de estado, em duas colunas: `●` colorido para ligado, `○` apagado para desligado (o clipboard e o `pass-env` ligados em amarelo, porque abrem algo ao agente):

- o nível de proteção, com a ABI do Landlock;
- a camada de escrita;
- a pasta de trabalho e as pastas extras, em leitura ou com escrita;
- GPU e KVM;
- a escrita no clipboard;
- as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente;
- os tetos de memória e de processos;
- a rede.

**Recursos** mostra, atualizado a cada segundo:

- a memória em uso, o pico e o teto, com um medidor de uso;
- os processos, também com medidor;
- o CPU;
- as mortes por falta de memória;
- as chaves reais do HOME sintético: movidas ao cofre desde o lançamento (reinicie a aba), esperando o cofre ou esperando um domínio;
- o Docker privado da pasta.

O medidor fica verde até 75% do teto, amarelo até 90% e vermelho acima disso. Uma morte por falta de memória aparece em vermelho.

**Portas** mostra as portas publicadas no `127.0.0.1` do host: as que o agente escuta no sandbox e as dos contêineres do Docker privado da pasta, como `3000 → 127.0.0.1:3000 (sandbox)`.

**Rede** mostra o modo de rede da sessão e as últimas 6 conexões registradas da aba, numa tabela sem bordas: domínio (ou "sem domínio"), bytes enviados (`↑`) e recebidos (`↓`) e status (o código HTTP, `✗` para bloqueio, "novo"). Um domínio longo fica numa linha só, com os números embaixo, e o motivo de um bloqueio aparece embaixo da linha. Um domínio visto pela primeira vez na pasta de trabalho aparece como "novo". Uma aba inativa ganha na barra `•` quando a sessão acessa um domínio novo e `!` quando tem um bloqueio, até ficar ativa. O status HTTP de erro e o bloqueio aparecem em vermelho, e "novo" em amarelo. O registro completo está em [network.md](network.md).

**MCP** mostra os servidores MCP da aba, com ✓ em verde para os habilitados e ✗ em vermelho para os desabilitados.

**Ordem e seções recolhidas:** um clique no título de uma seção a recolhe (`▸`) ou expande (`▾`), e as setas `↑` e `↓` no fim da faixa a movem uma posição. As duas coisas ficam gravadas em `[tui]` e valem na próxima vez que a TUI abrir:

```toml
[tui]
panel-order = ["rede", "recursos", "protecao", "portas", "mcp"]
panel-collapsed = ["protecao"]
```

As seções são `protecao`, `recursos`, `portas`, `rede` e `mcp`. As que faltam em `panel-order` vão para o fim, na ordem padrão. Voltar à ordem padrão, ou expandir todas, apaga a chave.

Ctrl+b `s` esconde e mostra o painel. A chave `panel` de `[tui]` (`true` ou `false`) define como a TUI abre.

## Cofre trancado

Quando o [cofre](vault.md) existe e está trancado, a TUI abre com um modal pedindo a senha. O que é digitado aparece como `•`. Enter desbloqueia, e as sessões que aguardavam o cofre sobem; uma senha errada mostra o erro e pede de novo; Esc fecha o modal e deixa o cofre trancado. Uma aba aguardando o cofre mostra `(cofre)` ao lado do nome.

## Tela de rede

Ctrl+b `f` abre a tela de arquivos da aba ativa: os acessos às pastas do agente registrados desde ontem, um por caminho, do mais recente ao mais antigo, com `•` nos caminhos nunca vistos nesta pasta de trabalho e o motivo dos bloqueios. `b` nega o caminho escolhido na aba (o caminho exato, como `sandmux files deny`), `l` o libera, ↑ ↓ (ou `k` e `j`) andam e Esc fecha. A regra vale na abertura seguinte ([files.md](files.md)).

Ctrl+b `m` abre a tela de MCPs da aba ativa: os servidores do harness, com o tipo, a origem, o alvo e o estado. Espaço habilita ou desabilita o escolhido na aba, valendo no próximo lançamento (Ctrl+b `r`); Esc fecha. O painel tem a seção **MCP**, com `✓` e `✗` ([mcp.md](mcp.md)).

Ctrl+b `o` abre a tela de pastas da aba ativa: as pastas do host montadas em `/mnt/sandmux`, com o modo (`ro` ou `rw`). `a` abre um campo para digitar a pasta (Enter confirma) e pergunta o modo (`r` só leitura, `w` leitura e escrita); a pasta é montada na hora, sem relançar o agente. `x` desmonta a escolhida, e Esc fecha ([files.md](files.md#pastas-novas-ao-vivo)).

Ctrl+b `a` abre a tela de rede da aba ativa, com duas listas alternadas por Tab:

- **Acessos:** as últimas 50 conexões e consultas da aba, das mais novas às mais velhas, com o protocolo, o domínio, o destino, o volume e "novo" ou o bloqueio.
- **Regras:** o modo e as regras efetivas, com o estado (ligado ou desligado), o tipo (nega ou libera), o padrão e a origem.

| Tecla | Lista | Efeito |
|---|---|---|
| `n` | Acessos | Nega o destino do acesso escolhido |
| `l` | Acessos | Libera o destino (vale no modo `custom`) |
| espaço | Regras | Liga ou desliga um item de catálogo ou da allowlist |
| `x` | Regras | Tira a regra da camada de onde veio |
| `m` | as duas | Troca o modo da aba: aberta, só inferência, custom |
| ↑ ↓ | as duas | Escolhe a linha |
| Esc | as duas | Fecha a tela |

Negar e liberar perguntam onde gravar: `a` na aba (só esta sessão, guardada na lista de sessões), `p` no perfil do projeto ou `g` no global. Ligar e desligar gravam no projeto ou no global. A mudança vale na conexão seguinte da sessão. A troca de modo vale na aba; para entrar ou sair de `sem-rede`, use `sandmux net mode` e relance a sessão.

## Menu

Ctrl+b `:`, ou um clique no `≡` do começo da barra (estilos blocos e powerline), abre um menu com todas as ações e as teclas delas. As setas escolhem, Enter executa e Esc fecha. A tecla de uma ação, sem o prefixo, também a executa de dentro do menu.

## Configurações

Ctrl+b `e`, ou "configurações" no menu, abre uma página de tela cheia com as opções do [arquivo de configuração](config.md), em seções:

| Seção | Opções |
|---|---|
| Geral | idioma (`auto`, que segue o sistema, `pt` ou `en`; [config.md](config.md#idioma)); histórico em disco ([sessions.md](sessions.md#histórico-em-disco)); notificações do desktop ([veja abaixo](#notificações-do-desktop)) |
| Aparência | tema (com amostras de cor), estilo e posição da barra, painel lateral |
| Atalhos | tecla de prefixo |
| Sandbox | repo, KVM, GPU, janela, clipboard, memória, processos, pastas extras, `pass-env`, chaves SSH, bancos |
| Rede | modo, denylist, allowlist, catálogos, decifração, portas de entrada, portas do host |
| Arquivos | arquivos negados e liberados, arquivos de segredo |
| MCP | servidores negados e liberados |
| Assistente | modelos do chat e de comandos, modelos sugeridos (Enter, ou `a` na lista, abre a [lista dos provedores](chat.md#lista-de-modelos); `e` edita o texto) |
| Registro | dias e teto do registro de rede |
| Cofre | o estado e os segredos do cofre, sem os valores; criar, desbloquear, trancar, trocar a senha, acrescentar segredo e chave SSH, editar domínios e apagar ([vault.md](vault.md#pela-tui)) |

Cada item mostra o valor atual e de onde ele vem ("padrão", "do global"):

- toggles: `● ligado` ou `○ desligado`;
- escolhas: `‹ valor ›`;
- textos;
- listas.

**Teclas:**

| Tecla | Ação |
|---|---|
| ↑ ↓ | Movem nas seções ou nos itens |
| Tab | Troca entre as seções e os itens |
| ← → | Mudam uma escolha |
| Espaço ou Enter | Alterna um toggle |
| Enter | Edita um texto (Enter grava, vazio volta ao padrão) ou abre uma lista |
| `a` e `x` (na lista) | Acrescentam e tiram |
| `x` (num item) | Volta o item ao padrão |
| `g` | Troca o alvo entre o global e o projeto da pasta da aba ativa |
| Esc ou `q` | Fecha |

Um clique escolhe uma seção ou um item, e alterna um toggle.

**Gravação:** cada mudança é gravada na hora no `config.toml` pelo mesmo editor do `sandmux config set/add/remove`. Ele preserva os comentários e o resto do arquivo, e valida o arquivo inteiro antes de gravar. Uma mudança recusada mostra o motivo embaixo, e o arquivo não muda.

**Quando vale:**

- idioma, tema, barra, painel e prefixo: na hora;
- modelos do assistente: no próximo pedido;
- política das sessões: no próximo lançamento da aba (Ctrl+b `r`).

As opções de `[tui]`, `[assistant]` e do registro valem só no global; com o alvo num projeto, elas aparecem como "só global". As teclas de cada ação (`[tui.keys]`), as cores de cada elemento (`[tui.colors]`), os perfis e os provedores próprios do assistente ficam no arquivo.

## Tema de cores

A chave `theme` de `[tui]` escolhe um tema embutido:

- `padrao`: na barra, fundo escuro com o número da aba ativa em lilás e os blocos da direita em rosa, verde, amarelo e ciano; no estilo simples, as cores do terminal com vídeo inverso. É o padrão.
- `escuro`: fundo escuro com destaque azul.
- `claro`: fundo claro com destaque azul-escuro.
- `catppuccin`: Catppuccin Mocha, destaque mauve.
- `rose-pine`: Rosé Pine, destaque iris.
- `tokyo-night`: Tokyo Night (Night), destaque azul.
- `kanagawa`: Kanagawa (Wave), destaque crystal blue.
- `gruvbox`: Gruvbox escuro, destaque laranja.
- `dracula`: Dracula, destaque roxo.

Os temas pintam a barra, os modais, o painel e a página de configurações. Na página, passar pelos temas com ← → mostra cada um na hora e já grava a escolha.

Em `[tui.colors]`, cada elemento pode trocar de cor por cima do tema:

| Elemento | O que pinta |
|---|---|
| `tab-active-fg`, `tab-active-bg` | Aba ativa no estilo simples |
| `tab-fg`, `tab-bg` | Outras abas (nos blocos, a cor e o fundo do nome; no Powerline, `tab-fg` é a cor do nome) |
| `tab-index-fg`, `tab-index-bg` | Bloco do número das abas (blocos e Powerline) |
| `tab-active-index-fg`, `tab-active-index-bg` | Bloco do número da aba ativa e cor do nome dela (blocos e Powerline) |
| `chip-fg` | Texto dos blocos da direita e do `≡` (blocos e Powerline) |
| `chip-group-bg`, `chip-harness-bg`, `chip-folder-bg`, `chip-vault-bg`, `chip-alert-bg`, `chip-time-bg` | Blocos da direita: grupo, harness, pasta, cofre, alertas e tempo (blocos e Powerline) |
| `accent` | Destaque: o `≡` do menu, a borda e os títulos da página de configurações |
| `muted-fg` | Texto secundário da página de configurações |
| `ok-fg` | Toggle ligado e aviso de gravação |
| `page-bg`, `page-fg` | Fundo e texto da página de configurações |
| `surface-bg` | Seções e item escolhido na página de configurações |
| `bar-bg` | Fundo da barra |
| `status-fg` | Aviso à direita da barra |
| `note-fg`, `note-bg` | Faixa de aba encerrada, parada ou no histórico |
| `modal-border` | Borda dos modais |
| `modal-title` | Título dos modais |
| `selected-fg`, `selected-bg` | Item selecionado e campo ativo dos modais |
| `error-fg` | Erro nos modais |
| `selection-fg`, `selection-bg` | Texto selecionado com o mouse |

Uma cor é `"#rrggbb"`, um índice de 0 a 255, um dos 16 nomes ANSI (`black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white` e as versões `bright-`) ou `"default"`, a cor do terminal.

```toml
[tui]
theme = "escuro"

[tui.colors]
tab-active-index-bg = "#aa0000"
```

Um tema, elemento ou cor desconhecido torna o arquivo inválido. O tema vale ao abrir a TUI, ou na hora quando muda pela página de configurações.

## Nova aba

O modal pede três coisas:

- **harness:** com ← e →, entre `opencode`, `hermes`, `claude`, `omp`, `agy`, `shell` e `chat`. Com `chat`, Enter abre o [modal de chat](chat.md#abrir-uma-conversa), com o modelo e a mensagem, e a pasta e o perfil não valem;
- **pasta de trabalho:** por padrão, a pasta de onde a TUI foi aberta;
- **perfil nomeado:** opcional.

Tab troca de campo, e Enter cria. A aba passa pelas mesmas verificações e pela mesma resolução de camadas do `sandmux run`: um erro, como pasta perigosa, harness não instalado ou configuração inválida, aparece no modal sem criar sessão.

Na primeira execução de um harness com configuração para importar, o próprio modal pergunta se deve importar (s/n). Os valores de `pass-env` vêm do ambiente do terminal que abriu a TUI.

## Aba encerrada ou parada

- **Encerrada:** a aba continua na barra, com a última tela e o código de saída. `r` relança a sessão na mesma aba, e `x` fecha.
- **Pergunta ao terminar:** quando o harness termina com a TUI aberta (o hermes sai depois do assistente de configuração, por exemplo), um modal mostra o código e pergunta: Enter ou `r` reinicia, `x` fecha a aba, Esc deixa como está. Se a aba não estava ativa, a pergunta aparece quando você a ativa, uma vez. Abas que já estavam encerradas ao abrir a TUI não perguntam. Um clique no ✕ da aba na barra também fecha, sem confirmação.
- **Parada:** a aba mostra o motivo, por exemplo uma variável de `pass-env` que faltou na restauração.

## Mouse

- **Clique na barra:** troca de aba.
- **Harness que ligou o mouse** (opencode, por exemplo): os cliques, a roda e o arrasto vão para ele, com as linhas contadas a partir da área do terminal.
- **Harness sem mouse** (um shell, por exemplo):
  - a roda rola o histórico da aba, com um aviso na última linha;
  - arrastar seleciona texto, que ao soltar vai para o clipboard do seu terminal por OSC 52 e para o clipboard do sistema (Wayland ou X11), o que funciona também dentro do tmux. A cópia pela TUI funciona sempre, mesmo com a escrita no clipboard bloqueada para o harness.
- **Cópia do harness** (o opencode copia por OSC 52): bloqueada por padrão. Com `clipboard = true` na configuração, ou `sandmux run --allow-clipboard`, a TUI repassa a cópia ao seu terminal e ao clipboard do sistema.
- **Shift com arrasto:** fica com a seleção nativa do seu terminal, que funciona também quando o harness usa o mouse.

## Colar imagem

O prefixo seguido de `v` lê a imagem do clipboard do sistema (X11, ou Wayland com o protocolo de data-control) e a grava como `colado-<data>.png` na pasta de entrada. Depois escreve na aba ativa, sem Enter, o caminho dela no sandbox, dentro de `~/inbox`. Assim um screenshot chega ao agente sem sair da pasta de trabalho.

Sem imagem no clipboard, ou sem acesso a ele, a barra avisa e nada é escrito.

## Modos do harness

A entrada vai crua para a aba ativa: o harness recebe as teclas como num terminal direto. Os modos que ele ligou e que mudam o que o seu terminal envia ficam ligados no seu terminal enquanto a aba está ativa, e são desfeitos ao trocar de aba e ao sair:

- protocolo de teclado do kitty;
- eventos de foco;
- colagem entre marcadores;
- cursor e teclado de aplicação.

Com um modal aberto, a TUI desliga esses modos para entender as teclas.

Os eventos de foco são a exceção: a TUI os liga sempre, para as [notificações do desktop](#notificações-do-desktop). A aba só os recebe quando o harness dela pediu.

## Painéis

Uma aba pode ter vários painéis, cada um com a sua sessão, como as janelas divididas do tmux.

- **Dividir:** Ctrl+b `%` divide o painel ativo lado a lado, e Ctrl+b `"` em cima e embaixo. O [modal de nova aba](#nova-aba) abre com a pasta do painel ativo, e a sessão criada (um harness, o shell ou o [chat](chat.md)) ocupa a parte nova. Esc no modal cancela a divisão.
- **Foco:** Ctrl+b com as setas vai ao painel vizinho, e um clique num painel também põe o foco nele. O teclado vai só ao painel em foco, e as ações da aba (reiniciar, fechar, renomear, o painel lateral, as telas de rede, arquivos e MCP) valem para ele.
- **Ampliar:** Ctrl+b `z` mostra só o painel ativo na aba inteira; de novo, volta ao layout.
- **Tamanho:** Ctrl+b `<` e `>` diminuem e aumentam o painel ativo em 5%, entre 10% e 90% da divisão. Cada sessão recebe o tamanho do seu painel.
- **Fechar:** Ctrl+b `x` fecha o painel ativo (encerra a sessão dele), e o vizinho fica com o espaço. Com um painel só, a aba volta a ser comum.
- **Barra:** uma entrada por aba, com o nome do primeiro painel e `+N` para os outros. Os números de Ctrl+b `1`..`9`, `n` e `p` contam as abas.
- **Separadores:** `│` e `─` na cor secundária, e os do painel ativo na cor de destaque.

O layout fica no daemon, com as sessões. Ele volta ao reabrir a TUI, aparece igual em outra janela e sobrevive ao reinício do daemon e ao reboot ([sessions.md](sessions.md)). Levar uma aba para outro grupo (`m` na lista de grupos) leva todos os painéis dela.

## Grupos de abas

A TUI mostra só as abas do grupo atual ([sessions.md](sessions.md#grupos-de-abas)), e o nome do grupo aparece no primeiro bloco da direita (no estilo simples, no fim da barra).

- **Trocar:** Ctrl+b `(` e `)` vão ao grupo anterior e ao próximo, em ordem de nome. As abas do grupo que sai continuam vivas; a TUI só deixa de mostrá-las.
- **Lista de grupos:** Ctrl+b `G` abre a lista, com o número de abas de cada grupo e `●` no atual:
  - Enter troca para o grupo escolhido;
  - `n` pede um nome e troca para um grupo novo, vazio até a primeira aba (Ctrl+b `c`);
  - `r` renomeia o grupo escolhido;
  - `m` leva a aba ativa para o grupo escolhido;
  - Esc fecha.
- **Várias TUIs:** cada uma tem o seu grupo, então duas janelas podem mostrar grupos diferentes ao mesmo tempo.

## Notificações do desktop

O daemon avisa no desktop, pelo serviço de notificações do sistema (`org.freedesktop.Notifications`):

- **bloqueio de rede:** a aba, o domínio e o motivo;
- **chave real** no HOME sintético: movida ao cofre (reinicie a aba), esperando o cofre ou esperando um domínio ([vault.md](vault.md#movimento-automático));
- **aba encerrada:** o harness terminou sozinho, com o código. Um `sandmux kill` ou o fechamento pedido por você não avisa;
- **abas esperando o cofre** depois de um reboot, se ele continua trancado alguns segundos depois de as sessões voltarem.

Eventos do mesmo tipo na mesma aba, a menos de 60 segundos um do outro, atualizam a mesma notificação, com a contagem: "3 bloqueios: a.com, b.com, c.com".

Só sem foco: a TUI liga o aviso de foco do terminal e diz ao daemon quando ganha e perde o foco, e qual aba mostra. O daemon não avisa sobre a aba que uma TUI em foco está mostrando, porque o aviso já está na tela. Com a TUI fechada, numa janela de trás ou em outra aba, ele avisa.

- **Terminal sem aviso de foco:** a TUI fica sempre "em foco", e a aba ativa não é notificada. Konsole, GNOME Terminal, kitty, Alacritty e WezTerm mandam o aviso.
- **Modo anexado:** uma aba anexada fora da TUI (`sandmux run` ou `attach` num terminal) é notificada como se ninguém olhasse.

Vêm ligadas. Para desligar: `notify = false` em `[global]` ([config.md](config.md)), ou o item Notificações do desktop na seção Geral das [configurações](#configurações).

## Limites

- **Consultas de aba inativa:** uma consulta do harness ao terminal (atributos, cor de fundo) feita numa aba inativa só é respondida quando a aba volta a ficar ativa.
- **Seleção com mouse do harness:** com o mouse ligado pelo harness, a seleção da TUI não funciona; use Shift com arrasto.
- **Sem painéis divididos.** Só as abas e o painel lateral.
