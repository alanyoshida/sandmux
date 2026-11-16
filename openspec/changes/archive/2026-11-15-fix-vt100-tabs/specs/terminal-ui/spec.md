## MODIFIED Requirements

### Requirement: Emulação de terminal da aba
Cada aba MUST emular o terminal da sua sessão, mostrando o texto, as cores (16, 256 e cor verdadeira), os atributos (negrito, itálico, sublinhado, inverso), a posição e a visibilidade do cursor e a tela alternativa. Com a quebra automática de linha desligada (`CSI ? 7 l`), um caractere escrito depois da última coluna MUST sobrescrevê-la, sem passar para a linha de baixo. O cursor MUST se mover pelas paradas de tabulação para trás (`CSI Z`) e para a frente (`CSI I`). Trocar de aba MUST mostrar a tela atual da outra aba sem esperar o harness redesenhar.

#### Scenario: Cor verdadeira
- **WHEN** o harness da aba ativa escreve um texto com cor de frente `38;2;10;200;30`
- **THEN** a TUI mostra esse texto com a mesma cor

#### Scenario: Troca de aba
- **WHEN** a aba 1 mostra a interface do opencode e o usuário vai para a aba 2 e volta
- **THEN** a interface do opencode aparece de novo na hora, igual a antes

#### Scenario: Quebra automática desligada
- **WHEN** o harness desliga a quebra com `CSI ? 7 l` e escreve mais caracteres que a largura da linha
- **THEN** a linha mostra o começo do texto com o último caractere na última coluna, e a linha de baixo não muda

#### Scenario: Tabulação para trás
- **WHEN** o cursor está na coluna 20 e o harness manda `CSI 2 Z` e escreve um caractere
- **THEN** o caractere aparece na coluna 9, duas paradas de tabulação para trás
