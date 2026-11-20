Os testes rodam no host e na VM do Ubuntu 24.04 (`scripts/vm-ubuntu-2404.sh`), sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF é carregado.

## 1. Testes de publicação de portas

- [x] 1.1 `porta_do_sandbox_publicada_no_host`: a espera de 10 s, a mensagem com a lista e a resposta, e o servidor parado pelo arquivo de parada (não por `sleep 4`). Verificar rodando o teste no host
- [x] 1.2 `allowlist_de_portas_endereco_e_registro`: o mesmo (espera de 10 s, diagnóstico e arquivo de parada no lugar do `sleep 8`). Verificar rodando o teste no host

## 2. Verificação e fechamento

- [x] 2.1 Rodar a suíte de rede na VM logo depois de um boot (como nas falhas) e numa carga de CPU, e o `./scripts/check.sh` inteiro no host
- [x] 2.2 Atualizar o roadmap: o achado do change 74 fecha como não reproduzido, com os prazos corrigidos

## Workflow follow-up

- Commit e push num branch novo, PR e merge, sem release nova (decisão do usuário, 2026-11-20).
- Arquivar o change depois do merge.
