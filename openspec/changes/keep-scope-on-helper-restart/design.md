## Context

O escopo do sandbox (change 74) existe só nos kernels com o Landlock abaixo do ABI 6. O lançador o pede antes de liberar o filho (`scope::register`), com o user namespace do processo 1, o pid dele, a lista de dispositivos e o filtro de `ioctl` (ABI 4). A conexão fica aberta até o fim, sem ninguém lendo: quando o auxiliar sai, o registro some, e o lançador não percebe.

O change 78 já trata a queda do auxiliar para os arquivos pelo kernel. O daemon congela o escopo do systemd da aba, refaz o confinamento e descongela; o auxiliar, no `SIGTERM`, fecha a escrita das conexões antes de soltar as regras.

Restrições achadas ao propor:
- **O lançador não congela a própria aba:** ele está no mesmo cgroup e congelaria a si mesmo. Quem congela precisa estar fora, e só o daemon está.
- **O user namespace só pode ser aberto antes da liberação:** o processo 1 se torna não despejável (`PR_SET_DUMPABLE 0`) logo depois dela. A partir daí, `/proc/<processo 1>/ns/user` exige `CAP_SYS_PTRACE` no namespace do `mm` dele, que é o do host, e nem o daemon nem o lançador têm. O lançador precisa abrir o namespace antes e guardá-lo.
- **O daemon lê os quadros direto do socket (`read_frame`), sem buffer:** um byte com o descritor por `SCM_RIGHTS`, logo depois do quadro, chega intacto a um `recvmsg`.

## Goals / Non-Goals

**Goals:**
- Numa sessão do daemon no 6.8, nenhum processo da aba roda sem o escopo depois de um reinício limpo do auxiliar. A aba congela e segue.
- No `run` sem o daemon, o escopo volta sozinho, ou o sandbox termina em 10 s.
- Um lançamento durante a troca do auxiliar sobe.
- Uma recuperação só por aba: o escopo e os arquivos pelo kernel voltam juntos, sob o mesmo congelamento.

**Non-Goals:**
- Escopo que sobrevive ao auxiliar (programas fixados no bpffs): resolveria sem congelar, mas deixaria programas BPF globais carregados depois de um auxiliar morto, o risco do incidente de 2026-11-20.
- A janela de um auxiliar morto à força (`SIGKILL`, travamento) antes do congelamento: continua como nos arquivos do change 78.
- O kernel com o ABI 6 (o 7.0), em que o escopo não passa pelo auxiliar.

## Decisions

### 1. O lançador abre o namespace uma vez e o entrega ao daemon

No modo `KernelLevel::Helper`, o lançador abre `/proc/<filho>/ns/user` antes da liberação e guarda o descritor. Ele serve ao `scope::register` (que hoje o abre sozinho), à vigia do lançador (decisão 4) e ao pedido ao daemon.

O `Msg::ConfineTab` ganha o campo `scope: Option<TabScope>` (`init_pid`, `ioctl`, `devs`). Com ele, o lançador manda, logo depois do quadro, um byte com o descritor por `SCM_RIGHTS`. O daemon só lê o byte quando o campo vem, e confere que o descritor é um user namespace do mesmo usuário (`NS_GET_NSTYPE`, `NS_GET_OWNER_UID`).

O `ConfineTabReply` ganha `scope: bool`: o daemon assumiu a vigia do escopo. O protocolo sobe para 44. Alternativa descartada: o daemon abrir o namespace por um processo despejável do sandbox na hora da queda. Um agente que se torna não despejável impediria o registro, e a aba morreria.

### 2. Uma vigia e uma recuperação por aba, no daemon

O `tabfiles` vira o lugar da aba no kernel. O contexto tem o escopo do systemd, os parâmetros do escopo (com o descritor) e se os arquivos estão pelo kernel. O daemon guarda o contexto quando há escopo ou arquivos pelo kernel, e abre uma conexão de vigia com o auxiliar (só o `status`). Uma thread espera ela cair (`wait_closed`).

Na queda, a recuperação:
1. congela o escopo da aba;
2. solta o confinamento antigo dos arquivos;
3. a cada 300 ms até 10 s: conecta, liga os arquivos (quando pelo kernel), registra o escopo numa conexão própria (`Client::scope` com o `init_pid`) e abre a vigia nova;
4. descongela.

Se algo não volta, ou o congelamento falha, mata o escopo (`cgroup.kill`). Depois da primeira queda, quem segura o escopo é o daemon. A conexão do lançador já caiu.

Uma aba com os arquivos pelo supervisor e escopo pelo auxiliar também tem contexto: só o escopo volta.

### 3. O lançador fica de fora quando o daemon assume

Com `scope: true` na resposta, a vigia do lançador só espera o fim da própria conexão e não faz nada: a recuperação é do daemon, e o lançador estaria congelado de qualquer modo.

### 4. A vigia do lançador sem o daemon

Sem o daemon (o `run` sem terminal), ou com `scope: false`, uma thread do lançador espera a conexão do escopo cair. Depois tenta o registro de novo, com o descritor guardado, a cada 300 ms até 10 s. Se não consegue, mata o processo 1 (`SIGKILL`), e o namespace de pid inteiro vai junto. Aqui há janela, a da troca do auxiliar. É aceita porque não há quem congele de fora.

### 5. A troca do auxiliar sem lançamentos perdidos

- **No `SIGTERM`:** o auxiliar marca a saída e fecha, sem atender, as conexões que o `accept` entrega daí em diante. Com o socket do systemd, elas esperariam na fila do socket pelo auxiliar seguinte, mas as que já foram aceitas precisam cair para quem pediu tentar de novo.
- **No lançamento:** o `scope::register` tenta de novo, a cada 300 ms até 10 s, quando a conexão é recusada, quando falta o socket ou quando ela cai sem resposta. Uma recusa com mensagem (um erro do pedido) não se repete.

## Risks / Trade-offs

- **[A janela do `run` sem o daemon]** → aceita (decisão 4). O `run` sem terminal é curto, e o reinício do auxiliar também.
- **[O daemon morre com a aba congelada]** → a aba fica congelada, como no change 78. As sessões do daemon terminam com ele.
- **[Mais uma conexão por aba no auxiliar]** → a vigia só pede o `status` e não conta nos limites de confinamentos, observações ou escopos.
- **[Um descritor de namespace guardado pelo daemon]** → segura o namespace enquanto a aba vive, e é solto quando ela termina ou é relançada.

## Migration Plan

- O lançador, o daemon e o auxiliar mudam juntos (protocolo 44). Um daemon antigo não conhece o campo `scope` e responde sem ele: o lançador fica com a vigia dele (decisão 4).
- Um auxiliar antigo não fecha as conexões novas no `SIGTERM`, e as tentativas do lançador cobrem isso.
