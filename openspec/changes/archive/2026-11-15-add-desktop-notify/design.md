## Context

- **Onde os eventos acontecem:**
  - os bloqueios são gravados pelos gateways, processos à parte, nos arquivos diários do registro de rede (`<estado>/net/AAAA-MM-DD.jsonl`), que o daemon lê pelo `Tail`;
  - as chaves reais são do vigia do daemon (change 46);
  - o fim de uma aba é visto pela thread leitora da sessão;
  - a espera do cofre aparece na restauração.
- **A TUI** lê a entrada crua e repassa os modos que o harness pediu (`ModeTracker`, inclusive o foco, `?1004`).
- **O zbus** já está no build pelo crate `keyring`.

## Goals / Non-Goals

**Goals:**
- Avisar no desktop o que importa quando ninguém está olhando para a aba.
- Não incomodar quem está olhando, nem com rajadas.

**Non-Goals:**
- Domínio novo (decisão do usuário).
- Ação na notificação (clicar para abrir a aba).
- Foco no modo anexado (`sandmux run` e `attach` num terminal). Ali a entrada vai crua ao harness, e ligar o aviso de foco mudaria o que ele recebe. Uma aba anexada fora da TUI é avisada como se ninguém olhasse.

## Decisions

- **No daemon,** porque é o único que vê tudo, inclusive com a TUI fechada. Uma thread `notificacoes` recebe os eventos por um canal e, a cada segundo, lê as linhas novas do registro de rede.
  - O leitor novo, `sandmux_net::log::Follow`, começa no fim do arquivo do dia, devolve, em `read_new`, só as linhas completas novas e recomeça no arquivo do dia seguinte.
  - O dono de uma linha é achado pela tag, como em `net::lines`.
- **Envio pelo D-Bus com o zbus,** como dependência direta, com as mesmas features do keyring (tokio), sem crates novos. A chamada roda num runtime tokio de uma thread só, criado na thread de notificações.
  - Alternativa: o `notify-send`. Recusada: nem toda máquina o tem.
  - O endereço vem de `DBUS_SESSION_BUS_ADDRESS`, ou de `$XDG_RUNTIME_DIR/bus`.
  - `SANDMUX_NOTIFY_BUS` troca o endereço, para os testes. Os testes comuns apontam para um endereço inexistente, para nunca notificar o desktop de quem roda os testes.
- **Agrupamento:** a chave é (aba, tipo). A notificação guarda o id devolvido pelo serviço, e um evento a menos de 60 segundos do anterior manda `replaces_id` com esse id e a contagem. A janela recomeça a cada evento.
  - O texto lista até três domínios ou chaves, com "e mais N".
  - As chaves reais agrupam por harness, não por aba, porque o HOME sintético é do harness.
- **Foco:**
  - **TUI:** ela liga `?1004` ao abrir, depois de cada troca de modos (o `host_reset` desliga) e desliga ao sair. O analisador da entrada transforma `CSI I`/`CSI O` em `Input::Focus`. A TUI repassa os bytes à aba só se o `ModeTracker` dela tem o foco ligado.
  - **Mensagem:** a TUI manda `Focus { focused, active }` ao ganhar ou perder o foco e ao trocar de aba. Ela começa como em foco, porque acabou de abrir na frente do usuário.
  - **Daemon:** guarda por PID do par do socket (`SO_PEERCRED`), sem confiar num PID mandado pelo cliente. Ao decidir, descarta as entradas de processos que não existem mais. Suprime o evento de uma aba se alguma TUI está em foco nela.
  - Para as chaves reais: suprime se uma TUI em foco mostra uma das abas do harness.
- **Aba encerrada:** só quando a sessão não estava sendo encerrada a pedido (`killing`). Com `Encerrada { codigo }`, o texto diz o código.
- **Abas esperando o cofre:** a restauração dispara uma checagem 5 segundos depois. Assim, o desbloqueio pelo keyring (change 47) tem tempo de abrir o cofre, e a notificação só sai se ainda há abas esperando.
- **`[global] notify`:** chave booleana, padrão ligada. O daemon lê o arquivo de configuração na hora de mandar, que é um evento raro e agrupado, e não guarda o valor em memória. Assim, mudar a chave vale sem reiniciar.
- **Protocolo 29.**

## Risks / Trade-offs

- [Terminal sem aviso de foco] → A TUI nunca recebe a perda de foco e fica "em foco" na aba ativa: a aba ativa não é notificada, e as outras são. É o comportamento seguro para quem olha a TUI. Konsole, GNOME Terminal, kitty, Alacritty e WezTerm mandam o aviso.
- [Uma rajada de bloqueios] → O agrupamento mantém uma notificação por aba.
- [Desktop sem servidor de notificações] → O envio falha em silêncio, e a falha fica no registro do daemon uma vez.
