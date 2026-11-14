# Design

## Context

Veja proposal.md, seção Why. O que já existe e a TUI reaproveita:

- **Protocolo do daemon:** anexar (`Attach`) devolve `Attached`, o reenvio da tela e dos modos, `Replayed`, depois a saída crua (quadros de dados) e os eventos `Exited` e `Detached`. A entrada vai em quadros de dados, e o tamanho em `Resize`. Criar (`Create`) e reiniciar (`Restart`) aceitam `attach: false`.
- **Cliente do modo anexado:**
  - `client::Prefix` reconhece o Ctrl+b cru e na forma do kitty;
  - `ModeTracker` sabe os modos do kitty, foco, colagem e tela alternativa, e gera o reenvio e o desligamento;
  - `RawMode` põe o terminal em modo cru.
- **`run` com terminal:**
  - as verificações (preset, harness instalado, configuração efetiva, pergunta de importação) ficam em `main.rs`;
  - os valores de `pass-env` saem de `client_vars`.
- **`vt100` 0.18** já é dependência, com tela, cores, atributos, cursor, tela alternativa e histórico com `set_scrollback`.
- **A máquina de referência** roda KDE Plasma com Wayland e XWayland (`DISPLAY=:0`), sem `wl-paste` nem `xclip`.

## Goals / Non-Goals

**Goals:**

- A entrada do usuário chega à sessão ativa como num terminal direto, sem decodificar e recodificar teclas.
- Trocar de aba é instantâneo, porque cada aba já tem o emulador alimentado.
- Uma aba é uma sessão do daemon. A TUI não guarda estado próprio além da aba ativa.

**Non-Goals:**

- Painéis divididos (decisão do roadmap: só abas e o painel lateral, change 7).
- Temas (change 6) e painel lateral (change 7).
- Sessões nomeadas, com conjuntos de abas separados (pós-v1).

## Decisions

### 1. Estrutura e threads

O módulo `tui` fica no binário `sandmux`, ao lado de `main.rs`, para reaproveitar as verificações do `run` e a pergunta de importação. Há três tipos de thread, que mandam eventos ao laço principal por um canal:

- uma thread lê o `stdin` em bytes;
- uma thread por aba lê os quadros da conexão anexada daquela sessão;
- um temporizador manda um tique a cada segundo, para atualizar a lista de sessões com `List`. Assim aparecem as sessões criadas por outro cliente, os nomes novos e os estados.

O laço principal guarda as abas (emulador `vt100`, `ModeTracker`, conexão, nome, estado), a aba ativa e o modal aberto. Ele aplica os eventos e redesenha no máximo a cada 16 ms. O desenho usa o `ratatui` com o backend do `crossterm`, só para a saída. A entrada é lida crua, sem o `crossterm`.

### 2. Abas e conexões

- **Uma conexão anexada por aba**, com o tamanho da área do terminal: colunas do terminal e linhas menos uma, da barra. Ao redimensionar (SIGWINCH), todas as conexões recebem `Resize`.
- **Aba encerrada:** a conexão recebe o reenvio e `Exited` e fecha. A aba guarda o emulador com a última tela.
- **Reiniciar** abre uma conexão nova com `Restart { attach: true }`.
- **Criar** abre uma conexão nova com `Create { attach: true }`, depois das verificações do `run`.

### 3. Desenho da tela da aba

- Cada célula do `vt100` vira uma célula do `ratatui`: o texto, a cor (`Default`, `Idx(n)` e `Rgb`), o negrito, o itálico, o sublinhado e o inverso.
- O cursor é mostrado com `set_cursor_position` quando o `vt100` não o esconde e a aba está no fim do histórico.
- A rolagem do histórico usa `set_scrollback(n)` do `vt100`, e um indicador na barra mostra "histórico".

### 4. Entrada crua e tecla de prefixo

- **Prefixo:** o `Prefix` do cliente vira um reconhecedor configurável: letra do prefixo e tabela de ações. Ele reconhece o prefixo como byte de controle e como tecla do kitty. Depois do prefixo, a próxima tecla, em qualquer das duas formas, escolhe a ação.
- **Mouse:** o reconhecedor separa também as sequências de mouse SGR (`CSI < b ; x ; y M/m`), que a TUI trata (decisão 6). Todo o resto vai cru para a conexão da aba ativa.
- **Modal aberto:** a entrada vai para o modal. Ao abrir um modal, a TUI desliga no terminal do usuário o protocolo do kitty da aba (é a única forma de as teclas chegarem como sequências comuns, que o modal entende: setas, Enter, Esc, Backspace e texto UTF-8). Ao fechar, religa.

### 5. Modos da aba ativa no terminal do usuário

