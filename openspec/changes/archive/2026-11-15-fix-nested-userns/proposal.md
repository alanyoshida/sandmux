## Why

Pergunta do usuário (2026-11-15): dá para subir o `google-chrome` no sandbox com a opção `window`? Testado nesta máquina, o Chrome abortou: "The SUID sandbox helper binary was found, but is not configured correctly". Ele só abria com `--no-sandbox`, sem a camada de isolamento dele.

A spec já promete que um navegador que cria o próprio isolamento inicia dentro do sandbox, e isso não acontecia. A causa está no supervisor das aberturas de arquivo:

- **Mapeamento de ids:** o kernel só aceita gravar no `/proc/<pid>/uid_map` (e `gid_map`, `setgroups`) por um descritor aberto no user namespace do processo ou no pai dele. O supervisor abre os arquivos pelo processo, de fora dos dois, e a gravação dava `EPERM`, até mapeando o próprio uid. Sem o mapeamento, o sandbox de namespaces do Chrome (e o `unshare -r`) não sobe.
- **Pid namespace aninhado:** o supervisor traduz o `/proc/self` pelo último número do `NSpid`, o do namespace mais interno. Num processo que criou o próprio pid namespace, como o zygote do Chrome, o `/proc/self` ia parar em outro processo do sandbox.

## What Changes

- **Mapeamento pelo namespace pai:** o `uid_map`, `gid_map`, `projid_map` e `setgroups` do próprio processo são abertos por um processo auxiliar do supervisor que entra no user namespace pai do processo (`NS_GET_PARENT` e `setns`). O descritor é o que o kernel daria se o processo abrisse o arquivo ele mesmo, e as regras da gravação continuam as do kernel: o processo só mapeia ids que o namespace pai já tem.
- **Pid no nível do sandbox:** o `/proc/self` é traduzido pelo pid no pid namespace do sandbox (o segundo número do `NSpid`), que é o do `/proc` montado lá dentro.
- **Resultado:** o Chrome, o Chromium e o Electron sobem com o sandbox deles (namespaces e seccomp), sem `--no-sandbox`, com e sem a janela.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `file-access-log`: o mapeamento de ids do próprio processo é aberto no user namespace pai, e o `/proc/self` de um processo num pid namespace aninhado é o dele.
- `sandbox-isolation`: dentro de um user namespace aninhado, o processo mapeia o próprio uid, e um navegador sobe com o sandbox dele.

## Impact

- **`sandmux-launcher` (supervisor):** o alvo novo `IdMap`, o auxiliar que abre no namespace pai e a tradução do pid pelo nível do sandbox.
- **Testes:** `unshare -r` dentro do sandbox, `/proc/self` num pid namespace aninhado, a tradução dos alvos; o Chrome manualmente, nesta máquina.
- **Documentação:** `docs/window.md` (o Chrome com o sandbox dele) e o par em inglês.
