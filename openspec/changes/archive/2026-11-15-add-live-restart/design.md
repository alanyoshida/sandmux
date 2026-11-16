## Context

`relaunch` no daemon recusa uma sessão viva. O `kill` encerra com TERM, espera 5 s, manda KILL e remove a sessão (`killing`). No fim de um processo, o daemon marca a sessão como encerrada, avisa os clientes (`Exited`), manda a notificação do desktop e, no gateway do hermes, agenda o relançamento. Na TUI, os quadros de uma conexão antiga são descartados pelo identificador da conexão, e o reinício troca a conexão da aba (`bind`).

## Decisions

- **`force` no `Restart` (protocolo 35):** com a sessão viva e `force`, o daemon marca `restarting`, manda TERM ao processo, espera até 5 s pelo fim e manda KILL, esperando mais 5 s; depois segue o relançamento comum. Sem `force`, o erro de hoje, com a menção a `--force`.
- **Fim de um reinício:** com `restarting`, o fim do processo marca a sessão como encerrada e desanexa os clientes com "reiniciando", sem a notificação e sem o relançamento do gateway; a marca é limpa no fim.
- **TUI:** a ação `restart` numa aba viva abre um modal de confirmação (`s/n`), como o de fechar. O sim chama o mesmo caminho de hoje, com `force`.
- **Aviso:** os itens de camada da tela de configurações (seções Sandbox, Rede, Arquivos, MCP) valem no lançamento. Depois de gravar um deles, a página acrescenta "N abas vivas continuam com o valor antigo; Ctrl+b r reinicia a aba", contando as abas vivas da TUI. No `config set` e afins, a CLI consulta o daemon só se ele já está rodando (sem subir um) e mostra o aviso com `sandmux restart <id> --force`.

## Risks / Trade-offs

- [O Docker privado da pasta para quando a última sessão viva dela termina] → No reinício, ele para e volta, como num relançamento comum.
- [Um harness que demora para sair com TERM] → KILL depois de 5 s, como o `kill`.
