## Why

Numa aba confinada pelo kernel (change 78), o programa da abertura manda só o bit de escrita do arquivo. O daemon registra uma abertura de leitura e escrita como `w`, e o supervisor registra a mesma abertura como `rw`. O mesmo arquivo aparece com outro acesso conforme o modo da aba, e quem filtra o registro por `rw` não acha as aberturas das abas pelo kernel. A diferença está nos riscos do change 78 desde 2026-11-20, e o change 81 deixou o shell de entrada no mesmo caminho do agente.

## What Changes

- O evento de abertura leva os dois bits do modo do arquivo: leitura e escrita. O mesmo vale para o bloqueio de uma abertura.
- O daemon registra `r`, `w` ou `rw`, como o supervisor, numa aba pelo kernel e no Docker privado.
- O mapa das repetições do change 80 passa a separar os três modos: uma abertura `rw` depois de uma `r` do mesmo arquivo vai para o registro. Renomear ou criar um link apaga os três.
- **Compatibilidade:** o JSON que o auxiliar manda ao daemon ganha o campo `read`, e o `write` continua como hoje.
  - Um daemon antigo com o auxiliar novo segue registrando `w`.
  - O daemon novo com um auxiliar antigo (sem o campo `read`) também registra `w`, até o auxiliar ser atualizado.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `root-helper`: o requisito "Eventos do confinamento" diz que o evento de uma abertura, negada ou não, tem a leitura e a escrita, com um cenário da abertura de leitura e escrita.
- `file-access-log`: o requisito "Registro de arquivos" ganha o cenário da leitura e escrita numa aba pelo kernel, com a linha `rw`, também a bloqueada.

## Impact

- `crates/sandmux-ebpf/src/prog.rs`:
  - o evento de abertura e o mapa das repetições com o modo de leitura e escrita;
  - o bloqueio de uma abertura com o modo do arquivo;
  - os outros bloqueios com o bit de escrita como constante.
- `crates/sandmux-ebpf/src/ring.rs`: o `Event` com `read` e `write` pelos bits do modo.
- `crates/sandmux/src/helper.rs`: o campo `read` no JSON dos eventos de abertura e de bloqueio.
- `crates/sandmux/src/daemon/dockerfiles.rs`: o acesso `rw`.
- O auxiliar root muda: depois de atualizar, é preciso rodar `sandmux setup`. O protocolo entre o cliente e o daemon não muda.
- Testes:
  - unitários na montagem dos programas, na leitura do evento e no acesso do daemon;
  - com root, na VM, no 6.8 e no 7.0: aberturas `r`, `w` e `rw` numa aba pelo kernel, e o bloqueio `rw` de um arquivo em `file-deny`.
- Documentação: `docs/files.md` (o registro pelo kernel) e `docs/helper.md` (os eventos), com os pares em inglês.
