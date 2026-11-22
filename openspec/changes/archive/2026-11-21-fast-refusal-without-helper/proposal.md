## Why

O teste mais longo da suíte, `kernel_antigo_sem_o_auxiliar_recusa_sem_executar` (30 s), virou o piso do `check.sh` depois do change 89, e o usuário pediu para encurtá-lo (2026-11-21). O tempo não vinha do teste: num kernel com o Landlock abaixo do ABI 6 e sem o auxiliar root, o lançador tenta o registro do escopo de novo por 30 s antes de recusar. Essa espera existe para a troca do auxiliar, mas valia também quando o auxiliar nem está instalado. Quem roda o sandmux no 6.8 do Ubuntu 24.04 sem o auxiliar esperava 30 s pela mensagem que aponta o `sandmux doctor`.

## What Changes

- **Lançamento sem o socket do auxiliar:** quando o socket do auxiliar não existe, o lançador desiste do registro em até 1 s, e não em 30 s. A espera de 1 s cobre a troca de um auxiliar que recria o próprio socket.
- **Troca do auxiliar, como antes:** com o socket presente e o auxiliar sem responder, o lançamento segue tentando por até 30 s, e a vigia do escopo depois de uma queda segue com os 30 s, inclusive sem o socket.
- O teste passa a conferir que a recusa sai em menos de 10 s.

## Capabilities

### New Capabilities
(nenhuma)

### Modified Capabilities
- `root-helper`: o prazo do registro no lançamento quando o socket do auxiliar não existe.

## Impact

- `sandmux-launcher/src/scope.rs`: o erro de socket inexistente separado do auxiliar que não responde, e os dois prazos.
- `tests/escape_launcher.rs`: o prazo da recusa.
- `docs/helper.md` e `docs/en/helper.md`, e o roadmap.
- Sem protocolo novo e sem mudança no auxiliar root.
