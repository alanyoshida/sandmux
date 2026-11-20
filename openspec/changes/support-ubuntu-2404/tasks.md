Os testes com o Docker real, o `./scripts/check.sh`, o `scripts/check-root.sh` e a VM rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Os testes com root precisam da linha do `sudoers` de `scripts/check-root.sh`.

## 1. Programas do escopo (com root primeiro)

- [ ] 1.1 Montar os programas `BPF_LSM_MAC` de `unix_stream_connect`, `unix_may_send`, `task_kill` e `file_ioctl`, com o mapa dos user namespaces registrados, a subida pelos ancestrais (8 níveis) e a lista fixa de comandos de `ioctl` do Landlock. Verificar com testes de montagem de cada programa e um teste unitário da lista de comandos
- [ ] 1.2 Carregar os programas no kernel desta máquina por um teste com root: um processo num user namespace registrado não conecta a um socket abstrato do host, não sinaliza um processo de fora e não faz `ioctl` num dispositivo fora da lista, e um processo de fora do namespace não é afetado

## 2. Auxiliar

- [ ] 2.1 O pedido `scope`: a conferência do dono do namespace e do tipo, o registro no mapa, a carga no primeiro registro e a soltura quando o último fecha, o limite de 256 por usuário e o estado no `status`. Verificar com testes unitários do pedido (namespace de outro usuário, limite, pedido desconhecido) e com o teste com root do fim da conexão

## 3. Lançador

- [ ] 3.1 A escolha da ABI do Landlock (`check_kernel` com o nível, `landlock_rules::apply` com a ABI do kernel) e a variável de teste `SANDMUX_RUN_LANDLOCK_ABI`, que só baixa. Verificar com testes unitários da escolha e com o teste de recusa sem o auxiliar
- [ ] 3.2 O registro no auxiliar antes de liberar o filho (o descritor do user namespace, a lista de dispositivos do `/dev` do sandbox, a conexão guardada até o fim) e a recusa quando o registro falha. Verificar com um teste com root que roda a suíte de fuga do sandbox com `SANDMUX_RUN_LANDLOCK_ABI=4` e o auxiliar real, e com o teste sem o auxiliar (lançamento recusado e nenhum comando executado)

## 4. Diagnóstico

- [ ] 4.1 `support.rs` com o BPF LSM ativo e o auxiliar, e o `sandmux doctor` com os três estados e os dois caminhos (o HWE no Ubuntu 24.04 pelo `/etc/os-release`, e o `lsm=` com a lista atual mais `bpf`). Verificar com testes unitários do texto com os estados simulados

## 5. Ubuntu 24.04 de verdade

- [ ] 5.1 O roteiro `scripts/vm-ubuntu-2404.sh`: a imagem de nuvem do noble em `~/.cache/sandmux-vm/` conferida pelo `SHA256SUMS`, a VM com QEMU/KVM e cloud-init (4 GB), os `.deb` do `scripts/dist.sh` instalados lá dentro. Verificar subindo a VM e rodando `sandmux doctor` no 6.8
- [ ] 5.2 No 6.8 sem o `bpf`: o `doctor` mostra os dois caminhos e o lançamento é recusado. No 6.8 com `lsm=...,bpf` e o auxiliar: a suíte de fuga, uma sessão com um harness de teste e o Docker privado. Anotar no roadmap o que passou e o que não passou
- [ ] 5.3 No kernel HWE da VM: o `doctor` limpo, a suíte de fuga sem o auxiliar e uma sessão. Medir o custo dos programas globais no 6.8 (um laço de `ioctl` num terminal, com e sem escopo registrado) e anotar no roadmap

## 6. Documentação e fechamento

- [ ] 6.1 Atualizar `docs/install.md` (requisitos, o Ubuntu 24.04 com o HWE ou com o 6.8 pelo auxiliar), `docs/helper.md` (o pedido `scope`), `README.md` (a lista de distros) e os pares em inglês, com os testes de `docs.rs`
- [ ] 6.2 Rodar `./scripts/check.sh` e `scripts/check-root.sh` inteiros sob o vigia

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Arquivar o change depois da release, com as specs sincronizadas.
