## Context

- O sandbox nega montagens pelo Landlock (um domínio com regras de arquivo não monta) e pelo seccomp (`MOUNT_SYSCALLS`). O Landlock pode ser empilhado: um processo restringe ainda mais o próprio domínio, sem privilégio e sem montar.
- O `/run/sandmux/bin` da visão já recebe programas do sandmux (`sandmux-chat`, `sandmux-db-mcp`) e vem antes no `PATH`. O `sandmux-run` é estático e já tem o código do Landlock (`landlock_rules`) e do seccomp.
- O Claude Code 2.1.329 usa `--ro-bind`, `--bind`, `--ro-bind-try`, `--dev-bind`, `--tmpfs`, `--dir`, `--symlink`, `--chmod`, `--remount-ro`, `--proc`, `--dev`, os `--unshare-*`, `--hostname`, `--chdir`, `--setenv`, `--unsetenv`, `--uid`, `--cap-drop`, `--cap-add`, `--seccomp`, `--sync-fd`, `--info-fd`, `--args`, `--bind-fd`, `--ro-bind-fd`, `--die-with-parent`, `--new-session` e `--disable-userns` (lidos do executável).

## Goals / Non-Goals

**Goals:** o Claude Code, o opencode e quem usa o bubblewrap como isolamento funcionam dentro do sandbox, sem tirar nada do confinamento da aba, e com o pedido respeitado onde dá.

**Non-Goals:** reproduzir a árvore de arquivos que o bubblewrap montaria (o `flatpak-builder` continua sem funcionar); montagens de verdade; podman.

## Decisions

### 1. Um modo do `sandmux-run`

O `main` do `sandmux-run` olha o nome com que foi chamado: como `bwrap`, vai para `sandmux_launcher::bwrap::main`. A visão monta o próprio binário do `sandmux-run` em `/run/sandmux/bin/bwrap` (já no `PATH`) e, quando a visão tem `/usr/bin/bwrap`, cobre esse também. Um binário a menos para distribuir e o mesmo código do Landlock e do seccomp.

### 2. As opções

Um leitor de opções na ordem do bubblewrap, com `--args FD` (opções separadas por NUL num descritor) e os valores de `--*-fd` lidos como descritores. Três grupos:

- **aplicadas:** os `--unshare-*` sem montagem, `--hostname` (com o `--unshare-uts`), `--chdir`, `--setenv`, `--unsetenv`, `--clearenv`, `--die-with-parent` (`PR_SET_PDEATHSIG`), `--new-session` (`setsid`), `--cap-drop` (o comando já roda sem capabilities), `--seccomp FD` (o filtro do pedido instalado sobre o do sandmux), `--uid` e `--gid` (o mapeamento do `--unshare-user`, quando ele vem);
- **convertidas em Landlock:** as montagens de arquivos viram regras de leitura ou escrita sobre o caminho de origem, quando a origem e o destino são o mesmo caminho. Um bind para outro destino não tem como ser imitado sem montar: a origem entra com o acesso pedido, e o destino não muda (limite documentado). Um `--ro-bind /dev/null X` (negar `X`) não vira regra: o Landlock só concede, e `X` fica sem acesso se nenhuma outra regra o cobrir;
- **ignoradas:** `--proc`, `--dev`, `--dir`, `--symlink`, `--remount-ro`, `--chmod`, `--perms`, `--size`, `--file`, `--bind-data`, `--ro-bind-data`, `--mqueue`, `--overlay*`, `--disable-userns`, `--assert-userns-disabled`, `--cap-add` (não há o que acrescentar).

Opção desconhecida: erro e código 1, como o bubblewrap.

### 3. O Landlock empilhado

Quando o pedido tem alguma montagem de arquivos, o substituto aplica um Landlock com as regras do pedido mais o mínimo para o comando rodar: leitura e execução em `/usr`, `/lib*`, `/bin`, `/sbin`, `/etc` e `/run/sandmux/bin`, e escrita em `/dev/null`, `/dev/tty` e `/dev/pts`. Sem nenhuma montagem de arquivos (`bwrap --dev-bind / / ...`), não empilha nada. Usa a ABI do kernel e só os direitos de arquivo: os escopos já vêm do Landlock da aba.

### 4. Os descritores de sincronização

- `--info-fd` e `--json-status-fd`: escreve o JSON `{"child-pid": N}` do bubblewrap e fecha;
- `--sync-fd`: segura o descritor aberto enquanto o comando roda;
- `--block-fd`: espera ler antes de executar.

### 5. Processo 1 com `--unshare-pid`

Com `--unshare-pid`, o substituto faz `unshare(CLONE_NEWPID)` e um `fork`. O filho é o processo 1 do namespace e executa o comando; o pai espera e devolve o código. O `/proc` não é montado de novo, então o comando ainda vê os pids do sandbox no `/proc` (limite documentado).

## Risks / Trade-offs

- **[O pedido dá mais acesso que a aba]** → impossível: o Landlock empilhado só restringe, e a aba continua valendo por baixo.
- **[Uma ferramenta confia que o bubblewrap escondeu algo]** → o que ela cobriu sem bind no mesmo caminho não fica escondido. Os segredos da pasta continuam protegidos pelo sandmux. Documentado.
- **[O bubblewrap muda as opções]** → a lista é a do 0.11, e uma opção nova dá erro claro em vez de ser ignorada em silêncio.
