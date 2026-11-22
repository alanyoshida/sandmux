## ADDED Requirements

### Requirement: Aviso de auxiliar root antigo
Quando o auxiliar root em execução responde com uma versão anterior à do sandmux, o `sandmux doctor` MUST mostrar um aviso com as duas versões e a correção (`sandmux setup`). O aviso MUST NOT contar como falha nem mudar o código de saída. Sem auxiliar em execução, ou com o auxiliar na mesma versão ou numa mais nova, o `sandmux doctor` MUST NOT mostrar o aviso.

#### Scenario: Auxiliar de antes da atualização
- **WHEN** o sandmux instalado é o 0.1.32, o auxiliar em execução responde 0.1.30 e o usuário roda `sandmux doctor`
- **THEN** a saída tem um aviso com `0.1.30`, `0.1.32` e `sandmux setup`, e o código de saída é o mesmo de sem o aviso

#### Scenario: Auxiliar na mesma versão
- **WHEN** o auxiliar em execução responde a mesma versão do sandmux
- **THEN** o `sandmux doctor` não mostra o aviso

#### Scenario: Sem auxiliar
- **WHEN** nenhum auxiliar responde no socket
- **THEN** o `sandmux doctor` não mostra o aviso de versão

#### Scenario: Comparação por número
- **WHEN** o auxiliar responde 0.1.9 e o sandmux é o 0.1.10
- **THEN** o auxiliar conta como antigo
