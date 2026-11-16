## 1. Opção e protocolo

- [ ] 1.1 `sandmux-policy`: a chave `portal` (padrão ligado) em `BOOL_KEYS`, com a precedência das outras; `sandmux-proto`: `portal` nas opções da sessão e protocolo 37 (`docs/sessions.md` e o par em inglês). Verificação: testes de unidade da configuração e o teste dos docs
- [ ] 1.2 `sandmux run`: `--portal` e `--no-portal`; sem terminal, segue sem portal e recusa o `--portal` explícito. Verificação: teste de integração da recusa

## 2. Barramento e serviços

- [ ] 2.1 `daemon/portal.rs`: o socket por aba e o servidor zbus ponto a ponto por conexão (recurso `p2p`), com o `org.freedesktop.DBus` mínimo e erro para o resto. Verificação: testes de unidade com um cliente zbus (Hello, GetNameOwner, interface desconhecida)
- [ ] 2.2 Aparência: `Read`, `ReadAll` e `ReadOne` de `org.freedesktop.appearance`, lidos do portal do host com cache, e "sem preferência" sem ele. Verificação: teste de integração com um portal falso no barramento de teste
- [ ] 2.3 Notificações: `Notify`, `CloseNotification`, `GetCapabilities` e `GetServerInformation`, e o `AddNotification` e o `RemoveNotification` do portal, repassados com o nome da aba, cortes e o limite por aba. Verificação: teste de integração com o servidor de notificações falso (`notify-send` e `gdbus` de dentro do sandbox)
- [ ] 2.4 Abrir link: `OpenURI` com `http` e `https`, decidido pela política de rede da aba, aberto pelo portal do host ou pelo `xdg-open`, no registro de rede, com o `Response`; `OpenFile` e `OpenDirectory` recusados. Verificação: teste de integração (permitido, fora da política, `xdg-open` de dentro) com um abridor falso

## 3. Sandbox e TUI

- [ ] 3.1 Composição: o `XDG_RUNTIME_DIR` em memória com o `bus` e o `flatpak-info`, as variáveis, e a mesma pasta da janela. Verificação: testes de unidade da composição e de integração (com e sem `--no-portal`)
- [ ] 3.2 TUI: o item "Portal" na seção Sandbox e o estado no painel de proteção. Verificação: teste da página e do painel

## 4. Documentação e suíte

- [ ] 4.1 `docs/window.md` (o portal, os limites), `docs/config.md` (a chave), `README.md`, e os pares em inglês; roadmap. Verificação: o teste dos docs
- [ ] 4.2 Suíte completa (`scripts/check.sh`) sob o vigia, com `CARGO_BUILD_JOBS=8`
