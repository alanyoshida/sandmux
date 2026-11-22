## ADDED Requirements

### Requirement: Aviso de auxiliar antigo na seção Processos
Quando o auxiliar root em execução é de uma versão anterior à do sandmux, a seção "Processos" MUST mostrar, acima da árvore ou do motivo de ela não existir, um aviso com as duas versões e a correção (`sandmux setup`), dizendo que a árvore pode sair sem os pais. A seção MUST conferir a versão ao abrir e ao reler. Sem auxiliar, ou com o auxiliar na mesma versão ou numa mais nova, a seção MUST NOT mostrar o aviso.

#### Scenario: Auxiliar antigo
- **WHEN** o auxiliar em execução responde uma versão anterior à do sandmux e o usuário abre a seção Processos
- **THEN** a seção mostra o aviso com as duas versões e `sandmux setup`

#### Scenario: Auxiliar em dia
- **WHEN** o auxiliar em execução responde a mesma versão do sandmux e o usuário abre a seção Processos
- **THEN** a seção não mostra o aviso

#### Scenario: Reler depois de atualizar
- **WHEN** o aviso está na tela, o auxiliar passa a responder a versão do sandmux e o usuário aperta `r`
- **THEN** o aviso some
