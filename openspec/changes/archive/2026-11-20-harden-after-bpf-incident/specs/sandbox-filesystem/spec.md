## MODIFIED Requirements

### Requirement: Pastas acrescentadas pelo usuário
`sandmux run` MUST aceitar `--ro <caminho>` e `--rw <caminho>`, repetíveis, que acrescentam um arquivo ou pasta do host à visão do sandbox, no caminho original, em somente leitura ou com leitura e escrita. Um caminho que não existe MUST fazer o `run` terminar com erro.

#### Scenario: Pasta somente leitura
- **WHEN** o usuário passa `--ro ~/docs`
- **THEN** o processo confinado lê `~/docs` e não consegue escrever nela

#### Scenario: Pasta com escrita
- **WHEN** o usuário passa `--rw ~/saida`
- **THEN** o processo confinado cria arquivos em `~/saida` e eles aparecem no host

#### Scenario: Caminho inexistente
- **WHEN** o caminho passado não existe
- **THEN** o `run` termina com erro e não lança o sandbox

#### Scenario: Pipe com nome
- **WHEN** o usuário passa `--ro` com um FIFO
- **THEN** o lançador aplica a regra sem abrir o pipe, e o sandbox sobe

#### Scenario: Arquivo que o usuário só alcança pelo caminho
- **WHEN** o usuário passa `--ro` com uma pasta que ele atravessa mas não lista
- **THEN** a regra do Landlock vale para ela, nos binários das releases e nos de desenvolvimento
