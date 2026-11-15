## ADDED Requirements

### Requirement: Painéis na aba
Ctrl+b `%` MUST dividir o painel ativo lado a lado, e Ctrl+b `"` em cima e embaixo. As duas abrem o modal de nova aba com a pasta do painel ativo, e a sessão criada MUST ocupar a parte nova. Cada painel MUST mostrar a sua sessão, com o tamanho do painel. A entrada do teclado MUST ir só ao painel ativo.

#### Scenario: Dividir lado a lado
- **WHEN** a aba tem um painel e o usuário aperta Ctrl+b `%` e cria uma sessão do shell no modal
- **THEN** a aba mostra as duas sessões lado a lado, e o que é digitado vai à sessão nova

### Requirement: Foco, ampliar, redimensionar e fechar painéis
Ctrl+b com uma seta MUST mover o foco ao painel vizinho naquela direção, e um clique num painel MUST pôr o foco nele. Ctrl+b `z` MUST alternar entre o painel ativo na aba inteira e o layout. Ctrl+b `<` e `>` MUST diminuir e aumentar o painel ativo. Fechar o painel ativo MUST encerrar a sessão dele e dar o espaço ao vizinho; com um painel só, a aba volta a ser comum.

#### Scenario: Foco pela seta
- **WHEN** a aba tem dois painéis lado a lado, o da direita está ativo e o usuário aperta Ctrl+b ←
- **THEN** o painel da esquerda passa a receber o que é digitado

#### Scenario: Fechar um painel
- **WHEN** o usuário fecha o painel ativo com Ctrl+b `x`
- **THEN** a sessão dele é encerrada, e o outro painel ocupa a aba inteira

### Requirement: Barra por aba com painéis
A barra MUST mostrar uma entrada por aba, com o nome do primeiro painel e `+N` para os outros painéis. Os números de Ctrl+b `1`..`9` e a troca de aba MUST contar as abas, não os painéis.

#### Scenario: Aba com dois painéis
- **WHEN** a aba 1 tem dois painéis
- **THEN** a barra mostra uma só entrada para ela, com `+1`
