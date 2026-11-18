## Context

Motivação em `proposal.md`. O que importa para o desenho:

- **O gateway de uma aba** roda no lançador (`sandmux-run`), fora do sandbox, com o TUN do namespace de rede da aba (`sandmux-net`). Cada conexão TCP chega como um `IpStackTcpStream`, com a origem (o endereço e a porta do socket do processo no sandbox) e o destino. A linha do registro é gravada quando a conexão termina, ou na hora quando ela é bloqueada.
- **O gateway já conversa com o daemon** pelo socket do daemon (`VaultAccess`): uma conexão curta por pedido, com `Hello`/`Welcome` e uma mensagem (`VaultInject`, `VaultCapture`).
- **O change 72 deixou a observação de uma aba** (`observe`): um programa `tp_btf` preso pelo id do cgroup, uma fila de eventos e a thread do daemon (`execlog.rs`) que lê os eventos da aba.
- **O kernel 7.0 tem `tcp_v4_connect` e `tcp_v6_connect`** com BTF (o `sock` é o primeiro argumento). Na saída delas, o processo atual é o que chamou o `connect`, e a porta de origem já foi escolhida.
- **Ler `/proc/<pid>/net/tcp` custa caro:** o kernel monta o arquivo varrendo a tabela inteira de conexões do host (524.288 entradas na máquina de referência), cerca de 2,5 ms por leitura, mesmo num namespace de rede vazio.
- **No Docker privado**, a ponte do Docker faz NAT dentro do `rootlesskit`, e a porta de origem que o gateway vê não é a do socket do processo. Por isso o Docker fica fora (decisão do usuário).

## Goals / Non-Goals

**Goals:**

- O pid e o executável de cada conexão das abas no registro de rede, com a fonte.
- O kernel como fonte do TCP quando há o auxiliar, e o `/proc` em toda máquina.
- Pouco atraso numa conexão por causa da busca: nenhum pelo kernel, cerca de 1 ms pelo `/proc`.

**Non-Goals:**

- O processo das conexões do Docker privado.
- O processo das consultas de DNS, que o próprio gateway atende.
- Entregar a chave real só ao binário do harness. Isso só dificultaria, porque o agente pode depurar o harness dentro do sandbox.
- Os argumentos na linha de rede: eles estão no registro de execuções, pelo pid.

## Decisions

### 1. Programas `fexit` em `tcp_v4_connect` e `tcp_v6_connect`

A observação da aba carrega um programa `fexit` (`BPF_TRACE_FEXIT`) para cada uma das duas funções, com o mesmo filtro pelo id do cgroup e a mesma fila do programa de execução.

- **O que ele faz:** lê o `sock` do primeiro argumento e sai quando a porta de origem (`skc_num`) ainda é 0, isto é, quando o `connect` falhou antes de escolhê-la. Depois reserva o evento `EV_CONN` (48 bytes) na fila, com:
  - o pid (o `tgid` atual);
  - a família;
  - a porta de origem (`skc_num`) e a de destino (`skc_dport`, convertida no daemon);
  - o endereço de destino (`skc_daddr`, ou os 16 bytes de `skc_v6_daddr` no IPv6).
- **Por que na saída do `connect`:** nesse momento o processo atual ainda é o que pediu a conexão, e a porta de origem já foi escolhida. O tracepoint `inet_sock_set_state` foi a primeira escolha, mas na passagem para `SYN_SENT` a porta de origem ainda é 0. No `connect` pelo cgroup (`cgroup/connect4`), ela também ainda não existe.

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

Quando o daemon não sabe (sem o auxiliar, ou UDP), o gateway procura sozinho, sem root. O `/proc` só mostra o socket enquanto ele está aberto, e uma conexão curta (um pedido e a resposta, cerca de 2 ms) fecha antes de a linha ser gravada. Por isso a busca acontece no começo da conexão, e o gateway a espera (até 100 ms) antes de abrir a conexão de saída: até a resposta chegar, o processo do sandbox mantém o socket aberto. O resultado fica guardado pela origem até a linha ser gravada.

1. **O inode do socket**, pela consulta exata do sock_diag (`inet_diag_req_v2` sem `NLM_F_DUMP`, pelas quatro pontas da conexão). O socket de sock_diag fica preso ao namespace de rede em que foi criado: o filho do lançador o cria logo depois do TUN, ainda no namespace do sandbox, e o manda ao lançador pelo mesmo canal. Sem ele (ou com erro), o gateway lê `/proc/<pid do sandbox>/net/{tcp,tcp6,udp,udp6}` e acha a linha com a porta de origem e, no TCP, o destino.
2. **O dono do socket:** o gateway percorre a árvore de processos do sandbox, pelos `/proc/<pid>/task/<tid>/children` a partir do processo do sandbox (pela árvore de pais de `/proc/*/stat` quando o kernel não tem esse arquivo), e acha o processo que tem o `socket:[inode]` entre os descritores. O link do namespace de pid exige mais permissão do que o gateway tem.
3. **O executável**, em `/proc/<pid>/exe`.

Numa aba pelo kernel (o daemon já respondeu assim), o gateway não faz essa busca nem a espera.

Uma conexão bloqueada na hora tem o processo; uma conexão cujo processo fecha sem esperar resposta pode ficar sem ele.

### 5. Os campos e a consulta

O `Entry` do registro de rede ganha três campos, omitidos quando não há processo:

- `pid: Option<u32>`;
- `exe: Option<String>`;
- `proc_via: Option<String>`, com `kernel` ou `proc`.

`sandmux net` ganha a coluna `PROCESSO`, com o nome do executável e o pid (`curl 4321`). A tela de rede da TUI mostra o mesmo na linha escolhida.

## Risks / Trade-offs

- **[Processo que fecha a conexão sem esperar resposta]** → a busca pelo `/proc` pode chegar depois, e a linha fica sem processo. Com o auxiliar, o TCP não depende disso.
- **[Porta de origem repetida depois do fim de uma conexão]** → o mapa do daemon usa também o destino, e as entradas antigas saem pelo teto. Uma colisão daria o processo errado numa linha, nunca uma decisão errada.
- **[Custo da busca no `/proc` por conexão]** → só quando o kernel não sabe. A espera fica em cerca de 1 ms com o sock_diag e só na árvore do sandbox; sem o sock_diag, a leitura de `net/tcp` custa uns 5 ms. Medida: 200 conexões a um servidor local em 0,5 s com o auxiliar e 0,7 a 0,8 s sem ele, com o processo em todas as linhas nos dois casos.
- **[Uma conexão por pedido ao daemon]** → é local e curta. As linhas são gravadas no fim das conexões, fora do caminho dos dados.

## Migration Plan

- O auxiliar, o daemon e o lançador mudam juntos. Depois de atualizar: `sandmux setup` e `sandmux daemon restart`.
