## 1. Base

- [x] 1.1 `sandmux_net::log::Follow` (só as linhas novas, com a troca de dia), com teste unitário
- [x] 1.2 Chave `[global] notify` (padrão ligada) na configuração e no item da seção Geral; `Msg::Focus` e protocolo 29; `zbus` como dependência direta

## 2. Daemon

- [x] 2.1 Módulo de notificações: fila, leitura do registro de rede, agrupamento por (aba, tipo) com `replaces_id` e contagem, envio pelo D-Bus (`SANDMUX_NOTIFY_BUS` para os testes), foco por PID do par com descarte dos processos mortos, leitura de `notify` na hora
- [x] 2.2 Ganchos: bloqueios pelo registro, chaves reais no vigia, aba encerrada sem pedido na thread leitora, abas esperando o cofre 5 s depois da restauração

## 3. TUI, testes e documentação

- [x] 3.1 TUI: `?1004` ligado enquanto aberta, `Input::Focus` no analisador (repassado só à aba que pediu), mensagem `Focus` ao daemon ao ganhar ou perder o foco e ao trocar de aba
- [x] 3.2 Testes com barramento próprio e `dbus-monitor` (pulados sem as ferramentas): bloqueio notificado, agrupamento, `notify = false`, supressão com a TUI em foco, aba encerrada com e sem `kill`; testes do analisador para o foco
- [x] 3.3 Documentação: `docs/tui.md`, `docs/config.md`, protocolo em `docs/sessions.md` e os pares em inglês
