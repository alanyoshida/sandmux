Os programas BPF só rodam na VM (`scripts/vm-ubuntu-2404.sh check-root`). Os testes pesados rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`.

## 1. Programas BPF

- [ ] 1.1 As marcas `SECRET` (`ENOENT`) e `FDENY` (`EACCES`) nos programas do confinamento, sem mudar `DENY`, `PROTECT` e `GITDIR`. Verificar com testes de montagem e um teste com root na VM
- [ ] 1.2 O evento `EV_OPEN` no `file_open`: o filtro pelo `mnt_id`, o caminho por `bpf_d_path` e o `f_mode`, e a fila maior. Verificar com testes de montagem e de leitura do evento, e um teste com root na VM no 6.8 e no 7.0

## 2. Auxiliar

- [ ] 2.1 As opções novas do `confine` (as montagens a registrar e as marcas novas no `mark`) e o evento de abertura em JSON. Verificar com testes unitários do pedido

## 3. Daemon

- [ ] 3.1 `Msg::ConfineTab` e a resposta (protocolo 43): o escopo do processo, o `confine`, as marcas pela política e o `Files` da aba. Verificar com testes unitários com o auxiliar falso
- [ ] 3.2 O `Files` generalizado: as marcas da aba, as linhas de abertura a partir de `EV_OPEN` e a parada da aba na queda do auxiliar. Verificar com testes unitários

## 4. Lançador

- [ ] 4.1 O pedido ao daemon antes de liberar o filho, os ids das montagens registradas e o byte `1` ou `2`; no modo kernel, sem o filtro de aberturas. Verificar com o teste de recuo no host (sem auxiliar, o supervisor como hoje) e com os testes da VM

## 5. O que mostra

- [ ] 5.1 O `PolicySummary` com a fonte, o `stats`, o painel e o `doctor`. Verificar com os testes do `stats` e da TUI

## 6. Testes e medida na VM

- [ ] 6.1 Testes com root, pelo `check-root` da VM:
  - o `.env` dá `ENOENT`, e o arquivo de `file-deny` dá `EACCES`;
  - um segredo criado pelo agente continua legível;
  - um segredo criado no host fica negado em até 1 s;
  - o registro tem as leituras e as escritas;
  - a queda do auxiliar para a aba.
- [ ] 6.2 Medir a leitura em massa e o import do Python, com e sem o caminho do kernel, no 6.8 e no 7.0, e anotar no roadmap

## 7. Documentação e fechamento

- [ ] 7.1 `docs/files.md` (as duas fontes, a janela e a medida), `docs/helper.md` e `docs/sessions.md` (o protocolo), com os pares em inglês e os testes de `docs.rs`
- [ ] 7.2 Rodar o `./scripts/check.sh` inteiro no host e o `check-root` na VM

## Workflow follow-up

- Arquivar, PR, merge e release nova (ordem do usuário, 2026-11-20).
