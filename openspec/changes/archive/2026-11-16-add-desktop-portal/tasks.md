## 1. Opção e protocolo

- [x] 1.1 `sandmux-policy`: a chave `portal` (padrão ligado) em `BOOL_KEYS`, com a precedência das outras; `sandmux-proto`: `portal` nas opções da sessão e protocolo 37 (`docs/sessions.md` e o par em inglês). Verificação: testes de unidade da configuração e o teste dos docs
- [x] 1.2 `sandmux run`: `--portal` e `--no-portal`; sem terminal, segue sem portal e recusa o `--portal` explícito. Verificação: teste de integração da recusa

## 2. Barramento e serviços

- [x] 2.1 `portal.rs` (na biblioteca, ao lado de `window.rs`) e o repassador do processo 1 (`sandmux-launcher/src/relay.rs`), que cria o `bus` dentro do sandbox: o socket por aba e o servidor zbus ponto a ponto por conexão (recurso `p2p`), com o `org.freedesktop.DBus` mínimo e erro para o resto. Verificação: o teste de integração com clientes reais (o `gdbus` e o `notify-send` fazem o `Hello`; um serviço fora da lista recebe erro) e testes de unidade da URL e dos cortes
- [x] 2.2 Aparência: `Read`, `ReadAll` e `ReadOne` de `org.freedesktop.appearance`, lidos do portal do host com cache, e "sem preferência" sem ele. Verificação: teste de integração com um portal falso no barramento de teste
- [x] 2.3 Notificações: `Notify`, `CloseNotification`, `GetCapabilities` e `GetServerInformation`, e o `AddNotification` e o `RemoveNotification` do portal, repassados com o nome da aba, cortes e o limite por aba. Verificação: teste de integração com o servidor de notificações falso (`notify-send` e `gdbus` de dentro do sandbox)
- [x] 2.4 Abrir link: `OpenURI` com `http` e `https`, decidido pela política de rede da aba, aberto pelo portal do host ou pelo `xdg-open`, no registro de rede, com o `Response`; `OpenFile` e `OpenDirectory` recusados. Verificação: teste de integração (permitido, fora da política, `xdg-open` de dentro) com um abridor falso

## 3. Sandbox e TUI

- [x] 3.1 Composição: o `XDG_RUNTIME_DIR` em memória com o `bus` e o `flatpak-info`, as variáveis, e a mesma pasta da janela. Verificação: testes de unidade da composição e de integração (com e sem `--no-portal`)
- [x] 3.2 TUI: o item "Portal" na seção Sandbox e o estado no painel de proteção. Verificação: teste da página e do painel

## 4. Documentação e suíte

- [x] 4.1 `docs/window.md` (o portal, os limites), `docs/config.md` (a chave), `README.md`, e os pares em inglês; roadmap. Verificação: o teste dos docs
- [ ] 4.2 Suíte completa (`scripts/check.sh`) sob o vigia, com `CARGO_BUILD_JOBS=8`
