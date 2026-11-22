Os testes rodam no host, sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF é carregado: o auxiliar dos testes é um socket falso que só responde ao pedido `status`.

## 1. Versão do auxiliar

- [x] 1.1 Em `helper.rs`: a comparação de versões por número (`0.1.9` antes de `0.1.10`; versão ilegível não conta como antiga) e a consulta da versão do auxiliar em execução, que devolve a versão só quando ela é anterior à do sandmux. Verificar com testes de unidade da comparação e com o auxiliar de teste do módulo (mesma versão: nada; sem socket: nada)

## 2. Doctor

- [x] 2.1 Verificação nova no `sandmux doctor`, como aviso, com as duas versões e o `sandmux setup`, nos dois idiomas. Verificar com um teste em `tests/cli.rs` com um auxiliar falso: versão antiga mostra o aviso e mantém o código de saída; versão igual não mostra
- [x] 2.2 `docs/helper.md` e `docs/en/helper.md`: o auxiliar só troca ao reiniciar, o aviso do `doctor` e o `sandmux setup` depois de atualizar. Verificar com os testes de `docs.rs`

## 3. Seção Processos

- [x] 3.1 A seção confere a versão ao abrir e no `r`, e desenha o aviso acima da árvore ou do motivo. Verificar com um teste em `tests/tui.rs` com um auxiliar falso: versão antiga mostra o aviso; depois de o auxiliar falso passar a responder a versão atual, `r` tira o aviso
- [x] 3.2 `docs/tui.md` e `docs/en/tui.md`, na parte de Processos. Verificar com os testes de `docs.rs`

## 4. Verificação e fechamento

- [x] 4.1 Rodar o `./scripts/check.sh` inteiro no host e conferir que termina verde e sem processo do `target/` vivo
- [x] 4.2 Atualizar o roadmap com o change e o achado

## Workflow follow-up

- Commit e push num branch novo, PR e merge.
- Arquivar o change e sincronizar as specs depois do merge.
