Os testes rodam no host, sob o vigia de memória, e os com root na VM do Ubuntu 24.04, que o usuário pediu (2026-11-21). Nenhum programa BPF é carregado no host.

## 1. Lançador

- [x] 1.1 No `retry` de `sandmux-launcher/src/scope.rs`, o prazo curto conta da primeira falta seguida do socket, e qualquer outra resposta zera a conta. Verificar com o caso novo da troca no teste do `retry`, que falha com o código da 0.1.35 e passa com a correção

## 2. Verificação e fechamento

- [x] 2.1 Rodar o `./scripts/check.sh` inteiro e conferir que termina verde e sem processo de teste vivo
- [x] 2.2 Rodar os testes com root na VM, no 6.8 e no 7.0, com a correção
- [x] 2.3 Atualizar o roadmap com o change, o resultado da VM e o caso aberto do change 79

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova (a ordem do usuário para cada change).
