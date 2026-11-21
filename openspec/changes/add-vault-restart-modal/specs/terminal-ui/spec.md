# Spec Delta

## ADDED Requirements

### Requirement: Aviso de chaves movidas ao cofre

Quando o contador de chaves movidas ao cofre de uma sessão viva aumenta, a TUI MUST mostrar um modal por cima da aba que estiver ativa no momento, mesmo que não seja a aba daquela sessão, listando cada sessão viva nessa condição (identificador, nome e harness). O modal MUST NOT reiniciar nenhuma sessão sozinho, sem o usuário escolher isso nele.

#### Scenario: Chave movida numa aba fora de foco

- **WHEN** o vigia de segredos move uma chave real ao cofre de uma sessão viva e a aba ativa da TUI é outra
- **THEN** a TUI mostra o modal por cima da aba ativa, listando a sessão afetada, sem trocar de aba

#### Scenario: Várias sessões do mesmo harness afetadas de uma vez

- **WHEN** o vigia move uma chave do HOME sintético compartilhado por duas sessões vivas do mesmo harness
- **THEN** o modal lista as duas sessões numa única lista, em vez de abrir um modal por sessão

#### Scenario: Reiniciar a sessão selecionada

- **WHEN** o usuário seleciona uma sessão na lista do modal e confirma
- **THEN** a TUI pede ao daemon o reinício forçado daquela sessão e a remove da lista; se a lista ficar vazia, o modal fecha

#### Scenario: Reiniciar todas as listadas

- **WHEN** o usuário escolhe a ação de reiniciar todas
- **THEN** a TUI pede ao daemon o reinício forçado de cada sessão listada e o modal fecha

#### Scenario: Fechar sem reiniciar

- **WHEN** o usuário aperta Esc no modal
- **THEN** o modal fecha, nenhuma sessão é reiniciada, e o aviso continua disponível no `sandmux ls` e na seção Diagnóstico

#### Scenario: Nova chave movida depois de o modal ter sido fechado

- **WHEN** o usuário fecha o modal sem reiniciar e, depois, o vigia move uma chave de uma sessão ainda não mostrada nessa leva
- **THEN** a TUI abre o modal de novo, agora com essa sessão

#### Scenario: Sessão reiniciada não aparece mais

- **WHEN** uma sessão listada no modal é reiniciada (pelo modal ou por Ctrl+b `r` direto na aba)
- **THEN** essa sessão não aparece de novo no modal até que uma nova chave seja movida para ela depois do reinício

#### Scenario: Duas TUIs anexadas ao mesmo conjunto de sessões

- **WHEN** duas TUIs diferentes estão anexadas e uma chave é movida numa sessão
- **THEN** cada TUI decide por conta própria se já mostrou aquele aumento do contador, podendo uma já ter fechado o modal enquanto a outra ainda o mostra
