# process-tree Specification

## Purpose
Acompanha, por sessão, a árvore de processos vivos do sandbox da aba — quem lançou quem, nome, pid, memória e linha de comando — a partir das execuções e saídas vistas pelo kernel, e a entrega ao cliente da TUI.

## Requirements

### Requirement: Árvore de processos por sessão
Com a observação das execuções pelo kernel ligada para a aba, o daemon MUST manter uma árvore em memória dos processos vivos no escopo dela, com o pid e o ppid de cada um, atualizada a cada evento de execução e de saída recebido do auxiliar root. Ao ligar a observação, o daemon MUST incluir os processos que já estão no escopo.

#### Scenario: Execução nova entra na árvore
- **WHEN** um processo dentro do escopo da aba executa um programa
- **THEN** a árvore da sessão ganha ou atualiza o nó daquele pid, preso ao ppid de quem o lançou

#### Scenario: Saída remove o nó
- **WHEN** um processo observado termina
- **THEN** o nó dele MUST sair da árvore da sessão

#### Scenario: Órfão herda o ascendente vivo mais próximo
- **WHEN** um processo do meio da árvore termina e os filhos dele continuam vivos
- **THEN** os filhos MUST continuar na árvore, presos ao pai do processo que terminou, ou à raiz quando ele não está na árvore

#### Scenario: Processos de antes da observação
- **WHEN** a observação liga com a aba já rodando processos
- **THEN** a árvore MUST começar com eles, sem esperar uma execução nova

### Requirement: Árvore sem o auxiliar root
Sem a observação das execuções pelo kernel, o daemon MUST NOT montar a árvore da aba, e a resposta ao pedido da árvore MUST trazer o motivo, o mesmo que a proteção do lançamento mostra para a fonte das execuções.

#### Scenario: Aba com o supervisor
- **WHEN** a aba registra as execuções pelo supervisor, porque o auxiliar root não está disponível
- **THEN** o pedido da árvore devolve nenhum processo e o motivo

### Requirement: Pedido da árvore pelo cliente
O protocolo do daemon MUST ter um pedido da árvore de processos da sessão de uma aba, respondido com os processos vivos (pid, ppid, nome, linha de comando, memória residente, CPU usado, início e cgroup). Antes de responder, o daemon MUST tirar da árvore os processos que já não existem ou cujo pid passou a outro processo.

#### Scenario: Pedido de uma aba observada
- **WHEN** a TUI pede a árvore de uma aba com a observação pelo kernel ligada
- **THEN** o daemon devolve os processos vivos da aba, cada um com o ppid

#### Scenario: Saída perdida
- **WHEN** um processo terminou sem que o evento de saída chegasse ao daemon
- **THEN** o pedido seguinte da árvore MUST vir sem ele
