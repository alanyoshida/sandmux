## Why

Depois de uma rodada inteira do `./scripts/check.sh` fica vivo um `sandmuxd` de teste, com a árvore de `sandmux-run` de uma sessão e o shell dela. O usuário notou processos sobrando na máquina (2026-11-21) e perguntou se os testes removiam os processos antigos. Três daemons assim, de 2026-11-20 e 2026-11-21, estavam vivos no checkout principal, de rodadas de outras sessões.

A causa, achada rodando os arquivos de teste um a um e listando os processos do `target/` depois de cada um:

- O teste `tui_em_foco_na_aba_suprime` (`tests/notify.rs`) termina com a TUI ainda aberta. É o único que faz isso.
- O `Term` dos testes não mata o comando ao ser largado, e a thread de leitura segura o mestre do PTY. O cliente dura até o processo de teste sair.
- Quando o `Env` é largado, ele roda `sandmux daemon stop`. A TUI ainda viva perde a conexão, e o pedido seguinte dela usa `connect_or_start`, que sobe o daemon de novo. O daemon novo restaura a sessão viva.
- O processo de teste sai, a TUI morre com o PTY, e o daemon novo fica órfão. O `Env` já tinha terminado, e o PID que ele conhecia era o do daemon antigo.

## What Changes

- **`Term` dos testes:** largar o terminal mata o comando que ainda roda nele e fecha o mestre do PTY. Um cliente não sobrevive mais ao teste que o abriu, qualquer que seja o teste.
- **Teste novo:** uma TUI aberta e largada morre, e o daemon parado depois disso não volta.
- Nenhuma mudança no programa nem nas specs. Uma TUI aberta continua subindo o daemon quando ele some, que é o comportamento esperado fora dos testes.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

(nenhuma: o change só muda o apoio dos testes)

## Impact

- `crates/sandmux/tests/common/pty.rs`: `Drop` do `Term`; o campo `child` deixa de ser público e ganha o método `pid`.
- `crates/sandmux/tests/attach.rs`: o teste novo e o uso de `pid`.
- `openspec/roadmap.md`: o registro do change.
