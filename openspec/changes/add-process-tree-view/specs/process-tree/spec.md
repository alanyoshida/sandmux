# Spec Delta

## Purpose

Acompanha, por sessão, a árvore de processos vivos do sandbox da aba — quem lançou quem, nome, pid, memória e linha de comando — e a expõe ao cliente da TUI para navegação.

## ADDED Requirements

### Requirement: Árvore de processos por sessão
O daemon MUST manter, para a sessão de cada aba, uma árvore em memória dos processos vivos dentro do sandbox dela: pid, ppid, nome, memória residente, cmdline e cgroup. A árvore MUST se atualizar a cada evento de execução e de saída de processo recebido do auxiliar root ou do supervisor de aberturas.

#### Scenario: Execução nova entra na árvore
- **WHEN** um processo dentro do sandbox da aba executa um programa
- **THEN** a árvore da sessão ganha ou atualiza o nó daquele pid, com o ppid de quem o lançou

#### Scenario: Saída remove o nó
- **WHEN** um processo observado termina
- **THEN** o nó dele MUST sair da árvore da sessão

#### Scenario: Órfão herda o ascendente vivo mais próximo
- **WHEN** um processo cujo processo-pai monitorado já saiu da árvore continua vivo
- **THEN** ele MUST continuar aparecendo na árvore, preso ao ascendente vivo mais próximo, ou à raiz quando não há nenhum

### Requirement: Fonte da árvore e sua marcação
Com o auxiliar root observando a aba, a árvore MUST se basear nos eventos do kernel (pid, ppid e saída) pelo mesmo caminho da observação das execuções. Sem o auxiliar, a árvore MUST se basear no que o supervisor de aberturas já acompanha para o registro de execuções, com a mesma marcação de fonte (`kernel` ou `supervisor`) usada lá.

#### Scenario: Fonte acompanha a do registro de execuções
- **WHEN** a aba troca de fonte de execuções (do supervisor para o kernel, ao auxiliar assumir)
- **THEN** a árvore de processos MUST passar a usar a mesma fonte, sem precisar reabrir a seção

### Requirement: Protocolo expõe a árvore ao cliente
O protocolo do daemon MUST ter uma mensagem para o cliente pedir a árvore de processos da sessão de uma aba e receber as atualizações seguintes enquanto a conexão dura.

#### Scenario: Cliente assina a árvore de uma aba
- **WHEN** a TUI abre a seção "Processos" para uma aba
- **THEN** ela MUST receber o estado atual da árvore e, depois, cada mudança, sem precisar pedir de novo

#### Scenario: Fim da assinatura
- **WHEN** a TUI fecha a seção "Processos" ou sai da aba
- **THEN** o daemon MUST parar de mandar atualizações daquela árvore para aquele cliente
