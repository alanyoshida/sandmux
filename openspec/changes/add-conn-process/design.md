## Context

Motivação em `proposal.md`. O que importa para o desenho:

- **O gateway de uma aba** roda no lançador (`sandmux-run`), fora do sandbox, com o TUN do namespace de rede da aba (`sandmux-net`). Cada conexão TCP chega como um `IpStackTcpStream`, com a origem (o endereço e a porta do socket do processo no sandbox) e o destino. A linha do registro é gravada quando a conexão termina, ou na hora quando ela é bloqueada.
- **O gateway já conversa com o daemon** pelo socket do daemon (`VaultAccess`): uma conexão curta por pedido, com `Hello`/`Welcome` e uma mensagem (`VaultInject`, `VaultCapture`).
- **O change 72 deixou a observação de uma aba** (`observe`): um programa `tp_btf` preso pelo id do cgroup, uma fila de eventos e a thread do daemon (`execlog.rs`) que lê os eventos da aba.
- **O kernel 7.0 tem o tracepoint `sock:inet_sock_set_state`** com BTF (`btf_trace_inet_sock_set_state`: o `sock`, o estado antigo e o novo). Na passagem para `TCP_SYN_SENT`, o processo atual é o que chamou o `connect`.
- **No Docker privado**, a ponte do Docker faz NAT dentro do `rootlesskit`, e a porta de origem que o gateway vê não é a do socket do processo. Por isso o Docker fica fora (decisão do usuário).

## Goals / Non-Goals

**Goals:**

- O pid e o executável de cada conexão das abas no registro de rede, com a fonte.
- O kernel como fonte do TCP quando há o auxiliar, e o `/proc` em toda máquina.
- Nenhum atraso numa conexão por causa da busca: ela acontece só na hora de gravar a linha.

**Non-Goals:**

- O processo das conexões do Docker privado.
- O processo das consultas de DNS, que o próprio gateway atende.
- Entregar a chave real só ao binário do harness. Isso só dificultaria, porque o agente pode depurar o harness dentro do sandbox.
- Os argumentos na linha de rede: eles estão no registro de execuções, pelo pid.

## Decisions

### 1. Programa no tracepoint `inet_sock_set_state`

A observação da aba carrega um segundo programa `tp_btf`, com o mesmo filtro pelo id do cgroup e a mesma fila do programa de execução.

- **O que ele faz:** sai quando o estado novo não é `TCP_SYN_SENT` (2). Depois reserva o evento `EV_CONN` (32 bytes) na fila, com:
  - o pid (o `tgid` atual);
  - a família;
  - a porta de origem (`skc_num`) e a de destino (`skc_dport`, convertida no daemon);
  - o endereço de destino (`skc_daddr`, ou os 16 bytes de `skc_v6_daddr` no IPv6).
- **Por que no `SYN_SENT`:** nesse momento o processo atual ainda é o que pediu a conexão, e a porta de origem já foi escolhida. No `connect` pelo cgroup (`cgroup/connect4`), a porta de origem ainda não existe.

Os campos do `sock` são lidos pelo BTF (`sock.__sk_common.skc_*`), como os outros deslocamentos.

### 2. O mapa de conexões no daemon

A thread da aba (`execlog.rs`) recebe os eventos de conexão junto com os de execução.

- **A chave** de cada evento é a porta de origem, o IP e a porta de destino. O valor é o pid e o executável, lido em `/proc/<pid>/exe` na hora em que o evento chega, com o processo quase sempre ainda vivo.
- **O mapa é da sessão**, com no máximo 4096 entradas, e as mais velhas saem primeiro.

### 3. A pergunta do gateway

Na hora de gravar a linha, o gateway da aba manda ao daemon `ConnProcess { tag, proto, sport, ip, port }`. Ele recebe `ConnProcessReply { pid, exe }`, com o processo ou nada.

- **A resposta vem do mapa da sessão** cuja última etiqueta é `tag`.
- **Um evento atrasado na fila** (a fila é lida a cada 300 ms) é coberto por uma segunda pergunta, 400 ms depois, só quando a aba está pelo kernel e a primeira não achou.
- **A resposta diz se a aba está pelo kernel**, para o gateway não esperar à toa nas abas pelo `/proc`.

A versão do protocolo sobe para 42.

### 4. O recuo pelo `/proc`

Quando o daemon não sabe (sem o auxiliar, ou UDP), o gateway procura sozinho, sem root:

1. Lê `/proc/<pid do sandbox>/net/{tcp,tcp6,udp,udp6}`, que mostra os sockets do namespace de rede da aba, e acha o inode do socket com a porta de origem e, no TCP, o destino.
2. Percorre os processos que estão no namespace de pid do sandbox (o mesmo link `/proc/<pid>/ns/pid`) e acha o que tem o `socket:[inode]` entre os descritores.
3. Lê o `/proc/<pid>/exe`.

Para não percorrer a máquina a cada conexão, a lista de processos do sandbox fica guardada por 1 segundo.

A busca acontece na hora de gravar a linha. Por isso uma conexão bloqueada na hora tem o processo quase sempre, e uma conexão longa que termina depois do processo, não.

### 5. Os campos e a consulta

O `Entry` do registro de rede ganha três campos, omitidos quando não há processo:

- `pid: Option<u32>`;
- `exe: Option<String>`;
- `proc_via: Option<String>`, com `kernel` ou `proc`.

`sandmux net` ganha a coluna `PROCESSO`, com o nome do executável e o pid (`curl 4321`). A tela de rede da TUI mostra o mesmo na linha escolhida.

## Risks / Trade-offs

- **[Processo que termina antes da busca pelo `/proc`]** → a linha fica sem processo. Com o auxiliar, o TCP não depende disso.
- **[Porta de origem repetida depois do fim de uma conexão]** → o mapa do daemon usa também o destino, e as entradas antigas saem pelo teto. Uma colisão daria o processo errado numa linha, nunca uma decisão errada.
- **[Custo da busca no `/proc` por conexão]** → só quando o kernel não sabe, com a lista de processos guardada por 1 segundo. É medido numa tarefa.
- **[Uma conexão por pedido ao daemon]** → é local e curta. As linhas são gravadas no fim das conexões, fora do caminho dos dados.

## Migration Plan

- O auxiliar, o daemon e o lançador mudam juntos. Depois de atualizar: `sandmux setup` e `sandmux daemon restart`.
