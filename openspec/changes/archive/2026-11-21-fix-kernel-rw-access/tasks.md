## 1. Programas BPF e eventos

- [x] 1.1 `prog.rs`: o evento de abertura e a chave do mapa das repetições com `f_mode & 3`; o `forget_opened` apaga as chaves de 0 a 3; testes unitários da montagem com o `and` de 3 na abertura.
- [x] 1.2 `prog.rs`: o bloqueio do `file_open` grava o modo lido do arquivo; os outros bloqueios gravam `FMODE_WRITE` ou 0; teste unitário da montagem.
- [x] 1.3 `ring.rs`: `Event` com `read` e `write` pelos bits do campo; testes do `decode` com 1, 2 e 3.

## 2. Auxiliar e daemon

- [x] 2.1 `helper.rs`: o campo `read` no JSON da abertura e do bloqueio; o teste do `event_json` atualizado.
- [x] 2.2 `dockerfiles.rs`: `r`, `w` ou `rw` na abertura e no bloqueio do `file_open`, com o `read` ausente tratado como falso; testes unitários com os três modos e com o evento de um auxiliar antigo.

## 3. Testes com root (na VM)

- [x] 3.1 Teste com root: numa aba pelo kernel, `cat`, `echo >` e `exec 3<>` dão `r`, `w` e `rw`; um arquivo lido e depois aberto com `<>` tem as duas linhas; o `exec 3<>` num arquivo em `file-deny` dá a linha `rw` bloqueada. O teste do shell de entrada (change 81) passa a esperar `rw`.
- [x] 3.2 `scripts/vm-ubuntu-2404.sh check-root` no 6.8 e no 7.0, e `./scripts/check.sh` no host, sob a vigia de memória.

## 4. Documentação e roadmap

- [x] 4.1 `docs/files.md` (o acesso no registro pelo kernel) e `docs/helper.md` (o modo nos eventos), com os pares em `docs/en/`.
- [x] 4.2 `openspec/roadmap.md`: a linha do change 82 na etapa 16, e o risco do `w` tirado do change 78.

## Workflow follow-up

- Arquivar o change, com as specs sincronizadas.
- PR, merge e release.