- **Modos para o terminal do usuário.** `host_modes` junta o `ModeTracker` (protocolo do kitty e eventos de foco) e o estado que o `vt100` já acompanha: colagem entre marcadores (2004), cursor e teclado de aplicação (`?1h` e `ESC =`), e o modo e a codificação do mouse. Não entram a tela alternativa (a TUI tem a dela) nem o mouse (a TUI liga o dela). `host_reset` desfaz o que `host_modes` ligou. O modo e a codificação do mouse servem para recodificar os eventos (decisão 6).
- **Troca de aba:** a TUI escreve o desligamento dos modos da aba anterior e o `replay_for_host` da nova.
- **Saída:** a TUI desliga tudo e restaura o terminal, como o modo anexado.

### 6. Mouse

- **Mouse do usuário:** a TUI liga 1002 (arrasto) e 1006 (SGR) no terminal do usuário.
- **Clique na linha 0:** acha a aba pela coluna, guardando as faixas de coluna de cada aba no desenho.
- **Área do terminal, com mouse ligado pelo harness:** o evento é recodificado com a linha menos um, no formato que o harness pediu (SGR se ligou 1006, senão o formato X10 com bytes), e vai para a sessão. Os movimentos sem botão só vão se o harness ligou 1003.
- **Área do terminal, sem mouse do harness:**
  - a roda move o histórico em 3 linhas;
  - arrastar com o botão esquerdo marca uma seleção, desenhada em inverso;
  - ao soltar, o texto das células selecionadas, linha a linha e sem os espaços do fim, vai para o terminal do usuário em OSC 52 (`ESC ] 52 ; c ; <base64> BEL`). O base64 é uma função própria, sem dependência.

### 7. Colar imagem

- **Leitura:** `arboard::Clipboard::new()?.get_image()` devolve RGBA, que o crate `png` grava em `<pasta de entrada>/colado-<AAAAMMDD-HHMMSS>.png` (modo 644).
- **Caminho na aba:** a aba ativa recebe o caminho absoluto do arquivo no sandbox (`<HOME>/inbox/<arquivo>`), sem Enter, para o usuário completar a mensagem.
- **Erros:** o `arboard` usa X11 e, com a feature `wayland-data-control`, Wayland. Sem imagem (`ContentNotAvailable`), a barra mostra "o clipboard não tem imagem". Sem acesso ao clipboard, mostra "não foi possível ler o clipboard: <erro>".

### 8. `[tui]` no `config.toml`

- **Formato:**
  - `prefix = "C-b"`: uma letra minúscula com `C-`;
  - `[tui.keys]`: ação e um caractere imprimível.
- **Leitura:** `Config` ganha `tui: TuiConfig`, lido e validado pelo mesmo código, com a linha nos erros. As ações são `new`, `next`, `prev`, `list`, `rename`, `restart`, `close`, `paste-image`, `detach` e `help`. Os dígitos e o próprio prefixo são reservados. Duas ações na mesma tecla tornam o arquivo inválido.
- **Escopo:** `[tui]` não é camada de política: `resolve` ignora a seção.
- **`sandmux config show`** não mostra `[tui]`, que é preferência de interface.

### 9. Renomear e protocolo 5

- **`Msg::Rename { id, name }`:** o daemon valida o nome (1 a 40 caracteres, sem controle), troca e grava a lista de sessões.
- **`sandmux rename`** e o modal usam o mesmo pedido.
- **`PROTOCOL`** passa a 5.

### 10. Testes

- **PTY:** a TUI roda num PTY dos testes (`common::pty::Term`, que responde às consultas de terminal). Os testes leem a tela pelo `vt100` do lado do teste:
  - barra de abas;
  - troca por teclas e por clique (sequências SGR escritas no PTY);
  - modais de nova aba, lista, renomear e fechar;
  - aba encerrada e reinício;
  - saída restaurando o terminal;
  - seleção com OSC 52 na saída crua.
- **Clipboard:** o teste da colagem sem clipboard roda sem `DISPLAY` nem `WAYLAND_DISPLAY`. A gravação do PNG tem teste de unidade.
- **Fidelidade com o opencode e o hermes reais:** é a verificação da etapa 3, feita à mão no fim.

## Risks / Trade-offs

- **[Consultas ao terminal]** O harness consulta o terminal (DA1, cor de fundo). Com a TUI no meio, a resposta vem do terminal do usuário, porque a entrada vai crua. Sai certo, a não ser que a aba não esteja ativa: a resposta não chega até o usuário voltar à aba. → É o mesmo limite das sessões restauradas sem cliente. Registrado.
- **[Mouse do harness e seleção]** Com o mouse ligado pelo harness, a seleção da TUI não funciona. → O Shift com arrasto do terminal do usuário seleciona o que a TUI desenhou.
- **[`arboard` sem Wayland data-control]** Alguns compositores não oferecem o protocolo de data-control. → No KDE e no wlroots ele existe, e há o recuo para X11 pelo XWayland. Sem os dois, a barra explica.
- **[Muitas abas]** Uma conexão e uma thread por aba. → Dezenas de abas cabem com folga. O limite é o do daemon.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart`, que o protocolo 5 exige. O `config.toml` antigo continua válido.
