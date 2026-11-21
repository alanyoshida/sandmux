## 1. Programas BPF

- [x] 1.1 `Layout`: o `i_generation` do `inode` pelo BTF, as funções `ktime_get_ns`, `map_update_elem` e `map_delete_elem`, e o tipo `BPF_MAP_TYPE_LRU_HASH`.
- [x] 1.2 `Confinement::load_with`: o mapa LRU das aberturas (16384 entradas, chave de 24 bytes, valor u64), só com montagens a registrar, passado aos programas no `EventMaps`.
- [x] 1.3 `open_event`: a chave na pilha depois do teste da montagem, a busca com a janela de 1 s, o evento do tamanho do caminho (`EV_HEADER + R0`, só com R0 de 1 a 256) e a gravação da hora só com o `bpf_ringbuf_output` bem-sucedido.
- [x] 1.4 `inode_rename` e `inode_link` da aba: apagar do mapa as chaves de leitura e de escrita do inode de origem.
- [x] 1.5 Testes unitários da montagem: o programa da aba chama o `ktime_get_ns`, grava e apaga no mapa, e o do Docker não tem nada disso; o tamanho do `bpf_ringbuf_output` da abertura vem de um registrador.

## 2. Auxiliar

- [x] 2.1 `Reader::poll` guarda os bytes pendentes do último esvaziamento; `Events::backlog` com o padrão `(0, 0)` e a versão real.
- [x] 2.2 `forward_events` em duas threads (a leitora e a que envia), com a fila em memória e os descartes juntados ao lote seguinte.
- [x] 2.3 O descritor do cgroup guardado na conexão no `confine` com montagens; o congelamento e o descongelamento pelos limites do design, a espera de 50 ms congelada, e nenhum descongelamento depois do fim da conexão ou com o servidor fechando (sob a trava do fim).
- [x] 2.4 A linha no stderr do auxiliar a cada congelamento, com a duração, nos dois idiomas.
- [x] 2.5 Testes unitários com eventos falsos e um cgroup falso (um diretório com `cgroup.freeze`): congela com a fila cheia, com o backlog alto e com descarte; descongela quando esvazia; não descongela depois do fim da conexão nem com o servidor fechando.

## 3. Testes com root (na VM)

- [x] 3.1 Teste de leitura em massa: trinta mil arquivos diferentes lidos na aba, cada um três vezes, com uma linha de cada um no registro e sem linha de descarte.
- [x] 3.2 Teste do arquivo renomeado pela aba e lido com o nome novo logo depois, com a linha do nome novo.
- [x] 3.3 O `custo_dos_arquivos_da_aba` medido de novo no 6.8 e no 7.0.
- [x] 3.4 `scripts/vm-ubuntu-2404.sh check-root` no 6.8 e no 7.0, e `./scripts/check.sh` no host, sob a vigia de memória.

## 4. Documentação e roadmap

- [x] 4.1 `docs/files.md` ("Pelo kernel": as repetições, a fila e a aba que espera) e `docs/helper.md` (os eventos e os congelamentos no journal), com os pares em `docs/en/`.
- [x] 4.2 `openspec/roadmap.md`: a linha do change 80, o risco da fila do change 78 atualizado e as medidas novas.
