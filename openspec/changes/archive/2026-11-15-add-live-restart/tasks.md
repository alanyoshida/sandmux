## 1. Reinício de aba viva

- [x] 1.1 Protocolo 35 com `force` no `Restart`; no daemon, encerrar a sessão marcada como reiniciando, sem notificação nem relançamento do gateway, e relançar
- [x] 1.2 CLI: `sandmux restart <id> --force`
- [x] 1.3 TUI: Ctrl+b `r` numa aba viva abre a confirmação; o sim reinicia com `force`

## 2. Aviso na configuração

- [x] 2.1 Tela de configurações: o número de abas vivas com o valor antigo e Ctrl+b `r`
- [x] 2.2 `config set`, `unset`, `add` e `remove`: o aviso quando o daemon tem sessões vivas

- [x] 2.3 Painel lateral e `sandmux stats`: a janela (ligada, direta ou desligada) na proteção da aba

## 3. Testes e documentação

- [x] 3.1 Testes: `restart --force` numa sessão viva (sem notificação), Ctrl+b `r` com `s` e `n`, o aviso da página e da CLI
- [x] 3.2 Documentação: `docs/sessions.md`, `docs/tui.md`, `docs/config.md` e os pares em inglês
