Os testes com o Docker real, o `./scripts/check.sh` e o `scripts/check-root.sh` rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Os testes com root precisam da linha do `sudoers` de `scripts/check-root.sh` (pedir ao usuário se ela tiver sido tirada).

## 1. Base no kernel (com root primeiro)

- [ ] 1.1 Provar, num programa `BPF_LSM_CGROUP` de `inode_create`, o `bpf_ringbuf_output` de um evento da pilha, o `bpf_probe_read_kernel_str` do nome da `dentry`, a leitura do `s_dev` pelo `i_sb` do diretório e a soma atômica num mapa `ARRAY`. Verificar com um teste em `docker_ebpf.rs`. Se um deles for recusado, aplicar o recuo escrito nos riscos do design e anotar no design e no spec
- [ ] 1.2 Acrescentar ao montador a soma atômica (`BPF_STX | BPF_ATOMIC`), e ao `sandmux-ebpf` os mapas `RINGBUF` e `ARRAY` e a leitura da fila (`mmap` das páginas, `epoll`, os cabeçalhos dos registros). Verificar com testes unitários da codificação e da leitura sobre um buffer montado no teste
- [ ] 1.3 Mandar os eventos nos programas: o de bloqueio antes de negar, nos dez ganchos; o de criação no `inode_create`, com o filtro do `s_dev`; o contador da fila cheia. Verificar com os testes de montagem (cada programa usa a fila) e com o teste com root da 1.1

## 2. Auxiliar root

- [ ] 2.1 Acrescentar o `unmark` (procura do descritor guardado pelo `st_dev` e pelo `st_ino`, tirada dos bits, fechamento quando não sobra bit) e o campo `dev` do `confine`. Verificar com testes sem root, pela carga falsa: desmarcar um arquivo, desmarcar o que não está marcado, os bits que sobram
- [ ] 2.2 Ler a fila de cada confinamento numa thread e mandar os lotes `{"ev": [...], "dropped": n}` pela conexão, com o envio atrás de um `Mutex`. No cliente, uma thread de leitura que separa as respostas dos eventos. Verificar com um teste sem root em que a carga falsa produz eventos no meio de pedidos

## 3. Vigia da pasta e marcas ao vivo

- [ ] 3.1 Escrever `docker_mark_for` em `sandmux-policy` (a decisão de um caminho, com as pastas negadas) e a diferença entre dois conjuntos de marcas. Verificar com testes unitários: segredo novo, exemplo, segredo do agente, arquivo e pasta dentro de pasta negada, e `file-allow`
- [ ] 3.2 Escrever `daemon/dockerwatch.rs`: as vigias por pasta, a espera de 200 ms, a releitura do `created`, o índice de inodes das pastas e o recuo para a varredura a cada 10 s com `ENOSPC`. Verificar com testes numa pasta temporária, sem Docker e com um auxiliar falso: criar `.env`, criar uma pasta com arquivos dentro, renomear para dentro e o recuo forçado
- [ ] 3.3 Ligar `refresh_engine` a `Engines::refresh_marks`, que recalcula e manda a diferença. Acrescentar `files_poll` ao `DockerStats`, subir a versão do protocolo e mostrar o aviso no `sandmux stats`. Verificar com os testes de protocolo e de `stats`

## 4. Registro

- [ ] 4.1 Traduzir os eventos em linhas `FileEntry` (bloqueios pelas marcas em vigor, criações pelo índice da vigia, uma criação por caminho em cada subida, a linha da contagem de descartados) e gravá-las no registro do dia. Verificar com testes unitários da tradução
- [ ] 4.2 Fazer `sandmux files <id>` incluir as linhas `docker-<id>` da pasta da sessão. Verificar com o teste de `filescmd`

## 5. Verificações com root

- [ ] 5.1 Teste com o Docker real e o auxiliar: um `.env` criado no host com o Docker rodando, e um arquivo criado no host dentro de uma pasta negada, não são lidos por um contêiner `--privileged` depois de 1 segundo; um `chave.pem` criado pelo agente continua legível
- [ ] 5.2 Teste com o Docker real e o auxiliar: acrescentar `file-deny` na configuração nega a leitura seguinte; acrescentar `file-allow` a um `.env` negado deixa o contêiner abrir o arquivo (vazio, coberto)
- [ ] 5.3 Teste com o Docker real e o auxiliar: o registro tem a linha `docker-<id>` do `.env` negado com o bloqueio `segredo`, a linha `create` de `/w/saida.txt` com o caminho do host, e nenhuma linha de `/tmp/x` criado no contêiner; `sandmux files <id>` mostra as linhas
- [ ] 5.4 Medir de novo o `custo_do_confinamento` e um `docker build` que cria muitos arquivos, e anotar no roadmap

## 6. Documentação e fechamento

- [ ] 6.1 Atualizar `docs/docker.md` (arquivos novos, política ao vivo, o limite da liberação ao vivo), `docs/files.md` (as linhas dos contêineres), `docs/helper.md` (`unmark` e eventos) e os pares em inglês, com os testes de `docs.rs`
- [ ] 6.2 Rodar `./scripts/check.sh` e `scripts/check-root.sh` inteiros sob o vigia

## Workflow follow-up

- Commit e push num branch novo, PR, merge e release nova (ordem do usuário, 2026-11-17).
- Lembrar o usuário de tirar a linha do `sudoers` de `scripts/check-root.sh` quando o change acabar.
