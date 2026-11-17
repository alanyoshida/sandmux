## Why

Para trocar uma API key que venceu, hoje só há dois caminhos.

- **`sandmux vault add` com o mesmo nome:** ele grava o valor novo, mas gera também um valor falso novo. Um arquivo do harness que guarda o falso antigo deixa de funcionar, como o `auth.json` do opencode com a chave importada ou movida pelo `doctor --fix`.
- **`vault remove` e depois `vault import`:** mantém o falso, mas só serve a chaves da importação, e o usuário precisa conhecer esse truque.

Pergunta do usuário (2026-11-16): como trocar uma API key que venceu.

## What Changes

- **`sandmux vault set <nome>`:** pede só o valor novo, sem eco, e troca o valor do segredo. O falso, os domínios e as ferramentas continuam os mesmos, então nenhum arquivo do harness precisa mudar.
  - Um nome que não existe é recusado, e a mensagem aponta o `sandmux vault add`.
  - Uma chave SSH ou uma conexão de banco também é recusada, com o comando certo para cada uma.
  - Decisão do usuário (2026-11-16): o comando novo, e o `vault add` continua como está.
- **TUI:** a seção Cofre das configurações ganha a tecla `v`, que troca o valor do segredo escolhido, com o campo mascarado como os outros. O rodapé mostra a tecla quando há um segredo de token escolhido.
- **Sem reiniciar a aba:** o gateway pede os segredos ao daemon a cada conexão, então o valor novo vale na próxima conexão ao domínio.
- **BREAKING (interno):** mensagem nova `VaultSetValue` entre o cliente e o daemon; protocolo 38. Depois de atualizar, `sandmux daemon restart`.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `secret-vault`: a troca do valor de um segredo, pela linha de comando e pela TUI, mantendo o falso e os domínios.

## Impact

- `crates/sandmux-proto`: `Msg::VaultSetValue`, protocolo 38.
- `crates/sandmux/src/daemon/vault.rs` e `conn.rs`: a troca no cofre.
- `crates/sandmux/src/vaultcmd.rs` e `main.rs`: o subcomando `set`.
- `crates/sandmux/src/tui/settings.rs` e `render.rs`: a tecla `v`, o formulário e o rodapé.
- `docs/vault.md`, `docs/sessions.md` (protocolo) e os pares em inglês.
