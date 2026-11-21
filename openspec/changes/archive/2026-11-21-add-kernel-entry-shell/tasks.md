## 1. Lançador

- [x] 1.1 Uma função só para os filtros de notificação, pelo modo e pelo registro das execuções, usada pelo `command_process`; os testes do `check.sh` continuam verdes.
- [x] 1.2 A thread `sandmux-files` guarda o modo da aba num valor compartilhado (desconhecido, kernel ou supervisor), também quando a aba não pede o modo (supervisor).
- [x] 1.3 A thread de entrada espera o modo até 15 s e o manda com cada pedido; `Enterer::call` e o auxiliar de entrada passam o modo ao shell, que instala os filtros pela função da 1.1. Teste unitário da espera: com o modo já decidido, sem espera; sem modo no prazo, supervisor.
- [x] 1.4 O teste `shell_dentro_do_sandbox_da_aba` (sem BPF, aba pelo supervisor) continua verde, com o `.env` negado e o `/etc` só leitura.

## 2. Testes com root (na VM)

- [x] 2.1 Teste novo em `docker_ebpf.rs`: numa aba pelo kernel, `sandmux enter` lê o `.env` (`No such file`), um arquivo da pasta em `file-deny` (`Permission denied`) e um arquivo comum; o registro tem a leitura do arquivo comum e o bloqueio do `file-deny`; o `Seccomp_filters` do `/proc/self/status` do shell é igual ao do comando da aba.
- [x] 2.2 A leitura dos 13 mil arquivos medida no shell de entrada de uma aba pelo kernel, ao lado da aba pelo kernel e da aba pelo supervisor (o custo de antes), no 6.8 e no 7.0 (`custo_dos_arquivos_da_aba`).
- [x] 2.3 `scripts/vm-ubuntu-2404.sh check-root` no 6.8 e no 7.0, e `./scripts/check.sh` no host, sob a vigia de memória.

## 3. Documentação e roadmap

- [x] 3.1 `docs/files.md` ("Pelo kernel": o shell de entrada segue a aba) e `docs/sessions.md` (o shell de entrada), com os pares em `docs/en/`.
- [x] 3.2 `openspec/roadmap.md`: a linha do change 81 na etapa 16, e a nota do shell de entrada tirada dos riscos do change 78.

## Workflow follow-up

- Arquivar o change, com a spec sincronizada.
- PR, merge e release.
