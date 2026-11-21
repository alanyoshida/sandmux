# Spec Delta

## MODIFIED Requirements

### Requirement: Observação das execuções
O pedido `observe` MUST carregar só a observação das execuções e das conexões TCP no cgroup pedido e nos de baixo, sem regra de arquivos nem marca, e o `confine` MUST incluir a das execuções. Cada execução MUST virar um evento com o caminho executado, o pid, o ppid e os argumentos; cada conexão TCP, um evento com o pid, a porta de origem e o destino, pela conexão que fez o pedido. A observação MUST NOT negar nem atrasar nada, e MUST terminar quando a conexão com o auxiliar fecha.

#### Scenario: Aba observada
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente roda `ls -la`
- **THEN** o daemon recebe um evento com o caminho do `ls` e os argumentos `ls` e `-la`

#### Scenario: Conexão da aba
- **WHEN** o daemon pede o `observe` do escopo de uma aba e o agente abre uma conexão TCP para `1.1.1.1:443`
- **THEN** o daemon recebe um evento com o pid do processo, a porta de origem e o destino `1.1.1.1:443`

#### Scenario: Conexão fechada
- **WHEN** o daemon que pediu o `observe` termina
- **THEN** o auxiliar solta os programas, e as execuções e as conexões daquele cgroup deixam de gerar eventos

#### Scenario: Evento de execução traz o ppid
- **WHEN** um processo dentro do cgroup observado executa um programa
- **THEN** o evento mandado ao daemon MUST incluir o ppid de quem o executou, além do caminho, do pid e dos argumentos

## ADDED Requirements

### Requirement: Saída de processo observado
O pedido `observe` e o `confine` MUST mandar um evento quando um processo que executou dentro do cgroup observado termina, com o pid dele. Esse evento MUST seguir as mesmas regras da observação: não nega nem atrasa a saída, e termina quando a conexão com o auxiliar fecha.

#### Scenario: Processo observado termina
- **WHEN** um processo que apareceu num evento de execução termina
- **THEN** o auxiliar MUST mandar ao daemon um evento de saída com o pid dele, pela mesma conexão
