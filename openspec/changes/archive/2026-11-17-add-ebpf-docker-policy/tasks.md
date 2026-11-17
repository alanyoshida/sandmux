Os testes com o Docker real, o `./scripts/check.sh` e o `scripts/check-root.sh` rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Os testes com root só rodam depois que o usuário liberar `scripts/check-root.sh` no `sudoers` (decisão de 2026-11-17).

## 1. Base do eBPF sem root

- [x] 1.1 Criar o crate `crates/sandmux-ebpf` com o leitor do BTF: tipos, nomes, `FUNC` → `FUNC_PROTO` com os nomes dos parâmetros, e deslocamentos de campos aninhados por caminho (`file.f_path.dentry`). Verificar com testes contra o `/sys/kernel/btf/vmlinux` da máquina: os ids dos dez ganchos `bpf_lsm_*`, a posição de `dir` e `dentry` em `inode_create`, e deslocamentos plausíveis de `f_inode`, `d_parent` e `d_name`
- [x] 1.2 Escrever o montador de instruções BPF, com rótulos e desvios resolvidos, e as chamadas `bpf()` (`MAP_CREATE`, `MAP_UPDATE_ELEM` com descritor como chave, `PROG_LOAD` com o log do verificador, `LINK_CREATE`). Verificar com testes unitários das codificações conhecidas (comparadas com a tabela de opcodes do kernel, `Documentation/bpf/standardization/instruction-set.rst`)
- [x] 1.3 Escrever os construtores dos programas de cada gancho da decisão 5 do design, a partir do BTF, com o laço `acima()` de 64 níveis e as comparações de nome. Verificar com testes que o programa de cada gancho sai montado sem rótulo pendente e que um BTF sem um gancho dá erro com o nome dele

## 2. Auxiliar root

- [x] 2.1 Escrever o protocolo (`status`, `confine`, `mark`) com `SCM_RIGHTS`, o teto de 4 KiB por linha e os limites por usuário (16 confinamentos, 16384 marcas). A carga dos programas fica atrás de uma interface, com uma versão falsa para os testes. Verificar com testes sem root: linha inválida, `mark` antes de `confine`, cgroup de outro dono recusado (diretório de teste com outro `st_uid` simulado), limites, e a liberação de tudo quando a conexão fecha
- [x] 2.2 Criar o binário `sandmux-helper`: socket do systemd (`LISTEN_FDS`) ou `--socket <caminho>`, uma thread por conexão, `RLIMIT_NOFILE` alto, a carga real dos programas e o armazenamento por inode. Verificar com `--help` e com um teste sem root em que o `confine` falha com a mensagem de permissão, sem derrubar o auxiliar
- [x] 2.3 Escrever `scripts/check-root.sh` (decisão 10 do design) e o arquivo de testes `crates/sandmux/tests/docker_ebpf.rs`, ignorado quando `SANDMUX_HELPER_SOCKET` não existe. Verificar que o `check.sh` continua verde sem root. Pedir ao usuário a linha do `sudoers`

## 3. Verificações com root (dependem do `sudoers`)

- [x] 3.1 Provar a carga e o anexo `BPF_LSM_CGROUP` de um programa de `file_open` com o armazenamento por inode e o laço por `d_parent`, num cgroup de teste. Se o verificador ou o kernel recusarem, aplicar o recuo da decisão 3 ou da 6 e anotar no design e no spec. Feito: o verificador recusou o ponteiro da subida por `d_parent`; recuo da decisão 6 aplicado, e o anexo `BPF_LSM_CGROUP` vale
- [x] 3.2 Descobrir a lista mínima de capacidades do serviço e escrevê-la nas unidades. Feito: `CAP_BPF` e `CAP_PERFMON`, com o `check-root.sh` rodando o auxiliar por `systemd-run` com as propriedades da unidade
- [x] 3.3 Teste com o Docker real e o auxiliar: um contêiner `--privileged` com a pasta montada desmonta a cobertura do `.env` e não o lê (`EPERM`); um hard link para o `.env` não é lido; um arquivo criado pelo contêiner dentro de uma pasta de `file-deny` é negado (o criado depois no host fica fora, pelo recuo da decisão 6); o agente continua lendo um segredo que ele criou
- [x] 3.4 Teste com o Docker real e o auxiliar: um contêiner `--privileged` tira o somente-leitura de `.git/hooks` e não grava `pre-commit`; `.git/commondir` não é criado; renomear `.git/hooks` falha; `git log` no contêiner funciona; `docker build`, `docker run` e `docker compose` dos testes atuais continuam passando
- [x] 3.5 Teste com root: o auxiliar recusa o `confine` num cgroup do root pedido por um usuário comum; parar o auxiliar com o Docker rodando para o Docker da pasta; fechar a conexão solta os programas (o `bpftool` sem root não lista os programas no teste; a soltura vem de fechar os descritores dos links, como o `Drop` do confinamento)
- [x] 3.6 Medir um `docker build` grande e uma leitura em massa num contêiner, com e sem o confinamento, e anotar no roadmap. Feito: sem diferença mensurável (teste `custo_do_confinamento`, ignorado por padrão)

## 4. Daemon, estado e diagnóstico

- [x] 4.1 Tirar da especificação da visão a lista de marcas (`DENY`, `PROTECT`, `GITDIR`) em `sandmux-policy`. Verificar com testes unitários: as coberturas de segredos e de `/etc`, os binds em somente leitura de `git::protect` com submódulo, e nada fora da pasta de trabalho como `PROTECT`
- [x] 4.2 No `start()` do Docker: `status`, `confine` com o cgroup da slice, os `mark` em lotes, a thread que para o Docker quando a conexão cai, e o recuo com o motivo (sem auxiliar, sem `bpf` no LSM, sem slice). Verificar com um teste do Docker real sem auxiliar (o motivo aparece) e com os testes com root da seção 3 (estes ficam nas tarefas 3.3 a 3.5)
- [x] 4.3 Acrescentar `ebpf` ao `DockerStats`, subir a versão do protocolo, e mostrar a linha no `sandmux stats` e no painel da TUI. Verificar com os testes de protocolo e de `stats`
- [x] 4.4 Acrescentar ao `sandmux doctor` a verificação do confinamento do Docker por eBPF, com a causa e a correção. Verificar com testes do diagnóstico (sem `bpf` na lista, sem auxiliar)

## 5. Instalação e pacotes

- [x] 5.1 Escrever as unidades `packaging/systemd/sandmux-helper.socket` e `.service` e a instalação delas no `sandmux setup`, com a recusa a um binário gravável por outro usuário que não o root. Verificar com testes do setup (caminho no HOME recusado, caminho do root aceito, comandos mostrados com `--no-sudo`)
- [x] 5.2 Levar o `sandmux-helper` e as unidades a `dist.sh`, `install.sh` (com o `--uninstall`), `.deb` (`postinst` e `postrm`), `.rpm` e `PKGBUILD`. Verificar com o `dist.sh` e com `dpkg-deb -c` do pacote gerado

## 6. Documentação

- [x] 6.1 Escrever `docs/helper.md` (o que o auxiliar faz, a autorização, como instalar e desligar, os limites), atualizar `docs/docker.md` (confinamento por eBPF, recuo, limites revistos) e `docs/install.md`, com os pares em inglês e os testes de `docs.rs`
- [x] 6.2 Rodar `./scripts/check.sh` e `scripts/check-root.sh` inteiros sob o vigia

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Lembrar o usuário de tirar a linha do `sudoers` de `scripts/check-root.sh`.
- Change 71 (provisório): registro de arquivos dos contêineres, segredos criados depois e política ao vivo.
