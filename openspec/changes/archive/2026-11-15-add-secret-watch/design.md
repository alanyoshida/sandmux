## Context

O `sandmux doctor --fix` (change 33) já sabe:

- achar as chaves reais nos HOMEs sintéticos do opencode e do hermes (`sandmux_import::find_real_secrets`);
- inferir o domínio de cada uma (`infer_domains_for`, hoje na CLI);
- guardar no cofre e regravar o arquivo com os falsos (`apply_fakes`).

Ele roda na CLI, com perguntas no terminal. O daemon tem o cofre aberto na memória e já usa inotify para os cgroups.

## Goals / Non-Goals

**Goals:**
- Tirar a chave real do arquivo em segundos, sem ação do usuário, quando o cofre está aberto e o domínio é conhecido.
- Não perder nada: um caso que não dá para resolver sozinho fica visível e resolvível depois.

**Non-Goals:**
- Tirar a chave da memória do harness. Ele continua com a real até a aba ser reiniciada, e o aviso pede o reinício. A aba não é reiniciada sozinha, para não matar o trabalho do agente.
- Outros harnesses. O Claude Code já tem a captura no proxy (change 43). O oh-my-pi guarda as chaves num banco SQLite, que a varredura não lê. A lista é a mesma do `doctor`.
- Uma chave para desligar o vigia. Fica para depois, se fizer falta.

## Decisions

- **No daemon, não num processo à parte.** O daemon tem o cofre aberto e sabe quais abas são de qual harness. Uma thread própria (`chaves-reais`) faz o trabalho.
- **inotify nas pastas, não nos arquivos.** Os harnesses gravam por troca de arquivo (um temporário seguido de rename), que um inotify no arquivo perderia. A thread observa as pastas dos arquivos da tabela com `IN_CLOSE_WRITE | IN_MOVED_TO | IN_CREATE`. Depois de um evento, ela espera 500 ms de silêncio antes de varrer, para não pegar um arquivo pela metade.
- **A varredura periódica, de 60 segundos,** refaz a lista de pastas observadas, porque uma pasta pode não existir quando o daemon sobe e ser criada depois pelo harness. Ela também pega um evento perdido (fila do inotify cheia). `SANDMUX_SECRET_SCAN_SECS` muda o intervalo, para os testes.
- **O cofre aberto acorda a thread** por um canal, para mover na hora o que esperava o cofre.
- **Conferência antes de regravar:** o arquivo é relido logo antes de `apply_fakes`. Se mudou desde a leitura, a varredura é desfeita e o próximo evento varre de novo. A nossa própria gravação gera um evento, mas a varredura seguinte só acha falsos e não faz nada.
- **Nome do segredo, domínios e falso** iguais aos do `doctor --fix` (`<harness>:<arquivo>:<provedor>.<chave>`). A inferência passa para a biblioteca (`sandmux::secretdomains`), e a CLI a reusa.
- **Avisos pela lista de sessões:** `SessionInfo.keys` tem três contadores:
  - `moved`: as chaves movidas desde o lançamento daquela aba. Zera no lançamento e só conta nas abas vivas no momento do movimento.
  - `waiting_vault` e `no_domain`: as chaves esperando, por harness, preenchidas no `list`.

  A TUI já pede a lista periodicamente, então não precisa de mensagem nova. O protocolo vai a 27 pelo campo novo.
- **Registro do daemon:** uma linha por arquivo movido ou esperando, com o harness, o arquivo e as chaves (nomes), nunca os valores.

## Risks / Trade-offs

- [O harness regrava o arquivo entre a releitura e a nossa gravação] → A janela é de milissegundos. Se acontecer, a gravação do harness se perde, e a chave dele, se era real, já está no cofre. A releitura reduz a janela, mas não a fecha.
- [O harness regrava a chave real a partir da memória] → Ela é movida de novo, com outro falso. O cofre guarda a última, e o manifesto da importação ganha uma entrada por vez.
- [Um falso positivo da varredura] → As regras são as da importação, que já tratam esses arquivos. O `doctor` mostra o que foi movido, e `sandmux vault list` mostra os segredos com o nome do arquivo.
