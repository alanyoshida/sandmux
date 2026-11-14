# Design

## Context

- **Montagens ao vivo já provadas:** o roadmap registrou em 2026-11-12 que uma pasta pré-autorizada no Landlock aceita montagens posteriores.
- **Auxiliares do supervisor:** o de portas (change 10) e o de nomes (change 16) nascem por `fork` antes da liberação, com uma só thread. Eles abrem `/proc/<pid1>/ns/*` antes de o processo 1 desligar o dumpable.
- **Supervisor:** recebe a especificação por memfd. O daemon fala com ele só por arquivos (a política de rede, a política de arquivos).
- **Pastas extras:** `--ro`/`--rw` montam no mesmo caminho do host, no lançamento, com as recusas de `check_dangerous`.

## Goals / Non-Goals

**Goals:**

- Montar e desmontar uma pasta do host na sessão viva, com leitura ou escrita, e tudo valendo na hora.

**Non-Goals:**

- Montar no mesmo caminho do host ao vivo (decisão do usuário: `/mnt/sandmux`).
- Pastas no Docker privado.

## Decisions

### 1. `/mnt/sandmux`

- **Na composição:** todo sandbox tem um tmpfs em `/mnt/sandmux` (modo 755) e a variável `SANDMUX_FOLDERS=/mnt/sandmux`.
- **Landlock:** `/mnt/sandmux` entra nas regras de escrita; o modo de cada pasta vem da flag da montagem (`MOUNT_ATTR_RDONLY`).
- **Pastas do agente:** `/mnt/sandmux` entra nas raízes do registro de arquivos.
- **Pastas lembradas:** as da aba entram na composição como montagens comuns em `/mnt/sandmux/<nome>`, e o `PASTAS.txt` é gerado com elas.

### 2. Auxiliar de montagem

- **Nascimento:** por `fork`, antes da liberação (uma só thread), como os outros auxiliares, com os descritores de `/proc/<pid1>/ns/{user,mnt}` abertos antes.
- **Preparação:**
  - entra no user namespace do sandbox (fica com as capabilities de lá);
  - cria um mount namespace próprio com `unshare(CLONE_NEWNS)`, uma cópia da árvore do host que pertence ao user namespace do sandbox;
  - guarda o descritor do próprio mount namespace.
- **Montar uma pasta**, a pedido do supervisor:
  1. `open_tree(pasta, OPEN_TREE_CLONE | AT_RECURSIVE)` no namespace próprio, que vê o host;
  2. `mount_setattr` com `MOUNT_ATTR_RDONLY` quando é só leitura, e `nosuid`/`nodev` sempre;
  3. `setns` no mount namespace do sandbox;
  4. `mkdirat` em `/mnt/sandmux` e `move_mount` da árvore separada para `/mnt/sandmux/<nome>`;
  5. regrava `/mnt/sandmux/PASTAS.txt` (um arquivo gerado por um memfd, montado em somente leitura por cima);
  6. `setns` de volta para o namespace próprio.
- **Desmontar:** `umount2(..., MNT_DETACH)` e `rmdir` no namespace do sandbox.
- **Por que no auxiliar:** o `setns` num user namespace exige uma só thread, e o supervisor tem várias; o `open_tree` precisa ver o host e ter `CAP_SYS_ADMIN` sobre a árvore copiada.

### 3. Controle

- **Socket:** o supervisor escuta em `$XDG_RUNTIME_DIR/sandmux/files/<etiqueta>.ctl` (600), indicado na especificação, e repassa os pedidos ao auxiliar.
- **Pedido:** uma linha JSON: `{"op":"add","path":...,"name":...,"rw":true}` ou `{"op":"remove","name":...}`. A resposta é `{"ok":true}` ou `{"error":"..."}`.
- **Só o daemon fala com ele:** a pasta 700 do diretório de execução e o socket 600 deixam de fora o agente, que está em outro mount namespace e não vê o diretório de execução do host.

### 4. Daemon e protocolo

- **Pedidos:** `FolderEdit { id, action: add|remove, value, rw }` e `FolderList { id }` → `Folders { list }` (protocolo 15).
- **Ao adicionar:**
  - confere a pasta (existe, é pasta, `check_dangerous` com o modo pedido, aviso de credenciais);
  - escolhe o nome;
  - guarda em `LaunchOptions.folders` (as pastas da aba) e manda ao supervisor da sessão viva. Sem sessão viva, vale no próximo lançamento.
- **Linha de comando:** `sandmux folder add|remove|list`.

### 5. Tela de pastas

- **Abrir:** Ctrl+b `o` (ação `folders`) lista as pastas da aba (nome, pasta do host, modo).
- **Teclas:**
  - `a` abre um campo para a pasta e pergunta `r`/`w`;
  - `x` desmonta a escolhida;
  - Esc fecha.

## Risks / Trade-offs

- **[Capabilities no auxiliar]** O auxiliar de montagem tem as capabilities do user namespace do sandbox. → Ele não é visível para o agente (fora do pid namespace do sandbox, não despejável), só atende o supervisor, e só monta em `/mnt/sandmux`.
- **[Pasta perigosa]** Montar o HOME ao vivo daria tudo ao agente. → As mesmas recusas do `--rw`.
- **[Montagens presas]** Uma pasta em uso não desmonta na hora. → `MNT_DETACH` desliga a montagem, e ela some quando o último uso termina.

## Migration Plan

Atualizar o binário e rodar `sandmux daemon restart` (protocolo 15). Os sandboxes novos ganham `/mnt/sandmux`.
