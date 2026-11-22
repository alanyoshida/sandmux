Os testes rodam no host, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF é carregado: o teste usa um socket que não existe.

## 1. Lançador

- [x] 1.1 Em `sandmux-launcher/src/scope.rs`: o `Fail::Missing` para o socket que não existe (`ENOENT`), o prazo curto de 1 s no registro do lançamento e o prazo de 30 s mantido para a troca e para a vigia do escopo. Verificar com o teste de unidade do `retry` (desiste no prazo curto sem o socket; o socket que volta passa; o auxiliar que passa a não responder segue até o prazo longo)
- [x] 1.2 `kernel_antigo_sem_o_auxiliar_recusa_sem_executar` confere que a recusa sai em menos de 10 s. Verificar rodando o teste (de 30 s para 1,2 s)

## 2. Documentação e fechamento

- [x] 2.1 `docs/helper.md` e `docs/en/helper.md`: o lançamento sem o auxiliar desiste em 1 s. Verificar com os testes de `docs.rs`
- [x] 2.2 Rodar o `./scripts/check.sh` inteiro e conferir que termina verde e sem processo de teste vivo
- [x] 2.3 Atualizar o roadmap com o change

## Workflow follow-up

- Commit e push num branch novo, PR e merge; arquivar o change e sincronizar a spec no mesmo PR; release nova (a ordem do usuário para cada change).
