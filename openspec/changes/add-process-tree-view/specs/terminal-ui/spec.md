# Spec Delta

## ADDED Requirements

### Requirement: Processos na página de configurações
A página de configurações MUST ter a seção "Processos", depois de "Diagnóstico", com a árvore de processos do último lançamento da aba ativa: um processo por linha, em outline, com uma migalha de pão no topo mostrando o caminho da raiz até o nó atual. A árvore MUST atualizar a cada segundo.

#### Scenario: Abrir a seção mostra a árvore da aba ativa
- **WHEN** o usuário seleciona a seção "Processos" na página de configurações
- **THEN** a tela mostra a árvore de processos do lançamento atual da aba que estava ativa ao abrir a página, com a migalha mostrando só a raiz

#### Scenario: Atualização ao vivo preserva navegação
- **WHEN** um processo novo aparece ou termina enquanto a seção está aberta
- **THEN** a árvore MUST refletir a mudança na atualização seguinte (até 1 segundo depois), sem mover a seleção nem desfazer dobras manuais do usuário

### Requirement: Zoom de densidade na árvore de processos
A seção MUST aceitar uma tecla de zoom que troca, para a árvore inteira, entre 3 níveis de densidade por linha: nome e pid; nome, pid e memória; e um cartão com cmdline, CPU, memória, início e cgroup. A troca de nível MUST NOT mudar a dobra ou a seleção.

#### Scenario: Aumentar o zoom mostra mais detalhe
- **WHEN** o usuário aumenta o zoom estando no nível 1 ou 2
- **THEN** cada linha visível passa a mostrar os campos do próximo nível, sem recolher nem expandir nenhum ramo

### Requirement: Dobra de ramos na árvore de processos
Cada processo com filhos MUST poder ser dobrado ou desdobrado por um atalho dedicado, marcado por ▸ (dobrado) ou ▾ (desdobrado). Um ramo com mais descendentes que um limiar configurado MUST nascer dobrado quando a árvore é montada ou re-enraizada.

#### Scenario: Ramo grande nasce dobrado
- **WHEN** a árvore é montada ou re-enraizada e um nó tem mais descendentes que o limiar
- **THEN** esse nó aparece dobrado (▸), com a contagem dos descendentes escondidos

#### Scenario: Usuário desdobra manualmente
- **WHEN** o usuário aciona o atalho de dobra num nó dobrado
- **THEN** os filhos diretos aparecem, respeitando a dobra automática de cada um deles

### Requirement: Drill-down na árvore de processos
Um atalho dedicado num processo selecionado MUST re-enraizar a tela nesse processo, mostrando só ele e os descendentes dele, e MUST acrescentar esse processo ao fim da migalha de pão. Um atalho de voltar, ou um clique num segmento anterior da migalha, MUST re-enraizar a tela nesse segmento, descartando os segmentos depois dele.

#### Scenario: Entrar num ramo
- **WHEN** o usuário aciona o atalho de entrar com um processo selecionado
- **THEN** a tela passa a mostrar só esse processo e os descendentes dele, e a migalha ganha mais um segmento com o nome dele

#### Scenario: Voltar pela migalha
- **WHEN** o usuário aciona o atalho de voltar, ou clica num segmento anterior da migalha
- **THEN** a tela volta a mostrar a árvore daquele segmento, e os segmentos depois dele desaparecem da migalha
