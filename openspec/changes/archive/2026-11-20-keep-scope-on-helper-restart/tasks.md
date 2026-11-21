Os programas BPF só rodam na VM (`scripts/vm-ubuntu-2404.sh check-root`). Os testes pesados rodam sob o vigia de memória, com `CARGO_BUILD_JOBS=16`. Nenhum programa BPF muda neste change.

## 1. Auxiliar

- [x] 1.1 O `Client::scope` com o `init_pid`, e a saída limpa que fecha as conexões aceitas depois do `SIGTERM`. Verificar com testes unitários do servidor com o backend falso
- [x] 1.2 Os programas do escopo carregados na subida do auxiliar (Landlock abaixo do ABI 6) e soltos só depois de 60 s sem escopo. Verificar com o teste com root na VM no 6.8

## 2. Protocolo

- [x] 2.1 O `Msg::ConfineTab` com `scope` (`TabScope`: `init_pid`, `ioctl`, `devs`), o `ConfineTabReply` com `scope` e o protocolo 44; o `confine_tab` do `sandmux-net` com o descritor por `SCM_RIGHTS` depois do quadro. Verificar com testes de ida e volta das mensagens e um teste do descritor num par de sockets

## 3. Daemon

- [x] 3.1 A leitura do descritor no `ConfineTab`, com a conferência do namespace, e o contexto da aba com o escopo, guardado também nas abas com os arquivos pelo supervisor. Verificar com testes unitários
- [x] 3.2 A vigia por aba e a recuperação única: congelar, soltar os arquivos antigos, registrar os arquivos e o escopo de novo, abrir a vigia nova, descongelar; matar sem tudo de volta em 30 s. Verificar com testes unitários com o auxiliar falso

## 4. Lançador

- [x] 4.1 O namespace aberto antes da liberação, as tentativas no registro do escopo e o pedido ao daemon com o escopo. Verificar com testes unitários da decisão de tentar de novo
- [x] 4.2 A vigia do lançador: sem o daemon (ou com `scope: false`), o escopo de novo em até 30 s ou o fim do sandbox; com `scope: true`, nada. Verificar com o teste com root na VM

## 5. Testes com root na VM (6.8)

- [x] 5.1 Pelo `check-root` da VM, no 6.8:
  - numa aba com os arquivos pelo kernel e noutra com os arquivos pelo supervisor (`file-deny` por nome), depois do reinício do auxiliar, um sinal para o supervisor do sandbox continua negado, e a aba segue;
  - um `run` sem o daemon depois do reinício continua com o sinal negado;
  - um lançamento logo depois do reinício sobe;
  - o resto da suíte passa no 6.8 e no 7.0.

## 6. Documentação e fechamento

- [x] 6.1 `docs/helper.md` (o escopo no reinício do auxiliar) e `docs/sessions.md` (o protocolo 44), com os pares em inglês, e o roadmap
- [x] 6.2 Rodar o `./scripts/check.sh` inteiro no host e o `check-root` na VM no 6.8 e no 7.0

## Workflow follow-up

- Arquivar, PR, merge e release nova (ordem do usuário, 2026-11-20).
