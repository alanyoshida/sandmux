Os testes rodam no host, sob o vigia de memória, e os com root na VM do Ubuntu 24.04. Nenhum programa BPF é carregado no host.

## 1. Lançador

- [x] 1.1 Em `sandmux-launcher/src/scope.rs`, o `status` ganha uma forma com o caminho do socket, e um teste com um servidor `SOCK_SEQPACKET` que fecha as duas primeiras conexões sem responder e responde à terceira. Conferir que o teste falha com o código de antes
- [x] 1.2 O `status` passa pelo `retry`, com os prazos do registro, e o envio recusado vira "sem resposta". Conferir que o teste novo e o `tenta_de_novo_so_sem_resposta` passam

## 2. Verificação e fechamento

- [x] 2.1 Rodar o `./scripts/check.sh` inteiro e conferir que termina verde e sem processo de teste vivo
- [x] 2.2 Rodar os testes com root na VM, no 6.8 (várias rodadas) e no 7.0, com a correção
- [x] 2.3 Atualizar o roadmap com o change e corrigir o caso aberto do change 79

## Workflow follow-up

- Commit e push num branch novo, PR e merge, com o change arquivado no mesmo PR; release nova (a ordem do usuário para cada change).
