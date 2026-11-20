## Context

Motivação em `proposal.md`. O que importa:

- `landlock_rules::apply` abre cada caminho de `rules.read` e `rules.write` com `PathFd::new`, confere com `fstat` se é diretório e acrescenta `PathBeneath::new(fd, acesso)`. `PathBeneath` aceita qualquer descritor (`AsFd`).
- O roteiro de trabalho que rodou os testes com root na VM ficou no scratchpad: ele compilava os testes em musl, copiava para o mesmo caminho na VM e rodava com `systemd-run --user --scope`. O `scripts/check-root.sh` sobe o auxiliar com as propriedades da unidade real (`systemd-run`) e o reinicia quando o teste cria `<socket>.restart`.

## Goals / Non-Goals

**Goals:**

- Nenhum `O_PATH` passa pelo `OpenOptions` no código do sandmux.
- Os testes com root rodam inteiros na VM, sem o `sudo` do host.

**Non-Goals:**

- Trocar o crate `landlock` ou mandar a correção para ele.
- Rodar o `check.sh` inteiro na VM.

## Decisions

### 1. As regras do Landlock pelo `open` direto

Uma função `open_path(&Path) -> io::Result<OwnedFd>` no lançador abre com `O_PATH | O_CLOEXEC`, seguindo links, como o `PathFd`, e o resultado vai para o `PathBeneath`. O `fstat` continua igual. O padrão já existe em `helper::open_path` e em `files::open_proc`.

Um teste unitário abre um FIFO e uma pasta sem leitura e aplica as regras num processo filho, que confere o acesso. Ele roda também no alvo musl, sem carregar nada no kernel.

### 2. O teste de TUI

`tui_cols` ganha a altura como parâmetro (`tui_size(e, cols, rows)`). O teste do painel de rede usa 50 linhas: o painel cabe inteiro, com as duas linhas do domínio. A causa foi vista na tela da falha: o "novo" da linha do DNS ficava na última linha, cortada.

### 3. O passo `check-root` da VM

`scripts/vm-ubuntu-2404.sh check-root [filtro]`:

1. compila em musl o `sandmux-helper` e o teste `docker_ebpf` (com os binários que ele usa);
2. copia tudo para a VM, no mesmo caminho do host (os testes acham os binários pelo caminho da compilação);
3. na VM, sobe o auxiliar como root num socket próprio, com um laço que o reinicia quando aparece `<socket>.restart`, como o `check-root.sh`;
4. roda o teste como o usuário, com `systemd-run --user --scope`, `LANG=pt_BR.UTF-8` e o socket na variável;
5. para o auxiliar e mostra o fim do log dele.

`scripts/vm-ubuntu-2404.sh docker` instala na VM o Docker sem root: o `docker.io` do Ubuntu e o `docker-ce-rootless-extras` e o `docker-compose-plugin` do repositório do Docker, com a chave dele. A VM precisa estar ligada (`up`).

### 4. O aviso do `check-root.sh`

Antes de subir o auxiliar, o script diz que vai carregar programas BPF no kernel desta máquina, aponta o passo da VM e pergunta. Com `SANDMUX_CHECK_ROOT_HOST=1`, segue sem perguntar.

## Risks / Trade-offs

- **[A VM diverge do kernel do host]** → o HWE do 24.04 era o 7.0, o mesmo do host, em 2026-11-20. O `check-root.sh` continua para quando for preciso testar o kernel da máquina.
- **[A compilação em musl leva tempo]** → só os binários que os testes com root usam.
