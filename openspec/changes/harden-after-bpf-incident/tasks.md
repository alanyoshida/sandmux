Os programas BPF só rodam na VM (`scripts/vm-ubuntu-2404.sh`). Os testes pesados rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`.

## 1. Lançador

- [ ] 1.1 `open_path` com `O_PATH | O_CLOEXEC` no lugar do `PathFd` em `landlock_rules::apply`. Verificar com um teste unitário (um FIFO e uma pasta sem leitura, num processo filho), na glibc e no musl, e com a suíte de fuga

## 2. Teste de TUI

- [ ] 2.1 `tui_size` e o teste do painel de rede com 50 linhas. Verificar rodando o teste 25 vezes seguidas

## 3. Testes com root na VM

- [ ] 3.1 Os passos `docker` e `check-root` em `scripts/vm-ubuntu-2404.sh`. Verificar rodando o `check-root` inteiro na VM, com o teste do escopo
- [ ] 3.2 O aviso e a confirmação no `scripts/check-root.sh`, com `SANDMUX_CHECK_ROOT_HOST=1`. Verificar rodando o script sem `sudo` (ele avisa antes de pedir root)

## 4. Documentação e fechamento

- [ ] 4.1 `docs/helper.md` (os testes com root na VM) e `docs/install.md` (o teste nas distros), com os pares em inglês e os testes de `docs.rs`
- [ ] 4.2 Rodar o `./scripts/check.sh` inteiro no host e o `check-root` na VM

## Workflow follow-up

- Arquivar, PR, merge e release nova (ordem do usuário, 2026-11-20).
