## Context

O supervisor (`sandmux-launcher/src/files.rs`) atende as aberturas do sandbox por seccomp user-notify: ele abre o arquivo com `openat2` e `RESOLVE_IN_ROOT` a partir da raiz do sandbox e injeta o descritor no processo (`SECCOMP_IOCTL_NOTIF_ADDFD`). Ele roda no lançador, no user namespace de quem lançou, acima do namespace do sandbox, e fora do pid namespace dele.

Verificado nesta máquina (kernel 7.0, 2026-11-15): dentro do sandbox, `unshare -U --map-user=1000` falhava com `write(3, "1000 1000 1") = EPERM`, e o mesmo comando funcionava no host, até aninhado. Com o Landlock liberando escrita em tudo, a falha continuava. O `setgroups` da mesma pasta passava. O kernel exige que o descritor do `uid_map` tenha sido aberto no namespace do processo ou no pai, e o supervisor está acima dos dois.

## Goals / Non-Goals

**Goals:**
- O mapeamento de ids que funciona fora do sandbox funciona dentro dele, sem acesso a mais.
- O Chrome com o sandbox dele, sem `--no-sandbox`.

**Non-Goals:**
- `SECCOMP_USER_NOTIF_FLAG_CONTINUE`: o kernel releria o caminho da memória do processo, e um processo poderia trocar o caminho entre a conferência e a abertura, contornando a política de arquivos.
- Mapeamentos com vários ids (exigem privilégio no namespace pai, como fora do sandbox).

## Decisions

- **Alvo `IdMap`:** `uid_map`, `gid_map`, `projid_map` e `setgroups` como último componente do `/proc` do próprio processo (`self`, `thread-self` ou o pid dele) viram `/proc/<pid no host>/<nome>`, abertos por [`open_idmap`].
- **Auxiliar no namespace pai:** o supervisor pega o namespace pai do processo (`NS_GET_PARENT` no `/proc/<pid>/ns/user`). Se o pai é o namespace do próprio supervisor, o processo não criou namespace e o pedido é recusado com `EPERM`, como antes. Senão, um filho do supervisor entra no pai (`setns`; o supervisor é dono da cadeia e tem as capabilities dela), abre o arquivo e devolve o descritor por `SCM_RIGHTS`. O filho é criado com `fork` num processo com várias threads, então tudo o que ele usa é preparado antes, e depois só há chamadas de sistema e `_exit`.
- **Sem acesso a mais:** a gravação continua conferida pelo kernel com as credenciais de quem abriu. No namespace pai, quem abriu é dono do namespace do processo, então mapear o próprio uid passa, como passaria para o processo fora do sandbox. Mapear vários ids exige `CAP_SETUID` no pai para quem *grava*, e o processo do sandbox não tem.
- **Pid no nível do sandbox:** a tradução do `/proc/self` usa o segundo número do `NSpid` (o primeiro é o pid no namespace do supervisor, o segundo o do sandbox, que é o do `/proc` montado lá dentro), no lugar do último. Para um processo sem pid namespace próprio, os dois são iguais.

## Risks / Trade-offs

- [Um `fork` por mapeamento] → Acontece poucas vezes por processo (uma por namespace criado).
- [Um processo que cria um namespace dentro de outro namespace aninhado] → O pai é o namespace intermediário, também descendente do sandbox, e o supervisor tem as capabilities dele.
