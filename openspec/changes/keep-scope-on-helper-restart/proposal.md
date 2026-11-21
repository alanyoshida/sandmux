## Why

Num kernel com o Landlock abaixo do ABI 6 (o Ubuntu 24.04 com o 6.8), o lançador registra o escopo do sandbox no auxiliar root uma vez, antes de liberar o comando:
- o bloqueio dos sockets abstratos de fora;
- o bloqueio dos sinais para fora e para o processo 1;
- o filtro de `ioctl` em dispositivo.

Quando o auxiliar reinicia (uma atualização, um travamento, um `systemctl restart`), o registro morre com ele, e as abas que já rodam seguem sem essas proteções, sem aviso e sem registro novo. Achado no change 78 (2026-11-20).

No mesmo trabalho apareceu um lançamento recusado logo depois de um reinício ("o auxiliar root não respondeu ao pedido"). Na saída limpa, o auxiliar antigo ainda aceita conexões novas e as corta ao sair, e o lançador não tenta de novo.

## What Changes

- **O daemon guarda o escopo de cada aba:** no `Msg::ConfineTab` (que toda sessão do daemon manda), o lançador passa também os parâmetros do escopo e o user namespace do sandbox (por `SCM_RIGHTS`, aberto antes de liberar o filho, quando o processo 1 ainda deixa). Isso vale só quando o escopo vem do auxiliar. O daemon passa a vigiar uma conexão com o auxiliar por aba.
- **Queda do auxiliar:** o daemon congela a aba (`cgroup.freeze`) e registra de novo, com o auxiliar de volta, o escopo e, quando a aba está nesse modo, os arquivos pelo kernel do change 78. Depois descongela. Sem tudo de volta em 10 s, ou sem o congelamento, a aba morre. É a decisão do usuário de 2026-11-20 e vale também para as abas com os arquivos pelo supervisor.
- **Sem o daemon:** no `sandmux run` sem terminal, ou quando o daemon não assume o escopo, o próprio lançador vigia a conexão do escopo e registra de novo, com prazo de 10 s. Sem o registro de volta, mata o sandbox. Nesse caso sobra a janela da troca de auxiliar.
- **Saída limpa do auxiliar:** no `SIGTERM`, o auxiliar para de atender conexões novas (fecha as que chegam), e quem conectar na hora tenta de novo.
- **Lançamento durante a troca:** o lançador tenta o registro do escopo de novo por até 10 s quando a conexão é recusada ou cai sem resposta.
- O protocolo entre o lançador e o daemon sobe para 44.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `root-helper`: o requisito "Escopo de um sandbox" ganha o registro de novo pelo daemon, com a aba congelada, quando o auxiliar reinicia, e a saída limpa que não atende conexões novas.
- `sandbox-isolation`: o requisito "Processos do host fora do alcance" ganha o cenário do auxiliar reiniciado com o sandbox rodando.

## Impact

- `crates/sandmux-launcher`:
  - `launch.rs`: o user namespace aberto antes da liberação e o pedido ao daemon com o escopo;
  - `scope.rs`: as tentativas no registro e a vigia da conexão sem o daemon.
- `crates/sandmux-net/src/procinfo.rs`: o `confine_tab` com o escopo e o descritor.
- `crates/sandmux-proto`: o `Msg::ConfineTab` com o escopo, o `ConfineTabReply` com `scope`, e o protocolo 44.
- `crates/sandmux/src/daemon/`:
  - `conn.rs`: a leitura do descritor depois do quadro;
  - `tabfiles.rs`: a vigia por aba, e a recuperação que congela e registra o escopo e os arquivos.
- `crates/sandmux/src/helper.rs`: o `Client::scope` com o `init_pid`, e o `accept_loop` que fecha as conexões novas na saída limpa.
- Testes: unitários no lançador, no daemon e no auxiliar; com root, na VM, no 6.8: o escopo de volta depois do reinício do auxiliar, com e sem os arquivos pelo kernel, e o `run` sem o daemon. No 7.0 (ABI 6), o escopo não passa pelo auxiliar, e nada muda.
- Documentação: `docs/helper.md` (o escopo e o reinício) e `docs/sessions.md` (o protocolo), com os pares em inglês.
