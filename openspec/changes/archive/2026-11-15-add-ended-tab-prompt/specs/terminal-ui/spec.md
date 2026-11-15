## MODIFIED Requirements

### Requirement: Aba encerrada ou parada
Uma aba cuja sessão está encerrada MUST continuar aberta, com a última tela, o código de saída e as teclas de reiniciar (`r`) e fechar (`x`). Uma aba parada MUST mostrar o motivo. Reiniciar MUST relançar a sessão como `sandmux restart`, com as variáveis de `pass-env` do ambiente da TUI. Quando o harness de uma aba termina com a TUI aberta, a TUI MUST abrir um modal com o código de saída, na hora se a aba está ativa ou ao ativá-la, uma vez: Enter ou `r` reinicia, `x` fecha a aba e Esc mantém a aba encerrada. Abas que já estavam encerradas ao abrir a TUI MUST NOT abrir o modal.

#### Scenario: Shell encerrado
- **WHEN** o usuário digita `exit 3` no shell da aba ativa
- **THEN** a aba continua na barra, marcada como encerrada, e a área mostra o código 3 e as teclas de reiniciar e fechar

#### Scenario: Reiniciar
- **WHEN** a aba ativa está encerrada e o usuário digita Ctrl+b r
- **THEN** a sessão volta viva na mesma aba

#### Scenario: Modal ao terminar
- **WHEN** o hermes da aba ativa termina o assistente de configuração e sai com código 0
- **THEN** a TUI abre o modal com o código 0, e Enter relança o hermes na mesma aba

#### Scenario: Aba em segundo plano
- **WHEN** o shell da aba 2 sai enquanto a aba 1 está ativa, e o usuário ativa a aba 2
- **THEN** o modal aparece ao ativar a aba 2, e Esc a deixa encerrada sem perguntar de novo
