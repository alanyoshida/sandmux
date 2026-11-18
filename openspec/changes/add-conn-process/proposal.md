## Why

O registro de rede diz o domínio, o IP, a porta e os bytes de cada conexão de uma aba, mas não diz qual processo a abriu. Não dá para separar a chamada de inferência do harness de um `curl` lançado por uma ferramenta, nem de um pacote que um `npm install` baixou. Era a segunda das "Outras lacunas do backend eBPF" do roadmap (seção 5), e o segundo item que o usuário pediu na etapa 15.

Decisões do usuário (2026-11-18), ao propor:

- os dois mecanismos, como no change 72: o kernel, pelo auxiliar root, e o `/proc` como recuo nas máquinas sem ele, com a linha marcando a fonte;
- o registro guarda o pid e o executável, e os argumentos ficam no registro de execuções (change 72), pelo pid;
- só as abas. No Docker privado, o NAT da ponte troca a porta de origem antes do gateway, e o processo não casa de forma exata. As linhas do Docker continuam sem processo.

## What Changes

- **Pelo kernel:** a observação de uma aba (o pedido `observe` do change 72) ganha programas `fexit` na saída de `tcp_v4_connect` e `tcp_v6_connect`. Quando o `connect` escolheu a porta de origem, eles mandam:
  - o pid do processo que chamou o `connect`;
  - a porta de origem;
  - o endereço e a porta de destino.

  O daemon guarda esse mapa por aba, com o executável lido em `/proc/<pid>/exe`.
- **Pelo `/proc` (recuo):** quando o kernel não sabe, o gateway da aba acha o socket no começo da conexão, antes de abrir a conexão de saída, enquanto o processo espera a resposta. O inode vem da consulta exata do sock_diag, num socket que o sandbox cria no próprio namespace de rede e passa ao lançador. Sem ele, vem de `/proc/<pid>/net/tcp` (ou `udp`, `tcp6`, `udp6`). Depois o gateway procura o processo dono do socket na árvore de processos do sandbox, como o `ss -p`. Não precisa de root.
- **Gateway da aba:** quando grava a linha de uma conexão, pergunta ao daemon pelo processo (o kernel) e usa o resultado do `/proc` quando não há resposta.
- **Registro de rede:** três campos novos, `pid`, `exe` e `proc_via` (`kernel` ou `proc`). `sandmux net` e a tela de rede da TUI ganham a coluna do processo.

Decisões do agente:

- O UDP vem só do `/proc`, porque os programas do kernel são só de TCP.
- O DNS que o processo manda ao gateway não ganha processo: a consulta é atendida pelo próprio gateway.

Limites, escritos na proposta:

- Sem o auxiliar, um processo que fecha a conexão sem esperar resposta pode terminar antes da busca e ficar sem processo na linha. Cada conexão espera a busca, cerca de 1 ms.
- O Docker privado fica sem processo.

## Capabilities

### New Capabilities

(nenhuma)

### Modified Capabilities

- `network-gateway`: o registro de rede ganha o processo de cada conexão das abas.
- `root-helper`: a observação de uma aba passa a mandar também as conexões TCP.

## Impact

- `crates/sandmux-ebpf`: os programas `fexit` de `tcp_v4_connect` e `tcp_v6_connect`, o evento `EV_CONN` e a carga junto com o de execução, na mesma fila.
- `crates/sandmux-launcher`: o socket de sock_diag criado no namespace de rede do sandbox e passado ao gateway junto com o TUN.
- `crates/sandmux/src/helper.rs`: a observação com os dois programas e o evento como JSON.
- `crates/sandmux/src/daemon/execlog.rs`: o mapa de conexões por aba, o executável e a resposta à pergunta do gateway.
- `crates/sandmux-net`: a pergunta ao daemon, a busca no `/proc` e os campos novos do registro.
- `crates/sandmux-proto`: o pedido novo do gateway ao daemon. A versão do protocolo muda.
- `crates/sandmux/src/main.rs` (o `sandmux net`) e a tela de rede da TUI.
- `docs/network.md`, `docs/helper.md` e os pares em inglês.
