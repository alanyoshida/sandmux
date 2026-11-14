# Interface com abas

`sandmux` sem subcomando, ou `sandmux tui`, num terminal, abre a interface do sandmux. Cada sessão do daemon é uma aba: as mesmas abas aparecem em todo terminal que abre a TUI, e as sessões continuam quando ela fecha.

```
 1:shell:app  2:opencode:api ✕  3:hermes:notas                  imagem colada: ~/inbox/colado-…png
[sandmux] ana@host:~/projetos/app$ █
```

- **Primeira linha:** as abas, com a posição (o número dos atalhos), o nome e uma marca: ✕ para encerrada e ‖ para parada. À direita aparecem os avisos.
- **Resto da tela:** o terminal da aba ativa, com cores (inclusive cor verdadeira), atributos, cursor e tela alternativa. Trocar de aba mostra a tela da outra na hora.

Sem nenhuma sessão, a TUI abre o modal de nova aba. Sem terminal interativo (pipe, script), `sandmux tui` termina com erro.

## Atalhos

Todos começam pela tecla de prefixo, Ctrl+b por padrão:

| Tecla | Ação |
|---|---|
| `c` | Nova aba |
| `n` / `p` | Próxima / anterior |
| `1` a `9` | Ir para a aba nessa posição (`0` é a décima) |
| `w` | Lista de abas (setas, Enter, Esc) |
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

- **Ações:** `new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach`, `help`, `panel`, `network`, `files`, `folders`, `mcp`, `chat` e `command`.
- **Teclas:** um caractere visível, que não seja dígito, porque os dígitos levam às abas.
- **Duas ações na mesma tecla** tornam o arquivo inválido.
- **`[tui]` não é camada de política:** não muda nada nos sandboxes.

## Painel lateral

Com o terminal de pelo menos 100 colunas, um painel de 34 colunas fica à direita. A área do terminal das abas encolhe na mesma medida, e as sessões recebem o tamanho novo. O painel tem quatro seções sobre a aba ativa.

**Proteção** mostra o que foi aplicado no último lançamento da sessão:

- o nível de proteção, com a ABI do Landlock;
- a camada de escrita;
- a pasta de trabalho e as pastas extras, em leitura ou com escrita;
- GPU e KVM;
- a escrita no clipboard;
- as variáveis de `pass-env`, com o aviso de que ficam ao alcance do agente;
- os tetos de memória e de processos;
- a rede.

**Recursos** mostra, atualizado a cada segundo:

- a memória em uso, o pico e o teto;
- os processos;
- o CPU;
- as mortes por falta de memória;
- o Docker privado da pasta.

**Portas** mostra as portas publicadas no `127.0.0.1` do host: as que o agente escuta no sandbox e as dos contêineres do Docker privado da pasta, como `3000 → 127.0.0.1:3000 (sandbox)`.

**Rede** mostra o modo de rede da sessão e as últimas 6 conexões registradas da aba, com o domínio (ou "sem domínio"), os bytes enviados e recebidos, e o bloqueio. Um domínio visto pela primeira vez na pasta de trabalho aparece como "novo". Uma aba inativa ganha na barra `•` quando a sessão acessa um domínio novo e `!` quando tem um bloqueio, até ficar ativa. O registro completo está em [network.md](network.md).

Ctrl+b `s` esconde e mostra o painel. A chave `panel` de `[tui]` (`true` ou `false`) define como a TUI abre. Nas etapas seguintes, o painel ganha os MCPs.

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

## Tema de cores

A chave `theme` de `[tui]` escolhe um tema embutido:

- `padrao`: as cores do terminal, com vídeo inverso. É o padrão.
- `escuro`: fundo escuro com destaque azul.
- `claro`: fundo claro com destaque azul-escuro.

Em `[tui.colors]`, cada elemento pode trocar de cor por cima do tema:

| Elemento | O que pinta |
|---|---|
| `tab-active-fg`, `tab-active-bg` | Aba ativa |
| `tab-fg`, `tab-bg` | Outras abas |
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
tab-active-bg = "#aa0000"
```

Um tema, elemento ou cor desconhecido torna o arquivo inválido. O tema vale ao abrir a TUI.

## Nova aba

O modal pede três coisas:

- **harness:** com ← e →, entre `opencode`, `hermes` e `shell`;
- **pasta de trabalho:** por padrão, a pasta de onde a TUI foi aberta;
- **perfil nomeado:** opcional.

Tab troca de campo, e Enter cria. A aba passa pelas mesmas verificações e pela mesma resolução de camadas do `sandmux run`: um erro, como pasta perigosa, harness não instalado ou configuração inválida, aparece no modal sem criar sessão.

Na primeira execução de um harness com configuração para importar, o próprio modal pergunta se deve importar (s/n). Os valores de `pass-env` vêm do ambiente do terminal que abriu a TUI.

## Aba encerrada ou parada

- **Encerrada:** a aba continua na barra, com a última tela e o código de saída. `r` relança a sessão na mesma aba, e `x` fecha.
- **Parada:** a aba mostra o motivo, por exemplo uma variável de `pass-env` que faltou na restauração.

## Mouse

- **Clique na barra:** troca de aba.
- **Harness que ligou o mouse** (opencode, por exemplo): os cliques, a roda e o arrasto vão para ele, com as linhas contadas a partir da área do terminal.
- **Harness sem mouse** (um shell, por exemplo):
  - a roda rola o histórico da aba, com um aviso na última linha;
  - arrastar seleciona texto, que ao soltar vai para o clipboard do seu terminal por OSC 52. A cópia pela TUI funciona sempre, mesmo com a escrita no clipboard bloqueada para o harness.
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

## Limites

- **Consultas de aba inativa:** uma consulta do harness ao terminal (atributos, cor de fundo) feita numa aba inativa só é respondida quando a aba volta a ficar ativa.
- **Seleção com mouse do harness:** com o mouse ligado pelo harness, a seleção da TUI não funciona; use Shift com arrasto.
- **Sem painéis divididos.** Só as abas e o painel lateral.
