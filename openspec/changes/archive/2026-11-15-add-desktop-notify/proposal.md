## Why

Item "Notificação do desktop" da lista depois da v1, escolhido pelo usuário (2026-11-15).

Hoje os avisos do sandmux só aparecem na TUI: a marca da aba, os alertas da barra e o painel. Com a TUI fechada ou numa janela de trás, um bloqueio de rede, uma chave real no sandbox ou uma aba que terminou passam despercebidos.

Decisões do usuário (2026-11-15):

- **Quando:** só sem foco. O daemon avisa, a não ser que uma TUI diga que está em foco e mostrando aquela aba. Com a TUI fechada ou sem foco, avisa sempre.
- **Eventos:**
  - bloqueio de rede;
  - chave real (movida ao cofre, ou esperando o cofre ou um domínio);
  - aba encerrada, e as abas esperando o cofre.

  Domínio novo fica de fora.
- **Padrão:** ligadas, com `[global] notify = false` para desligar.

## What Changes

- **Notificações pelo daemon:** o `sandmuxd` manda notificações ao desktop pelo serviço padrão (`org.freedesktop.Notifications`, no barramento de sessão):
  - **bloqueio de rede:** a aba, o domínio e o motivo;
  - **chave real:** movida ao cofre (reinicie a aba), esperando o cofre ou esperando um domínio;
  - **aba encerrada:** o harness terminou sozinho, com o código. Um `kill` ou fechamento pedido pelo usuário não avisa;
  - **abas esperando o cofre:** depois de um reboot, se o cofre continua trancado alguns segundos depois de as sessões voltarem.
- **Agrupamento:** eventos do mesmo tipo na mesma aba, a menos de 60 segundos um do outro, atualizam a mesma notificação, com a contagem, em vez de abrir outra.
- **Foco:** a TUI liga o aviso de foco do terminal (`CSI ?1004 h`) e diz ao daemon quando ganha e perde o foco, e qual aba mostra. O daemon não avisa sobre a aba que uma TUI em foco está mostrando. Os eventos de foco continuam indo para a aba quando o harness dela também pediu esse aviso.
- **Configuração:** `[global] notify` (ligado por padrão), também na seção Geral das configurações da TUI.
- **Protocolo 29:** a mensagem de foco.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `session-daemon`: as notificações do desktop, o agrupamento e a supressão pelo foco.
- `terminal-ui`: o aviso de foco do terminal mandado ao daemon e a chave `notify` nas configurações.

## Impact

- **`sandmux` (lib), daemon:**
  - módulo de notificações (thread com fila, agrupamento e envio pelo D-Bus);
  - seguidor do registro de rede para os bloqueios;
  - ganchos no fim de sessão, no vigia de chaves e na restauração;
  - foco por TUI (pelo PID do par do socket).
- **`sandmux-net`:** leitor que devolve só as linhas novas do registro.
- **`sandmux-policy`:** chave `notify` em `[global]`.
- **TUI:** o modo de foco, o evento de foco na entrada e a mensagem ao daemon; o item nas configurações.
- **`sandmux-proto`:** `Focus`; protocolo 29.
- **Dependência:** o `zbus` vira dependência direta. Ele já entra pelo keyring, então não acrescenta crates.
- **Testes:** com um barramento próprio do teste e o `dbus-monitor`, nunca o desktop de quem roda os testes. Pulados sem as ferramentas.
- **Documentação:** `docs/tui.md`, `docs/config.md`, `docs/sessions.md` e os pares em inglês.
