Os testes rodam no host, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF é carregado. Depois de cada rodada, conferir os processos cujo executável está no `target/` do worktree.

## 1. Apoio dos testes

- [x] 1.1 Teste `terminal_largado_mata_o_cliente_e_o_daemon_nao_volta` em `tests/attach.rs`: abre a TUI sobre uma sessão viva, larga o terminal, confere que o cliente morreu, para o daemon e confere que ele não voltou em 3 s. Verificar que o teste falha antes da correção
- [x] 1.2 `Drop` do `Term` em `tests/common/pty.rs`: mata o comando ainda vivo, espera por ele e fecha o mestre do PTY; `child` passa a `Option`, com `pid()` e `close()` ajustados. Verificar que o teste de 1.1 passa e que `cargo test -p sandmux --test notify` não deixa processo do `target/` vivo

## 2. Verificação e fechamento

- [x] 2.1 Rodar o `./scripts/check.sh` inteiro no host e conferir que termina verde e sem processo do `target/` vivo
- [x] 2.2 Atualizar o roadmap com o change e o achado

## Workflow follow-up

- Commit e push num branch novo, PR e merge.
- Arquivar o change depois do merge.
